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
//! (and only) build has no database** (D-OWNER-1). The library ships no first-party database and no
//! DB-gated feature; a durable/queryable backend is an app-side adapter the host wires behind the
//! `oce-store` port. No store-backend-specific type ever escapes this facade (R-API-8).
//!
//! Status: **M1.** The full `build_model_in_memory` → `tick` loop works (M0), and
//! [`Engine::load_cxf`] now runs the end-to-end CXF ingest pipeline (resolve → flatten → validate →
//! BUILD) returning a [`LoadReport`] — **M1 exit #1**. `simulate` / `step_realtime` and the IO
//! inventory remain stubs that land with the frozen-surface fill in M1-PR-10.

use std::sync::Arc;

use oce_blocks::{Block, BlockKind, lookup};
use oce_graph::{EvalContext, RunState, Schedule, allocate_state, compile, eval_tick};
use oce_model::{ConnectorId, Dir, ModelGraph, Value};
use oce_store::{DomainKey, PointHandle, Store};
use oce_store_mem::MemStore;

/// An owned, enumerable snapshot of the model's **output** connector values after a tick (`08`
/// R-API-4a). Refreshed in place each [`Engine::tick`]; no store-backend type ever appears here.
#[derive(Clone, Debug, Default)]
pub struct Outputs {
    entries: Vec<(ConnectorId, Value)>,
}

impl Outputs {
    /// The current value of output connector `c`, if it is an output of the loaded model.
    ///
    /// M0 does a linear scan (output sets are small); a dense-index lookup is a perf revision for
    /// the hot read path (`08` §8) once large models exercise it.
    #[must_use]
    pub fn get(&self, c: ConnectorId) -> Option<&Value> {
        self.entries.iter().find(|(id, _)| *id == c).map(|(_, v)| v)
    }

    /// Iterate `(connector, value)` for every output connector, in declaration order.
    pub fn iter(&self) -> impl Iterator<Item = (ConnectorId, &Value)> {
        self.entries.iter().map(|(id, v)| (*id, v))
    }

