//! The results of the supported CXF ingest path, [`crate::Engine::load_cxf`]: the
//! [`LoadReport`] summary and, beside it, the plain-data [`LoadedModel`].
//!
//! # Dependency direction
//!
//! [`LoadedModel`] is what a host hands its own persistence layer when it wants durability. The
//! engine never persists it: `oce-api` depends on no storage crate, and the storage port
//! (`oce-store`) depends on no facade crate. The resolved-model DTOs both sides exchange are
//! defined once in the core model crate (`oce_model::resolved_model`) and re-exported here, so a
//! host passes `loaded.model` straight to a storage adapter without a conversion and without
//! either crate naming the other.
use oce_model::resolved_model::{DomainKey, PointValueType, ResolvedModel};

use crate::io::{IoInventory, IoSummary};

/// The result of a successful load (`08` §3 R-PUB-5 / R-LOAD-1/2). Carries the `should`-level
/// diagnostics (the load succeeded; these are advisory), the model identity, the IO summary the host
/// needs immediately, and the model size signals. `#[non_exhaustive]`, `Clone` (a PyO3 binder owns
/// it), `Default` (the pre-load empty report).
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct LoadReport {
    /// Stable identity of the loaded model (the plain-data [`DomainKey`]; never a backend type,
    /// R-API-8). The CXF top-composite `@id` when the document carries one; otherwise a
    /// deterministic synthetic key derived from the resolved-model projection.
    pub model_id: DomainKey,
    /// `should`-level diagnostics from ingest + validation (the shared `oce-diag` vocabulary, AD-4).
    pub warnings: Vec<oce_diag::Diagnostic>,
    /// §6 IO summary: counts by `IoClass` + point total (built at load from the model connectors).
    pub io: IoSummary,
    /// Number of (elementary) block instances in the loaded model.
    pub block_count: usize,
    /// Number of stateful `[S]` block instances — a state-footprint signal (`08` §3 / §5).
    pub stateful_blocks: usize,
}

/// The plain-data result of a successful load, returned beside [`LoadReport`].
///
/// Everything here is owned data: no `Arc<dyn …>`, no borrow of the engine, no adapter handle.
/// The engine keeps its own copy of what it needs to execute, so a host may move, persist, send or
/// drop this value freely; doing so never affects the loaded engine. `model_id` and `model` derive
/// `serde::{Serialize, Deserialize}` (through `oce_model::resolved_model`), so a host persists the
/// projection with any serde codec; `io` and the diagnostics are the facade's existing `Clone`
/// value types. The tick never reads any of it.
///
/// `#[non_exhaustive]`: fields may be added; construct it only through a load entry point.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct LoadedModel {
    /// Stable identity of the loaded model; equal to [`LoadReport::model_id`] and to
    /// `model.model_id`.
    pub model_id: DomainKey,
    /// The resolved-model projection of the flat executable graph (classes, block instances with
    /// ground parameters, points with effective semantic metadata, connections), as plain data.
    pub model: ResolvedModel,
    /// The typed IO inventory built at load; the same rows [`crate::Engine::io`] serves.
    pub io: IoInventory,
    /// `should`-level diagnostics from the load-time semantic resolver alone.
    pub semantic_warnings: Vec<oce_diag::Diagnostic>,
    /// Every load diagnostic in report order; equal to [`LoadReport::warnings`].
    pub diagnostics: Vec<oce_diag::Diagnostic>,
}

impl LoadedModel {
    /// Every input point as `(canonical path, value type)`, in inventory order: the plain-data
    /// view a host's persistence or point-binding layer validates against. Borrows only;
    /// allocates nothing; never panics.
    pub fn input_points(&self) -> impl Iterator<Item = (&str, PointValueType)> + '_ {
        self.io.input_points()
    }
}
