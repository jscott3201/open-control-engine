//! From the set as written to the set as held: the rules a shape cannot say (vectors note,
//! "Clock", "Inputs", "Windows", "Diagnostic at an instant").
//!
//! The checks run in one fixed order, and the first refusal wins: the clock, the set's
//! `envelope`, the set's `params`, the set's `mutants` in file order (each path, then repeats),
//! then each scenario in file order (its name, its `params`, its inputs in file order, its
//! expectations in file order: a window's output name, its bounds and ticks, then its `within`,
//! its `of` and its `when`). Each refusal
//! names the place by a path into the set, with indexes counted from zero in file order. Every
//! check is arithmetic on the values at hand: nothing iterates over ticks, so a clock at the tick
//! bound costs no more than a clock of two ticks.

use std::collections::BTreeSet;

use crate::clock::Clock;
use crate::dto::{
    Entries, ExpectationDto, InputDto, MutantDto, OfDto, OfStepDto, Optional, ScenarioDto, Seconds,
    StepDto, ValueDto, VectorSetDto, WithinDto,
};
use crate::envelope::Envelope;
use crate::error::{VectorError, VectorErrorKind};
use crate::limits::VerifyLimits;
use crate::model::{
    Check, DiagnosticInstant, Expectation, Held, Input, Mutant, OfHeld, OfStep, Parameter,
    Scenario, Step, Window, Within,
};
use crate::number::{Literal, Number};
use crate::relative_path::{self, PathDefect};
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
    let envelope = dto
        .envelope
        .0
        .map(|name| {
            Envelope::from_name(&name.0).ok_or_else(|| {
                VectorError::on(
                    VectorErrorKind::UnknownEnvelope,
                    "envelope",
                    format!(
                        "the envelope is `{}`; it is `funnel` or `aligned-grid`",
                        name.0
                    ),
                )
            })
        })
        .transpose()?;
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
    Ok(VectorSet::assemble(
        clock, envelope, params, mutants, scenarios,
    ))
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
/// absolute path, an empty, `.` or `..` segment, or a backslash ([`crate::relative_path`]).
fn path_defect(path: &str) -> Option<&'static str> {
    relative_path::defect(path).map(|defect| match defect {
        PathDefect::Backslash => "holds a backslash; segments are separated by `/` only",
        PathDefect::Absolute => "is absolute; it must be relative to the vector file",
        PathDefect::EmptySegment => "has an empty segment",
        PathDefect::DotSegment => "has a `.` segment",
        PathDefect::ParentSegment => {
            "has a `..` segment; it must stay inside the vector file's directory"
        }
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
            let (first_tick, last_tick) =
                window_ticks(&window.output, window.from_s, window.to_s, place, clock)?;
            let when = when(window.when, place)?;
            Ok(Expectation::Window(Window {
                output: window.output,
                from: window.from_s.0,
                to: window.to_s.0,
                check: Check::Equals(window.equals.0),
                when,
                first_tick,
                last_tick,
            }))
        }
        ExpectationDto::ToleranceWindow(window) => {
            let window = *window;
            let (first_tick, last_tick) =
                window_ticks(&window.output, window.from_s, window.to_s, place, clock)?;
            let written = tolerances(&window.within, place)?;
            let of = of(
                window.of,
                window.from_s.0,
                window.to_s.0.seconds(),
                &format!("{place}.of"),
            )?;
            let when = when(window.when, place)?;
            Ok(Expectation::Window(Window {
                output: window.output,
                from: window.from_s.0,
                to: window.to_s.0,
                check: Check::Within(Box::new(Within { written, of })),
                when,
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

/// A window's output name, bounds and ticks, for either window form: the first and last tick.
fn window_ticks(
    output: &str,
    from: Seconds,
    to: Seconds,
    place: &str,
    clock: &Clock,
) -> Result<(u64, u64), VectorError> {
    if output.is_empty() {
        return Err(empty_name(format!("{place}.output"), "an output name"));
    }
    let (from_s, to_s) = (from.0.seconds(), to.0.seconds());
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
    clock.ticks_within(from_s, to_s).ok_or_else(|| {
        VectorError::on(
            VectorErrorKind::WindowUncovered,
            place.to_owned(),
            "the window holds no tick of the clock",
        )
    })
}

/// A `within`'s written names, in byte order: at least one, each not negative (a non-finite one
/// never loads: it is a lexical refusal), and `ltolx` at most 1.
fn tolerances(within: &WithinDto, place: &str) -> Result<Vec<(&'static str, Number)>, VectorError> {
    let named = [
        ("atolx", &within.atolx),
        ("atoly", &within.atoly),
        ("ltolx", &within.ltolx),
        ("ltoly", &within.ltoly),
        ("rtolx", &within.rtolx),
        ("rtoly", &within.rtoly),
    ];
    let mut written = Vec::with_capacity(named.len());
    for (name, slot) in named {
        let Some(amount) = slot.0 else { continue };
        let value = amount.0.seconds();
        let refuse = |detail: String| {
            Err(VectorError::on(
                VectorErrorKind::InvalidTolerance,
                format!("{place}.within.{name}"),
                detail,
            ))
        };
        if value < 0.0 {
            return refuse(format!("tolerance `{name}` is negative"));
        }
        if name == "ltolx" && value > 1.0 {
            return refuse(
                "`ltolx` is above 1, where funnel's loop removal defines no result".to_owned(),
            );
        }
        written.push((name, amount.0));
    }
    if written.is_empty() {
        return Err(VectorError::on(
            VectorErrorKind::InvalidTolerance,
            format!("{place}.within"),
            "a within names at least one of atolx, atoly, ltolx, ltoly, rtolx and rtoly",
        ));
    }
    Ok(written)
}

/// An `of`: a number is one step at `from`; a step list has at least one step, the first at
/// exactly the window's `from_s`, strictly increasing, none after `to_s`.
fn of(written: OfDto, from: Number, to_s: f64, place: &str) -> Result<OfHeld, VectorError> {
    let steps = match written {
        OfDto::Constant(amount) => {
            return Ok(OfHeld::Constant(OfStep {
                t: from,
                value: amount.0,
            }));
        }
        OfDto::Steps(steps) => steps,
    };
    if steps.is_empty() {
        return Err(VectorError::on(
            VectorErrorKind::StepListEmpty,
            place.to_owned(),
            "an of step list needs at least one step, at the window's from_s",
        ));
    }
    let from_s = from.seconds();
    let mut list: Vec<OfStep> = Vec::with_capacity(steps.len());
    for (index, OfStepDto { t, value }) in steps.into_iter().enumerate() {
        let time_s = t.0.seconds();
        let refuse =
            |kind, detail: &str| Err(VectorError::on(kind, format!("{place}[{index}].t"), detail));
        match list.last() {
            None if time_s != from_s => {
                return refuse(
                    VectorErrorKind::OfNotFromWindowStart,
                    "the first step of of must be at the window's from_s",
                );
            }
            Some(before) if time_s <= before.time_s() => {
                return refuse(
                    VectorErrorKind::StepTimesNotIncreasing,
                    "each step must be later than the one before it",
                );
            }
            _ => {}
        }
        if time_s > to_s {
            return refuse(
                VectorErrorKind::OfStepAfterWindow,
                "a step of of must not be later than the window's to_s",
            );
        }
        list.push(OfStep {
            t: t.0,
            value: value.0,
        });
    }
    Ok(OfHeld::Steps(list))
}

/// A `when` list: absent is no mask; present, at least one name and none empty. Its bound was
/// applied as it was read.
fn when(written: Optional<Vec<String>>, place: &str) -> Result<Vec<String>, VectorError> {
    let Some(names) = written.0 else {
        return Ok(Vec::new());
    };
    if names.is_empty() {
        return Err(VectorError::on(
            VectorErrorKind::WhenEmpty,
            format!("{place}.when"),
            "a when names at least one Boolean input or output",
        ));
    }
    if let Some(index) = names.iter().position(String::is_empty) {
        return Err(empty_name(format!("{place}.when[{index}]"), "a when name"));
    }
    Ok(names)
}
