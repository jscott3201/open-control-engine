#![forbid(unsafe_code)]
//! `oce-semantics` — annotation parsing and effective-metadata resolution for the Open Control
//! Engine (`05-semantics-and-point-graph.md`).
//!
//! Parses the `__cdl(...)`/`__CDL(...)` vendor annotations (point-list, trend, connection,
//! propagate, …) and resolves them — once, at ingest — into the *effective* per-point metadata
//! the store projects (point type AI/AO/DI/DO/Mode, hardwired flag, trend interval with the
//! `interval=0` on-change sentinel, quantity/unit, description). Propagation resolves
//! higher-overrides-lower over dotted paths. This is **non-computational** data (CDL §7.17): it
//! is never read on the tick. The crate is **Group A** (no store, no selene-db).
//!
//! Status: **M0 scaffold.** The annotation resolver lands in M1/M3.

use oce_model::ModelGraph;

/// The point type inferred for a connector (`05` §5; `06` §2.1). `Mode` is an open-control
/// extension to the §7.7.5 Table 7.3 column (collapsed to AI/AO by direction in a conformant
/// export).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum PointType {
    /// Analog input (`RealInput`).
    Ai,
    /// Analog output (`RealOutput`).
    Ao,
    /// Digital input (`BooleanInput`).
    Di,
    /// Digital output (`BooleanOutput`).
    Do,
    /// Operating-mode signal (an Integer connector whose quantity/enum indicates a mode).
    Mode,
}

/// A semantics-resolution error (typed; never a panic).
#[derive(Clone, Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SemanticsError {
    /// A vendor annotation was malformed.
    #[error("annotation parse error: {0}")]
    Annotation(String),
}

/// Resolve a model's vendor annotations into effective per-point metadata (resolved once at
/// ingest; never resolved on read — Integration Brief §1.3). At M0 this is a no-op.
///
/// # Errors
/// Returns [`SemanticsError`] if an annotation is malformed.
pub fn resolve(_model: &ModelGraph) -> Result<(), SemanticsError> {
    unimplemented!("oce-semantics::resolve — M0 scaffold (annotation resolver lands in M1/M3)")
}
