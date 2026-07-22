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
//! The lossless Layer-A DTO ([`dto`]), [`parse_document`], the §7.1 resolver ([`import_cxf`]),
//! and the minimal RT-2 exporter ([`export()`]) are implemented. The exporter covers the flat,
//! ground, single-root, scalar-parameter, attribute-free subset — exactly what the resolver
//! produces for that shape of document; anything outside it is a typed [`CxfError::Validation`]
//! carrying [`oce_diag::DiagCode::ExportUnsupported`] — never a panic.

use oce_model::ModelGraph;

mod arrays;
mod bridge;
pub mod dto;
mod export;
#[cfg(test)]
mod export_tests;
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
mod g36_catalog_package_guard;
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

/// Export a [`ModelGraph`] back to a CXF JSON-LD document — the RT-2 partner of [`import_cxf`].
///
/// Accepts the flat, ground, single-root, scalar-parameter, attribute-free subset (the shape the
/// resolver produces for documents like the `minimal_loop` fixture). The emitted bytes are
/// deterministic — repeated calls are byte-identical — and re-import to a `ModelGraph`
/// bit-identical to the input (Reals by IEEE-754 bits). The source root `@id` is not recorded in
/// [`ModelGraph`], so the root composite is emitted under the fixed synthetic IRI
/// `urn:open-control:cxf-export:root`; block nodes reuse their `instance_iri` verbatim, and port
/// nodes get deterministically minted `@id`s (re-import rebuilds wiring from `isConnectedTo`, so
/// port names never round-trip). Parameter bindings are bare JSON literals; Reals always carry a
/// fractional part or exponent, so a whole-number Real never re-grounds as an Integer.
///
/// # Errors
/// - [`CxfError::Validation`] with [`oce_diag::DiagCode::ExportUnsupported`] error diagnostics
///   (subject = the owning block's `instance_iri`; connectors carry no IRI of their own) for
///   anything outside the subset: enumeration-valued or non-finite parameters, connectors with
///   declared §7.4.1 attributes, String/Enum-typed connectors, blocks without an `instance_iri`,
///   external inputs without a recorded boundary IRI, structurally inconsistent wiring, or an
///   empty (zero-block) graph. Never panics.
/// - [`CxfError::Json`] if document serialization itself fails.
pub fn export(model: &ModelGraph) -> Result<Vec<u8>, CxfError> {
    let doc = export::document(model)?;
    write_document(&doc)
}
