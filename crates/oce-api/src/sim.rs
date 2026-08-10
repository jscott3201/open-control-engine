//! Execution modes (`08` §5) and the post-tick [`Outputs`] snapshot. Both modes drive the *same*
//! loaded model and frozen schedule (`01` §6); they differ only in who owns the time axis and what
//! is collected. [`Engine::simulate`] is an offline horizon loop with a per-timestep trace + timing
//! metrics; [`Engine::step_realtime`] is one host-driven step with a verification report.
//!
//! Timing uses [`std::time::Instant`] (a monotonic timer, never a wall clock — no `Date`/`SystemTime`
//! dependency, no determinism hazard: timing is reported, never fed back into the model).

use std::cell::RefCell;
use std::time::Instant;

use oce_blocks::Diagnostics;
use oce_graph::{RunState, allocate_state};
use oce_model::{ConnectorId, Dir, ModelGraph, Value};
use oce_store::{DomainKey, Durability, OcValue, PointSample, PointStatus, PointWrite, Store};

use crate::engine::Engine;
use crate::error::OcError;
use crate::io::IoInventory;

/// An owned, enumerable snapshot of the model's **output** connector values after a tick (`08`
/// R-API-4a). Refreshed in place each [`Engine::tick`]; no store-backend type ever appears here. The
/// parallel `paths` (captured at load) keys [`Outputs::to_map`] (R-PUB-6).
#[derive(Clone, Debug, Default)]
pub struct Outputs {
    entries: Vec<(ConnectorId, Value)>,
    /// `paths[i]` is the canonical path of `entries[i]` — same length, same order, captured at load.
    paths: Vec<String>,
}

impl Outputs {
    /// Build the snapshot from a flattened model + its allocated state. `entries` and `paths` are
    /// derived from the **same** `connectors.filter(Out)` walk, so they are equal-length and aligned
    /// by construction (the `to_map` keying contract).
    pub(crate) fn build(model: &ModelGraph, state: &RunState) -> Outputs {
        let entries: Vec<(ConnectorId, Value)> = model
            .connectors
            .iter()
            .filter(|c| c.dir == Dir::Out)
            .map(|c| (c.id, state.values[c.id.0 as usize].clone()))
            .collect();
        let paths = crate::engine::out_connector_paths(model);
        debug_assert_eq!(
            entries.len(),
            paths.len(),
            "Outputs entries/paths must align"
        );
        // The ordering `Outputs::get`'s binary search depends on. Load validation rejects any
        // connector whose id differs from its arena index. `Engine::resume` is the other caller and
        // changes parameters only, so it rebuilds from that already-validated connector arena.
        debug_assert!(
            entries.windows(2).all(|w| w[0].0 < w[1].0),
            "Outputs entries must be strictly ascending by ConnectorId"
        );
        Outputs { entries, paths }
    }

    /// Refresh each entry's value from the current state in place (the per-tick update; order and
    /// keys are frozen at load, so `paths` need not change).
    pub(crate) fn refresh_from(&mut self, state: &RunState) {
        for (id, value) in &mut self.entries {
            *value = state.values[id.0 as usize].clone();
        }
    }

    /// The current value of output connector `c`, if it is an output of the loaded model.
    ///
    /// `O(log n)` in the number of outputs, so a host reading many connectors in a loop does not
    /// pay a scan per read. This relies on the entries being ascending by [`ConnectorId`]: they are
    /// built by an in-order `connectors.filter(Out)` walk over the connector arena validated at
    /// load. [`Engine::resume`] can rebuild the snapshot after parameter changes but cannot change
    /// connector ids. The invariant is also pinned by `entries_are_strictly_ascending_by_connector_id`
    /// under both build profiles.
    #[must_use]
    pub fn get(&self, c: ConnectorId) -> Option<&Value> {
        self.entries
            .binary_search_by_key(&c, |(id, _)| *id)
            .ok()
            .map(|i| &self.entries[i].1)
    }

    /// Iterate `(connector, value)` for every output connector, in declaration order.
    pub fn iter(&self) -> impl Iterator<Item = (ConnectorId, &Value)> {
        self.entries.iter().map(|(id, v)| (*id, v))
    }

