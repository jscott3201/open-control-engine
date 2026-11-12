//! From the set as written to the set as held: the rules a shape cannot say (vectors note,
//! "Clock", "Inputs", "Windows", "Diagnostic at an instant").
//!
//! The checks run in one fixed order, and the first refusal wins: the clock, the set's `params`,
//! the set's `mutants` in file order (each path, then repeats), then each scenario in file order
//! (its name, its `params`, its inputs in file order, its expectations in file order). Each refusal
//! names the place by a path into the set, with indexes counted from zero in file order. Every
//! check is arithmetic on the values at hand: nothing iterates over ticks, so a clock at the tick
//! bound costs no more than a clock of two ticks.

use std::collections::BTreeSet;

use crate::clock::Clock;
use crate::dto::{
    Entries, ExpectationDto, InputDto, MutantDto, ScenarioDto, StepDto, ValueDto, VectorSetDto,
};
use crate::error::{VectorError, VectorErrorKind};
use crate::limits::VerifyLimits;
use crate::model::{
    DiagnosticInstant, Expectation, Held, Input, Mutant, Parameter, Scenario, Step, Window,
};
use crate::number::{Literal, Number};
use crate::vector_set::VectorSet;

/// Check `dto` against the format's rules under `limits` (already
/// [`VerifyLimits::effective`]) and build the set.
///
/// # Errors
/// The first of `invalid-clock`, `too-many-ticks`, `horizon-off-cadence`, `empty-name`,
/// `duplicate-scenario`, a step-list code, a window code or `instant-off-cadence`, in the order
/// the module documentation gives.
pub(crate) fn vector_set(
    dto: VectorSetDto,
    limits: &VerifyLimits,
) -> Result<VectorSet, VectorError> {
    let clock = Clock::new(dto.clock.step_s.0, dto.clock.horizon_s.0, limits.max_ticks)?;
    let params = dto
        .params
        .0
        .map(|entries| parameters(entries, "params"))
        .transpose()?;
    let mutants = dto.mutants.0.map(mutant_list).transpose()?;
    let mut names = BTreeSet::new();
    let mut scenarios = Vec::with_capacity(dto.scenarios.len());
    for (index, written) in dto.scenarios.into_iter().enumerate() {
        let place = format!("scenarios[{index}]");
        if written.name.is_empty() {
            return Err(empty_name(format!("{place}.name"), "a scenario name"));
        }
        if !names.insert(written.name.clone()) {
            return Err(VectorError::on(
                VectorErrorKind::DuplicateScenario,
                format!("{place}.name"),
                format!("an earlier scenario is already named `{}`", written.name),
            ));
        }
        scenarios.push(scenario(written, &place, &clock)?);
    }
    Ok(VectorSet::assemble(clock, params, mutants, scenarios))
}

/// The mutant listing, in file order: every path inside the vector file's directory (decision
/// 28), and no path twice.
fn mutant_list(written: Vec<MutantDto>) -> Result<Vec<Mutant>, VectorError> {
    let mut paths = BTreeSet::new();
    let mut mutants = Vec::with_capacity(written.len());
    for (index, mutant) in written.into_iter().enumerate() {
        let place = format!("mutants[{index}].path");
        if let Some(why) = path_defect(&mutant.path) {
            return Err(VectorError::on(
                VectorErrorKind::MutantPathInvalid,
                place,
                format!("the mutant path `{}` {why}", mutant.path),
            ));
        }
        if !paths.insert(mutant.path.clone()) {
            return Err(VectorError::on(
                VectorErrorKind::DuplicateMutant,
                place,
                format!("an earlier mutant already has the path `{}`", mutant.path),
            ));
        }
        mutants.push(Mutant {
            path: mutant.path,
            description: mutant.description.0,
        });
    }
    Ok(mutants)
}

/// Why `path` could leave the vector file's directory or name a file two ways, or `None`: an
/// absolute path, an empty, `.` or `..` segment, or a backslash.
fn path_defect(path: &str) -> Option<&'static str> {
    if path.contains('\\') {
        return Some("holds a backslash; segments are separated by `/` only");
    }
    if path.starts_with('/') {
        return Some("is absolute; it must be relative to the vector file");
    }
    path.split('/').find_map(|segment| match segment {
        "" => Some("has an empty segment"),
        "." => Some("has a `.` segment"),
        ".." => Some("has a `..` segment; it must stay inside the vector file's directory"),
        _ => None,
    })
}

fn empty_name(subject: String, what: &str) -> VectorError {
    VectorError::on(
        VectorErrorKind::EmptyName,
        subject,
        format!("{what} must not be empty"),
    )
}

/// A `params` map, sorted by path bytes.
fn parameters(entries: Entries<ValueDto>, place: &str) -> Result<Vec<Parameter>, VectorError> {
    let mut params = Vec::with_capacity(entries.0.len());
    for (path, ValueDto(value)) in entries.0 {
        if path.is_empty() {
            return Err(empty_name(place.to_owned(), "a parameter path"));
        }
        params.push(Parameter { path, value });
    }
    params.sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));
    Ok(params)
}

