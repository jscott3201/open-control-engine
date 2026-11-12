//! Stage 2, the engine side (authoring note, "Validator"; decisions 4 and 14).
//!
//! Emit the sequence with the default namespace, load the bytes in a throwaway
//! [`oce_api::Engine::new`] — never an engine a host runs — through the receipt path (the only
//! path that carries every coded diagnostic, the per-member `algebraic-loop` records included),
//! and map each diagnostic back onto the model:
//!
//! - every subject is an IRI of the emitter's scheme, so stripping the root prefix yields the
//!   model path: the root is the `sequence`, a declared name is itself, `block.param` is the
//!   assignment (an array node names its first authored element's anchor), and `block.port` is
//!   the first connection that drives it, else the first it drives, else the port path itself; a
//!   block-level message that backticks one of the block's assignments is about that assignment
//!   (`hys.uLow`), as stage 1 reports it; a name no declaration has but a connection uses is that
//!   connection;
//! - a subject outside the scheme, including the synthetic `connector#N` and `block#N` subjects
//!   the engine mints for a connector or block that has no IRI, cannot be mapped: the emitter
//!   gives every node an IRI and [`oce_api::LoadReport`] exposes no connector list, so such a
//!   finding is attached to the `sequence` with the raw subject quoted in `detail`;
//! - every warning becomes an error finding with the engine's code (decision 14), because the
//!   emitter's contract is a warning-free import; informational records are not findings.
//!
//! A load that fails without any coded diagnostic is one [`LOAD_FAILED`] finding on the
//! sequence; an emitter refusal after a clean stage 1 (only the document size can fail) is one
//! [`DOCUMENT_TOO_LARGE`] finding with `origin = author`. Every other finding here has
//! `origin = engine`.

use std::collections::BTreeSet;
use std::sync::{Mutex, OnceLock};

use oce_api::{ClassPorts, DiagnosticRecord, DiagnosticSeverity, DiagnosticSubject, Engine};

use crate::emit::{EmitOptions, Scheme, write};
use crate::identifier::Identifier;
use crate::model::{Endpoint, Sequence};
use crate::validate::catalog::{element_base, lookup_class};

use super::{Anchor, Finding, Findings, Origin, Severity};

/// The engine refused the load without a coded diagnostic; the detail carries its message.
pub const LOAD_FAILED: &str = "load-failed";
/// The emitted document is larger than [`oce_api::MAX_CXF_BYTES`].
pub const DOCUMENT_TOO_LARGE: &str = "document-too-large";

