//! Binding a vector set to its subject, once, before any scenario runs (vectors note, "Binding
//! and running"; decisions 6, 7, 14 and 20).
//!
//! The subject's bytes are loaded into a throwaway engine, and then, in this order, the first
//! refusal winning:
//!
//! 1. **Load.** `Engine::load_cxf`; a refusal is `subject-load-failed` with the engine's
//!    `DiagCode` spellings. Load warnings never refuse: their codes go to the receipt.
//! 2. **The boundary.** Every boundary input in slot order, then every output: an Enum port is
//!    `enum-port-unsupported`, a String port `unsupported-port-type`. These are properties of the
//!    subject alone, so they are judged before any name.
//! 3. **The set's `params`**, then **each scenario** in file order: its inputs in name order, then
//!    the inputs it does not supply, then its expectations in file order, then its `params`.
//!    Every name resolves by replay's one rule ([`oce_replay::resolve_path`]: the exact canonical
//!    path, or the one path the name ends at a `.` boundary): inputs against
//!    `Engine::input_definitions()`, outputs against `FrameSlots::outputs()`, parameter keys
//!    against `Engine::params()` sorted by path. Every value is typed by what it lands on
//!    ([`crate::typing`]): an input by `InputDefinition::value_type`, an `equals` literal by
//!    `FrameSlots::output_types()`, a parameter by its declared type.
//! 4. **The parameter edits.** Each scenario's effective edits (decision 14: a scenario's map
//!    overrides the set's by resolved path) are tried, in file order of the scenarios, on a
//!    freshly loaded engine by the passes of [`crate::parameters`], so an edit no pass admits is
//!    `param-refused` here, before any frame of any scenario runs (owner ruling of 2026-11-10;
//!    the note's table puts the code at bind). The run applies the same edits to its own fresh
//!    engine again, by the same deterministic passes.
//! 5. **The mutants** ([`crate::mutants`]): the bytes supplied against the set's listing and the
//!    byte bound, then each listed mutant in file order loaded, held to the subject's boundary and
//!    content id, and given every distinct edit list of the scenarios.
//! 6. **The frame budget.** `(N + 1) * scenarios * (1 + mutants)` against
//!    [`VerifyLimits::max_total_frames`], and `N + 1` against the replay limits' `max_frames`.
//!
//! Every refusal is a [`VerifyError`], never a failed scenario.

use std::collections::BTreeMap;

use oce_api::{
    CompatibilityDescriptor, Engine, InputDefinition, LoadReport, OcError, Value, ValueType,
};
use oce_replay::resolve_path;

use crate::limits::{MAX_LOAD_WARNINGS, VerifyLimits};
use crate::model::{Expectation, Parameter, Scenario};
use crate::mutants::{BoundMutant, SubjectBoundary};
use crate::number::Literal;
use crate::receipt::ReceiptValue;
use crate::typing::{Untyped, literal_name, type_name, typed};
use crate::vector_set::VectorSet;
use crate::verify_error::{VerifyError, VerifyErrorKind as Kind};

/// What binding learned: the subject's identity and boundary, every scenario resolved and typed,
/// and every listed mutant validated.
pub(crate) struct Bound<'a> {
    /// The subject's content id as loaded, before any parameter edit.
    pub(crate) content_id: String,
    /// The facade's Cargo version.
    pub(crate) engine_version: &'static str,
    /// Load-warning codes in load order, at most [`MAX_LOAD_WARNINGS`].
    pub(crate) load_warnings: Vec<String>,
    /// The number of load warnings before the cap.
    pub(crate) load_warnings_total: u64,
    /// The boundary inputs in slot order.
    pub(crate) inputs: Vec<InputDefinition>,
    /// The boundary outputs in output-slot order.
    pub(crate) outputs: Vec<String>,
    /// One per scenario, in file order.
    pub(crate) scenarios: Vec<BoundScenario>,
    /// One per listed mutant, in file order.
    pub(crate) mutants: Vec<BoundMutant<'a>>,
}