    /// Owned snapshot of `(path, value)` for every output connector (R-PUB-6; a PyO3 binder copies
    /// the post-tick values without holding the borrow). Keyed by canonical point path.
    #[must_use]
    pub fn to_map(&self) -> Vec<(String, Value)> {
        self.paths
            .iter()
            .cloned()
            .zip(self.entries.iter().map(|(_, v)| v.clone()))
            .collect()
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

/// A simulation horizon descriptor (`08` §5.1). Pure data; deterministic input to [`Engine::simulate`].
/// **Not `Default`** — a zero step is a footgun (it would divide-by-zero / loop unbounded), so the
/// host must supply the horizon explicitly; `simulate` validates `step > 0` and returns a typed
/// error otherwise (never panics).
#[derive(Debug)]
pub struct SimSpec {
    /// First model time evaluated (seconds).
    pub t_start: f64,
    /// Last model time evaluated (seconds); included iff it lands on the `t_start + k*step` grid.
    pub t_stop: f64,
    /// Fixed host step (seconds); validated finite and `> 0` by `simulate`.
    pub step: f64,
    /// Where staged input values come from each step.
    pub inputs: InputSource,
    /// Which output connectors to record, and at what stride.
    pub collect: CollectSpec,
}

/// Source of staged inputs for each simulation step (`08` §5.1). `None`/`Constant`/`Closure` are
/// live; `Csv` is frozen-as-variant but deferred behind a typed error. `#[non_exhaustive]` so a
/// future table-backed variant is additive. NOT `Clone`/`Debug`-derivable (the `Closure` variant
/// holds a boxed `Fn`); a manual [`std::fmt::Debug`] prints the variant name only.
#[non_exhaustive]
pub enum InputSource {
    /// No external inputs staged; the model runs on its own sources/state (the autonomous case).
    None,
    /// Stage a fixed `(point, value)` set on every step (e.g. a held setpoint sweep entry).
    Constant(Vec<(String, Value)>),
    /// Host closure mapping model time → staged `(point, value)` pairs. `Fn` (not `FnMut`) so it is
    /// callable through the frozen `&SimSpec` borrow; `Send + Sync` so `SimSpec` stays thread-safe.
    Closure(Box<dyn Fn(f64) -> Vec<(String, Value)> + Send + Sync>),
    /// Deferred: a CombiTimeTable-style CSV of `t, col0, col1, …` bound to named inputs.
    /// Frozen as a variant now; `simulate` returns a typed [`OcError::Load`] for it.
    Csv {
        /// Path to the CSV file.
        path: std::path::PathBuf,
        /// `(input point, CSV column index)` bindings.
        bindings: Vec<(String, usize)>,
    },
}

impl std::fmt::Debug for InputSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InputSource::None => f.write_str("InputSource::None"),
            InputSource::Constant(v) => f.debug_tuple("InputSource::Constant").field(v).finish(),
            InputSource::Closure(_) => f.write_str("InputSource::Closure(<fn>)"),
            InputSource::Csv { path, bindings } => f
                .debug_struct("InputSource::Csv")
                .field("path", path)
                .field("bindings", bindings)
                .finish(),
        }
    }
}

/// Selects which outputs the trace records and how often (`08` §5.1, req 5.1.5).
#[derive(Clone, Debug)]
pub enum CollectSpec {
    /// Record nothing (timing-only run; `SimMetrics.trace` is empty).
    None,
    /// Record every output connector, every `stride` steps (`stride` is clamped to `>= 1`).
    All {
        /// Recording stride (steps between recorded rows).
        stride: usize,
    },
    /// Record only the named outputs, every `stride` steps.
    Named {
        /// The output point paths to record: connector paths or root-declared boundary-output
        /// IRIs (read aliases for their driving connectors). The recorded column echoes the
        /// supplied name either way.
        points: Vec<String>,
        /// Recording stride (steps between recorded rows).
        stride: usize,
    },
}

/// The recording stride of a [`CollectSpec`] (clamped to `>= 1` by the caller).
fn collect_stride(c: &CollectSpec) -> usize {
    match c {
        CollectSpec::None => 1,
        CollectSpec::All { stride } | CollectSpec::Named { stride, .. } => *stride,
    }
}

