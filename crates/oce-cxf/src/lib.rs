#![forbid(unsafe_code)]
//! `oce-cxf` — CXF (Control eXchange Format, a JSON-LD representation of CDL) import/export for
//! the Open Control Engine.
//!
//! CXF is the v1 ingest format (FRAME D2): it arrives already flattened/monomorphic and maps
//! ~1:1 to the `oce-model` graph. The importer uses lossless "Layer A" serde DTOs (untagged
//! value sums, flatten-passthrough) plus a Layer A→B resolver that indexes `@graph` by `@id` and
//! joins instances to their block class by class IRI. This crate is **Group A** (no store, no
//! database); it depends on `serde`/`serde_json` only.
//!
//! Status: **M0 scaffold.** The importer/exporter land in M1.

use oce_model::ModelGraph;

/// A CXF import/export error (typed; never a panic).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CxfError {
    /// The input was not valid JSON / JSON-LD.
    #[error("CXF JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    /// The JSON-LD document was structurally invalid for CXF.
    #[error("CXF structure error: {0}")]
    Structure(String),
}

/// Import a CXF JSON-LD document into the in-memory [`ModelGraph`] (Layer A DTOs → Layer B
/// resolver).
///
/// # Errors
/// Returns [`CxfError`] on malformed JSON or invalid CXF structure.
pub fn import(_bytes: &[u8]) -> Result<ModelGraph, CxfError> {
    unimplemented!("oce-cxf::import — M0 scaffold (Layer A/B resolver lands in M1)")
}

/// Export a [`ModelGraph`] back to a CXF JSON-LD document.
///
/// # Errors
/// Returns [`CxfError`] if serialization fails.
pub fn export(_model: &ModelGraph) -> Result<Vec<u8>, CxfError> {
    unimplemented!("oce-cxf::export — M0 scaffold")
}
