//! The emitter: one deterministic path from the authoring model to flat CXF (authoring note,
//! "Emitter"; decisions 2, 3, 6, 10, 15, 16, 17 and 21).
//!
//! [`emit`] runs stage 1 ([`crate::validate_author_side`]) first and refuses with
//! [`EmitError::Invalid`] on any error finding, so it never writes a document it knows to be
//! wrong. It then writes one flat, single-root document in the fixed layout of
//! [`writer`](self): authored order everywhere, never sorted; instance IRIs
//! `{namespace}{sequence}.{local}`; block classes under the fixed class base; only the
//! assignments the author wrote; boundary `unit`, `quantity` and `displayUnit` and no semantic
//! record. The bytes are checked against [`oce_api::MAX_CXF_BYTES`] before they are returned.
//! Emitting one model twice yields identical bytes; text-form annotations and suffixes never
//! reach the output.

mod iri;
mod params;
mod writer;

use std::fmt;

use oce_api::{ClassPorts, MAX_CXF_BYTES};

use crate::model::Sequence;
use crate::validate::{Finding, Severity, author_side_with_ports};

pub(crate) use iri::Scheme;
use iri::namespace_problem;
pub use iri::{CLASS_BASE, DEFAULT_NAMESPACE, MAX_NAMESPACE_BYTES};

/// How to emit: today, the instance namespace.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmitOptions {
    /// The instance namespace every IRI starts with; [`DEFAULT_NAMESPACE`] unless overridden.
    /// It must end in `#`, `/` or `:` ([`emit`] refuses it otherwise).
    pub namespace: String,
}

impl Default for EmitOptions {
    fn default() -> Self {
        Self {
            namespace: DEFAULT_NAMESPACE.to_owned(),
        }
    }
}

impl EmitOptions {
    /// The default options: [`DEFAULT_NAMESPACE`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// These options with the instance namespace replaced. Never derive a namespace from a host
    /// path or a UUID-shaped value (decision 6); [`emit`] checks the spelling.
    #[must_use]
    pub fn with_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = namespace.into();
        self
    }
}

/// Why [`emit`] wrote nothing. Never a panic.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EmitError {
    /// Stage 1 found at least one error; every stage-1 finding is carried, in validator order.
    Invalid(Vec<Finding>),
    /// The namespace in [`EmitOptions`] is malformed.
    InvalidNamespace {
        /// The namespace as given.
        namespace: String,
        /// What is wrong with it.
        problem: &'static str,
    },
    /// The document is larger than [`oce_api::MAX_CXF_BYTES`], which every engine refuses.
    TooLarge {
        /// The document size in bytes.
        bytes: usize,
        /// The limit.
        limit: usize,
    },
}

impl fmt::Display for EmitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(findings) => {
                write!(f, "the sequence has {} error finding(s)", findings.len())?;
                if let Some(first) = findings.first() {
                    write!(f, "; first: {first}")?;
                }
                Ok(())
            }
            Self::InvalidNamespace { namespace, problem } => {
                write!(f, "namespace `{namespace}` {problem}")
            }
            Self::TooLarge { bytes, limit } => {
                write!(
                    f,
                    "the document is {bytes} bytes; an engine loads at most {limit}"
                )
            }
        }
    }
}

impl std::error::Error for EmitError {}

/// Emit `sequence` as flat CXF bytes after stage-1 validation.
///
/// # Errors
///
/// [`EmitError::Invalid`] when stage 1 finds an error, [`EmitError::InvalidNamespace`] for a
/// malformed namespace and [`EmitError::TooLarge`] for a document above [`oce_api::MAX_CXF_BYTES`]. Never panics.
pub fn emit(sequence: &Sequence, options: &EmitOptions) -> Result<Vec<u8>, EmitError> {
    check_namespace(options)?;
    let (findings, ports) = author_side_with_ports(sequence);
    if findings.iter().any(|f| f.severity == Severity::Error) {
        return Err(EmitError::Invalid(findings));
    }
    write(sequence, options, &ports)
}

fn check_namespace(options: &EmitOptions) -> Result<(), EmitError> {
    match namespace_problem(&options.namespace) {
        Some(problem) => Err(EmitError::InvalidNamespace {
            namespace: options.namespace.clone(),
            problem,
        }),
        None => Ok(()),
    }
}

/// Write the document from already-resolved ports and check its size.
pub(crate) fn write(
    sequence: &Sequence,
    options: &EmitOptions,
    ports: &[Option<ClassPorts>],
) -> Result<Vec<u8>, EmitError> {
    let scheme = Scheme::new(&options.namespace, sequence.name.as_str());
    let bytes = writer::document(sequence, &scheme, ports)?.into_bytes();
    check_size(bytes.len())?;
    Ok(bytes)
}

/// Refuse a document above [`MAX_CXF_BYTES`].
pub(crate) fn check_size(bytes: usize) -> Result<(), EmitError> {
    if bytes > MAX_CXF_BYTES {
        return Err(EmitError::TooLarge {
            bytes,
            limit: MAX_CXF_BYTES,
        });
    }
    Ok(())
}

/// Emit with stage 1 bypassed — the parity test's path (authoring note, "Finding codes"). Ports
/// come from stage 1 where it resolved them, else from the class defaults, else from the
/// catalog's named default ports, so a defect stage 1 would refuse reaches the engine with the
/// block's interface intact and the engine judges the defect itself. Never part of the public API.
#[cfg(test)]
pub(crate) fn emit_unchecked(
    sequence: &Sequence,
    options: &EmitOptions,
) -> Result<Vec<u8>, EmitError> {
    check_namespace(options)?;
    let (_, mut ports) = author_side_with_ports(sequence);
    for (block, slot) in sequence.blocks.iter().zip(ports.iter_mut()) {
        if slot.is_none() {
            *slot = oce_api::class_ports(&block.class_path, &[])
                .ok()
                .or_else(|| named_default_ports(&block.class_path));
        }
    }
    write(sequence, options, &ports)
}

/// The catalog's default ports of a class whose ports are all named.
#[cfg(test)]
fn named_default_ports(class_path: &str) -> Option<ClassPorts> {
    let entry = oce_api::catalog()
        .iter()
        .find(|e| e.class_path == class_path)?;
    let side = |ports: &[oce_api::CatalogPort]| -> Option<Vec<oce_api::ClassPort>> {
        ports
            .iter()
            .map(|p| {
                p.name.map(|name| oce_api::ClassPort {
                    name: name.to_owned(),
                    kind: p.kind,
                })
            })
            .collect()
    };
    Some(ClassPorts {
        class_path: entry.class_path,
        inputs: side(&entry.inputs)?,
        outputs: side(&entry.outputs)?,
    })
}