/// Per-timestep recorded outputs from a [`Engine::simulate`] run (`08` §5.1 req 5.1.5). PINNED
/// column-oriented dense table for bit-reproducibility: `times()[i]` is the model time of row `i`;
/// `column(j)[i]` is the value of `columns()[j]` at `times()[i]`. Rectangular invariant: every
/// column has length `times().len()` (enforced by `push_row`).
#[derive(Clone, Debug, Default)]
pub struct OutputTrace {
    columns: Vec<String>,
    times: Vec<f64>,
    series: Vec<Vec<Value>>,
}

impl OutputTrace {
    /// An empty trace with the given recorded-column paths (one empty series per column).
    pub(crate) fn with_columns(columns: Vec<String>) -> Self {
        let series = vec![Vec::new(); columns.len()];
        Self {
            columns,
            times: Vec::new(),
            series,
        }
    }

    /// Push one rectangular row: `cols` is parallel to `columns` (`(path, ConnectorId)`), `values`
    /// is the live connector array. `cid` is inventory-sourced (an in-range model connector), so the
    /// index never goes out of bounds (R-ERR-1).
    pub(crate) fn push_row(&mut self, t: f64, cols: &[(String, ConnectorId)], values: &[Value]) {
        self.times.push(t);
        for (j, (_, cid)) in cols.iter().enumerate() {
            self.series[j].push(values[cid.0 as usize].clone());
        }
        debug_assert!(self.series.iter().all(|s| s.len() == self.times.len()));
    }

    /// The recorded column paths, in resolution order (stable across identical specs).
    #[must_use]
    pub fn columns(&self) -> &[String] {
        &self.columns
    }

    /// The model time (seconds) of each recorded row, increasing.
    #[must_use]
    pub fn times(&self) -> &[f64] {
        &self.times
    }

    /// The recorded value column for `columns()[j]`, if `j` is in range.
    #[must_use]
    pub fn column(&self, j: usize) -> Option<&[Value]> {
        self.series.get(j).map(Vec::as_slice)
    }

    /// Number of recorded rows.
    #[must_use]
    pub fn rows(&self) -> usize {
        self.times.len()
    }
}

/// Timing + trace results of a [`Engine::simulate`] run (`08` §5.1). `Default` = the empty/zero run.
#[derive(Clone, Debug, Default)]
pub struct SimMetrics {
    /// Number of ticks evaluated over the horizon.
    pub ticks: u64,
    /// Wall time from input preflight through the state re-seed, final tick and any final trace row
    /// (nanoseconds; monotonic `Instant` elapsed, saturating). Recorded-column resolution is
    /// excluded.
    pub wall_nanos: u64,
    /// Worst single-tick latency over the run (nanoseconds) — the jitter signal (perf §8, R-SIM-4).
    pub max_tick_nanos: u64,
    /// Per-timestep recorded outputs (req 5.1.5); empty when `CollectSpec::None`.
    pub trace: OutputTrace,
}

/// Result of one real-time / batch verification step (`08` §5.2). `Clone` (a PyO3 binder owns it).
#[derive(Clone, Debug, Default)]
pub struct StepReport {
    /// `Assert`-block trips this step (CDL assertion sinks; 07 verification funnel). Data, never a
    /// panic.
    pub asserts: Vec<AssertEvent>,
    /// Points committed through the store this step (`06` `write_points`).
    pub written: usize,
    /// This step's single-tick latency (nanoseconds; monotonic `Instant`, saturating).
    pub tick_nanos: u64,
}

/// Convert model seconds relative to a host epoch into a UNIX-nanosecond instant.
///
/// Model seconds are rounded to the nearest nanosecond. A non-finite offset or final instant outside
/// `u64` fails with [`OcError::RealtimeInstantUnrepresentable`]. The range checks and integer
/// addition are explicit, so input never clamps, wraps, saturates, or panics.
fn model_time_to_unix_nanos(epoch_unix_nanos: u64, t_now: f64) -> Result<u64, OcError> {
    let fail = || OcError::RealtimeInstantUnrepresentable {
        epoch_unix_nanos,
        t_now,
    };
    let offset = (t_now * 1_000_000_000.0).round();
    let integer_limit = u64::MAX as f64;
    if !offset.is_finite() || offset < -integer_limit || offset > integer_limit {
        return Err(fail());
    }
    let instant = i128::from(epoch_unix_nanos)
        .checked_add(offset as i128)
        .ok_or_else(&fail)?;
    u64::try_from(instant).map_err(|_| fail())
}

