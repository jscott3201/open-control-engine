//! Running a vector set and issuing its receipt (vectors note, "Binding and running", "After a
//! scenario's run"; decisions 3, 11, 15, 19, 26 and 31 to 33).
//!
//! [`run_vectors`] binds the set to its subject once ([`crate::bind`], which also tries every
//! scenario's parameter edits, so `param-refused` is found before any frame runs), then runs each
//! scenario on a **freshly loaded engine** (decision 15): load, apply the scenario's parameter
//! edits ([`crate::parameters`]), then one [`Replay`] over a [`StepSource`] built with the horizon as an
//! instant, on `TimeBase::Grid { t0: 0, step: step_s }`, with replay's defaults
//! (`MissingPolicy::Refuse`, `OnRefusal::Stop`, `DuplicateTime::Refuse`,
//! `NonFiniteInputPolicy::Refuse`, outputs only), driven by `Replay::advance(chunk_frames)`. The
//! horizon instant makes the grid run to the horizon when no input changes late, and the load
//! rule that no step is after the horizon stops it running past: every scenario runs exactly the
//! clock's `N + 1` ticks unless a frame is refused.
//!
//! **After a scenario's run, in this order** (decisions 11 and 26):
//!
//! 1. `Progress::Stopped` with an `EngineRefusal::Run` cause is the error `run-refused`.
//! 2. `Progress::Stopped` with any other engine cause fails the scenario with one `frame-refused`
//!    failure at the refused tick; instants before it are compared and counted, later ones are
//!    not, and coverage is not checked.
//! 3. Otherwise every tick ran: `Trace::check_coverage` over the windows, where a miss is the
//!    error `window-uncovered`.
//! 4. Every tick in every window and every diagnostic instant is compared ([`crate::compare`]).
//!
//! A replay that ends in an error is `replay-failed` with its cause. Chunk sizes never change a
//! trace, so they never change a receipt byte.

use oce_api::{Engine, InputDefinition, ValueType};
use oce_replay::{
    ColumnKind, ColumnMap, ColumnSpec, EngineRefusal, FrameStatus, InputMap, MissingPolicy,
    Progress, RefusalCause, Replay, ReplayError, ReplayOptions, StepSource, TimeBase, Trace,
};

use crate::bind::{self, BoundExpectation, BoundScenario};
use crate::compare::{self, Caps, Compared};
use crate::limits::VerifyLimits;
use crate::parameters;
use crate::receipt::{
    Comparison, ComparisonMode, FrameCounts, ParamEdit, RECEIPT_SCHEMA, Receipt, ReceiptBody,
    ReceiptHeader, ReceiptKind, ScenarioCounts, ScenarioReceipt, ScenarioStatus, Timings, Totals,
    VectorBody,
};
use crate::refusal_classes::replay_failure_cause;
use crate::vector_set::VectorSet;
use crate::verify_error::{VerifyError, VerifyErrorKind};

/// The bytes a run reads, supplied by the caller: `oce-verify` opens no file (decision 20).
///
/// The subject is CXF; a caller with an authored subject emits it first. Mutant mode adds the
/// mutants' bytes here in place.
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct VectorInputs<'a> {
    /// The subject's CXF bytes.
    pub subject: &'a [u8],
}

impl<'a> VectorInputs<'a> {
    /// The inputs of a run against the subject in `subject`.
    #[must_use]
    pub fn new(subject: &'a [u8]) -> Self {
        Self { subject }
    }
}

/// How a run is carried out. `Default` is the documented limits, no timings, no traces kept and
/// chunks of 4,096 frames.
#[derive(Clone, Debug)]
pub struct VerifyOptions {
    /// The bounds of the run; lowered to their documented maxima, never raised
    /// ([`VerifyLimits::effective`]). The set's own load bounds applied when it was read.
    pub limits: VerifyLimits,
    /// Durations the caller measured, copied into the receipt's `timings`; zeros when `None`.
    /// They never move `receipt_id`.
    pub timings: Option<Timings>,
    /// Keep each scenario's trace in [`VectorRun::traces`].
    pub keep_traces: bool,
    /// Frames per `Replay::advance` call; zero is read as one. Never changes a receipt byte.
    pub chunk_frames: usize,
}

impl Default for VerifyOptions {
    fn default() -> Self {
        Self {
            limits: VerifyLimits::default(),
            timings: None,
            keep_traces: false,
            chunk_frames: 4_096,
        }
    }
}

