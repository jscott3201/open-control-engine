#![forbid(unsafe_code)]
//! `oce-flatten` — CDL elaboration for the Open Control Engine.
//!
//! Full Modelica elaboration (parameter propagation, expression folding, conditional-instance
//! removal, `replaceable`/`redeclare`/`extends`) is the largest correctness risk and is deferred
//! to a later elaboration layer (FRAME D2). Today this crate is a thin **CXF-resolution shim**:
//! CXF arrives already flattened/monomorphic, so the responsibility is to preserve the seam after
//! resolver-owned array normalization (`A[1]`→`A_1`, 1-based CDL → 0-based internal) and
//! ground-parameter binding.
//! It is **Group A** (no store, no database).
//!
//! As-built, the `oce-cxf` resolver owns lowering: it grounds parameters inline (it holds the
//! per-instance parameter scope) and discharges **array-parameter normalization** resolver-side — a
//! preserved array parameter (`isArray`/`sizeOfDimensions`, e.g. `k[2]`) is expanded to per-element
//! scalar `ParamTable` entries (`k_1`, `k_2`) right where grounding happens (doc 04 §3.6.1). The
//! metadata that drives that expansion (`isArray`/`numberDimensions`/`sizeOfDimensions`, the
//! decorated `label`, the per-instance ground scope) only exists at the Layer-A `Node`, which
//! [`flatten`] — operating on the already-lowered [`ModelGraph`] — never sees. [`flatten`] is
//! therefore an **identity passthrough**. The seam stays in the pipeline
//! (`import_cxf → flatten → build`) for future D2 elaboration.
//!
//! **Array-connector renumber obligation.** Current array normalization is **parameter-only**: it
//! mints no connectors, so `ConnectorId`s, `ModelGraph::external_inputs`, and every `Connector.iri`
//! are untouched. When array *connectors* (per-element signal expansion) are introduced, any stage
//! that renumbers connectors **must** remap the full dense numbering in lockstep —
//! `connectors[].id`, every `Connection { from, to }`, `external_inputs`, and each
//! `BlockInstance.inputs`/`outputs`. Whichever stage owns that expansion owns the remap.

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
/// **Identity passthrough.** The `oce-cxf` resolver already emits a fully-lowered `ModelGraph`:
/// composites are flattened away, parameters are ground, and array parameters are normalized to
/// per-element scalar `ParamTable` entries — all resolver-side, because only the Layer-A `Node`
/// carries the array metadata. Nothing array- or scalar-related remains to elaborate on the lowered
/// graph, so this returns the model unchanged. The seam is retained for the future D2
/// Modelica-elaboration band.
///
/// # Errors
/// Returns [`FlattenError`] if a binding cannot be resolved. Current CXF paths do not trigger this:
/// array normalization, with its typed failures, is owned by `oce-cxf`.
pub fn flatten(model: ModelGraph) -> Result<ModelGraph, FlattenError> {
    Ok(model)
}