/// Convert an engine output into its store-facing runtime carrier.
///
/// Unlike `projection::value_to_oc_value`, enum outputs use `OcValue::Int` so they round-trip
/// through `engine::sample_to_value`. String is retained for totality, but is unreachable from
/// [`DurableOutputBatch::refresh`] because `io::point_rows_at_load` excludes String points. Never
/// panics.
pub(crate) fn output_value_to_oc_value(value: &Value) -> OcValue {
    match value {
        Value::Real(value) => OcValue::Real(*value),
        Value::Integer(value) => OcValue::Int(*value),
        Value::Boolean(value) => OcValue::Bool(*value),
        Value::String(value) => OcValue::String(value.to_string()),
        Value::Enum { ordinal, .. } => OcValue::Int(i64::from(*ordinal)),
    }
}

/// The pre-built durable store batch for [`Engine::step_realtime`]: point identity is resolved
/// once at load, and each step rewrites only the value + timestamp of every row in place, so a
/// warm step allocates nothing on the write path (issue #242 slice 1).
///
/// `writes` and `connectors` are parallel vectors aligned by index. Two vectors are forced by
/// the [`DurableOutputBatch::writes`] `&[PointWrite]` signature (`write_points` takes that
/// slice, so the rows must already be contiguous `PointWrite`s); both builders therefore assert
/// the length invariant, mirroring the `Outputs::build` entries/paths assert.
///
/// # Invariants
/// - Rows are minted from [`IoInventory::durable_columns`] semantics only — never from
///   `trace_columns` and never from the declared boundary-output alias map (`_spec/18` D3:
///   an alias in the durable batch would double-write each aliased sample).
/// - Pre-refresh rows are placeholders (`OcValue::Bool(false)` at instant 0):
///   [`DurableOutputBatch::writes`] is meaningful only after a [`DurableOutputBatch::refresh`].
///   `step_realtime`, the only production caller, always refreshes first, so a placeholder is
///   never committed.
/// - The batch is valid only for the model whose [`IoInventory`] minted it: `refresh` indexes
///   the live value arena by the cached connector ids, so `step_realtime`'s panic-freedom
///   depends on `Engine::build_model_in_memory` re-minting this batch alongside `io` at every
///   load. Samples carry the exact supplied UNIX-nanosecond timestamp, `Ok` status, and
///   Telemetry durability.
#[derive(Debug, Default)]
pub(crate) struct DurableOutputBatch {
    writes: Vec<PointWrite>,
    connectors: Vec<ConnectorId>,
}

impl DurableOutputBatch {
    /// Resolve the durable key set once, minting one placeholder row per output point.
    pub(crate) fn build_at_load(io: &IoInventory) -> DurableOutputBatch {
        let columns = io.durable_columns();
        let mut writes = Vec::with_capacity(columns.len());
        let mut connectors = Vec::with_capacity(columns.len());
        for (path, connector_id) in columns {
            writes.push(PointWrite {
                key: DomainKey::new(path),
                sample: PointSample {
                    value: OcValue::Bool(false),
                    status: PointStatus::Ok,
                    at_unix_nanos: 0,
                },
                durability: Durability::Telemetry,
            });
            connectors.push(connector_id);
        }
        debug_assert_eq!(
            writes.len(),
            connectors.len(),
            "DurableOutputBatch writes/connectors must align"
        );
        DurableOutputBatch { writes, connectors }
    }

    /// Rewrite every row's sample in place from the live connector values — identity (key,
    /// durability, row order) is untouched. `values` is indexed by cached [`ConnectorId`],
    /// never by column position; ids are inventory-sourced and in range for the loaded model.
    pub(crate) fn refresh(&mut self, values: &[Value], at_unix_nanos: u64) {
        debug_assert_eq!(
            self.writes.len(),
            self.connectors.len(),
            "DurableOutputBatch writes/connectors must align"
        );
        for (write, connector_id) in self.writes.iter_mut().zip(&self.connectors) {
            write.sample = PointSample {
                value: output_value_to_oc_value(&values[connector_id.0 as usize]),
                status: PointStatus::Ok,
                at_unix_nanos,
            };
        }
    }

    /// The batch rows in durable-column order — the exact `write_points` argument.
    pub(crate) fn writes(&self) -> &[PointWrite] {
        &self.writes
    }
}