    /// Number of output connectors.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the model has no output connectors.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// The result of a successful load (`08` R-PUB-5 / R-LOAD-1). Carries the `should`-level
/// diagnostics (the load succeeded; these are advisory — e.g. an `Analog→Real` coercion) plus
/// summary counts for the loaded model.
///
/// `#[non_exhaustive]`: the frozen `08` shape additionally carries `model_id: DomainKey` and
/// `io: IoSummary` (the §5 IO inventory). Those land with the IO-inventory work in **M1-PR-10**
/// (the surface is baselined by `cargo public-api` in **M1-PR-12**, so growing it until then is
/// non-breaking). `Clone` is required by the PyO3 surface contract (`10` R-PY).
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct LoadReport {
    /// `should`-level diagnostics from ingest + validation (the shared `oce-diag` vocabulary, AD-4).
    pub warnings: Vec<oce_diag::Diagnostic>,
    /// Number of (elementary) block instances in the loaded model.
    pub block_count: usize,
    /// Number of stateful `[S]` block instances — a state-footprint signal (`08` §5).
    pub stateful_blocks: usize,
}

/// The single owned facade handle, generic over a `Store`; default `MemStore` (no DB,
/// D-OWNER-1). Not `Clone` (it owns mutable run state); shared across threads as
/// `Arc<Engine<S>>`. `Send + Sync` for every `S: Store`.
pub struct Engine<S: Store = MemStore> {
    store: Arc<S>,
    /// The flat executable truth (D1), frozen at load.
    model: Arc<ModelGraph>,
    /// The frozen Kahn schedule (D6; store-free).
    schedule: Schedule,
    /// Instantiated, parameter-resolved block impls, indexed by `BlockId.0` (frozen at load).
    blocks: Vec<Box<dyn Block>>,
    /// The sole mutable per-tick structure (`01` §8).
    state: RunState,
    /// Snapshot of the model's output connector values, refreshed each [`Engine::tick`].
    outputs: Outputs,
    /// Previous tick's absolute model time — enforces the monotonic-`t_now` contract (CDL §7.16).
    prev_t: Option<f64>,
    /// Hot point handles, pre-resolved at load (FRAME §3.3) — opaque, no DB type.
    handles: Vec<PointHandle>,
}

impl Engine<MemStore> {
    /// Default constructor — **no database** (D-OWNER-1). The full load → tick → simulate
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
            blocks: Vec::new(),
            state: RunState::default(),
            outputs: Outputs::default(),
            prev_t: None,
            handles: Vec::new(),
        }
    }

    /// Load a **hand-built**, already-flattened [`ModelGraph`] directly — the M0 path with **no
    /// parser** (CXF ingest in [`Engine::load_cxf`] will share this tail once it lands in M1).
    ///
    /// Instantiates each block from the `oce-blocks` registry by its `class_iri`, runs the
    /// `oce-graph` BUILD (direct-feedthrough DAG → deterministic Kahn schedule, hard-rejecting
    /// algebraic loops per CDL §7.16), allocates the parameter-seeded run state, snapshots the
    /// output connectors, and opens the store's durability lifecycle (`recover`). Replaces all
    /// per-run state, so calling it again reloads a fresh model.
    ///
    /// # Structural invariant
    /// `model` must be a **flattened, structurally-valid** graph — dense in-range connector/block
    /// ids and per-block input/output arities matching the registry impl's signature. M0's
    /// hand-built path supplies this by construction; from M1 it is the loader/`oce-validate`
    /// contract (CXF ingest). A structurally-malformed model is *not* host input this method
    /// guards — id-range/arity validation is the loader's job, not re-checked on the BUILD hot path.
    ///
    /// # Errors
    /// [`OcError::Load`] if a block's `class_iri` is not in the registry; [`OcError::Build`] if the
    /// graph has an algebraic loop (connector- or block-granularity); [`OcError::Store`] if the
    /// store's `recover` fails. Never panics on a structurally-valid model (R-ERR-1).
    pub fn build_model_in_memory(&mut self, model: ModelGraph) -> Result<(), OcError> {
        // Resolve every block instance to its native impl up front — an unknown class is a typed
        // load error, never a panic (R-IMPL-2 / R-ERR-1).
        let mut blocks: Vec<Box<dyn Block>> = Vec::with_capacity(model.blocks.len());
        for blk in &model.blocks {
            let entry = lookup(&blk.class_iri).ok_or_else(|| OcError::Load {
                detail: format!("unknown block class: {}", blk.class_iri),
            })?;
            blocks.push((entry.make)(&blk.params));
        }
        // BUILD (off the tick): schedule + state. A `?` on `compile` maps `BuildError` → `OcError`.
        let schedule = compile(&model, &blocks)?;
        let state = allocate_state(&model, &blocks);
        // Snapshot the output connectors (declaration order is connector order in the arena).
        let entries = model
            .connectors
            .iter()
            .filter(|c| c.dir == Dir::Out)
            .map(|c| (c.id, state.values[c.id.0 as usize].clone()))
            .collect();
        // Open the store's durability lifecycle before the first tick (no-op for `MemStore`).
        self.store.recover()?;
        self.model = Arc::new(model);
        self.blocks = blocks;
        self.schedule = schedule;
        self.state = state;
        self.outputs = Outputs { entries };
        self.prev_t = None;
        // M0 stages no store-backed inputs, so this path resolves no hot point handles; clear any
        // from a prior load so a reload never carries stale handles (populated by `load_cxf`, M1).
        self.handles = Vec::new();
        Ok(())
    }

    /// Primary v1 ingest (D2: CXF JSON-LD only). Runs the Group A pipeline
    /// (`oce-cxf` resolve → `oce-flatten` → `oce-validate`), builds the `oce-graph` schedule via the
    /// shared [`Engine::build_model_in_memory`] tail, and returns a [`LoadReport`]. Replaces all
    /// per-run state, so calling it again reloads a fresh model.
    ///
    /// Pipeline (M1):
    /// 1. [`oce_cxf::import_cxf`] (Ground mode) lowers the CXF JSON-LD directly to a flat, ground
    ///    [`ModelGraph`] plus a warning-only `ValidationReport` (any error → [`OcError::Cxf`]).
    /// 2. [`oce_flatten::flatten`] — scalar identity in M1 (array normalization lands in PR-9).
    /// 3. [`oce_validate::validate`] — trivial pass in M1 (the deep gate lands in PR-8); a future
    ///    `shall`-violation → [`OcError::Validate`].
    /// 4. [`Engine::build_model_in_memory`] — registry resolution, BUILD (loop-rejecting Kahn
    ///    schedule), state allocation, output snapshot, and `store.recover()`.
    ///
    /// # Errors
    /// Returns [`OcError`] on any ingest/validation/build/store failure (never panics; R-ERR-1):
    /// [`OcError::Cxf`], [`OcError::Flatten`], [`OcError::Validate`], [`OcError::Build`],
    /// [`OcError::Load`], or [`OcError::Store`].
    pub fn load_cxf(&mut self, bytes: &[u8]) -> Result<LoadReport, OcError> {
        // 1. Resolve CXF → flat, ground ModelGraph (+ warning-only report; errors are Err here).
        let (model, report) = oce_cxf::import_cxf(bytes, &oce_cxf::ResolveOptions::default())?;
        // 2. Flatten (scalar identity in M1; array normalization in PR-9).
        let model = oce_flatten::flatten(model)?;
        // 3. Deep validation (trivial pass in M1; PR-8 fills it). Propagates a shall-violation.
        let _validate_warnings = oce_validate::validate(&model)?;
        // 4. Shared BUILD tail: registry → schedule → state → outputs → store.recover.
        self.build_model_in_memory(model)?;
        let stateful_blocks = self
            .blocks
            .iter()
            .filter(|b| b.kind() == BlockKind::Stateful)
            .count();
        Ok(LoadReport {
            warnings: report.diagnostics,
            block_count: self.model.blocks.len(),
            stateful_blocks,
        })
    }

    /// Advance to absolute model time `t_now` (seconds; finite, monotonic non-decreasing), evaluate
    /// one tick of the frozen schedule, and refresh the [`Outputs`] snapshot. The host owns cadence.
    ///
    /// Returns a borrow of the refreshed outputs so a host can read results without a second call.
    ///
    /// **M0 scope:** no store-backed inputs are read yet, so this body does not acquire the
    /// per-tick `store.snapshot()` nor stage handles (`08` §6.1 R-HOT-1) — input staging lands with
    /// the IO inventory in M1. Today it only gathers connector values and evaluates the schedule.
    ///
    /// # Errors
    /// Returns [`OcError::NonFiniteTime`] if `t_now` is NaN or infinite, or
    /// [`OcError::TimeRegression`] if `t_now` is less than the previous tick's time (CDL §7.16
    /// monotonic time). A rejected tick does not advance the model. Never panics (R-ERR-1).
    pub fn tick(&mut self, t_now: f64) -> Result<&Outputs, OcError> {
        // Reject non-finite time first: `NaN < prev` is always false, so a NaN would otherwise slip
        // past the monotonic check, corrupt `state.t`/`prev_t`, and silently disable the guard.
        if !t_now.is_finite() {
            return Err(OcError::NonFiniteTime { now: t_now });
        }
        if let Some(prev) = self.prev_t
            && t_now < prev
        {
            return Err(OcError::TimeRegression { now: t_now, prev });
        }
        // Scope the EvalContext so its borrows of `model`/`schedule`/`blocks`/`state` end before we
        // mutate `prev_t`/`outputs` (disjoint fields, but the context holds three of them at once).
        {
            let mut ctx = EvalContext {
                model: &self.model,
                schedule: &self.schedule,
                blocks: &self.blocks,
                state: &mut self.state,
            };
            eval_tick(&mut ctx, t_now);
        }
        self.prev_t = Some(t_now);
        for (id, value) in &mut self.outputs.entries {
            *value = self.state.values[id.0 as usize].clone();
        }
        Ok(&self.outputs)
    }

    /// The frozen schedule (for trace tooling / determinism assertions; D6).
    #[must_use]
    pub fn schedule(&self) -> &Schedule {
        &self.schedule
    }

    /// The most recent output-connector snapshot (also returned by [`Engine::tick`]).
    #[must_use]
    pub fn outputs(&self) -> &Outputs {
        &self.outputs
    }

    /// Borrow the wired store backend (e.g. for model round-trips or durability hooks).
    #[must_use]
    pub fn store(&self) -> &S {
        &self.store
    }
}

