#![forbid(unsafe_code)]
//! `oce-api` — the embeddable host facade for the Open Control Engine
//! (`08-embeddable-api-and-performance.md`). The single public surface downstream projects link
//! against (published under the umbrella name `open-control-engine`).
//!
//! # Posture (binding, FRAME §6)
//!
//! Library-only, synchronous, in-process, `#![forbid(unsafe_code)]`, edition 2024, rust 1.95.0,
//! **no async runtime, no server**. The host owns transport, TLS, authN/Z, multi-tenancy,
//! off-host durability, and metrics export.
//!
//! # The store seam
//!
//! [`Engine`] is generic over a `Store`, defaulting to `oce_store_mem::MemStore` so the **default
//! build has no database and no selene-db** (D-OWNER-1). The `selene` cargo feature changes
//! *construction only* — it wires the optional `oce-store-selene` adapter (an empty stub at M0;
//! selene-db arrives at M3). selene-db types never escape this facade (R-API-8).
//!
//! Status: **M0 scaffold.** The `Engine` *shape* and `OcError` match the spec; load/tick/simulate
//! bodies are stubs (`unimplemented!()`) and land in M0/M1.

use std::sync::Arc;

use oce_graph::{RunState, Schedule};
use oce_model::ModelGraph;
use oce_store::{DomainKey, PointHandle, Store};
use oce_store_mem::MemStore;

/// The single owned facade handle, generic over a `Store`; default `MemStore` (no DB,
/// D-OWNER-1). Not `Clone` (it owns mutable run state); shared across threads as
/// `Arc<Engine<S>>`. `Send + Sync` for every `S: Store`.
pub struct Engine<S: Store = MemStore> {
    store: Arc<S>,
    /// The flat executable truth (D1), frozen at load.
    model: Arc<ModelGraph>,
    /// The frozen Kahn schedule (D6; selene-free).
    schedule: Schedule,
    /// The sole mutable per-tick structure (`01` §8).
    state: RunState,
    /// Hot point handles, pre-resolved at load (FRAME §3.3) — opaque, no DB type.
    handles: Vec<PointHandle>,
}

impl Engine<MemStore> {
    /// Default constructor — **no DB, no selene-db** (D-OWNER-1). The full load → tick → simulate
    /// loop works on this.
    #[must_use]
    pub fn in_memory() -> Self {
        Engine::with_store(Arc::new(MemStore::default()))
    }
}

impl<S: Store> Engine<S> {
    /// Generic constructor over any `Store` backend. Engine logic is written only against the
    /// `Store` trait, so backends are drop-in.
    #[must_use]
    pub fn with_store(store: Arc<S>) -> Self {
        Self {
            store,
            model: Arc::new(ModelGraph::new()),
            schedule: Schedule::default(),
            state: RunState::default(),
            handles: Vec::new(),
        }
    }

    /// Primary v1 ingest (D2: CXF JSON-LD only). Runs the Group A pipeline (oce-cxf → oce-flatten
    /// → oce-validate → oce-semantics), builds the `oce-graph` schedule, projects into the store,
    /// and pre-resolves hot point handles.
    ///
    /// # Errors
    /// Returns [`OcError`] on any ingest/validation/build/store failure (never panics; R-ERR-1).
    pub fn load_cxf(&mut self, _bytes: &[u8]) -> Result<(), OcError> {
        let _ = (
            &self.store,
            &self.model,
            &self.schedule,
            &self.state,
            &self.handles,
        );
        unimplemented!("Engine::load_cxf — M0 scaffold (end-to-end ingest lands in M1)")
    }

    /// Advance to absolute model time `t_now` (seconds; monotonic non-decreasing), apply staged
    /// inputs, and evaluate one tick of the frozen schedule. The host owns cadence.
    ///
    /// # Errors
    /// Returns [`OcError::TimeRegression`] if `t_now` decreases, or a store error.
    pub fn tick(&mut self, _t_now: f64) -> Result<(), OcError> {
        unimplemented!("Engine::tick — M0 scaffold (hot-path tick lands with the M0 graph)")
    }
}

/// The unified, typed facade error. Never panics on host input (R-ERR-1); `#[non_exhaustive]` so
/// variants evolve additively (R-ERR-2). It wraps Group A errors and the store's `StoreError`,
/// and — critically — **never wraps a selene-db error type directly**: those are already
/// flattened to `StoreError::Backend(String)` inside the adapter (R-ERR-3), so the error surface
/// is byte-identical in shape whether or not the `selene` feature is on.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum OcError {
    /// A CXF ingest failure.
    #[error("CXF ingest error: {0}")]
    Cxf(#[from] oce_cxf::CxfError),
    /// A loader-conformance failure.
    #[error("validation failed: {0}")]
    Validate(#[from] oce_validate::ValidationError),
    /// A flattening failure.
    #[error("flatten error: {0}")]
    Flatten(#[from] oce_flatten::FlattenError),
    /// A model build/schedule failure (algebraic loop, etc.).
    #[error("model build/schedule error: {0}")]
    Build(#[from] oce_graph::BuildError),
    /// A generic load failure.
    #[error("load failed: {detail}")]
    Load {
        /// Human-readable detail.
        detail: String,
    },
    /// `t_now` went backwards (host error; CDL §7.16 monotonic time).
    #[error("time regression: t_now={now} < previous {prev}")]
    TimeRegression {
        /// The supplied (too-small) time.
        now: f64,
        /// The previous tick's time.
        prev: f64,
    },
    /// A store-seam failure — the only path any backend (including selene) reaches the error type.
    #[error("store error: {0}")]
    Store(#[from] oce_store::StoreError),
}

/// Convenience result alias for facade operations.
pub type OcResult<T> = Result<T, OcError>;

/// Open an engine backed by the selene-db adapter (the only selene touch point). Available only
/// under `--features selene`. At M0 the adapter is an empty stub; the real opener lands at M3.
#[cfg(feature = "selene")]
#[must_use]
pub fn selene_feature_enabled() -> bool {
    // Placeholder under the `selene` feature so the feature is exercised by the build at M0.
    // The real `open_selene(dir, cfg) -> Result<Engine<SeleneStore>, OcError>` lands at M3, when
    // oce-store-selene gains its (git, branch = "development") selene-db dependency.
    let () = oce_store_selene::PLACEHOLDER;
    true
}

// Keep a reference to a DomainKey-using path so the re-exported store types are linked even in
// the M0 scaffold (documents that oce-api re-exports the oce-store seam, never selene-db types).
#[doc(hidden)]
pub fn _doc_link_domain_key(k: DomainKey) -> DomainKey {
    k
}

/// Re-export of the selene-free store seam DTOs/traits (08 §11 R-PUB-1). No selene-db type is
/// ever re-exported here.
pub use oce_store;
