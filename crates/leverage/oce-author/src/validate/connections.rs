//! Connection checks: endpoint syntax, what each endpoint names, and port names against the
//! ports the catalog resolves for each block as configured (#440).

use crate::identifier::check_identifier_syntax;
use crate::model::{Endpoint, Sequence};

use super::index::{Declared, Ports};

use super::{Anchor, Findings, MALFORMED_CONNECTION, UNKNOWN_PORT, UNKNOWN_SOURCE, UNKNOWN_TARGET};

/// Which end of a connection.
#[derive(Clone, Copy)]
enum End {
    Source,
    Target,
}

impl End {
    fn word(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Target => "target",
        }
    }
}

pub(super) fn check(
    sequence: &Sequence,
    declared: &Declared<'_>,
    ports: &[Option<Ports>],
    findings: &mut Findings,
) {
    for (index, connection) in sequence.connections.iter().enumerate() {
        let position = Anchor::Connection(index);
        let subject = format!("connection[{index}]");
        let spelled = connection.spelling();
        let mut well_formed = true;
        for (end, endpoint) in [
            (End::Source, &connection.source),
            (End::Target, &connection.target),
        ] {
            if let Some(problem) = syntax_problem(endpoint) {
                well_formed = false;
                findings.error(
                    position,
                    MALFORMED_CONNECTION,
                    subject.as_str(),
                    format!("`{spelled}`: the {} {problem}", end.word()),
                );
            }
        }
        if !well_formed {
            continue;
        }
        if connection.source == connection.target {
            findings.error(
                position,
                MALFORMED_CONNECTION,
                subject.as_str(),
                format!("`{spelled}` connects an endpoint to itself"),
            );
            continue;
        }
        for (end, endpoint) in [
            (End::Source, &connection.source),
            (End::Target, &connection.target),
        ] {
            if let Some((code, detail)) = resolve(declared, ports, end, endpoint) {
                findings.error(
                    position,
                    code,
                    subject.as_str(),
                    format!("`{spelled}`: {detail}"),
                );
            }
        }
    }
}

/// The syntax problem of an endpoint, if any: every part must be an identifier.
fn syntax_problem(endpoint: &Endpoint) -> Option<String> {
    let parts: &[(&str, &str)] = match endpoint {
        Endpoint::Boundary(name) => &[("name", name.as_str())],
        Endpoint::Port { block, port } => &[("block", block.as_str()), ("port", port.as_str())],
    };
    parts.iter().find_map(|(what, part)| {
        check_identifier_syntax(part)
            .err()
            .map(|error| format!("{what} `{part}` is malformed: {error}"))
    })
}

/// The longest port list an `unknown-port` detail spells out.
const PORT_HINT_LIMIT: usize = 16;

/// What a well-formed endpoint fails to name, as `(code, detail)`, or `None` when it resolves.
fn resolve(
    declared: &Declared<'_>,
    ports: &[Option<Ports>],
    end: End,
    endpoint: &Endpoint,
) -> Option<(&'static str, String)> {
    let unknown = match end {
        End::Source => UNKNOWN_SOURCE,
        End::Target => UNKNOWN_TARGET,
    };
    match endpoint {
        Endpoint::Boundary(name) => {
            let name = name.as_str();
            let (wanted, other) = match end {
                End::Source => (&declared.inputs, &declared.outputs),
                End::Target => (&declared.outputs, &declared.inputs),
            };
            if wanted.contains_key(name) {
                return None;
            }
            let what = if other.contains_key(name) {
                match end {
                    End::Source => "a boundary output, which cannot drive a connection",
                    End::Target => "a boundary input, which cannot be driven",
                }
            } else if declared.params.contains_key(name) {
                "a sequence parameter, not a connector"
            } else if declared.blocks.contains_key(name) {
                "a block; name one of its ports as `block.port`"
            } else {
                "not declared"
            };
            Some((unknown, format!("the {} `{name}` is {what}", end.word())))
        }
        Endpoint::Port { block, port } => {
            let Some(&index) = declared.blocks.get(block.as_str()) else {
                return Some((
                    unknown,
                    format!("the {} block `{block}` is not declared", end.word()),
                ));
            };
            // Ports are unknown when the block's class or parameters were refused; that
            // finding is the one to fix first.
            let ports = ports.get(index)?.as_ref()?;
            let (wanted, other, listed, side, other_side) = match end {
                End::Source => (
                    &ports.outputs,
                    &ports.inputs,
                    &ports.resolved.outputs,
                    "output",
                    "input",
                ),
                End::Target => (
                    &ports.inputs,
                    &ports.outputs,
                    &ports.resolved.inputs,
                    "input",
                    "output",
                ),
            };
            if wanted.contains(port.as_str()) {
                return None;
            }
            let hint = if other.contains(port.as_str()) {
                format!("`{port}` is an {other_side}")
            } else if listed.is_empty() {
                format!("it has no {side}s")
            } else {
                let mut names: Vec<&str> = listed
                    .iter()
                    .take(PORT_HINT_LIMIT)
                    .map(|p| p.name.as_str())
                    .collect();
                if listed.len() > PORT_HINT_LIMIT {
                    names.push("…");
                }
                format!("its {side}s are: {}", names.join(", "))
            };
            Some((
                UNKNOWN_PORT,
                format!(
                    "block `{block}` (`{}`) has no {side} port `{port}`; {hint}",
                    ports.resolved.class_path
                ),
            ))
        }
    }
}
