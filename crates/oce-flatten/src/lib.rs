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
//! Status: **M1 (scalar).** As-built, the `oce-cxf` resolver already grounds parameters inline (it
//! owns the per-instance parameter scope), so this shim's M1 role narrows to **array normalization**
//! — which lands in M1-PR-9. For a **scalar** model (the M1-PR-6 path) there is nothing left to
//! elaborate, so [`flatten`] is an identity passthrough. The seam stays in the pipeline
//! (`import_cxf → flatten → build`) so PR-9 can add array normalization here without touching
//! `Engine::load_cxf`.

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
/// **M1 scalar path (identity).** The `oce-cxf` resolver already emits a flat, ground, scalar
/// `ModelGraph` — composites are flattened away and parameters are ground there. The remaining
/// elaboration is **array normalization** (both CXF array encodings → canonical per-element-scalar
/// connectors/params), which lands in M1-PR-9 and will transform the model here. Until then a
/// scalar model passes through unchanged.
///
/// # Errors
/// Returns [`FlattenError`] if a binding cannot be resolved (no scalar model triggers this in M1;
/// array-normalization failures land with PR-9).
pub fn flatten(model: ModelGraph) -> Result<ModelGraph, FlattenError> {
    Ok(model)
}