fn scenario(written: ScenarioDto, place: &str, clock: &Clock) -> Result<Scenario, VectorError> {
    let params = written
        .params
        .0
        .map(|entries| parameters(entries, &format!("{place}.params")))
        .transpose()?;
    let mut inputs = Vec::with_capacity(written.inputs.0.len());
    for (name, held) in written.inputs.0 {
        if name.is_empty() {
            return Err(empty_name(format!("{place}.inputs"), "an input name"));
        }
        let held = match held {
            InputDto::Constant(ValueDto(value)) => Held::Constant(constant_step(value)),
            InputDto::Steps(steps) => {
                Held::Steps(step_list(steps, &format!("{place}.inputs.{name}"), clock)?)
            }
        };
        inputs.push(Input { name, held });
    }
    // Names are unique, so an unstable sort (which allocates nothing) orders them the same way.
    inputs.sort_unstable_by(|left, right| left.name.as_bytes().cmp(right.name.as_bytes()));
    let mut expectations = Vec::with_capacity(written.expect.len());
    for (index, expected) in written.expect.into_iter().enumerate() {
        expectations.push(expectation(
            expected,
            &format!("{place}.expect[{index}]"),
            clock,
        )?);
    }
    Ok(Scenario {
        name: written.name,
        description: written.description.0,
        params,
        inputs,
        expectations,
    })
}

/// A constant is a one-step list at `t = 0`.
fn constant_step(value: Literal) -> Step {
    Step {
        t: Number::Integer(0),
        value,
    }
}

/// A step list: at least one step, from exactly `t = 0`, strictly increasing, never past the
/// horizon.
fn step_list(steps: Vec<StepDto>, place: &str, clock: &Clock) -> Result<Vec<Step>, VectorError> {
    let refuse = |kind, index: usize, detail: &str| {
        Err(VectorError::on(kind, format!("{place}[{index}].t"), detail))
    };
    if steps.is_empty() {
        return Err(VectorError::on(
            VectorErrorKind::StepListEmpty,
            place.to_owned(),
            "a step list needs at least one step, at t = 0",
        ));
    }
    let horizon_s = clock.horizon_s();
    let mut list: Vec<Step> = Vec::with_capacity(steps.len());
    for (index, written) in steps.into_iter().enumerate() {
        let time_s = written.t.0.seconds();
        match list.last() {
            None if time_s != 0.0 => {
                return refuse(
                    VectorErrorKind::StepListNotFromZero,
                    index,
                    "the first step must be at t = 0",
                );
            }
            Some(before) if time_s <= before.time_s() => {
                return refuse(
                    VectorErrorKind::StepTimesNotIncreasing,
                    index,
                    "each step must be later than the one before it",
                );
            }
            _ => {}
        }
        if time_s > horizon_s {
            return refuse(
                VectorErrorKind::StepAfterHorizon,
                index,
                "a step must not be later than horizon_s",
            );
        }
        list.push(Step {
            t: written.t.0,
            value: written.value.0,
        });
    }
    Ok(list)
}

fn expectation(
    expected: ExpectationDto,
    place: &str,
    clock: &Clock,
) -> Result<Expectation, VectorError> {
    match expected {
        ExpectationDto::Window(window) => {
            if window.output.is_empty() {
                return Err(empty_name(format!("{place}.output"), "an output name"));
            }
            let (from_s, to_s) = (window.from_s.0.seconds(), window.to_s.0.seconds());
            if from_s > to_s {
                return Err(VectorError::on(
                    VectorErrorKind::InvalidWindow,
                    place.to_owned(),
                    "from_s must not be after to_s",
                ));
            }
            if from_s < 0.0 || to_s > clock.horizon_s() {
                return Err(VectorError::on(
                    VectorErrorKind::WindowOutsideClock,
                    place.to_owned(),
                    "a window must lie between 0 and horizon_s",
                ));
            }
            let Some((first_tick, last_tick)) = clock.ticks_within(from_s, to_s) else {
                return Err(VectorError::on(
                    VectorErrorKind::WindowUncovered,
                    place.to_owned(),
                    "the window holds no tick of the clock",
                ));
            };
            Ok(Expectation::Window(Window {
                output: window.output,
                from: window.from_s.0,
                to: window.to_s.0,
                equals: window.equals.0,
                first_tick,
                last_tick,
            }))
        }
        ExpectationDto::Instant(instant) => {
            if instant.diagnostic.block.is_empty() {
                return Err(empty_name(
                    format!("{place}.diagnostic.block"),
                    "a diagnostic block",
                ));
            }
            let Some(tick) = clock.tick_at(instant.at_s.0.seconds()) else {
                return Err(VectorError::on(
                    VectorErrorKind::InstantOffCadence,
                    format!("{place}.at_s"),
                    "at_s must be a tick of the clock",
                ));
            };
            Ok(Expectation::Diagnostic(DiagnosticInstant {
                block: instant.diagnostic.block,
                message: instant.diagnostic.message.0,
                at: instant.at_s.0,
                tick,
            }))
        }
    }
}