/// The unified, typed facade error. Never panics on host input (R-ERR-1); `#[non_exhaustive]` so
/// variants evolve additively (R-ERR-2). It wraps Group A errors and the store's `StoreError`,
/// and — critically — **never wraps a backend-specific error type directly**: any durable-store
/// adapter flattens its errors to `StoreError::Backend(String)` at the seam (R-ERR-3), so the
/// facade error surface is identical in shape regardless of which `Store` backend is wired.
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
    /// `t_now` was NaN or infinite (host error; CDL §7.16 time is a finite real).
    #[error("non-finite tick time: t_now={now}")]
    NonFiniteTime {
        /// The supplied (non-finite) time.
        now: f64,
    },
    /// `t_now` went backwards (host error; CDL §7.16 monotonic time).
    #[error("time regression: t_now={now} < previous {prev}")]
    TimeRegression {
        /// The supplied (too-small) time.
        now: f64,
        /// The previous tick's time.
        prev: f64,
    },
    /// A store-seam failure — the only path any `Store` backend reaches the error type.
    #[error("store error: {0}")]
    Store(#[from] oce_store::StoreError),
}

/// Convenience result alias for facade operations.
pub type OcResult<T> = Result<T, OcError>;

// Keep a reference to a DomainKey-using path so the re-exported store types are linked even in
// the M0 scaffold (documents that oce-api re-exports the oce-store seam, never store-backend types).
#[doc(hidden)]
pub fn _doc_link_domain_key(k: DomainKey) -> DomainKey {
    k
}

/// Re-export of the store seam DTOs/traits (08 §11 R-PUB-1). No store-backend-specific type is
/// ever re-exported here.
pub use oce_store;

#[cfg(test)]
mod tests;