/// One scenario, resolved and typed.
pub(crate) struct BoundScenario {
    /// Per input slot, the typed steps in time order.
    pub(crate) steps: Vec<Vec<(f64, Value)>>,
    /// The effective parameter edits by canonical path, sorted by path bytes.
    pub(crate) edits: Vec<(String, ReceiptValue)>,
    /// The expectations, in file order.
    pub(crate) expectations: Vec<BoundExpectation>,
}

/// One expectation, resolved and typed.
pub(crate) enum BoundExpectation {
    /// An output equals a value over a window of ticks.
    Window(BoundWindow),
    /// A warning is emitted at one tick.
    Diagnostic(BoundDiagnostic),
}

/// A window expectation against an output slot.
pub(crate) struct BoundWindow {
    /// The output slot.
    pub(crate) output: usize,
    /// The window as written, seconds: the coverage check reads it.
    pub(crate) from_s: f64,
    /// The window's last second, included.
    pub(crate) to_s: f64,
    /// The first tick in the window.
    pub(crate) first_tick: u64,
    /// The last tick in the window.
    pub(crate) last_tick: u64,
    /// The expected value, typed by the output's declared type.
    pub(crate) equals: ReceiptValue,
}

/// A diagnostic expectation at one tick.
pub(crate) struct BoundDiagnostic {
    /// The tick.
    pub(crate) tick: u64,
    /// The class path to match.
    pub(crate) block: String,
    /// The message to match byte for byte, when given.
    pub(crate) message: Option<String>,
}

/// Load `subject` into a fresh engine.
///
/// # Errors
/// `subject-load-failed`, carrying the `DiagCode` spellings of every diagnostic the refusal has.
pub(crate) fn load(subject: &[u8]) -> Result<(Engine, LoadReport), VerifyError> {
    let mut engine = Engine::new();
    match engine.load_cxf(subject) {
        Ok((report, _)) => Ok((engine, report)),
        Err(error) => Err(VerifyError::new(
            Kind::SubjectLoadFailed,
            "subject",
            format!("the engine refused to load the subject: {error}"),
        )
        .with_diagnostic_codes(error.all_diagnostics().map(|d| d.code.as_str()).collect())),
    }
}

/// A facade call that sets a run up refused: `replay-failed` with the cause `engine-setup`.
pub(crate) fn setup_failed(subject: impl Into<String>, call: &str, error: &OcError) -> VerifyError {
    VerifyError::new(
        Kind::ReplayFailed,
        subject,
        format!("the engine refused {call}: {error}"),
    )
    .with_cause("engine-setup")
}

/// Bind `set` to the subject in `subject` under `limits` (already effective).
///
/// # Errors
/// The first refusal in the order the module documentation gives.
pub(crate) fn bind<'a>(
    set: &VectorSet,
    subject: &[u8],
    supplied: &[(&'a str, &'a [u8])],
    limits: &VerifyLimits,
) -> Result<Bound<'a>, VerifyError> {
    let (engine, report) = load(subject)?;
    let content_id = engine
        .content_id()
        .map_err(|error| setup_failed("subject", "its content id", &error))?
        .to_string();
    let inputs = engine
        .input_definitions()
        .map_err(|error| setup_failed("subject", "its input definitions", &error))?;
    let slots = engine
        .frame_slots()
        .map_err(|error| setup_failed("subject", "its slot map", &error))?;
    let outputs = slots.outputs().to_vec();
    let output_types = slots.output_types().to_vec();
    boundary(&inputs, &outputs, &output_types)?;
    let params = parameter_types(&engine);
    let set_edits = match set.params() {
        Some(written) => edits(written, "params", &params)?,
        None => BTreeMap::new(),
    };
    let boundary = Boundary {
        inputs: &inputs,
        outputs: &outputs,
        output_types: &output_types,
        params: &params,
    };
    let mut scenarios = Vec::with_capacity(set.scenarios().len());
    for (index, scenario) in set.scenarios().iter().enumerate() {
        scenarios.push(bind_scenario(index, scenario, &boundary, &set_edits)?);
    }
    for (index, scenario) in scenarios.iter().enumerate() {
        try_edits(subject, &scenario.edits, &format!("scenarios[{index}]"))?;
    }
    let mutants = crate::mutants::validate(
        set.mutants().unwrap_or_default(),
        supplied,
        &SubjectBoundary {
            inputs: &inputs,
            outputs: &outputs,
            output_types: &output_types,
            content_id: &content_id,
        },
        set,
        &scenarios,
        limits,
    )?;
    frame_budget(set, mutants.len(), limits)?;
    Ok(Bound {
        content_id,
        engine_version: CompatibilityDescriptor::current(None).oce_api_version(),
        load_warnings: report
            .warnings
            .iter()
            .take(MAX_LOAD_WARNINGS)
            .map(|warning| warning.code.as_str().to_owned())
            .collect(),
        load_warnings_total: u64::try_from(report.warnings.len()).unwrap_or(u64::MAX),
        inputs,
        outputs,
        scenarios,
        mutants,
    })
}