/// What a run produced: the receipt and, when asked for, each scenario's trace.
#[non_exhaustive]
#[derive(Debug)]
pub struct VectorRun {
    /// The receipt.
    pub receipt: Receipt,
    /// Each scenario's name and trace, in file order, when [`VerifyOptions::keep_traces`] is set;
    /// otherwise empty.
    pub traces: Vec<(String, Trace)>,
}

/// Bind `set` to the subject in `inputs`, run every scenario and issue the receipt. Blocks until
/// the last scenario has run; opens no file and reads no clock.
///
/// A scenario that fails, or checks nothing, is a verdict inside the receipt (`passed` false),
/// never an error.
///
/// # Errors
/// A [`VerifyError`] whose code is one of [`crate::VERIFY_ERROR_CODES`] for every bind and run
/// refusal; then no receipt exists.
pub fn run_vectors(
    set: &VectorSet,
    inputs: &VectorInputs<'_>,
    options: &VerifyOptions,
) -> Result<VectorRun, VerifyError> {
    let limits = options.limits.effective();
    let bound = bind::bind(set, inputs.subject, &limits)?;
    let caps = Caps {
        failures: usize::try_from(limits.max_recorded_failures).unwrap_or(usize::MAX),
        warnings: usize::try_from(limits.max_recorded_warnings).unwrap_or(usize::MAX),
    };
    let chunk = options.chunk_frames.max(1);
    let mut scenarios = Vec::with_capacity(bound.scenarios.len());
    let mut traces = Vec::new();
    for (index, (scenario, bound_scenario)) in
        set.scenarios().iter().zip(&bound.scenarios).enumerate()
    {
        let subject = format!("scenarios[{index}]");
        let ran = run_scenario(
            set,
            &subject,
            inputs.subject,
            &bound.inputs,
            bound_scenario,
            &limits,
            chunk,
        )?;
        let compared = compare::compare(
            &ran.trace,
            ran.refused,
            &bound_scenario.expectations,
            &bound.outputs,
            caps,
            &subject,
        )?;
        scenarios.push(scenario_receipt(
            scenario.name(),
            bound_scenario,
            &ran,
            compared,
            scenario.expectations().len(),
        ));
        if options.keep_traces {
            traces.push((scenario.name().to_owned(), ran.trace));
        }
    }
    let body = vector_body(set, &bound, scenarios);
    let mut receipt = Receipt {
        header: ReceiptHeader {
            schema: RECEIPT_SCHEMA.to_owned(),
            kind: ReceiptKind::Vectors,
            content_id: bound.content_id,
            engine_version: bound.engine_version.to_owned(),
            spec_id: set.spec_id(),
            comparison: Comparison {
                mode: ComparisonMode::Exact,
            },
            passed: body.status == ScenarioStatus::Pass,
            receipt_id: String::new(),
            timings: options.timings.unwrap_or_default(),
        },
        body: ReceiptBody::Vectors(body),
    };
    receipt.header.receipt_id = receipt.computed_receipt_id();
    Ok(VectorRun { receipt, traces })
}

/// One scenario's run: its trace, its refused frame when one stopped it, and the content id of
/// what ran when it has edits.
struct Ran {
    trace: Trace,
    refused: Option<(u64, EngineRefusal)>,
    content_id: Option<String>,
}

/// A replay error as a refusal at `subject`.
fn replay_failed(subject: &str, error: &ReplayError) -> VerifyError {
    VerifyError::new(
        VerifyErrorKind::ReplayFailed,
        subject,
        format!("the replay failed: {error}"),
    )
    .with_cause(replay_failure_cause(error))
}

/// The trace column kind that carries an input of `value_type`. Binding refuses every other.
fn column_kind(value_type: ValueType) -> Option<ColumnKind> {
    match value_type {
        ValueType::Real => Some(ColumnKind::Real),
        ValueType::Integer => Some(ColumnKind::Integer),
        ValueType::Boolean => Some(ColumnKind::Boolean),
        ValueType::String | ValueType::Enum(_) => None,
    }
}

/// Whether two boundaries are the same paths and types in the same order.
fn same_boundary(left: &[InputDefinition], right: &[InputDefinition]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(a, b)| a.path == b.path && a.value_type == b.value_type)
}