/// A single tripped CDL `Assert` block, surfaced for the verification report (`08` §5.2, R-RT-4).
/// Owned `String` fields only (R-API-8).
#[derive(Clone, Debug)]
pub struct AssertEvent {
    /// Dotted instance path of the `Assert` block that tripped.
    pub block: String,
    /// The assertion message (CDL `Assert` `message` parameter).
    pub message: String,
    /// Model time (seconds) at which it tripped.
    pub t: f64,
    /// Severity, mirroring Modelica `AssertionLevel`.
    pub level: AssertLevel,
}

/// CDL assertion severity (mirrors Modelica `AssertionLevel`). Default = `Error`.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum AssertLevel {
    /// Advisory; the run continues.
    Warning,
    /// A verification failure (the default).
    #[default]
    Error,
}

#[derive(Default)]
struct AssertCollector {
    events: RefCell<Vec<AssertEvent>>,
}

impl Diagnostics for AssertCollector {
    fn warn(&self, source: &str, message: &str, t: f64) {
        self.events.borrow_mut().push(AssertEvent {
            block: source.to_string(),
            message: message.to_string(),
            t,
            level: AssertLevel::Warning,
        });
    }
}

impl<S: Store> Engine<S> {
    /// Set the host wall-clock epoch corresponding to model time `t = 0`.
    ///
    /// [`Engine::step_realtime`] uses this additive mapping to timestamp computed point samples
    /// without consulting a wall clock. It must be called before real-time stepping.
    pub fn set_realtime_epoch_unix_nanos(&mut self, at_unix_nanos: u64) {
        self.realtime_epoch_unix_nanos = Some(at_unix_nanos);
    }

    /// Return the host wall-clock epoch corresponding to model time `t = 0`, if configured.
    #[must_use]
    pub fn realtime_epoch_unix_nanos(&self) -> Option<u64> {
        self.realtime_epoch_unix_nanos
    }

