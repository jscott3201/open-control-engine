//! The single owned facade handle, [`Engine<S>`], and the load → BUILD → HostTick core. The
//! parameter-table (`params.rs`), complete-frame (`frame.rs`), and IO-inventory (`io.rs`)
//! methods are additional `impl<S: Store> Engine<S>` blocks in their own
//! modules; every field is `pub(crate)` so those sibling impls can read/refresh engine state
//! without a public accessor leaking it.

use std::sync::Arc;

use oce_blocks::{Block, BlockKind, lookup};
use oce_graph::{EvalContext, RunState, Schedule, allocate_state, compile, eval_tick};
use oce_model::ModelGraph;
use oce_store::{DomainKey, Store, StoreError};
use oce_store_mem::MemStore;

use crate::diagnostics::{DiagnosticCapture, DiagnosticStage, LoadReceipt, OperationFailure};
use crate::error::OcError;
use crate::io::IoInventory;
use crate::loading::LoadReport;
use crate::params::{ParamTable, RunMode};
use crate::projection::project_resolved_model;

/// The single owned facade handle, generic over a `Store`; default `MemStore` (no DB, D-OWNER-1).
/// Not `Clone` (it owns mutable run state); intended to be shared across threads as `Arc<Engine<S>>`.
/// `Engine<S>` **is** `Send + Sync` for every `S: Store` (`08` R-ENG-1 / R-API-PY-2): `Arc<S>` and
/// the load-frozen `Arc<ModelGraph>` are `Send + Sync`, `blocks: Vec<Box<dyn Block>>` is `Send + Sync`
/// now that `oce-blocks` declares `Block: Send + Sync`, and every other field is plain owned data —
/// no `unsafe`, no raw pointers (R-API-3). CI-asserted by `guards::_assert_engine_send_sync`
/// Fields are `pub(crate)`: the split-out method modules (`params`/`frame`/`io`) and the
/// in-crate test harness read engine state directly, but nothing escapes the crate.
pub struct Engine<S: Store = MemStore> {
    pub(crate) store: Arc<S>,
    /// Host admission policy, independent of the loaded model and continuation state.
    pub(crate) cxf_byte_limit: usize,
    /// The flat executable truth (D1), frozen at load.
    pub(crate) model: Arc<ModelGraph>,
    /// The frozen Kahn schedule (D6; store-free).
    pub(crate) schedule: Schedule,
    /// Instantiated, parameter-resolved block impls, indexed by `BlockId.0` (frozen at load).
    pub(crate) blocks: Vec<Box<dyn Block>>,
    /// The sole mutable per-tick structure (`01` §8).
    pub(crate) state: RunState,
    /// Previous tick's absolute model time — enforces the monotonic-`t_now` contract (CDL §7.16).
    pub(crate) prev_t: Option<f64>,
    /// Stable durable identity of the loaded model projection.
    pub(crate) model_id: DomainKey,
    /// Should-level semantic diagnostics from the load-time resolver.
    pub(crate) semantic_warnings: Vec<oce_diag::Diagnostic>,
    /// The live parameter table (`08` §4): a dotted-path mirror of the model's block params.
    pub(crate) params: ParamTable,
    /// The tune-at-rest lifecycle gate (`08` §4); `Running` post-load.
    pub(crate) mode: RunMode,
    /// Set by `set_param`; drives the re-fold + re-instantiate on `resume`.
    pub(crate) params_dirty: bool,
    /// The typed IO inventory: built at load for frame preparation and latest-state inspection.
    pub(crate) io: IoInventory,
    /// True only after a model has completed the full load and store-open path successfully.
    pub(crate) loaded: bool,
    /// Durable restore is a startup operation and closes at the first mutation boundary.
    pub(crate) durable_restore_ready: bool,
    /// Process-local incarnation fence; retained artifacts keep old allocations alive (no ABA).
    pub(crate) frame_generation: Arc<()>,
    /// Lifetime-local complete-frame correlation, deliberately outside run state and its codecs.
    pub(crate) accepted_frame_sequence: u64,
}