fn run_scenario(
    set: &VectorSet,
    subject: &str,
    cxf: &[u8],
    bound_inputs: &[InputDefinition],
    scenario: &BoundScenario,
    limits: &VerifyLimits,
    chunk: usize,
) -> Result<Ran, VerifyError> {
    let (mut engine, _) = bind::load(cxf)?;
    parameters::apply(&mut engine, &scenario.edits, subject)?;
    let content_id = if scenario.edits.is_empty() {
        None
    } else {
        Some(
            engine
                .content_id()
                .map_err(|error| bind::setup_failed(subject, "the edited content id", &error))?
                .to_string(),
        )
    };
    let replay = replay_of(engine, set, subject, bound_inputs, scenario, limits)?;
    drive(replay, subject, &scenario.expectations, chunk).map(|(trace, refused)| Ran {
        trace,
        refused,
        content_id,
    })
}

/// Set up the scenario's replay on `engine`.
fn replay_of(
    engine: Engine,
    set: &VectorSet,
    subject: &str,
    bound_inputs: &[InputDefinition],
    scenario: &BoundScenario,
    limits: &VerifyLimits,
) -> Result<Replay<StepSource>, VerifyError> {
    let definitions = engine
        .input_definitions()
        .map_err(|error| bind::setup_failed(subject, "its input definitions", &error))?;
    let setup = |detail: &str| {
        VerifyError::new(VerifyErrorKind::ReplayFailed, subject, detail.to_owned())
            .with_cause("engine-setup")
    };
    if !same_boundary(&definitions, bound_inputs) {
        return Err(setup(
            "the scenario's engine has another boundary than the one the set was bound to",
        ));
    }
    let mut columns = Vec::with_capacity(definitions.len());
    for definition in &definitions {
        let Some(kind) = column_kind(definition.value_type) else {
            return Err(setup("binding refuses every Enum and String input"));
        };
        columns.push(ColumnSpec::new(definition.path.clone(), kind));
    }
    let map = InputMap::resolve(
        &definitions,
        &columns,
        &ColumnMap::by_name(&columns),
        &MissingPolicy::Refuse,
        &[],
    )
    .map_err(|error| setup(&format!("the input map was refused: {error}")))?;
    let steps = definitions
        .iter()
        .zip(&scenario.steps)
        .map(|(definition, steps)| (definition.path.clone(), steps.clone()))
        .collect();
    let clock = set.clock();
    let source = StepSource::with_instants(&map, steps, vec![clock.horizon_s()], &limits.replay)
        .map_err(|error| replay_failed(subject, &ReplayError::Source(error)))?;
    let mut options = ReplayOptions::new(TimeBase::Grid {
        t0: 0.0,
        step: clock.step_s(),
    });
    options.limits = limits.replay;
    Replay::new(engine, map, source, options).map_err(|error| replay_failed(subject, &error))
}

/// Advance `replay` to its end in chunks, then apply the post-run order's steps 1 to 3.
fn drive(
    mut replay: Replay<StepSource>,
    subject: &str,
    expectations: &[BoundExpectation],
    chunk: usize,
) -> Result<(Trace, Option<(u64, EngineRefusal)>), VerifyError> {
    let stopped = loop {
        match replay
            .advance(chunk)
            .map_err(|error| replay_failed(subject, &error))?
        {
            Progress::More { .. } => {}
            Progress::Done { .. } => break None,
            Progress::Stopped { frame, cause } => break Some((frame, cause)),
        }
    };
    let (_, trace) = replay.finish();
    match stopped {
        Some((frame, cause)) => {
            stop_outcome(subject, frame, &cause).map(|refused| (trace, Some(refused)))
        }
        None => {
            coverage(&trace, expectations, subject)?;
            Ok((trace, None))
        }
    }
}

/// Post-run steps 1 and 2: a run-class refusal is an error; any other engine refusal fails the
/// scenario at its row. A frame replay refused for a missing input cannot happen (every input is
/// a step list from `t = 0`, which is always current); it is `replay-failed` with the cause
/// `source`, never a verdict.
pub(crate) fn stop_outcome(
    subject: &str,
    frame: u64,
    cause: &RefusalCause,
) -> Result<(u64, EngineRefusal), VerifyError> {
    match cause {
        RefusalCause::Engine {
            refusal: EngineRefusal::Run,
            message,
        } => Err(VerifyError::new(
            VerifyErrorKind::RunRefused,
            subject,
            format!("the engine refused frame {frame} for a reason about the run: {message}"),
        )),
        RefusalCause::Engine { refusal, .. } => Ok((frame, *refusal)),
        RefusalCause::MissingInput { path, .. } => Err(VerifyError::new(
            VerifyErrorKind::ReplayFailed,
            subject,
            format!("the step source supplied no value for {path} at frame {frame}"),
        )
        .with_path(path.clone())
        .with_cause("source")),
    }
}