    /// Run a full horizon, collecting a per-timestep trace + timing metrics (`08` §5.1). A tight,
    /// deterministic loop over [`Engine::tick`]: identical `SimSpec` + params + **entry connector
    /// image** ⇒ identical [`OutputTrace`] (CDL §7.16, FRAME §1, R-SIM-2).
    ///
    /// The entry connector image is a determinant not derivable from the spec or the params: the
    /// values in the connector arena when the call begins carry into the horizon. `InputSource`
    /// overwrites the slots it names on every step; the rest are whatever the arena already held.
    /// Pinned by `tests::sim_tests::an_undriven_input_inherits_whatever_the_entry_image_holds`.
    ///
    /// A model with store-bound inputs takes one more. [`Engine::tick`] re-stages those points from
    /// the store on every tick, so two runs agreeing on spec, params and entry image still diverge
    /// when the store's samples differ — see `tests::sim_tests::store_bound_staging`. The list above
    /// is not the whole determinant set for such a model.
    ///
    /// Horizon: `n = floor((t_stop - t_start) / step)` samples at `t = t_start + k*step` for
    /// `k ∈ 0..=n` — a **fresh multiply** each step (no accumulating float error), so the time axis
    /// is bit-reproducible. `t_stop` is included iff it lands on that grid. The engine is left at
    /// the horizon end on return.
    ///
    /// **A call that returns `Ok` is a run restart, not a continuation.** After preflight succeeds,
    /// entry resets the run clock (`prev_t = None`) so a prior real-time tick never poisons the
    /// horizon, then re-seeds the `[S]` state words via [`allocate_state`]. `values` is left alone,
    /// so staged inputs survive (`tests::sim_tests::
    /// a_staged_input_still_reaches_the_horizon_after_the_reseed`) while integrators, timers,
    /// latches and filters do not. Restarting has consequences for a host: see
    /// `docs/host-responsibilities.md`.
    ///
    /// **A preflight refusal preserves the prior run.** Recorded columns, a `Constant` list, and the
    /// first list returned by a `Closure` are resolved and type-checked before the clock or state
    /// words reset. The closure is called exactly once at the first computed grid time; its
    /// validated plan is reused for the first tick. A closure refusal after completed ticks leaves
    /// those ticks in effect, so `simulate` is not transactional once execution has begun.
    ///
    /// # Errors
    /// [`OcError::Load`] if `step` is non-finite/`<= 0`, or `inputs` is the deferred `Csv` variant;
    /// [`OcError::NonFiniteTime`] if `t_start`/`t_stop` is non-finite; [`OcError::TimeRegression`] if
    /// `t_stop < t_start`; [`OcError::UnknownPoint`] if `CollectSpec::Named` or an input source names
    /// an unknown point; [`OcError::InputType`] if an input value has the wrong type; and
    /// [`OcError::Store`] if store-backed input staging fails. Collection, `Constant`, and
    /// first-closure-list failures are preflight refusals — no restart or tick occurs. Store-backed
    /// input failures occur after the restart but before that tick evaluates. Never panics
    /// (R-ERR-1).
    pub fn simulate(&mut self, spec: &SimSpec) -> Result<SimMetrics, OcError> {
        if !spec.step.is_finite() || spec.step <= 0.0 {
            return Err(OcError::Load {
                detail: format!("SimSpec.step must be finite and > 0 (got {})", spec.step),
            });
        }
        if !spec.t_start.is_finite() || !spec.t_stop.is_finite() {
            let now = if spec.t_start.is_finite() {
                spec.t_stop
            } else {
                spec.t_start
            };
            return Err(OcError::NonFiniteTime { now });
        }
        if spec.t_stop < spec.t_start {
            return Err(OcError::TimeRegression {
                now: spec.t_stop,
                prev: spec.t_start,
            });
        }
        if let InputSource::Csv { .. } = &spec.inputs {
            return Err(OcError::Load {
                detail: "CSV InputSource is deferred to M2".to_string(),
            });
        }
        // Resolve recorded columns ONCE before any tick — an unknown name fails fast with no
        // partial trace and no advanced model state.
        let cols = self.resolve_collect(&spec.collect)?;
        let stride = collect_stride(&spec.collect).max(1) as u64;
        let run_start = Instant::now();
        let staged_inputs = self.resolve_constant_inputs(&spec.inputs)?;
        let first_t = spec.t_start + 0.0 * spec.step;
        let mut first_closure_inputs = match &spec.inputs {
            InputSource::Closure(f) => Some(self.resolve_input_pairs(&f(first_t))?),
            InputSource::None | InputSource::Constant(_) | InputSource::Csv { .. } => None,
        };
        let n = (((spec.t_stop - spec.t_start) / spec.step).floor() as i64).max(0) as u64;
        let last_t = spec.t_start + (n as f64) * spec.step;
        let fresh_state = allocate_state(&self.model, &self.blocks);
        for slot in &fresh_state.slots {
            let block = &self.blocks[slot.block.0 as usize];
            let end = slot.offset + slot.len;
            if !block.simulation_time_is_representable(
                first_t,
                last_t,
                &fresh_state.words[slot.offset..end],
            ) {
                return Err(OcError::ModelTimeUnrepresentable { now: last_t });
            }
        }
        // All input lists available before the first tick have now passed whole-list validation.
        // Nothing else can refuse before the restart.
        // Fresh time axis (R-SIM-2): isolate from any prior real-time tick's prev_t.
        self.durable_restore_ready = false;
        self.prev_t = None;
        // Fresh `[S]` state (R-SIM-2): without this, a reused engine started the horizon from the
        // words the previous run left behind.
        //
        // `words` only. `allocate_state` also re-seeds `values`, which would discard the host's
        // staged input image and contradict [`Engine::set_input`]'s "before the next tick" contract
        // (`sim_tests::a_staged_input_still_reaches_the_horizon_after_the_reseed`). What carries in
        // `values` is the documented limit, not an oversight — see the rustdoc above.
        //
        // Below all first-tick name-resolution gates, so a refusal returns without re-seeding
        // (`sim_tests::a_refused_name_resolution_does_not_reseed_state_words`).
        self.state.words = fresh_state.words;
        let mut trace = OutputTrace::with_columns(cols.iter().map(|(p, _)| p.clone()).collect());
        let mut metrics = SimMetrics::default();
        for k in 0..=n {
            let t = if k == 0 {
                first_t
            } else {
                spec.t_start + (k as f64) * spec.step // fresh multiply — no float accumulator
            };
            if let Some(first) = first_closure_inputs.take() {
                self.stage_resolved_inputs(&first);
            } else {
                self.stage_inputs(&spec.inputs, &staged_inputs, t)?;
            }
            let t0 = Instant::now();
            self.tick(t)?;
            let dt = t0.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
            metrics.ticks += 1;
            metrics.max_tick_nanos = metrics.max_tick_nanos.max(dt);
            // Record only when there is something to collect — a timing-only run (`CollectSpec::None`,
            // empty `cols`) leaves `times`/`series` empty, honoring the "trace is empty" contract.
            if !cols.is_empty() && k % stride == 0 {
                trace.push_row(t, &cols, &self.state.values);
            }
        }
        metrics.wall_nanos = run_start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
        metrics.trace = trace;
        Ok(metrics)
    }

