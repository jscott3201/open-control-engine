#![forbid(unsafe_code)]
//! `oce-flatten` — CDL elaboration for the Open Control Engine.
//!
//! Full Modelica elaboration (parameter propagation, expression folding, conditional-instance
//! removal, `replaceable`/`redeclare`/`extends`) is the largest correctness risk and is deferred
//! to the post-v1 "later" band (FRAME D2). In v1 this crate is a thin **CXF-resolution shim**:
//! CXF arrives already flattened/monomorphic, so the work is array normalization
//! (`A[1]`→`A_1`, 1-based CDL → 0-based internal) and ground-parameter binding via `oce-expr`.
//! It is **Group A** (no store, no database).
//!
//! Status: **M1 (identity passthrough).** As-built, the `oce-cxf` resolver owns *all* M1
//! lowering: it grounds parameters inline (it holds the per-instance parameter scope) **and**, as of
//! M1-PR-9, it discharges **array normalization** resolver-side — a preserved array parameter
//! (`isArray`/`sizeOfDimensions`, e.g. `k[2]`) is expanded to per-element scalar `ParamTable`
//! entries (`k_1`, `k_2`) right where grounding happens (doc 04 §3.6.1). The metadata that drives
//! that expansion (`isArray`/`numberDimensions`/`sizeOfDimensions`, the decorated `label`, the
//! per-instance ground scope) only exists at the Layer-A `Node`, which [`flatten`] — operating on
//! the already-lowered [`ModelGraph`] — never sees; so the responsibility AD-1 nominally parked here
//! is correctly performed in the resolver. [`flatten`] is therefore an **identity passthrough** in
//! M1. The seam stays in the pipeline (`import_cxf → flatten → build`) for future D2 elaboration.
//!
//! **M2 renumber obligation (when array *connectors* land).** M1 array normalization is
//! **parameter-only**: it mints no connectors, so `ConnectorId`s, `ModelGraph::external_inputs`, and
//! every `Connector.iri` are untouched. When M2 introduces array *connectors* (per-element signal
//! expansion), any stage that renumbers connectors **must** remap the full dense numbering in
//! lockstep — `connectors[].id`, every `Connection { from, to }`, `external_inputs`, and each
//! `BlockInstance.inputs`/`outputs` (AD-2). Whichever stage owns that expansion owns the remap.

use oce_model::ModelGraph;

/// A flattening error (typed; never a panic).
#[derive(Clone, Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FlattenError {
    /// A binding expression failed to resolve to a ground value.
    #[error("binding resolution failed: {0}")]
    Binding(String),
}

/// Resolve a CXF-derived model into the flattened, monomorphic [`ModelGraph`] the scheduler
/// consumes.
///
/// **M1 identity passthrough.** The `oce-cxf` resolver already emits a fully-lowered `ModelGraph`:
/// composites are flattened away, parameters are ground, and (M1-PR-9) array parameters are
/// normalized to per-element scalar `ParamTable` entries — all resolver-side, because only the
/// Layer-A `Node` carries the array metadata. Nothing array- or scalar-related remains to elaborate
/// on the lowered graph in M1, so this returns the model unchanged. The seam is retained for the
/// post-v1 D2 Modelica-elaboration band.
///
/// # Errors
/// Returns [`FlattenError`] if a binding cannot be resolved. No M1 path triggers this (array
/// normalization, with its typed failures, is the resolver's; see `oce-cxf`).
pub fn flatten(model: ModelGraph) -> Result<ModelGraph, FlattenError> {
    Ok(model)
}
