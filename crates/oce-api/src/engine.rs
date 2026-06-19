//! The single owned facade handle, [`Engine<S>`], and the core load → BUILD → tick path. The
//! parameter-table (`params.rs`), execution-mode (`sim.rs`), IO-inventory (`io.rs`), and deferred-
//! load (`loading.rs`) methods are additional `impl<S: Store> Engine<S>` blocks in their own
//! modules; every field is `pub(crate)` so those sibling impls can read/refresh engine state
//! without a public accessor leaking it.

use std::sync::Arc;

use oce_blocks::{Block, BlockKind, NoopDiagnostics, lookup};
use oce_graph::{EvalContext, RunState, Schedule, allocate_state, compile, eval_tick};
use oce_model::{Dir, ModelGraph};
use oce_store::{PointHandle, Store};
use oce_store_mem::MemStore;

use crate::error::OcError;
use crate::io::IoInventory;
use crate::loading::LoadReport;
use crate::params::{ParamTable, RunMode};
use crate::sim::Outputs;

/// The single owned facade handle, generic over a `Store`; default `MemStore` (no DB, D-OWNER-1).
/// Not `Clone` (it owns mutable run state); intended to be shared across threads as `Arc<Engine<S>>`.
/// `Engine<S>` **is** `Send + Sync` for every `S: Store` (`08` R-ENG-1 / R-API-PY-2): `Arc<S>` and
/// the load-frozen `Arc<ModelGraph>` are `Send + Sync`, `blocks: Vec<Box<dyn Block>>` is `Send + Sync`
/// now that `oce-blocks` declares `Block: Send + Sync`, and every other field is plain owned data —
/// no `unsafe`, no raw pointers (R-API-3). CI-asserted by `guards::_assert_engine_send_sync`
/// (M1-PR-12). Fields are `pub(crate)`: the split-out method modules (`params`/`sim`/`io`/`loading`)
/// and the in-crate test harness read engine state directly, but nothing escapes the crate.
pub struct Engine<S: Store = MemStore> {
    pub(crate) store: Arc<S>,
    /// The flat executable truth (D1), frozen at load.
    pub(crate) model: Arc<ModelGraph>,
    /// The frozen Kahn schedule (D6; store-free).
    pub(crate) schedule: Schedule,
    /// Instantiated, parameter-resolved block impls, indexed by `BlockId.0` (frozen at load).
    pub(crate) blocks: Vec<Box<dyn Block>>,
    /// The sole mutable per-tick structure (`01` §8).
    pub(crate) state: RunState,
    /// Snapshot of the model's output connector values, refreshed each [`Engine::tick`].
    pub(crate) outputs: Outputs,
    /// Previous tick's absolute model time — enforces the monotonic-`t_now` contract (CDL §7.16).
    pub(crate) prev_t: Option<f64>,
    /// Hot point handles, pre-resolved at load (FRAME §3.3) — opaque, no DB type.
    pub(crate) handles: Vec<PointHandle>,
    /// The live parameter table (`08` §4): a dotted-path mirror of the model's block params.
    pub(crate) params: ParamTable,
    /// The tune-at-rest lifecycle gate (`08` §4); `Running` post-load.
    pub(crate) mode: RunMode,
    /// Set by `set_param`; drives the re-fold + re-instantiate on `resume`.
    pub(crate) params_dirty: bool,
    /// The typed IO inventory (`08` §6): built at load; the `set_input`/`get_output` name resolver.
    pub(crate) io: IoInventory,
}

