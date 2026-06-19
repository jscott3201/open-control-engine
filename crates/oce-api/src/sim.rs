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
use oce_graph::RunState;
use oce_model::{ConnectorId, Dir, ModelGraph, Value};
use oce_store::Store;

use crate::engine::Engine;
use crate::error::OcError;

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
    /// by construction (the `to_map` keying contract; critic M1/M2).
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
    #[must_use]
    pub fn get(&self, c: ConnectorId) -> Option<&Value> {
        self.entries.iter().find(|(id, _)| *id == c).map(|(_, v)| v)
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
/// live in M1; `Csv` is frozen-as-variant but deferred (a typed error). `#[non_exhaustive]` so a
/// future `CombiTimeTable` variant is additive. NOT `Clone`/`Debug`-derivable (the `Closure` variant
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
    /// DEFERRED (M2): a CombiTimeTable-style CSV of `t, col0, col1, …` bound to named inputs.
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
        /// The output point paths to record.
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
    /// Total wall time over the run (nanoseconds; monotonic `Instant` elapsed, saturating).
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
    /// panic. **Empty in M1** (the Assert sink lands in M2).
    pub asserts: Vec<AssertEvent>,
    /// Points committed through the store this step (`06` `write_points`). `0` with `MemStore` in M1.
    pub written: usize,
    /// This step's single-tick latency (nanoseconds; monotonic `Instant`, saturating).
    pub tick_nanos: u64,
}

/// A single tripped CDL `Assert` block, surfaced for the verification report (`08` §5.2, R-RT-4).
/// Owned `String` fields only (R-API-8). Frozen now; M1 always emits an empty `asserts` vec.
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
            level: AssertLevel::default(),
        });
    }
}

impl<S: Store> Engine<S> {
    /// Run a full horizon, collecting a per-timestep trace + timing metrics (`08` §5.1). A tight,
    /// deterministic loop over [`Engine::tick`]: identical `SimSpec` + params ⇒ identical
    /// [`OutputTrace`] (CDL §7.16, FRAME §1, R-SIM-2).
    ///
    /// Horizon: `n = floor((t_stop - t_start) / step)` samples at `t = t_start + k*step` for
    /// `k ∈ 0..=n` — a **fresh multiply** each step (no accumulating float error), so the time axis
    /// is bit-reproducible. `t_stop` is included iff it lands on that grid. The engine's run clock is
    /// reset (`prev_t = None`) at entry so a prior real-time tick never poisons the horizon, and the
    /// engine is left at the horizon end on return.
    ///
    /// # Errors
    /// [`OcError::Load`] if `step` is non-finite/`<= 0`, or `inputs` is the deferred `Csv` variant;
    /// [`OcError::NonFiniteTime`] if `t_start`/`t_stop` is non-finite; [`OcError::TimeRegression`] if
    /// `t_stop < t_start`; [`OcError::UnknownPoint`] if `CollectSpec::Named` names an unknown output
    /// (fail-fast — no tick runs). Never panics (R-ERR-1).
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
        // Fresh time axis (R-SIM-2): isolate from any prior real-time tick's prev_t.
        self.prev_t = None;
        let mut trace = OutputTrace::with_columns(cols.iter().map(|(p, _)| p.clone()).collect());
        let mut metrics = SimMetrics::default();
        let run_start = Instant::now();
        let n = (((spec.t_stop - spec.t_start) / spec.step).floor() as i64).max(0) as u64;
        for k in 0..=n {
            let t = spec.t_start + (k as f64) * spec.step; // fresh multiply — no float accumulator
            self.stage_inputs(&spec.inputs, t)?;
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
    /// then batch-write the resulting point state through the store (off the read). Returns any
    /// tripped `Assert` diagnostics (empty in M1).
    ///
    /// # Errors
    /// Propagates [`Engine::tick`]'s time guards ([`OcError::NonFiniteTime`] /
    /// [`OcError::TimeRegression`]) and [`OcError::Store`] from the batched write. Never panics.
    pub fn step_realtime(&mut self, t_now: f64) -> Result<StepReport, OcError> {
        let t0 = Instant::now();
        let collector = AssertCollector::default();
        self.tick_with(t_now, &collector)?;
        let tick_nanos = t0.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
        // M1 stages no store-backed point projection yet, so the batched write is empty (0 written);
        // the seam is exercised so the contract is real. The annotation fixes `&[]` inference.
        let written = self.store.write_points(&[] as &[oce_store::PointWrite])?;
        Ok(StepReport {
            asserts: collector.events.into_inner(),
            written,
            tick_nanos,
        })
    }

    /// Stage an external input value before the next tick (the host's view of a physical/logical
    /// input connector). Resolved by point path through the IO inventory; validated against the
    /// connector type (no coercion, `01` §5 invariant 3).
    ///
    /// # Errors
    /// [`OcError::UnknownPoint`] if `point` is not an input in the inventory; [`OcError::InputType`]
    /// if `value`'s type does not match the connector. Never panics (R-ERR-1).
    pub fn set_input(&mut self, point: &str, value: Value) -> Result<(), OcError> {
        let cid = self
            .io
            .resolve_input(point)
            .ok_or_else(|| OcError::UnknownPoint(point.to_string()))?;
        // `cid` is inventory-sourced (an in-range model connector) — never a host integer.
        let want = self.model.connectors[cid.0 as usize].value_type;
        if value.value_type() != want {
            return Err(OcError::InputType(point.to_string()));
        }
        self.state.values[cid.0 as usize] = value;
        Ok(())
    }

    /// Read an output connector value by point path after a tick (host-facing).
    ///
    /// # Errors
    /// [`OcError::UnknownPoint`] if `point` is not an OUTPUT connector in the inventory. Never
    /// panics (R-ERR-1).
    pub fn get_output(&self, point: &str) -> Result<Value, OcError> {
        let cid = self
            .io
            .resolve_output(point)
            .ok_or_else(|| OcError::UnknownPoint(point.to_string()))?;
        Ok(self.state.values[cid.0 as usize].clone())
    }

    /// Stage the inputs for one simulation step from an [`InputSource`] (the `simulate` helper).
    pub(crate) fn stage_inputs(&mut self, inputs: &InputSource, t: f64) -> Result<(), OcError> {
        match inputs {
            InputSource::None => Ok(()),
            InputSource::Constant(pairs) => {
                for (name, v) in pairs {
                    self.set_input(name, v.clone())?;
                }
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
            CollectSpec::All { .. } => Ok(self.io.out_columns()),
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
