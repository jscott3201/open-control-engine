//! The single owned facade handle, [`Engine<S>`], and the core load → BUILD → tick path. The
//! parameter-table (`params.rs`), execution-mode (`sim.rs`), IO-inventory (`io.rs`), and deferred-
//! load (`loading.rs`) methods are additional `impl<S: Store> Engine<S>` blocks in their own
//! modules; every field is `pub(crate)` so those sibling impls can read/refresh engine state
//! without a public accessor leaking it.

use std::sync::Arc;

use oce_blocks::{Block, BlockKind, NoopDiagnostics, lookup};
use oce_graph::{EvalContext, RunState, Schedule, allocate_state, compile, eval_tick};
use oce_model::{ConnectorId, Dir, ModelGraph, Value, ValueType};
use oce_store::{DomainKey, OcValue, PointHandle, PointSample, PointSnapshot, Store, StoreError};
use oce_store_mem::MemStore;

use crate::error::OcError;
use crate::io::IoInventory;
use crate::loading::LoadReport;
use crate::params::{ParamTable, RunMode};
use crate::projection::project_resolved_model;
use crate::sim::{DurableOutputBatch, Outputs};

/// One logical input point's load-time store handle and model-state slots.
#[derive(Clone, Debug)]
pub(crate) struct StoreInputHandle {
    connector_ids: Vec<ConnectorId>,
    handle: PointHandle,
    path: String,
}