impl Engine<MemStore> {
    /// Default constructor — **no database** (D-OWNER-1). The full load → tick → simulate loop
    /// works on this.
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
            params: ParamTable::default(),
            mode: RunMode::Running,
            params_dirty: false,
            io: IoInventory::default(),
        }
    }

    /// Load a **hand-built**, already-flattened [`ModelGraph`] directly inside the crate — the M0 path
    /// with **no parser** (CXF ingest in [`Engine::load_cxf`] shares this tail).
    ///
    /// Instantiates each block from the `oce-blocks` registry by its `class_iri`, runs the
    /// `oce-graph` BUILD (direct-feedthrough DAG → deterministic Kahn schedule, hard-rejecting
    /// algebraic loops per CDL §7.16), allocates the parameter-seeded run state, builds the IO
    /// inventory + parameter table, snapshots the output connectors, and opens the store's
    /// durability lifecycle (`recover`). Replaces all per-run state, so calling it again reloads a
    /// fresh model.
    ///
    /// # Structural invariant
    /// `model` must be flattened before this tail. The tail runs the pure `oce-validate` structural
    /// gate before `oce-graph` consumes the graph, so malformed hand-built graphs become typed
    /// [`OcError::Validate`] failures instead of reaching hot-path arena indexing.
    ///
    /// # Errors
    /// [`OcError::Validate`] if the graph is malformed; [`OcError::Load`] if a block's `class_iri` is
    /// not in the registry; [`OcError::Build`] if the graph has an algebraic loop; [`OcError::Store`]
    /// if the store's `recover` fails. Never panics on host input covered by the validation seam
    /// (R-ERR-1).
    pub(crate) fn build_model_in_memory(&mut self, model: ModelGraph) -> Result<(), OcError> {
        // Defense in depth for every in-crate caller: `oce-graph` assumes a validated graph and keeps
        // the tick/build arenas lean, so malformed hand-built graphs stop here as typed diagnostics.
        let _validate_warnings = oce_validate::validate(&model)?;
        // Resolve every block instance to its native impl up front — an unknown class is a typed
        // load error, never a panic (R-IMPL-2 / R-ERR-1).
        let blocks = instantiate_blocks(&model)?;
        // BUILD (off the tick): schedule + state. `?` on `compile` maps `BuildError` → `OcError`.
        let schedule = compile(&model, &blocks)?;
        let state = allocate_state(&model, &blocks);
        let outputs = Outputs::build(&model, &state);
        let io = IoInventory::build_at_load(&model);
        let params = ParamTable::build_at_load(&model);
        // Open the store's durability lifecycle before the first tick (no-op for `MemStore`).
        self.store.recover()?;
        self.model = Arc::new(model);
        self.blocks = blocks;
        self.schedule = schedule;
        self.state = state;
        self.outputs = outputs;
        self.io = io;
        self.params = params;
        self.mode = RunMode::Running;
        self.params_dirty = false;
        self.prev_t = None;
        // M1 stages no store-backed inputs through either `load_cxf` or this shared tail, so no hot
        // point handles are resolved yet; clear any from a prior/future load so reloads never carry
        // stale handles.
        self.handles = Vec::new();
        Ok(())
    }

    /// Primary v1 ingest (D2: CXF JSON-LD only). Runs the Group A pipeline (`oce-cxf` resolve →
    /// `oce-flatten` → `oce-validate`), builds the `oce-graph` schedule via the shared crate-private
    /// build tail, and returns a [`LoadReport`]. Replaces all per-run state, so calling it again
    /// reloads a fresh model.
    ///
    /// # Errors
    /// Returns [`OcError`] on any ingest/validation/build/store failure (never panics; R-ERR-1):
    /// [`OcError::Cxf`], [`OcError::Flatten`], [`OcError::Validate`], [`OcError::Build`],
    /// [`OcError::Load`], or [`OcError::Store`].
    pub fn load_cxf(&mut self, bytes: &[u8]) -> Result<LoadReport, OcError> {
        // 1. Resolve CXF → flat, ground ModelGraph (+ warning-only report; errors are Err here).
        let (model, report) = oce_cxf::import_cxf(bytes, &oce_cxf::ResolveOptions::default())?;
        // 2. Flatten (scalar identity in M1; array normalization is resolver-owned, PR-9).
        let mut model = oce_flatten::flatten(model)?;
        // 3. Deep gate (PR-8): §7.10 unification (mutates the graph to propagate one-sided units)
        //    then the structural/type rules. A shall-violation propagates as OcError::Validate.
        let validate_warnings = oce_validate::unify_and_validate(&mut model)?;
        // 4. Shared BUILD tail: registry → schedule → state → outputs → io → params → store.recover.
        //    This intentionally re-runs pure `validate`: `load_cxf` needs `unify_and_validate` above to
        //    capture warnings after §7.10 propagation, while the shared tail must defend every caller
        //    against malformed hand-built graphs before `oce-graph` indexes raw arenas.
        self.build_model_in_memory(model)?;
        let stateful_blocks = self
            .blocks
            .iter()
            .filter(|b| b.kind() == BlockKind::Stateful)
            .count();
        // One uniform `oce-diag` stream (AD-4): resolver `should`-warnings first, then the deep
        // gate's — each already internally sorted; no global re-sort across the seam.
        let mut warnings = report.diagnostics;
        warnings.extend(validate_warnings);
        Ok(LoadReport {
            // M1: no model-level IRI exists on `ModelGraph` yet, so the id is the empty `Default`
            // key (honest-empty, never fabricated). M2 derives it from the top-composite `@id`.
            model_id: oce_store::DomainKey::default(),
            warnings,
            io: self.io.summary(),
            block_count: self.model.blocks.len(),
            stateful_blocks,
        })
    }

    /// Advance to absolute model time `t_now` (seconds; finite, monotonic non-decreasing), evaluate
    /// one tick of the frozen schedule, and refresh the [`Outputs`] snapshot. The host owns cadence.
    ///
    /// # Errors
    /// [`OcError::NonFiniteTime`] if `t_now` is NaN or infinite; [`OcError::TimeRegression`] if
    /// `t_now` is less than the previous tick's time (CDL §7.16 monotonic time). A rejected tick
    /// does not advance the model. Never panics (R-ERR-1).
    pub fn tick(&mut self, t_now: f64) -> Result<&Outputs, OcError> {
        let diag = NoopDiagnostics;
        self.tick_with(t_now, &diag)
    }

    pub(crate) fn tick_with(
        &mut self,
        t_now: f64,
        diag: &dyn oce_blocks::Diagnostics,
    ) -> Result<&Outputs, OcError> {
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
        {
            let mut ctx = EvalContext {
                model: &self.model,
                schedule: &self.schedule,
                blocks: &self.blocks,
                diagnostics: diag,
                state: &mut self.state,
            };
            eval_tick(&mut ctx, t_now);
        }
        self.prev_t = Some(t_now);
        self.outputs.refresh_from(&self.state);
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

/// Instantiate every block instance from the `oce-blocks` registry (the lookup + `make` loop shared
/// by [`Engine::build_model_in_memory`] and `Engine::resume`, so the two never drift — critic M4).
/// An unknown `class_iri` is a typed [`OcError::Load`], never a panic.
pub(crate) fn instantiate_blocks(model: &ModelGraph) -> Result<Vec<Box<dyn Block>>, OcError> {
    let mut blocks: Vec<Box<dyn Block>> = Vec::with_capacity(model.blocks.len());
    for blk in &model.blocks {
        let entry = lookup(&blk.class_iri).ok_or_else(|| OcError::Load {
            detail: format!("unknown block class: {}", blk.class_iri),
        })?;
        blocks.push((entry.make)(&blk.params));
    }
    Ok(blocks)
}

/// The output-connector paths in `connectors.filter(Out)` order — the keys for [`Outputs::to_map`].
/// Derived from the model connectors (NOT the IO inventory, which excludes `String` connectors), so
/// it is always the same length and order as the `Outputs` value entries (critic M1/M2).
#[must_use]
pub(crate) fn out_connector_paths(model: &ModelGraph) -> Vec<String> {
    model
        .connectors
        .iter()
        .filter(|c| c.dir == Dir::Out)
        .map(|c| crate::io::connector_path(c.iri.as_deref(), c.id))
        .collect()
}