    /// One real-time / batch verification step (`08` §5.2): advance one tick at the host's `t_now`,
    /// then batch-write the resulting projected output-point state through the store (off the
    /// read). Sample timestamps use the host-supplied model-time epoch configured by
    /// [`Engine::set_realtime_epoch_unix_nanos`]. Returns any tripped `Assert` diagnostics.
    ///
    /// # Errors
    /// Propagates [`Engine::tick`]'s time guards ([`OcError::NonFiniteTime`] /
    /// [`OcError::TimeRegression`]) and [`OcError::Store`] from the batched write.
    /// [`OcError::RealtimeEpochUnset`] is returned before ticking when the host has not configured
    /// an epoch. [`OcError::RealtimeInstantUnrepresentable`] is returned before ticking when the
    /// epoch plus `t_now` seconds is not exactly representable as a `u64` UNIX-nanosecond instant.
    /// A failed store write leaves the completed tick in effect: model time and outputs have already
    /// advanced and are not rolled back. Never panics.
    pub fn step_realtime(&mut self, t_now: f64) -> Result<StepReport, OcError> {
        let epoch_unix_nanos = self
            .realtime_epoch_unix_nanos
            .ok_or(OcError::RealtimeEpochUnset)?;
        let at_unix_nanos = model_time_to_unix_nanos(epoch_unix_nanos, t_now)?;
        let t0 = Instant::now();
        let collector = AssertCollector::default();
        self.tick_with(t_now, &collector)?;
        let tick_nanos = t0.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
        self.durable_batch
            .refresh(&self.state.values, at_unix_nanos);
        let written = self.store.write_points(self.durable_batch.writes())?;
        Ok(StepReport {
            asserts: collector.events.into_inner(),
            written,
            tick_nanos,
        })
    }

    /// Stage an external input value before the next tick (the host's view of a physical/logical
    /// input connector). Resolved by point path through the IO inventory; validated against the
    /// connector type (no coercion, `01` §5 invariant 3). The declared boundary-output alias
    /// space is output-only: a declared output name is never accepted here.
    ///
    /// # Errors
    /// [`OcError::UnknownPoint`] if `point` is not an input in the inventory; [`OcError::InputType`]
    /// if `value`'s type does not match the connector. Never panics (R-ERR-1).
    pub fn set_input(&mut self, point: &str, value: Value) -> Result<(), OcError> {
        let connectors = self
            .io
            .resolve_inputs(point)
            .ok_or_else(|| OcError::UnknownPoint(point.to_string()))?;
        for &cid in connectors {
            // `cid` is inventory-sourced (an in-range model connector) — never a host integer.
            let want = self.model.connectors[cid.0 as usize].value_type;
            if value.value_type() != want {
                return Err(OcError::InputType(point.to_string()));
            }
        }
        for &cid in connectors {
            self.durable_restore_ready = false;
            self.state.values[cid.0 as usize] = value.clone();
        }
        Ok(())
    }

    /// Read an output value by point path after a tick (host-facing). `point` is an output
    /// connector path or a root-declared boundary-output IRI; the declared spelling is a read
    /// alias for its driving connector's slot, so both keys return bit-equal values.
    ///
    /// # Errors
    /// [`OcError::UnknownPoint`] if `point` resolves to no output point. Never panics
    /// (R-ERR-1).
    pub fn get_output(&self, point: &str) -> Result<Value, OcError> {
        let cid = self
            .io
            .resolve_output(point)
            .ok_or_else(|| OcError::UnknownPoint(point.to_string()))?;
        Ok(self.state.values[cid.0 as usize].clone())
    }

