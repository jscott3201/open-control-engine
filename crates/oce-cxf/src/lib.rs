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
//! ground, single-root, scalar-parameter subset — exactly what the resolver produces for that
//! shape of document — plus the in-subset §7.4.1 connector attributes. Content outside that
//! subset is a typed [`CxfError::Validation`] carrying
//! [`oce_diag::DiagCode::ExportUnsupported`], except on a **deferred** block — one carrying an
//! enumeration, or any downstream consumer the cascade reached from it — where the block is
//! omitted from the document with a non-aborting warning and its out-of-subset content is never
//! rejected; never a panic either way. See [`export()`] for the full contract.

use oce_model::ModelGraph;

mod arrays;
mod bridge;
pub mod dto;
mod export;
mod export_defer;
mod export_pass_through;
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

/// A CXF import/export error. Ingest failures in bounded parser and composite-lowering paths are
/// typed; composite boundary resolution still recurses per `isConnectedTo` hop without a depth
/// bound. Variants follow doc 04 §5; kept `#[non_exhaustive]` so resolver/exporter error variants
/// can be added without breaking callers.
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
/// Accepts the flat, ground, single-root, scalar-parameter subset (the shape the resolver
/// produces for documents like the `minimal_loop` fixture) and emits the in-subset §7.4.1
/// connector attributes (`unit`/`quantity`/`displayUnit`/`min`/`max`) on each port node under
/// the Bare-Scalar Canonical wire shape. The emitted bytes are deterministic — repeated calls are
/// byte-identical.
///
/// **Round-trip (RT-2).** For an accepted graph whose class paths name registered block classes
/// (everything the resolver itself produces), the emitted bytes re-import to a `ModelGraph` that
/// renders bit-identically (Reals by IEEE-754 bits) to the **survivor cone** of the input: the
/// blocks that were not deferred, the connections whose endpoints are both survivors, and the
/// `external_inputs` entries whose connector belongs to a survivor. When nothing is deferred the
/// survivor cone *is* the whole graph, so re-import renders bit-identically to the input itself.
/// When deferral fires, equality holds over the cone and not over the input — the omitted blocks
/// are gone from the document by design, and no re-import can restore them. Acceptance is what
/// makes the promise good on the single-assignment axis: an input driven by more than one
/// surviving output is **rejected** (§7.10) rather than emitted, because those bytes would fail
/// re-import outright rather than round-trip imperfectly. Block ids are
/// renumbered densely on re-import, so the comparison is by `instance_iri`, never by raw
/// `BlockId`. `export_with_report` is the only way to tell the two cases apart: an empty
/// `warnings` list means nothing was deferred and the round trip covers the whole input.
///
/// Reserved `urn:oce:lowering#PassThrough.*` blocks are emitted as bare boundary edges rather
/// than `containsBlock` nodes; boundary-output nodes are re-emitted and import synthesizes the
/// lowering blocks again. Thus RT-2 holds by render identity with zero warnings even though a
/// pass-through document's `containsBlock` count is less than `ModelGraph::blocks.len()`. An empty
/// warning list does not mean the emitted document explicitly lists every internal lowering block.
///
/// One carve-out, and it is a real gap rather than a definition: export does **not** check a
/// block's port ARITY against the class its `class_path` names, because it takes no registry
/// dependency at all. A hand-built block naming a registered class while declaring fewer ports
/// than that class requires exports `Ok`, and the bytes then fail re-import with
/// `MalformedDocument` — the same "bytes that do not load" class the single-assignment rejection
/// above exists to prevent, reached by a different route. Every graph the resolver produces has
/// correct arity by construction, so this is reachable only from a hand-built graph.
///
/// Export deliberately takes no registry
/// dependency, so a hand-built graph with an unregistered class path still exports; its bytes
/// then fail re-import loudly with `ClassNotFound` — never silently. The source root `@id` is
/// not recorded in [`ModelGraph`], so the root composite is emitted under the fixed synthetic
/// IRI `urn:open-control:cxf-export:root`; block nodes reuse their `instance_iri` verbatim, and
/// port nodes get deterministically minted `@id`s (re-import rebuilds wiring from
/// `isConnectedTo`, so port names never round-trip). Parameter bindings are bare JSON literals;
/// Reals always carry a fractional part or exponent, so a whole-number Real never re-grounds as
/// an Integer.
///
/// Enum-carrying blocks (any `ValueType::Enum` connector or `Value::Enum` parameter) are
/// **deferred**, not rejected: the block and its transitive downstream consumers are omitted from
/// the emitted document so the enum-free remainder can still export, and the omission is reported
/// as an [`oce_diag::DiagCode::ExportDeferred`] **warning** (non-aborting). This function
/// **discards deferral warnings**, so a caller using it alone cannot tell a complete export from
/// one that dropped blocks; use [`export_with_report`] for that. Deferral is
/// non-aborting only while something survives it: a graph whose every block is deferred has no
/// runtime composite left to emit and is an error, not an empty document.
///
/// # Errors
/// - [`CxfError::Validation`] with [`oce_diag::DiagCode::ExportUnsupported`] error diagnostics
///   (subject = the owning block's `instance_iri`; connectors carry no IRI of their own) for
///   anything outside the subset that is NOT an enum deferral: non-finite parameters, connectors
///   carrying `nominal`/`unbounded` or non-finite `min`/`max` bounds (outside the canonical
///   subset), String-typed connectors, blocks without an `instance_iri`, external inputs
///   without a recorded boundary IRI, the same connector listed more than once in
///   `external_inputs` (re-import deduplicates the repeat, so it cannot round-trip), an input
///   connector driven by more than one **surviving** output (§7.10 single assignment; those bytes
///   fail re-import entirely rather than come back lossy),
///   structurally inconsistent wiring, or an empty (zero-block)
///   graph. The per-node checks in that list — a String connector, an out-of-subset connector
///   attribute, a missing `instance_iri`, a class path that fails the bridge round-trip, a
///   parameter defect, an external input with no boundary IRI or listed twice, a multiply-driven
///   input — reject only where
///   the offender sits on a **surviving** block; a deferred block is omitted from the document and
///   so contributes no error diagnostic of its own. The whole-graph guards (an empty graph,
///   non-dense ids) and a connection that is not output→input reject either way, being
///   attributable to no single block's presence in the document. Deferred blocks — enum-carrying
///   ones and the cascade they reach alike — are a warning, not a rejection, and do NOT trigger
///   this variant, unless deferral is *total*, which leaves no block to emit and rejects. Never
///   panics.
/// - [`CxfError::Json`] if document serialization itself fails.
pub fn export(model: &ModelGraph) -> Result<Vec<u8>, CxfError> {
    let (doc, _warnings) = export::document(model)?;
    write_document(&doc)
}

