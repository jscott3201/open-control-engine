#![forbid(unsafe_code)]
//! `oce-cxf` — CXF (Control eXchange Format, a JSON-LD representation of CDL) import/export for
//! the Open Control Engine.
//!
//! CXF is the v1 ingest format (FRAME D2): it arrives already flattened/monomorphic and maps
//! ~1:1 to the `oce-model` graph. The importer uses lossless "Layer A" serde DTOs (untagged
//! value sums, flatten-passthrough) plus a resolver that lowers `@graph` **directly** to the flat
//! `oce_model::ModelGraph` (AD-1), joining instances to their block class by class IRI. This crate
//! is **Group A** (no store, no database); it depends on `serde`/`serde_json`/`oce-diag` plus
//! `oce-blocks` (the registry, for class resolution) and `oce-expr` (Ground-mode bindings).
//!
//! The lossless Layer-A DTO ([`dto`]), [`parse_document`], and §7.1 resolver ([`import_cxf`]) are
//! implemented. The exporter ([`export`]) is deferred and currently panics if called; it is never on
//! a load path.

use oce_model::ModelGraph;

mod arrays;
mod bridge;
pub mod dto;
#[cfg(test)]
mod g36_catalog_fixture_manifest;
#[cfg(test)]
mod g36_catalog_guard_data;
#[cfg(test)]
mod g36_catalog_guard_helpers;
#[cfg(test)]
mod g36_catalog_guard_support;
#[cfg(test)]
mod g36_catalog_literal_guard;
#[cfg(test)]
mod g36_catalog_tests;
mod ground;
mod resolve;
#[cfg(test)]
mod tests;

pub use dto::{Context, CxfDocument, CxfValue, IriRef, Node, OneOrMany, TermAttr};
pub use resolve::{ImportMode, ResolveOptions, ValidationReport};

/// A CXF import/export error (typed; never a panic). Variants follow doc 04 §5; kept
/// `#[non_exhaustive]` so resolver/exporter error variants can be added without breaking callers.
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
/// It assigns **no** semantics — interpretation happens in the resolver.
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

/// Import a CXF JSON-LD document into the flat [`ModelGraph`] (D1's executable truth), lowering the
/// lossless Layer-A DTO directly via the §7.1 resolver. Ground mode evaluates every parameter
/// binding to a ground literal.
///
/// Returns `Ok((graph, report))` where `report` carries the top-composite model `@id` plus
/// `Warning`/`Info` diagnostics only (zero errors). On any [`oce_diag::Severity::Error`]
/// diagnostic — or any `Warning` when
/// [`ResolveOptions::deny_warnings`] is set — returns [`CxfError::Validation`] instead, with the
/// graph withheld because it may be structurally unsound.
///
/// # Errors
/// - [`CxfError::Json`] if `bytes` is not valid JSON in the CXF document shape.
/// - [`CxfError::Validation`] if resolution produced at least one error diagnostic.
pub fn import_cxf(
    bytes: &[u8],
    opts: &ResolveOptions,
) -> Result<(ModelGraph, ValidationReport), CxfError> {
    let doc = parse_document(bytes)?;
    resolve::resolve(&doc, opts)
}

/// Export a [`ModelGraph`] back to a CXF JSON-LD document.
///
/// # Errors
/// Returns [`CxfError`] if serialization fails.
pub fn export(_model: &ModelGraph) -> Result<Vec<u8>, CxfError> {
    unimplemented!("oce-cxf::export — M1-PR-5")
}
