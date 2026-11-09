//! Connection checks: endpoint syntax, what each endpoint names, and port names against the
//! ports the catalog resolves for each block as configured (#440).

use oce_api::{ClassPort, ClassPorts};

use crate::identifier::check_identifier_syntax;
use crate::model::{Endpoint, Sequence};

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

pub(super) fn check(sequence: &Sequence, ports: &[Option<ClassPorts>], findings: &mut Findings) {
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
            if let Some((code, detail)) = resolve(sequence, ports, end, endpoint) {
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

/// What a well-formed endpoint fails to name, as `(code, detail)`, or `None` when it resolves.
fn resolve(
    sequence: &Sequence,
    ports: &[Option<ClassPorts>],
    end: End,
    endpoint: &Endpoint,
) -> Option<(&'static str, String)> {
    let unknown = match end {
        End::Source => UNKNOWN_SOURCE,
        End::Target => UNKNOWN_TARGET,
    };
    match endpoint {
        Endpoint::Boundary(name) => {
            let (wanted, other) = match end {
                End::Source => (&sequence.inputs, &sequence.outputs),
                End::Target => (&sequence.outputs, &sequence.inputs),
            };
            if wanted.iter().any(|b| b.name == *name) {
                return None;
            }
            let what = if other.iter().any(|b| b.name == *name) {
                match end {
                    End::Source => "a boundary output, which cannot drive a connection",
                    End::Target => "a boundary input, which cannot be driven",
                }
            } else if sequence.params.iter().any(|p| p.name == *name) {
                "a sequence parameter, not a connector"
            } else if sequence.blocks.iter().any(|b| b.name == *name) {
                "a block; name one of its ports as `block.port`"
            } else {
                "not declared"
            };
            Some((unknown, format!("the {} `{name}` is {what}", end.word())))
        }
        Endpoint::Port { block, port } => {
            let Some(index) = sequence.blocks.iter().position(|b| b.name == *block) else {
                return Some((
                    unknown,
                    format!("the {} block `{block}` is not declared", end.word()),
                ));
            };
            // Ports are unknown when the block's class or parameters were refused; that
            // finding is the one to fix first.
            let resolved = ports.get(index)?.as_ref()?;
            let (wanted, other, side, other_side) = match end {
                End::Source => (&resolved.outputs, &resolved.inputs, "output", "input"),
                End::Target => (&resolved.inputs, &resolved.outputs, "input", "output"),
            };
            if wanted.iter().any(|p| p.name == port.as_str()) {
                return None;
            }
            let hint = if other.iter().any(|p| p.name == port.as_str()) {
                format!("`{port}` is an {other_side}")
            } else {
                format!("its {side}s are: {}", names(wanted))
            };
            Some((
                UNKNOWN_PORT,
                format!(
                    "block `{block}` (`{}`) has no {side} port `{port}`; {hint}",
                    resolved.class_path
                ),
            ))
        }
    }
}

fn names(ports: &[ClassPort]) -> String {
    if ports.is_empty() {
        return "none".to_owned();
    }
    ports
        .iter()
        .map(|p| p.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}