impl Engine<MemStore> {
    /// Default constructor — **no database** (D-OWNER-1). Load CXF, then prepare and execute frames.
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
            cxf_byte_limit: crate::MAX_CXF_BYTES,
            model: Arc::new(ModelGraph::new()),
            schedule: Schedule::default(),
            blocks: Vec::new(),
            state: RunState::default(),
            prev_t: None,
            model_id: DomainKey::default(),
            semantic_warnings: Vec::new(),
            params: ParamTable::default(),
            mode: RunMode::Running,
            params_dirty: false,
            io: IoInventory::default(),
            loaded: false,
            durable_restore_ready: false,
            frame_generation: Arc::new(()),
            accepted_frame_sequence: 0,
        }
    }

    /// Load a **hand-built**, already-flattened [`ModelGraph`] directly inside the crate, with **no
    /// parser** in front of it (CXF ingest in [`Engine::load_cxf`] shares this tail and supplies its
    /// top-composite model IRI).
    ///
    /// Instantiates each block from the `oce-blocks` registry by its `class_iri`, runs the
    /// `oce-graph` BUILD (direct-feedthrough DAG → deterministic Kahn schedule, hard-rejecting
    /// algebraic loops per CDL §7.16), allocates the parameter-seeded run state, builds the IO
    /// inventory + parameter table, snapshots the output connectors, projects the durable
    /// [`oce_store::ResolvedModel`], saves it through the store port, and opens the store's
    /// durability lifecycle (`recover`). Replaces all per-run state, so calling it again reloads a
    /// fresh model.
    ///
    /// # Structural invariant
    /// `model` must be flattened before this tail. The tail runs the pure `oce-validate` structural
    /// gate before `oce-graph` consumes the graph, so malformed hand-built graphs become typed
    /// [`OcError::Validate`] failures instead of reaching hot-path arena indexing.
    ///
    /// # Model identity
    /// `model_iri` is `Some` only for CXF loads, where it is the top-composite `@id` from the
    /// resolver side-channel. `None` preserves the deterministic synthetic stable-hash fallback used
    /// by hand-built, resumed, and future non-CXF load paths.
    ///
    /// # Errors
    /// [`OcError::Validate`] if the graph is malformed; [`OcError::Load`] if a block's `class_iri` is
    /// not in the registry or semantic resolution fails; [`OcError::Build`] if the graph has an
    /// algebraic loop; [`OcError::Store`] if the store's `recover`/`save_model` fails. Never panics
    /// on host input covered by the validation seam (R-ERR-1).
    #[cfg(test)]
    pub(crate) fn build_model_in_memory(
        &mut self,
        model: ModelGraph,
        model_iri: Option<&str>,
    ) -> Result<(), OcError> {
        // Defense in depth for every in-crate caller: `oce-graph` assumes a validated graph and keeps
        // the tick/build arenas lean, so malformed hand-built graphs stop here as typed diagnostics.
        let validate_warnings = oce_validate::validate(&model)?;
        self.build_validated_model_in_memory(
            model,
            model_iri,
            validate_warnings,
            &mut DiagnosticCapture::new(false, DiagnosticStage::Validation),
        )
        .map(|_| ())
    }

    /// Build a graph that has already passed the structural gate, preserving prior diagnostics if a
    /// later registry, schedule, semantic, projection, or store stage fails.
    fn build_validated_model_in_memory(
        &mut self,
        model: ModelGraph,
        model_iri: Option<&str>,
        mut diagnostics: Vec<oce_diag::Diagnostic>,
        capture: &mut DiagnosticCapture,
    ) -> Result<Vec<oce_diag::Diagnostic>, OcError> {
        let result: Result<(), OcError> = (|| {
            // Resolve every block instance to its native impl up front — an unknown class is a typed
            // load error, never a panic (R-IMPL-2 / R-ERR-1).
            capture.enter(DiagnosticStage::Instantiation);
            let blocks = instantiate_blocks(&model)?;
            // BUILD (off the tick): schedule + state. `?` on `compile` maps `BuildError` → `OcError`.
            capture.enter(DiagnosticStage::Schedule);
            let schedule = compile(&model, &blocks)?;
            let state = allocate_state(&model, &blocks);
            let io = IoInventory::build_at_load(&model);
            let params = ParamTable::build_at_load(&model);
            capture.enter(DiagnosticStage::Semantics);
            let semantics = oce_semantics::resolve(&model).map_err(|err| OcError::Load {
                detail: format!("semantic resolution failed: {err}"),
            })?;
            capture.record(&semantics.diagnostics);
            diagnostics.extend(semantics.diagnostics.iter().cloned());
            capture.enter(DiagnosticStage::Projection);
            let resolved_model = project_resolved_model(&model, &semantics, model_iri)?;
            let semantic_warnings = semantics.diagnostics;
            // Open the store's durability lifecycle before the first tick (no-op for `MemStore`).
            capture.enter(DiagnosticStage::StoreRecovery);
            self.store.recover()?;
            capture.enter(DiagnosticStage::StoreSave);
            self.store.save_model(&resolved_model)?;
            capture.enter(DiagnosticStage::StoreInputs);
            validate_store_inputs(self.store.as_ref(), &io)?;
            // In-memory commit boundary: every ordinary fallible stage is complete. Store
            // effects above are NOT rolled back, even if one of those calls returned an error.
            self.model = Arc::new(model);
            self.blocks = blocks;
            self.schedule = schedule;
            self.state = state;
            self.io = io;
            self.params = params;
            self.mode = RunMode::Running;
            self.params_dirty = false;
            self.prev_t = None;
            self.model_id = resolved_model.model_id;
            self.semantic_warnings = semantic_warnings;
            self.loaded = true;
            self.frame_generation = Arc::new(());
            self.durable_restore_ready = true;
            Ok(())
        })();
        match result {
            Ok(()) => Ok(diagnostics),
            Err(error) => Err(error.with_load_context(diagnostics)),
        }
    }

    /// Primary v1 ingest (D2: CXF JSON-LD only). Runs the Group A pipeline (`oce-cxf` resolve →
    /// `oce-flatten` → `oce-validate`), builds the `oce-graph` schedule via the shared crate-private
    /// build tail, and returns a [`LoadReport`]. The returned `model_id` is the raw CXF
    /// top-composite `@id` carried by the resolver side-channel. Replaces all per-run state, so
    /// calling it again reloads a fresh model.
    ///
    /// The serialized length is checked against [`Self::cxf_byte_limit`] before parsing,
    /// Store calls or engine mutation. On any returned error the prior in-memory executable
    /// and run image remain unchanged. Store recovery, model saves and handle resolution
    /// may already have external effects: the host owns compensation, and old external
    /// handle validity is not promised. This is not a distributed transaction or a panic/
    /// allocation-failure recovery guarantee. Successful reload replaces model-bound state
    /// and caches; prior ephemeral model references are invalid under their existing contract.
    ///
    /// # Errors
    /// Returns [`OcError`] on any ingest/validation/build/store failure (never panics; R-ERR-1):
    /// [`OcError::CxfTooLarge`], [`OcError::Cxf`], [`OcError::Flatten`], [`OcError::Validate`], [`OcError::Build`],
    /// [`OcError::Load`], or [`OcError::Store`]. If a completed stage returned diagnostics before a
    /// later failure, [`OcError::LoadContext`] retains them and exposes the terminal variant through
    /// [`std::error::Error::source`].
    pub fn load_cxf(&mut self, bytes: &[u8]) -> Result<LoadReport, OcError> {
        self.load_cxf_pipeline(
            bytes,
            &mut DiagnosticCapture::new(false, DiagnosticStage::Import),
        )
    }

    /// Load CXF through the same pipeline as [`Self::load_cxf`], with immutable producer evidence.
    ///
    /// Captures every returned diagnostic at its actual producer boundary, independently of the
    /// legacy report. The receipt has its own revisioned machine order. Engine/store side effects,
    /// numerical behavior, error variants and legacy warning order remain unchanged. Allocates
    /// evidence only on this opt-in path; no runtime warning collection is enabled.
    ///
    /// # Errors
    /// Returns [`OperationFailure`] with prior evidence, terminal stage and original error context
    /// on any load failure, including failures without structured diagnostics.
    pub fn load_cxf_with_receipt(&mut self, bytes: &[u8]) -> Result<LoadReceipt, OperationFailure> {
        let mut capture = DiagnosticCapture::new(true, DiagnosticStage::Import);
        match self.load_cxf_pipeline(bytes, &mut capture) {
            Ok(report) => Ok(LoadReceipt::new(report, capture)),
            Err(error) => Err(OperationFailure::new(error, capture)),
        }
    }

    fn load_cxf_pipeline(
        &mut self,
        bytes: &[u8],
        capture: &mut DiagnosticCapture,
    ) -> Result<LoadReport, OcError> {
        if bytes.len() > self.cxf_byte_limit {
            return Err(OcError::CxfTooLarge {
                actual_bytes: bytes.len(),
                limit_bytes: self.cxf_byte_limit,
            });
        }
        // 1. Resolve CXF → flat, ground ModelGraph (+ warning-only report; errors are Err here).
        let (model, report) = oce_cxf::import_cxf(bytes, &oce_cxf::ResolveOptions::default())?;
        let model_iri = report.model_iri.clone();
        capture.record(&report.diagnostics);
        let mut diagnostics = report.diagnostics;
        capture.enter(DiagnosticStage::Flatten);
        // 2. Flatten (scalar identity; array-parameter normalization is resolver-owned).
        let mut model = match oce_flatten::flatten(model) {
            Ok(model) => model,
            Err(error) => return Err(OcError::from(error).with_load_context(diagnostics)),
        };
        // 3. Deep gate: §7.10 unification (mutates the graph to propagate one-sided units), then
        //    the structural/type rules. A shall-violation propagates as OcError::Validate.
        capture.enter(DiagnosticStage::AttributeUnification);
        match oce_validate::unify_attributes(&mut model) {
            Ok(warnings) => {
                capture.record(&warnings);
                diagnostics.extend(warnings);
            }
            Err(error) => return Err(OcError::from(error).with_load_context(diagnostics)),
        }
        capture.enter(DiagnosticStage::Validation);
        match oce_validate::validate(&model) {
            Ok(warnings) => {
                capture.record(&warnings);
                diagnostics.extend(warnings);
            }
            Err(error) => return Err(OcError::from(error).with_load_context(diagnostics)),
        }
        // 4. The explicit gate above authorizes the validated tail. Hand-built callers still use
        //    `build_model_in_memory`, which validates defensively before entering this helper.
        let warnings = self.build_validated_model_in_memory(
            model,
            model_iri.as_deref(),
            diagnostics,
            capture,
        )?;
        let stateful_blocks = self
            .blocks
            .iter()
            .filter(|b| b.kind() == BlockKind::Stateful)
            .count();
        Ok(LoadReport {
            model_id: self.model_id.clone(),
            warnings,
            io: self.io.summary(),
            block_count: self.model.blocks.len(),
            stateful_blocks,
        })
    }

    /// Infallible HostTick core after caller-specific preflight and input staging.
    ///
    /// Closes startup restore, evaluates once, and refreshes time/latest outputs. The caller
    /// owns diagnostics; no sink, Store access, sequence or output projection is selected here.
    /// Validated BUILD arenas and caller-checked representable time are required. No ordinary
    /// recoverable failure remains in this seam; panic/allocation failure is not rollback.
    pub(crate) fn transition_host_tick(&mut self, t_now: f64, diag: &dyn oce_blocks::Diagnostics) {
        #[cfg(test)]
        transition_tests::TRANSITIONS.with(|count| count.set(count.get() + 1));

        self.durable_restore_ready = false;
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
    }

    pub(crate) fn time_is_representable(&self, t_now: f64) -> bool {
        self.state.slots.iter().all(|slot| {
            let block = &self.blocks[slot.block.0 as usize];
            let end = slot.offset + slot.len;
            block.time_is_representable(t_now, &self.state.words[slot.offset..end])
        })
    }

    /// The frozen schedule (for trace tooling / determinism assertions; D6).
    ///
    /// This exposes an internal graph type and is retained only for compatibility with existing
    /// trace tooling. It is classified as implementation leakage for a future coordinated removal;
    /// new hosts should use output and topology facade views instead.
    #[must_use]
    pub fn schedule(&self) -> &Schedule {
        &self.schedule
    }

    /// Borrow the wired store backend (e.g. for model round-trips or durability hooks).
    ///
    /// This is conditional storage-port surface: the returned backend and every handle it owns keep
    /// their adapter-specific lifecycle and validity rules.
    #[must_use]
    pub fn store(&self) -> &S {
        &self.store
    }
}