/// Try `edits` on a freshly loaded engine by the passes the run applies them with, so a refusal
/// is found at bind (step 4). Nothing is loaded for a scenario with no edits.
///
/// # Errors
/// `param-refused`, or `replay-failed` with the cause `engine-setup` for a facade refusal that is
/// not about an edit's value ([`crate::parameters::apply`]).
fn try_edits(
    subject: &[u8],
    edits: &[(String, ReceiptValue)],
    place: &str,
) -> Result<(), VerifyError> {
    if edits.is_empty() {
        return Ok(());
    }
    let (mut engine, _) = load(subject)?;
    crate::parameters::apply(&mut engine, edits, place)
}

/// Refuse a boundary a vector cannot drive or check: an Enum port (until #465) or a String
/// port, every input in slot order before every output.
///
/// # Errors
/// `enum-port-unsupported` or `unsupported-port-type` for the first such port.
pub(crate) fn boundary(
    inputs: &[InputDefinition],
    outputs: &[String],
    output_types: &[ValueType],
) -> Result<(), VerifyError> {
    let ports = inputs
        .iter()
        .map(|definition| ("input", definition.path.as_str(), definition.value_type))
        .chain(
            outputs
                .iter()
                .zip(output_types)
                .map(|(path, value_type)| ("output", path.as_str(), *value_type)),
        );
    for (direction, path, value_type) in ports {
        let kind = match value_type {
            ValueType::Real | ValueType::Integer | ValueType::Boolean => continue,
            ValueType::Enum(_) => Kind::EnumPortUnsupported,
            ValueType::String => Kind::UnsupportedPortType,
        };
        return Err(VerifyError::new(
            kind,
            "subject",
            format!(
                "boundary {direction} {path} is {}; a vector set drives and checks Real, Integer and Boolean ports only",
                type_name(value_type)
            ),
        )
        .with_path(path));
    }
    Ok(())
}

/// Every parameter's canonical path and declared type, sorted by path bytes, one row per path
/// (the first, as `set_param` addresses it, where a table repeats one).
fn parameter_types(engine: &Engine) -> Vec<(String, ValueType)> {
    let mut params: Vec<(String, ValueType)> = engine
        .params()
        .iter()
        .map(|(path, _, declared)| (path, declared.value_type))
        .collect();
    // A stable sort keeps a repeated path's first row first, and `dedup_by` keeps that one.
    params.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    params.dedup_by(|later, earlier| later.0 == earlier.0);
    params
}

/// The subject's boundary and parameters, as every scenario binds against them.
struct Boundary<'a> {
    inputs: &'a [InputDefinition],
    outputs: &'a [String],
    output_types: &'a [ValueType],
    params: &'a [(String, ValueType)],
}

/// The names of one kind of target, for the three refusals of the name rule.
struct Names {
    what: &'static str,
    unknown: Kind,
    ambiguous: Kind,
}

const INPUTS: Names = Names {
    what: "boundary input",
    unknown: Kind::UnknownInput,
    ambiguous: Kind::AmbiguousInput,
};

const OUTPUTS: Names = Names {
    what: "boundary output",
    unknown: Kind::UnknownOutput,
    ambiguous: Kind::AmbiguousOutput,
};