    /// Resolve an [`InputSource`]'s staging targets once, before the horizon runs (the `simulate`
    /// helper). Every other variant resolves nothing and yields an empty plan.
    ///
    /// Only [`InputSource::Constant`] can be pre-resolved for the whole horizon: its `(point, value)`
    /// pairs are fixed, so the per-step point-path hash lookup and type check they used to cost are
    /// hoisted here. A [`InputSource::Closure`] may name different points at each `t`; only its first
    /// returned list is preflighted by [`Engine::simulate`].
    ///
    /// Each name flattens to **every** connector it stages — a composite boundary input fans one
    /// host point out to several internal connectors — in list order, so two pairs naming the same
    /// point stay last-wins when the plan is written.
    ///
    /// # Errors
    /// [`OcError::UnknownPoint`] if a `Constant` name is not an input in the inventory;
    /// [`OcError::InputType`] if a value's type does not match the connector it would stage. Either
    /// refusal discards the whole plan, so a run that fails here stages nothing at all — including
    /// the pairs preceding the failing one, which per-name staging used to write before refusing.
    /// Never panics (R-ERR-1).
    pub(crate) fn resolve_constant_inputs(
        &self,
        inputs: &InputSource,
    ) -> Result<Vec<(ConnectorId, Value)>, OcError> {
        let InputSource::Constant(pairs) = inputs else {
            return Ok(Vec::new());
        };
        self.resolve_input_pairs(pairs)
    }

    /// Resolve and type-check one complete input list without staging any value.
    fn resolve_input_pairs(
        &self,
        pairs: &[(String, Value)],
    ) -> Result<Vec<(ConnectorId, Value)>, OcError> {
        let mut staged = Vec::with_capacity(pairs.len());
        for (name, value) in pairs {
            let connectors = self
                .io
                .resolve_inputs(name)
                .ok_or_else(|| OcError::UnknownPoint(name.to_string()))?;
            for &cid in connectors {
                // `cid` is inventory-sourced (an in-range model connector) — never a host integer.
                if value.value_type() != self.model.connectors[cid.0 as usize].value_type {
                    return Err(OcError::InputType(name.to_string()));
                }
                staged.push((cid, value.clone()));
            }
        }
        Ok(staged)
    }

    /// Stage a plan whose names and types were checked as one complete list. `staged` must come from
    /// [`Engine::resolve_input_pairs`], which guarantees every connector id indexes the loaded value
    /// arena and every value matches its connector type.
    fn stage_resolved_inputs(&mut self, staged: &[(ConnectorId, Value)]) {
        for (cid, value) in staged {
            self.state.values[cid.0 as usize] = value.clone();
        }
    }

    /// Stage the inputs for one simulation step from an [`InputSource`] (the `simulate` helper).
    ///
    /// `staged` is the [`Engine::resolve_constant_inputs`] plan for `inputs`, resolved once per
    /// run. It is written on **every** step, at the same cadence as the per-name staging it
    /// replaces: only the resolution is hoisted, never the write.
    pub(crate) fn stage_inputs(
        &mut self,
        inputs: &InputSource,
        staged: &[(ConnectorId, Value)],
        t: f64,
    ) -> Result<(), OcError> {
        match inputs {
            InputSource::None => Ok(()),
            InputSource::Constant(_) => {
                self.stage_resolved_inputs(staged);
                Ok(())
            }
            InputSource::Closure(f) => {
                for (name, v) in f(t) {
                    self.set_input(&name, v)?;
                }
                Ok(())
            }
            InputSource::Csv { .. } => Err(OcError::Load {
                detail: "CSV InputSource is deferred to M2".to_string(),
            }),
        }
    }

    /// Resolve a [`CollectSpec`] to the `(path, ConnectorId)` columns to record. `Named` names that
    /// are not OUTPUT connectors in the inventory are an [`OcError::UnknownPoint`] (fail-fast, before
    /// any tick).
    pub(crate) fn resolve_collect(
        &self,
        c: &CollectSpec,
    ) -> Result<Vec<(String, ConnectorId)>, OcError> {
        match c {
            CollectSpec::None => Ok(Vec::new()),
            CollectSpec::All { .. } => Ok(self.io.trace_columns()),
            CollectSpec::Named { points, .. } => points
                .iter()
                .map(|name| {
                    self.io
                        .resolve_output(name)
                        .map(|cid| (name.clone(), cid))
                        .ok_or_else(|| OcError::UnknownPoint(name.clone()))
                })
                .collect(),
        }
    }
}
