#![forbid(unsafe_code)]
//! `oce-cxf` — CXF (Control eXchange Format, a JSON-LD representation of CDL) import/export for
//! the Open Control Engine.
//!
//! CXF is the v1 ingest format (FRAME D2): it arrives already flattened/monomorphic and maps
//! ~1:1 to the `oce-model` graph. The importer uses lossless "Layer A" serde DTOs (untagged
//! value sums, flatten-passthrough) plus a Layer A→B resolver that indexes `@graph` by `@id` and
//! joins instances to their block class by class IRI. This crate is **Group A** (no store, no
//! database); it depends on `serde`/`serde_json`/`oce-diag` only.
//!
//! Status: **M1.** The lossless Layer-A DTO ([`dto`]) and [`parse_document`] land in M1-PR-4; the
//! §7.1 resolver (`import_cxf`) and exporter land in M1-PR-5.

use oce_model::ModelGraph;

pub mod dto;
#[cfg(test)]
mod tests;

pub use dto::{Context, CxfDocument, CxfValue, IriRef, Node, OneOrMany};

/// A CXF import/export error (typed; never a panic). Variants follow doc 04 §5; the resolver
/// adds `Resolve`/`Expr` in M1-PR-5 (kept `#[non_exhaustive]` so that is not a breaking change).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CxfError {
    /// The input was not valid JSON / JSON-LD.
    #[error("CXF JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    /// Validation produced one or more diagnostics, at least one of them an error (the shared
    /// [`oce_diag`] vocabulary, so a host sees one diagnostic shape across resolve + validate).
    #[error("CXF validation failed with {} diagnostic(s)", .0.len())]
    Validation(Vec<oce_diag::Diagnostic>),
}

/// Deserialize a CXF JSON-LD document into the lossless [`CxfDocument`] Layer-A DTO (doc 04 §6).
///
/// This is the low-level entry point for round-trip / inspection / editing. `parse_document`
/// then serialize reproduces the input losslessly, modulo JSON key ordering / whitespace (RT-1).
/// It assigns **no** semantics — interpretation happens in the resolver (M1-PR-5).
///
/// # Errors
/// Returns [`CxfError::Json`] if `bytes` is not valid JSON in the CXF document shape.
pub fn parse_document(bytes: &[u8]) -> Result<CxfDocument, CxfError> {
    Ok(serde_json::from_slice(bytes)?)
}

/// Serialize a [`CxfDocument`] Layer-A DTO back to JSON-LD bytes (the RT-1 round-trip partner of
/// [`parse_document`]).
///
/// # Errors
/// Returns [`CxfError::Json`] if serialization fails.
pub fn write_document(doc: &CxfDocument) -> Result<Vec<u8>, CxfError> {
    Ok(serde_json::to_vec(doc)?)
}

/// Import a CXF JSON-LD document into the in-memory [`ModelGraph`] (Layer A DTOs → §7.1 Layer-B
/// resolver).
///
/// # Errors
/// Returns [`CxfError`] on malformed JSON or invalid CXF structure.
pub fn import(_bytes: &[u8]) -> Result<ModelGraph, CxfError> {
    unimplemented!("oce-cxf::import — Layer A/B resolver lands in M1-PR-5 (renamed import_cxf)")
}

/// Export a [`ModelGraph`] back to a CXF JSON-LD document.
///
/// # Errors
/// Returns [`CxfError`] if serialization fails.
pub fn export(_model: &ModelGraph) -> Result<Vec<u8>, CxfError> {
    unimplemented!("oce-cxf::export — M1-PR-5")
}