const PARAMETERS: Names = Names {
    what: "parameter",
    unknown: Kind::ParamPathUnknown,
    ambiguous: Kind::ParamPathAmbiguous,
};

/// Resolve `name` against `paths` by the one name rule, refusing at `subject`.
fn resolve<P: AsRef<str>>(
    paths: &[P],
    name: &str,
    names: &Names,
    subject: &str,
) -> Result<usize, VerifyError> {
    resolve_path(paths, name).map_err(|candidates| {
        if candidates.is_empty() {
            VerifyError::new(
                names.unknown,
                subject,
                format!(
                    "`{name}` matches no {}, by exact path or by suffix",
                    names.what
                ),
            )
        } else {
            VerifyError::new(
                names.ambiguous,
                subject,
                format!(
                    "`{name}` matches {} {}s: {}",
                    candidates.len(),
                    names.what,
                    candidates.join(", ")
                ),
            )
            .with_candidates(candidates)
        }
    })
}

/// Type `literal` by `value_type`, refusing at `subject` about `path`.
fn typed_at(
    literal: Literal,
    value_type: ValueType,
    subject: &str,
    path: &str,
) -> Result<ReceiptValue, VerifyError> {
    typed(literal, value_type).map_err(|untyped| {
        let (kind, why) = match untyped {
            Untyped::Mismatch => (Kind::ValueTypeMismatch, "does not take"),
            Untyped::NotRepresentable => (
                Kind::ValueNotRepresentable,
                "holds no binary64 exactly equal to",
            ),
        };
        VerifyError::new(
            kind,
            subject,
            format!(
                "{path} is {}, which {why} {}",
                type_name(value_type),
                literal_name(literal)
            ),
        )
        .with_path(path)
    })
}

/// One `params` map resolved to canonical paths and typed, at `place` (`params` or
/// `scenarios[i].params`).
fn edits(
    written: &[Parameter],
    place: &str,
    table: &[(String, ValueType)],
) -> Result<BTreeMap<String, ReceiptValue>, VerifyError> {
    let paths: Vec<&str> = table.iter().map(|(path, _)| path.as_str()).collect();
    let mut resolved: BTreeMap<String, (ReceiptValue, &str)> = BTreeMap::new();
    for param in written {
        let subject = format!("{place}.{}", param.path());
        let row = resolve(&paths, param.path(), &PARAMETERS, &subject)?;
        let (path, value_type) = &table[row];
        if let Some((_, first)) = resolved.get(path) {
            return Err(VerifyError::new(
                Kind::ParamPathDuplicate,
                subject,
                format!(
                    "`{}` and `{first}` both name parameter {path}",
                    param.path()
                ),
            )
            .with_path(path.clone()));
        }
        let value = typed_at(param.value(), *value_type, &subject, path)?;
        resolved.insert(path.clone(), (value, param.path()));
    }
    Ok(resolved
        .into_iter()
        .map(|(path, (value, _))| (path, value))
        .collect())
}

fn bind_scenario(
    index: usize,
    scenario: &Scenario,
    boundary: &Boundary<'_>,
    set_edits: &BTreeMap<String, ReceiptValue>,
) -> Result<BoundScenario, VerifyError> {
    let steps = bind_inputs(index, scenario, boundary.inputs)?;
    let mut expectations = Vec::with_capacity(scenario.expectations().len());
    for (position, expectation) in scenario.expectations().iter().enumerate() {
        let place = format!("scenarios[{index}].expect[{position}]");
        expectations.push(match expectation {
            Expectation::Window(window) => {
                let slot = resolve(
                    boundary.outputs,
                    window.output(),
                    &OUTPUTS,
                    &format!("{place}.output"),
                )?;
                let equals = typed_at(
                    window.equals(),
                    boundary.output_types[slot],
                    &format!("{place}.equals"),
                    &boundary.outputs[slot],
                )?;
                BoundExpectation::Window(BoundWindow {
                    output: slot,
                    from_s: window.from_s(),
                    to_s: window.to_s(),
                    first_tick: window.first_tick(),
                    last_tick: window.last_tick(),
                    equals,
                })
            }
            Expectation::Diagnostic(instant) => BoundExpectation::Diagnostic(BoundDiagnostic {
                tick: instant.tick(),
                block: instant.block().to_owned(),
                message: instant.message().map(str::to_owned),
            }),
        });
    }
    let mut effective = set_edits.clone();
    if let Some(written) = scenario.params() {
        effective.extend(edits(
            written,
            &format!("scenarios[{index}].params"),
            boundary.params,
        )?);
    }
    Ok(BoundScenario {
        steps,
        edits: effective.into_iter().collect(),
        expectations,
    })
}