/// The single owned facade handle, generic over a `Store`; default `MemStore` (no DB, D-OWNER-1).
/// Not `Clone` (it owns mutable run state); intended to be shared across threads as `Arc<Engine<S>>`.
/// `Engine<S>` **is** `Send + Sync` for every `S: Store` (`08` R-ENG-1 / R-API-PY-2): `Arc<S>` and
/// the load-frozen `Arc<ModelGraph>` are `Send + Sync`, `blocks: Vec<Box<dyn Block>>` is `Send + Sync`
/// now that `oce-blocks` declares `Block: Send + Sync`, and every other field is plain owned data —
/// no `unsafe`, no raw pointers (R-API-3). CI-asserted by `guards::_assert_engine_send_sync`
/// Fields are `pub(crate)`: the split-out method modules (`params`/`sim`/`io`/`loading`) and the
/// in-crate test harness read engine state directly, but nothing escapes the crate.
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
    /// Store-backed input handles, pre-resolved at load; tick reads only these opaque handles.
    pub(crate) store_inputs: Vec<StoreInputHandle>,
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
    /// The typed IO inventory (`08` §6): built at load; the `set_input`/`get_output` name resolver.
    pub(crate) io: IoInventory,
    /// The pre-built `step_realtime` store batch: identity resolved once at load from `io`'s
    /// durable columns, values refreshed in place each step. Re-minted with `io` on every load
    /// so it can never outlive its model.
    pub(crate) durable_batch: DurableOutputBatch,
    /// Host-supplied UNIX epoch corresponding to model time `t = 0` for real-time writes.
    pub(crate) realtime_epoch_unix_nanos: Option<u64>,
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
            store_inputs: Vec::new(),
            model_id: DomainKey::default(),
            semantic_warnings: Vec::new(),
            params: ParamTable::default(),
            mode: RunMode::Running,
            params_dirty: false,
            io: IoInventory::default(),
            durable_batch: DurableOutputBatch::default(),
            realtime_epoch_unix_nanos: None,
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
    pub(crate) fn build_model_in_memory(
        &mut self,
        model: ModelGraph,
        model_iri: Option<&str>,
    ) -> Result<(), OcError> {
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
        // Minted eagerly, from the same inventory the engine keeps: reload invalidation is
        // structural (a new `io` always ships a new batch), never remembered.
        let durable_batch = DurableOutputBatch::build_at_load(&io);
        let params = ParamTable::build_at_load(&model);
        let semantics = oce_semantics::resolve(&model).map_err(|err| OcError::Load {
            detail: format!("semantic resolution failed: {err}"),
        })?;
        let resolved_model = project_resolved_model(&model, &semantics, model_iri)?;
        let semantic_warnings = semantics.diagnostics;
        // Open the store's durability lifecycle before the first tick (no-op for `MemStore`).
        self.store.recover()?;
        self.store.save_model(&resolved_model)?;
        let store_inputs = resolve_store_inputs(self.store.as_ref(), &io)?;
        self.model = Arc::new(model);
        self.blocks = blocks;
        self.schedule = schedule;
        self.state = state;
        self.outputs = outputs;
        self.io = io;
        self.durable_batch = durable_batch;
        self.params = params;
        self.mode = RunMode::Running;
        self.params_dirty = false;
        self.prev_t = None;
        self.model_id = resolved_model.model_id;
        self.semantic_warnings = semantic_warnings;
        self.store_inputs = store_inputs;
        Ok(())
    }

    /// Primary v1 ingest (D2: CXF JSON-LD only). Runs the Group A pipeline (`oce-cxf` resolve →
    /// `oce-flatten` → `oce-validate`), builds the `oce-graph` schedule via the shared crate-private
    /// build tail, and returns a [`LoadReport`]. The returned `model_id` is the raw CXF
    /// top-composite `@id` carried by the resolver side-channel. Replaces all per-run state, so
    /// calling it again reloads a fresh model.
    ///
    /// # Errors
    /// Returns [`OcError`] on any ingest/validation/build/store failure (never panics; R-ERR-1):
    /// [`OcError::Cxf`], [`OcError::Flatten`], [`OcError::Validate`], [`OcError::Build`],
    /// [`OcError::Load`], or [`OcError::Store`].
    pub fn load_cxf(&mut self, bytes: &[u8]) -> Result<LoadReport, OcError> {
        // 1. Resolve CXF → flat, ground ModelGraph (+ warning-only report; errors are Err here).
        let (model, report) = oce_cxf::import_cxf(bytes, &oce_cxf::ResolveOptions::default())?;
        let model_iri = report.model_iri.clone();
        // 2. Flatten (scalar identity; array-parameter normalization is resolver-owned).
        let mut model = oce_flatten::flatten(model)?;
        // 3. Deep gate: §7.10 unification (mutates the graph to propagate one-sided units), then
        //    the structural/type rules. A shall-violation propagates as OcError::Validate.
        let validate_warnings = oce_validate::unify_and_validate(&mut model)?;
        // 4. Shared BUILD tail: registry → schedule → state → outputs → io → params → store.recover.
        //    This intentionally re-runs pure `validate`: `load_cxf` needs `unify_and_validate` above to
        //    capture warnings after §7.10 propagation, while the shared tail must defend every caller
        //    against malformed hand-built graphs before `oce-graph` indexes raw arenas.
        self.build_model_in_memory(model, model_iri.as_deref())?;
        let stateful_blocks = self
            .blocks
            .iter()
            .filter(|b| b.kind() == BlockKind::Stateful)
            .count();
        // One uniform `oce-diag` stream (AD-4): resolver `should`-warnings first, then the deep
        // gate's — each already internally sorted; no global re-sort across the seam.
        let mut warnings = report.diagnostics;
        warnings.extend(validate_warnings);
        warnings.extend(self.semantic_warnings.clone());
        Ok(LoadReport {
            model_id: self.model_id.clone(),
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
        self.stage_store_inputs()?;
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

    fn stage_store_inputs(&mut self) -> Result<(), OcError> {
        if self.store_inputs.is_empty() {
            return Ok(());
        }
        let snapshot = self.store.snapshot()?;
        stage_store_inputs_from_snapshot(
            &self.store_inputs,
            snapshot.as_ref(),
            &self.model,
            &mut self.state,
        )
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

/// Instantiate every block instance from the `oce-blocks` registry. This lookup + `make` loop is
/// shared by [`Engine::build_model_in_memory`] and `Engine::resume`, so load and tune-at-rest
/// refolds cannot drift. An unknown `class_iri` is a typed [`OcError::Load`], never a panic.
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

fn resolve_store_inputs<S: Store>(
    store: &S,
    io: &IoInventory,
) -> Result<Vec<StoreInputHandle>, OcError> {
    let inputs = io.input_bindings();
    if inputs.is_empty() {
        return Ok(Vec::new());
    }
    let keys: Vec<DomainKey> = inputs
        .iter()
        .map(|input| DomainKey::new(input.path.clone()))
        .collect();
    let handles = store.resolve_points(&keys)?;
    if handles.len() != inputs.len() {
        return Err(StoreError::Validation(format!(
            "PointStore::resolve_points returned {} handles for {} input points",
            handles.len(),
            inputs.len()
        ))
        .into());
    }
    Ok(inputs
        .into_iter()
        .zip(handles)
        .map(|(input, handle)| StoreInputHandle {
            connector_ids: input.connector_ids,
            handle,
            path: input.path,
        })
        .collect())
}

/// Stage store-backed input samples from one tick snapshot into the run state.
///
/// Missing samples deliberately leave the connector's current value untouched: before the first
/// sample this preserves the type's `zero_value()`, and after any prior staging it is the engine's
/// hold-last policy. The engine is also status-agnostic by design: [`oce_store::PointStatus`] is
/// quality metadata for the consuming app/BMS layer, so a staged [`Value`] is derived from the
/// sample's value regardless of status.
///
/// # Invariants
/// `inputs` are load-time resolved from the [`IoInventory`], so each connector id is in range for
/// `model.connectors` and `state.values`. `snapshot` must be the single per-tick snapshot.
///
/// # Panics
/// Does not panic when those load-time invariants hold.
fn stage_store_inputs_from_snapshot(
    inputs: &[StoreInputHandle],
    snapshot: &dyn PointSnapshot,
    model: &ModelGraph,
    state: &mut RunState,
) -> Result<(), OcError> {
    for input in inputs {
        let Some(sample) = snapshot.read_resolved(input.handle) else {
            // Deliberate hold-last: no store sample means no overwrite of the current state value.
            continue;
        };
        for &connector_id in &input.connector_ids {
            // StoreInputHandle connector ids are inventory-sourced and in range for the loaded model.
            let connector = &model.connectors[connector_id.0 as usize];
            let value = sample_to_value(sample.clone(), connector.value_type, &input.path)?;
            state.values[connector_id.0 as usize] = value;
        }
    }
    Ok(())
}

/// Convert a store sample into the target engine value type.
///
/// The conversion is intentionally status-agnostic: the returned [`Value`] depends only on the
/// sample value and target type, never on [`oce_store::PointStatus`]. Point quality/fault handling
/// lives above the engine in the consuming app/BMS layer. M3 supports native Real/Integer/Boolean
/// carriers, a String carrier for the total helper path, and the current Int-as-enum-ordinal
/// assumption; native store enum literals and decimal carriers return [`OcError::InputType`].
///
/// # Panics
/// Never panics; mismatches and unsupported carriers return [`OcError::InputType`].
fn sample_to_value(sample: PointSample, want: ValueType, path: &str) -> Result<Value, OcError> {
    let PointSample {
        value,
        status: _,
        at_unix_nanos: _,
    } = sample;
    match (value, want) {
        (OcValue::Real(v), ValueType::Real) => Ok(Value::Real(v)),
        (OcValue::Int(v), ValueType::Integer) => Ok(Value::Integer(v)),
        (OcValue::Bool(v), ValueType::Boolean) => Ok(Value::Boolean(v)),
        // String connectors are excluded from store-backed input bindings; this arm keeps the
        // conversion helper total for direct callers and future non-signal uses.
        (OcValue::String(v), ValueType::String) => Ok(Value::String(Arc::from(v))),
        (OcValue::Int(v), ValueType::Enum(class)) => {
            let ordinal = u32::try_from(v)
                .ok()
                .filter(|ordinal| *ordinal > 0)
                .ok_or_else(|| OcError::InputType(path.to_owned()))?;
            Ok(Value::Enum { class, ordinal })
        }
        _ => Err(OcError::InputType(path.to_owned())),
    }
}

/// The output-connector paths in `connectors.filter(Out)` order — the keys for [`Outputs::to_map`].
/// Derived from the model connectors (NOT the IO inventory, which excludes `String` connectors), so
/// it is always the same length and order as the `Outputs` value entries.
#[must_use]
pub(crate) fn out_connector_paths(model: &ModelGraph) -> Vec<String> {
    model
        .connectors
        .iter()
        .filter(|c| c.dir == Dir::Out)
        .map(|c| crate::io::connector_path(c.iri.as_deref(), c.id))
        .collect()
}

#[cfg(test)]
mod tests {
    use oce_model::{BlockId, Connector, EnumClassId};
    use oce_store::{
        Durable, EquipmentDto, ModelStore, PointListRow, PointStatus, PointStore, PointWrite,
        RelationDto, ResolvedModel, RetrievalHit, SemanticPayloadDto, SemanticQuery, SemanticStore,
        StoreResult, TemplatePointReq,
    };

    use super::*;

    const PATH: &str = "test:input";

    fn sample(value: OcValue) -> PointSample {
        PointSample {
            value,
            status: PointStatus::Fault,
            at_unix_nanos: 42,
        }
    }

    fn assert_input_type(result: Result<Value, OcError>) {
        match result {
            Err(OcError::InputType(path)) => assert_eq!(path, PATH),
            other => panic!("expected InputType for {PATH}, got {other:?}"),
        }
    }

    #[test]
    fn sample_to_value_accepts_native_signal_carriers_and_string_helper_path() {
        match sample_to_value(sample(OcValue::Real(1.25)), ValueType::Real, PATH).unwrap() {
            Value::Real(v) => assert_eq!(v.to_bits(), 1.25f64.to_bits()),
            other => panic!("expected real value, got {other:?}"),
        }
        match sample_to_value(sample(OcValue::Int(7)), ValueType::Integer, PATH).unwrap() {
            Value::Integer(v) => assert_eq!(v, 7),
            other => panic!("expected integer value, got {other:?}"),
        }
        match sample_to_value(sample(OcValue::Bool(true)), ValueType::Boolean, PATH).unwrap() {
            Value::Boolean(v) => assert!(v),
            other => panic!("expected boolean value, got {other:?}"),
        }
        match sample_to_value(
            sample(OcValue::String("metadata".to_owned())),
            ValueType::String,
            PATH,
        )
        .unwrap()
        {
            Value::String(v) => assert_eq!(&*v, "metadata"),
            other => panic!("expected string value, got {other:?}"),
        }
    }

    #[test]
    fn sample_to_value_accepts_positive_integer_enum_ordinals() {
        let class = EnumClassId(9);
        match sample_to_value(sample(OcValue::Int(3)), ValueType::Enum(class), PATH).unwrap() {
            Value::Enum {
                class: actual_class,
                ordinal,
            } => {
                assert_eq!(actual_class, class);
                assert_eq!(ordinal, 3);
            }
            other => panic!("expected enum value, got {other:?}"),
        }
    }

    #[test]
    fn sample_to_value_rejects_invalid_integer_enum_ordinals() {
        let class = EnumClassId(9);
        for ordinal in [0, -1, i64::from(u32::MAX) + 1] {
            assert_input_type(sample_to_value(
                sample(OcValue::Int(ordinal)),
                ValueType::Enum(class),
                PATH,
            ));
        }
    }

    #[test]
    fn sample_to_value_rejects_type_mismatch_pairs() {
        let class = EnumClassId(4);
        let cases = [
            (OcValue::Real(1.0), ValueType::Integer),
            (OcValue::Real(1.0), ValueType::Boolean),
            (OcValue::Real(1.0), ValueType::String),
            (OcValue::Real(1.0), ValueType::Enum(class)),
            (OcValue::Int(1), ValueType::Real),
            (OcValue::Int(1), ValueType::Boolean),
            (OcValue::Int(1), ValueType::String),
            (OcValue::Bool(true), ValueType::Real),
            (OcValue::Bool(true), ValueType::Integer),
            (OcValue::Bool(true), ValueType::String),
            (OcValue::Bool(true), ValueType::Enum(class)),
            (OcValue::String("value".to_owned()), ValueType::Real),
            (OcValue::String("value".to_owned()), ValueType::Integer),
            (OcValue::String("value".to_owned()), ValueType::Boolean),
            (OcValue::String("value".to_owned()), ValueType::Enum(class)),
        ];

        for (value, want) in cases {
            assert_input_type(sample_to_value(sample(value), want, PATH));
        }
    }

    #[test]
    fn sample_to_value_rejects_native_enum_and_decimal_store_carriers() {
        let class = EnumClassId(4);
        let wants = [
            ValueType::Real,
            ValueType::Integer,
            ValueType::Boolean,
            ValueType::String,
            ValueType::Enum(class),
        ];
        for want in wants {
            assert_input_type(sample_to_value(
                sample(OcValue::Enum {
                    type_iri: "CDL.Types.SimpleController".to_owned(),
                    literal: "PI".to_owned(),
                }),
                want,
                PATH,
            ));
            assert_input_type(sample_to_value(
                sample(OcValue::Decimal("1.25".to_owned())),
                want,
                PATH,
            ));
        }
    }

    #[test]
    fn resolve_store_inputs_rejects_mismatched_handle_count() {
        let store = MismatchedHandleStore::default();
        let mut model = ModelGraph::new();
        model.connectors.push(
            Connector::new(ConnectorId(0), BlockId(0), Dir::In, ValueType::Real, 0).with_iri(PATH),
        );
        let io = IoInventory::build_at_load(&model);

        let err = resolve_store_inputs(&store, &io).unwrap_err();
        match err {
            OcError::Store(StoreError::Validation(detail)) => {
                assert!(detail.contains("0 handles for 1 input points"));
            }
            other => panic!("expected StoreError::Validation, got {other:?}"),
        }
    }

    #[derive(Default)]
    struct MismatchedHandleStore {
        inner: MemStore,
    }

    impl ModelStore for MismatchedHandleStore {
        fn save_model(&self, model: &ResolvedModel) -> StoreResult<()> {
            self.inner.save_model(model)
        }

        fn load_model(&self, model_id: &DomainKey) -> StoreResult<ResolvedModel> {
            self.inner.load_model(model_id)
        }

        fn list_models(&self) -> StoreResult<Vec<DomainKey>> {
            self.inner.list_models()
        }

        fn delete_model(&self, model_id: &DomainKey) -> StoreResult<()> {
            self.inner.delete_model(model_id)
        }
    }

    impl PointStore for MismatchedHandleStore {
        fn resolve_points(&self, keys: &[DomainKey]) -> StoreResult<Vec<PointHandle>> {
            let _ = self.inner.resolve_points(keys)?;
            Ok(Vec::new())
        }

        fn snapshot(&self) -> StoreResult<Box<dyn PointSnapshot>> {
            self.inner.snapshot()
        }

        fn write_points(&self, batch: &[PointWrite]) -> StoreResult<usize> {
            self.inner.write_points(batch)
        }
    }

    impl SemanticStore for MismatchedHandleStore {
        fn upsert_equipment(&self, eq: &EquipmentDto) -> StoreResult<()> {
            self.inner.upsert_equipment(eq)
        }

        fn add_relation(&self, rel: &RelationDto) -> StoreResult<()> {
            self.inner.add_relation(rel)
        }

        fn put_semantic_payload(&self, p: &SemanticPayloadDto) -> StoreResult<()> {
            self.inner.put_semantic_payload(p)
        }

        fn get_semantic_payloads(
            &self,
            subject: &DomainKey,
        ) -> StoreResult<Vec<SemanticPayloadDto>> {
            self.inner.get_semantic_payloads(subject)
        }

        fn point_list(&self, controlled_device: Option<&str>) -> StoreResult<Vec<PointListRow>> {
            self.inner.point_list(controlled_device)
        }

        fn retrieve(&self, q: &SemanticQuery) -> StoreResult<Vec<RetrievalHit>> {
            self.inner.retrieve(q)
        }

        fn match_template(
            &self,
            required_points: &[TemplatePointReq],
        ) -> StoreResult<Vec<DomainKey>> {
            self.inner.match_template(required_points)
        }
    }

    impl Durable for MismatchedHandleStore {
        fn commit(&self) -> StoreResult<()> {
            self.inner.commit()
        }

        fn flush(&self) -> StoreResult<()> {
            self.inner.flush()
        }

        fn recover(&self) -> StoreResult<()> {
            self.inner.recover()
        }
    }
}