/// Instantiate every block instance from the `oce-blocks` registry. This lookup + `make` loop is
/// shared by the validated load tail and `Engine::resume`, so load and tune-at-rest refolds cannot
/// drift. An unknown `class_iri` is a typed [`OcError::Load`], never a panic.
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

// Preserve load-time adapter validation and its failure boundary. Handles are not retained:
// complete-frame execution never obtains determinants from the Store.
fn validate_store_inputs<S: Store>(store: &S, io: &IoInventory) -> Result<(), OcError> {
    let inputs = io.input_keys();
    if inputs.is_empty() {
        return Ok(());
    }
    let handles = store.resolve_points(&inputs)?;
    if handles.len() != inputs.len() {
        return Err(StoreError::Validation(format!(
            "PointStore::resolve_points returned {} handles for {} input points",
            handles.len(),
            inputs.len()
        ))
        .into());
    }
    Ok(())
}

/// Test inspection keys in connector declaration order, including metadata-only String outputs.
#[must_use]
#[cfg(test)]
pub(crate) fn out_connector_paths(model: &ModelGraph) -> Vec<String> {
    model
        .connectors
        .iter()
        .filter(|c| c.dir == oce_model::Dir::Out)
        .map(|c| crate::io::connector_path(c.iri.as_deref(), c.id))
        .collect()
}

#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "shared_transition_tests.rs"]
mod transition_tests;