/// Post-run step 3: every window holds an executed tick.
fn coverage(
    trace: &Trace,
    expectations: &[BoundExpectation],
    subject: &str,
) -> Result<(), VerifyError> {
    let (windows, positions): (Vec<(f64, f64)>, Vec<usize>) = expectations
        .iter()
        .enumerate()
        .filter_map(|(position, expectation)| match expectation {
            BoundExpectation::Window(window) => Some(((window.from_s, window.to_s), position)),
            BoundExpectation::Diagnostic(_) => None,
        })
        .unzip();
    trace
        .check_coverage(&windows)
        .map_err(|error| coverage_error(subject, &positions, &error))
}

/// A coverage refusal as `window-uncovered` at the expectation it names; any other error from
/// the check (a window that is not finite with `from <= to`, which no loaded window is) as
/// `replay-failed`.
pub(crate) fn coverage_error(
    subject: &str,
    positions: &[usize],
    error: &ReplayError,
) -> VerifyError {
    match error {
        ReplayError::Uncovered { index } => {
            let position = positions.get(*index).copied().unwrap_or(*index);
            VerifyError::new(
                VerifyErrorKind::WindowUncovered,
                format!("{subject}.expect[{position}]"),
                format!("the window holds no executed tick: {error}"),
            )
        }
        other => replay_failed(subject, other),
    }
}

/// The frame counts of a trace, by status byte.
fn frame_counts(trace: &Trace) -> FrameCounts {
    let mut counts = FrameCounts::default();
    for status in trace
        .status()
        .iter()
        .filter_map(|byte| FrameStatus::from_byte(*byte))
    {
        match status {
            FrameStatus::Executed => counts.executed += 1,
            FrameStatus::Substituted => counts.substituted += 1,
            FrameStatus::RefusedMissing | FrameStatus::RefusedEngine => counts.refused += 1,
            // A skip needs `MissingPolicy::SkipFrame`, which a vector run never sets.
            FrameStatus::Skipped => {}
        }
    }
    counts
}

fn scenario_receipt(
    name: &str,
    bound: &BoundScenario,
    ran: &Ran,
    compared: Compared,
    expectations: usize,
) -> ScenarioReceipt {
    let status = if ran.refused.is_some() || compared.instants.failed > 0 {
        ScenarioStatus::Fail
    } else if compared.instants.checked == 0 {
        ScenarioStatus::NoData
    } else {
        ScenarioStatus::Pass
    };
    ScenarioReceipt {
        name: name.to_owned(),
        status,
        params: bound
            .edits
            .iter()
            .map(|(path, value)| ParamEdit {
                path: path.clone(),
                value: *value,
            })
            .collect(),
        content_id: ran.content_id.clone(),
        frames: frame_counts(&ran.trace),
        expectations: u64::try_from(expectations).unwrap_or(u64::MAX),
        instants: compared.instants,
        failures: compared.failures,
        failures_total: compared.failures_total,
        warnings: compared.warnings,
        warnings_total: compared.warnings_total,
    }
}

fn vector_body(
    set: &VectorSet,
    bound: &bind::Bound,
    scenarios: Vec<ScenarioReceipt>,
) -> VectorBody {
    let mut totals = Totals::default();
    let mut counts = ScenarioCounts::default();
    for scenario in &scenarios {
        counts.total += 1;
        match scenario.status {
            ScenarioStatus::Pass => counts.pass += 1,
            ScenarioStatus::Fail => counts.fail += 1,
            ScenarioStatus::NoData => counts.no_data += 1,
        }
        totals.frames.executed += scenario.frames.executed;
        totals.frames.substituted += scenario.frames.substituted;
        totals.frames.refused += scenario.frames.refused;
        totals.instants.checked += scenario.instants.checked;
        totals.instants.passed += scenario.instants.passed;
        totals.instants.failed += scenario.instants.failed;
        totals.instants.masked += scenario.instants.masked;
        totals.failures += scenario.failures_total;
        totals.warnings += scenario.warnings_total;
    }
    totals.scenarios = counts;
    // Decision 26: `fail` when any scenario fails; else `no_data` when there is no scenario or
    // one is `no_data`; else `pass`.
    let status = if counts.fail > 0 {
        ScenarioStatus::Fail
    } else if counts.total == 0 || counts.no_data > 0 {
        ScenarioStatus::NoData
    } else {
        ScenarioStatus::Pass
    };
    VectorBody {
        status,
        clock: *set.clock(),
        load_warnings: bound.load_warnings.clone(),
        load_warnings_total: bound.load_warnings_total,
        totals,
        scenarios,
    }
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;