/// The name that supplied an input slot, and the slot's typed steps.
type Supplied<'a> = (&'a str, Vec<(f64, Value)>);

/// A scenario's inputs, one typed step list per input slot.
fn bind_inputs(
    index: usize,
    scenario: &Scenario,
    inputs: &[InputDefinition],
) -> Result<Vec<Vec<(f64, Value)>>, VerifyError> {
    let paths: Vec<&str> = inputs.iter().map(|d| d.path.as_str()).collect();
    // Per slot: the name that supplied it, and its typed steps.
    let mut by_slot: Vec<Option<Supplied<'_>>> = vec![None; inputs.len()];
    for input in scenario.inputs() {
        let place = format!("scenarios[{index}].inputs.{}", input.name());
        let slot = resolve(&paths, input.name(), &INPUTS, &place)?;
        let definition = &inputs[slot];
        if let Some((first, _)) = &by_slot[slot] {
            return Err(VerifyError::new(
                Kind::DuplicateInput,
                place,
                format!(
                    "`{}` and `{first}` both name boundary input {}",
                    input.name(),
                    definition.path
                ),
            )
            .with_path(definition.path.clone()));
        }
        let mut steps = Vec::with_capacity(input.steps().len());
        for (position, step) in input.steps().iter().enumerate() {
            let subject = if input.is_constant() {
                place.clone()
            } else {
                format!("{place}[{position}].value")
            };
            let value = typed_at(
                step.value(),
                definition.value_type,
                &subject,
                &definition.path,
            )?;
            steps.push((step.time_s(), value.engine_value()));
        }
        by_slot[slot] = Some((input.name(), steps));
    }
    let mut lists = Vec::with_capacity(inputs.len());
    for (definition, supplied) in inputs.iter().zip(by_slot) {
        let Some((_, steps)) = supplied else {
            return Err(VerifyError::new(
                Kind::InputNotSupplied,
                format!("scenarios[{index}].inputs"),
                format!(
                    "no input of the scenario supplies boundary input {}",
                    definition.path
                ),
            )
            .with_path(definition.path.clone()));
        };
        lists.push(steps);
    }
    Ok(lists)
}

/// The frame budget (bind step 6): every scenario runs `N + 1` frames, on the subject and on
/// each mutant at most (a mutant's run stops at its first failing scenario, and none runs against
/// a subject that did not pass, so the budget is the bound, not the count).
fn frame_budget(set: &VectorSet, mutants: usize, limits: &VerifyLimits) -> Result<(), VerifyError> {
    let ticks = set.clock().tick_count();
    let scenarios = u64::try_from(set.scenarios().len()).unwrap_or(u64::MAX);
    let runs = u64::try_from(mutants).unwrap_or(u64::MAX).saturating_add(1);
    if scenarios > 0 && ticks > limits.replay.max_frames {
        return Err(VerifyError::new(
            Kind::TooManyFrames,
            "clock",
            format!(
                "each scenario runs {ticks} frames; the replay limits allow {}",
                limits.replay.max_frames
            ),
        ));
    }
    let total = ticks
        .checked_mul(scenarios)
        .and_then(|frames| frames.checked_mul(runs));
    if total.is_none_or(|total| total > limits.max_total_frames) {
        return Err(VerifyError::new(
            Kind::TooManyFrames,
            "scenarios",
            format!(
                "{scenarios} scenarios of {ticks} frames on the subject and {mutants} mutants are more than the {} frames allowed",
                limits.max_total_frames
            ),
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "bind_tests.rs"]
mod tests;