/// The result of [`export_with_report`]: the emitted CXF bytes plus the deferral warnings
/// (empty for a fully-in-subset graph). `#[non_exhaustive]` so future report fields (e.g. a
/// deferred-block census) can land without breaking callers.
///
/// # Examples
/// ```
/// use oce_cxf::{ResolveOptions, export_with_report, import_cxf};
///
/// let (graph, _report) =
///     import_cxf(include_bytes!("../tests/fixtures/minimal_loop.jsonld"),
///                &ResolveOptions::default()).unwrap();
/// let report = export_with_report(&graph).unwrap();
/// assert!(!report.bytes.is_empty());
/// // minimal_loop carries no enum, so no deferral warnings are emitted.
/// assert!(report.warnings.is_empty());
/// ```
#[derive(Debug)]
#[non_exhaustive]
pub struct ExportReport {
    /// The emitted CXF JSON-LD document bytes (deterministic; a fixpoint of `import ∘ export`
    /// for the enum-free survivor cone).
    pub bytes: Vec<u8>,
    /// The [`oce_diag::DiagCode::ExportDeferred`] warnings naming every deferred block (empty
    /// when the whole graph was inside the export subset). Always `Warning` severity — never an
    /// error, so a non-`Err` result here does not imply a clean graph.
    pub warnings: Vec<oce_diag::Diagnostic>,
}

/// Export a [`ModelGraph`] and surface the deferral warnings — the non-discarding partner of
/// [`export()`]. The `bytes` are identical to what [`export()`] returns for the same graph; the
/// `warnings` carry every [`oce_diag::DiagCode::ExportDeferred`] diagnostic naming a deferred
/// block (an enum-carrying block plus its transitive downstream consumers). Use this when a host
/// needs to know which blocks were omitted; use [`export()`] when it does not.
///
/// # Errors
/// - [`CxfError::Validation`] with [`oce_diag::DiagCode::ExportUnsupported`] error diagnostics
///   for any error-severity subset failure (same surface as [`export()`]); enum deferrals are
///   warnings and do NOT trigger this variant.
/// - [`CxfError::Json`] if document serialization itself fails.
pub fn export_with_report(model: &ModelGraph) -> Result<ExportReport, CxfError> {
    let (doc, warnings) = export::document(model)?;
    let bytes = write_document(&doc)?;
    Ok(ExportReport { bytes, warnings })
}
