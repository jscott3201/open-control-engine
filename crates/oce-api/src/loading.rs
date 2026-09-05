//! The [`LoadReport`] returned by the supported CXF ingest path, [`crate::Engine::load_cxf`].
use crate::io::IoSummary;

/// The result of a successful load (`08` §3 R-PUB-5 / R-LOAD-1/2). Carries the `should`-level
/// diagnostics (the load succeeded; these are advisory), the model identity, the IO summary the host
/// needs immediately, and the model size signals. `#[non_exhaustive]`, `Clone` (a PyO3 binder owns
/// it), `Default` (the pre-load empty report).
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct LoadReport {
    /// Stable identity of the loaded model (re-exported `oce_store::DomainKey`; never a backend
    /// type, R-API-8). Until CXF model-level `@id` is carried through `ModelGraph`, this is a
    /// deterministic synthetic key derived from the durable model projection.
    pub model_id: oce_store::DomainKey,
    /// `should`-level diagnostics from ingest + validation (the shared `oce-diag` vocabulary, AD-4).
    pub warnings: Vec<oce_diag::Diagnostic>,
    /// §6 IO summary: counts by `IoClass` + point total (built at load from the model connectors).
    pub io: IoSummary,
    /// Number of (elementary) block instances in the loaded model.
    pub block_count: usize,
    /// Number of stateful `[S]` block instances — a state-footprint signal (`08` §3 / §5).
    pub stateful_blocks: usize,
}