/// A `'static` spelling of an engine code. Engine codes are a closed vocabulary
/// (`oce_api::DiagCode`), so the set this keeps grows to at most that vocabulary's size.
fn intern(code: &str) -> &'static str {
    static CODES: OnceLock<Mutex<BTreeSet<&'static str>>> = OnceLock::new();
    let mut codes = CODES
        .get_or_init(|| Mutex::new(BTreeSet::new()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(&known) = codes.get(code) {
        return known;
    }
    let leaked: &'static str = Box::leak(code.to_owned().into_boxed_str());
    codes.insert(leaked);
    leaked
}

/// Run stage 2 over a sequence stage 1 passed; `ports` are stage 1's resolved interfaces.
pub(super) fn check(sequence: &Sequence, ports: &[Option<ClassPorts>]) -> Vec<Finding> {
    let mut findings = Findings::default();
    let options = EmitOptions::default();
    let bytes = match write(sequence, &options, ports) {
        Ok(bytes) => bytes,
        Err(error) => {
            // Stage 1 passed, so the only refusal left is the document size.
            findings.push(Finding {
                code: DOCUMENT_TOO_LARGE,
                severity: Severity::Error,
                subject: "sequence".to_owned(),
                span: None,
                detail: error.to_string(),
                origin: Origin::Author,
                anchor: Anchor::Sequence,
            });
            return findings.finish();
        }
    };
    judge(sequence, &bytes, &options.namespace)
}

/// Load `bytes` — the emission of `sequence` under `namespace` — in a throwaway engine and map
/// every diagnostic back onto the model.
pub(crate) fn judge(sequence: &Sequence, bytes: &[u8], namespace: &str) -> Vec<Finding> {
    let mut findings = Findings::default();
    let scheme = Scheme::new(namespace, sequence.name.as_str());
    let mut engine = Engine::new();
    match engine.load_cxf_with_receipt(bytes) {
        Ok((receipt, _)) => {
            for record in receipt.diagnostics().records() {
                record_finding(sequence, &scheme, record, &mut findings);
            }
        }
        Err(failure) => {
            let records = failure.diagnostics().records();
            for record in records {
                record_finding(sequence, &scheme, record, &mut findings);
            }
            if records.is_empty() {
                findings.push(engine_finding(
                    Anchor::Sequence,
                    LOAD_FAILED,
                    "sequence",
                    failure.error().to_string(),
                ));
            }
        }
    }
    findings.finish()
}

fn engine_finding(
    anchor: Anchor,
    code: &'static str,
    subject: impl Into<String>,
    detail: String,
) -> Finding {
    Finding {
        code,
        severity: Severity::Error,
        subject: subject.into(),
        span: None,
        detail,
        origin: Origin::Engine,
        anchor,
    }
}

fn record_finding(
    sequence: &Sequence,
    scheme: &Scheme,
    record: &DiagnosticRecord,
    findings: &mut Findings,
) {
    let key = record.key();
    if key.severity() == DiagnosticSeverity::Info {
        return;
    }
    let code = intern(key.code());
    let message = record.message().to_owned();
    let raw = match key.subject() {
        DiagnosticSubject::Opaque(raw) => Some(raw.as_str()),
        _ => None,
    };
    let (anchor, subject, detail) = map_subject(sequence, scheme, raw, message);
    findings.push(engine_finding(anchor, code, subject, detail));
}

/// The anchor, subject and detail of an engine diagnostic with subject `raw` and `message`: a
/// subject under the scheme maps to its model path; any other — `connector#N` or `block#N` for an
/// IRI-less connector or block, a foreign IRI — stays on `sequence` with the raw subject quoted in
/// the detail; no subject is the sequence's.
pub(super) fn map_subject(
    sequence: &Sequence,
    scheme: &Scheme,
    raw: Option<&str>,
    message: String,
) -> (Anchor, String, String) {
    let mapped = raw
        .and_then(|raw| scheme.model_path(raw))
        .map(|path| locate(sequence, path));
    match (mapped, raw) {
        (Some((Anchor::Block(index, None), subject)), _) => {
            let (anchor, subject) = refine(sequence, index, subject, &message);
            (anchor, subject, message)
        }
        (Some((anchor, subject)), _) => (anchor, subject, message),
        (None, Some(raw)) => (
            Anchor::Sequence,
            "sequence".to_owned(),
            format!("engine subject `{raw}`: {message}"),
        ),
        (None, None) => (Anchor::Sequence, "sequence".to_owned(), message),
    }
}

/// A block-level engine message that names one of the block's assignments in backticks (the
/// first such) is about that assignment, as stage 1 reports it (`hys.uLow`, not `hys`).
fn refine(sequence: &Sequence, index: usize, subject: String, message: &str) -> (Anchor, String) {
    let Some(block) = sequence.blocks.get(index) else {
        return (Anchor::Block(index, None), subject);
    };
    if subject != block.name.as_str() {
        return (Anchor::Block(index, None), subject);
    }
    let named = message.split('`').skip(1).step_by(2).find_map(|token| {
        block
            .assignments
            .iter()
            .position(|a| a.name.as_str() == token)
    });
    match named {
        Some(j) => (
            Anchor::Block(index, Some(j)),
            format!("{}.{}", block.name, block.assignments[j].name),
        ),
        None => (Anchor::Block(index, None), subject),
    }
}

/// The first connection naming `endpoint`, as its target first, then as its source.
fn connection_naming(sequence: &Sequence, endpoint: &Endpoint) -> Option<usize> {
    let driving = sequence
        .connections
        .iter()
        .position(|c| &c.target == endpoint);
    driving.or_else(|| {
        sequence
            .connections
            .iter()
            .position(|c| &c.source == endpoint)
    })
}

/// The anchor and subject of a model path (the IRI with the root prefix stripped).
fn locate(sequence: &Sequence, path: &str) -> (Anchor, String) {
    let sequence_subject = (Anchor::Sequence, "sequence".to_owned());
    if path.is_empty() {
        return sequence_subject;
    }
    let Some((head, member)) = path.split_once('.') else {
        let named = |names: Vec<&str>| names.iter().position(|n| *n == path);
        let anchor = named(sequence.params.iter().map(|p| p.name.as_str()).collect())
            .map(Anchor::Param)
            .or_else(|| {
                named(sequence.inputs.iter().map(|b| b.name.as_str()).collect()).map(Anchor::Input)
            })
            .or_else(|| {
                named(sequence.outputs.iter().map(|b| b.name.as_str()).collect())
                    .map(Anchor::Output)
            })
            .or_else(|| {
                named(sequence.blocks.iter().map(|b| b.name.as_str()).collect())
                    .map(|i| Anchor::Block(i, None))
            });
        if let Some(anchor) = anchor {
            return (anchor, path.to_owned());
        }
        // An undeclared name a connection uses (only the stage-1 bypass emits one).
        return connection_naming(sequence, &Endpoint::Boundary(Identifier::unchecked(path)))
            .map_or(sequence_subject, |i| {
                (Anchor::Connection(i), format!("connection[{i}]"))
            });
    };
    let Some(index) = sequence.blocks.iter().position(|b| b.name.as_str() == head) else {
        let port = Endpoint::Port {
            block: Identifier::unchecked(head),
            port: Identifier::unchecked(member),
        };
        return connection_naming(sequence, &port).map_or(sequence_subject, |i| {
            (Anchor::Connection(i), format!("connection[{i}]"))
        });
    };
    let block = &sequence.blocks[index];
    if let Some(j) = assignment_of(block, member) {
        return (Anchor::Block(index, Some(j)), path.to_owned());
    }
    let port = Endpoint::Port {
        block: block.name.clone(),
        port: Identifier::unchecked(member),
    };
    match connection_naming(sequence, &port) {
        Some(i) => (Anchor::Connection(i), format!("connection[{i}]")),
        None => (Anchor::Block(index, None), path.to_owned()),
    }
}

/// The assignment a `block.member` subject names: the assignment of that name, or — for an
/// array node, which the emitter names by the template's base (`k`) — the first authored element
/// of that array.
fn assignment_of(block: &crate::model::Block, member: &str) -> Option<usize> {
    if let Some(j) = block
        .assignments
        .iter()
        .position(|a| a.name.as_str() == member)
    {
        return Some(j);
    }
    let class = lookup_class(&block.class_path)?;
    block
        .assignments
        .iter()
        .position(|a| element_base(class.metadata, a.name.as_str()) == Some(member))
}

#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;
