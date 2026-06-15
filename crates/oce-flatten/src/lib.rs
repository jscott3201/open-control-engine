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
//! Status: **M0 scaffold.** The shim lands in M1.

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
/// consumes (array normalization + ground-parameter binding). At M0 this is a no-op passthrough
/// of an already-resolved model.
///
/// # Errors
/// Returns [`FlattenError`] if a binding cannot be resolved.
pub fn flatten(_model: ModelGraph) -> Result<ModelGraph, FlattenError> {
    unimplemented!("oce-flatten::flatten — M0 scaffold (CXF-resolution shim lands in M1)")
}
