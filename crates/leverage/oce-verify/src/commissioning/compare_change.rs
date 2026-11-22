//! One compared output of a reference that gives no `sampling`: the change-of-value comparison
//! (commissioning note, "Comparison" and "As landed: the change-of-value comparison"; issue
//! #746, ruled by the owner on 2026-11-22).
//!
//! Both series are step functions of time ([`super::change_of_value`]): each holds its last
//! sample until the next, the engine's from frame to frame and a trend's from log entry to log
//! entry. The output is compared over the stretches between its instants, never only at them:
//!
//! 1. **The series instants**: `start`, `stop`, and every sample time within the window of the
//!    engine series, the trend series and each of the output's indicators, and every end of an
//!    uncovered stretch. Each series is held at them (the last sample at or before, the last of
//!    an equal-time pair, as on the grid path), so a series that stops logging is held to
//!    `stop`. Each instant is masked, uncovered or compared as on the grid path, and the masked
//!    and uncovered instants are zeroed in both series (OBC §12.7.2 step 8; the zero fill is
//!    unchanged, issue #745). Since every indicator sample and every stretch end is an instant,
//!    the fate holds from each instant to the next.
//! 2. **A Boolean or Integer output** is compared exactly at each series instant, a mismatch an
//!    exceedance of `1`; nothing changes between two instants, so the verdict is exact and
//!    `exceedance_time_s` is the time in mismatch.
//! 3. **A Real output**: the envelope is built around the reference's step series, and the
//!    test's step series is handed to it beside it ([`crate::Envelope::compare`]; both start at
//!    `start` and end at `stop`). The instants then take every corner of the two bounds inside the
//!    window, each inheriting the fate and the test value of the series instant before it. Each
//!    instant's exceedance is the largest error over the stretch to the next instant
//!    ([`super::change_of_value::piece_exceedances`]); the envelope's own errors, at the test's
//!    points, are not read. `exceedance_time_s` is then exact where the band is flat between two
//!    corners, as it is around a step series without a local `x` tolerance, and an upper bound
//!    otherwise.
//!
//! A non-finite engine sample fails the output as on the grid path, before any envelope.

use oce_api::ValueType;

use super::change_of_value::{piece_exceedances, series_instants, step_series, with_corners};
use super::choices::FunnelReference;
use super::compare_output::{
    Comparing, Fate, Sampled, Values, compare_refused, count, empty_output_report, exact, fates_of,
    non_finite_engine, verdict, zeroed,
};
use super::drive::{Stretch, uncovered_at};
use super::indicator_mask::{IndicatorSeries, active_at};
use super::prepare::{held, held_indexes};
use super::report::{Exceedance, MAX_MASKED_EXCEEDANCE, OutputReport, capped};
use super::run_options::{OutputBounds, OutputCurves};
use super::tolerances::PartialTolerances;
use crate::envelope::{Curve, Series};
use crate::verify_error::VerifyError;

/// What one output is compared under when its reference gives no `sampling`.
pub(crate) struct ChangePlan<'a> {
    /// The canonical path.
    pub(crate) path: &'a str,
    /// The output's type.
    pub(crate) value_type: ValueType,
    /// The engine's series: the executed frames.
    pub(crate) engine: Sampled<'a>,
    /// The trend's series.
    pub(crate) trend: Sampled<'a>,
    /// The window's start and stop, model seconds.
    pub(crate) window: (f64, f64),
    /// Its tolerances after the overrides.
    pub(crate) tolerances: PartialTolerances,
    /// The output's indicators, ANDed.
    pub(crate) indicators: &'a [&'a IndicatorSeries],
    /// The run's uncovered stretches, in time order.
    pub(crate) stretches: &'a [Stretch],
}

/// The ends of `stretches`, ascending: each `from` and each closing `until`.
fn stretch_ends(stretches: &[Stretch]) -> Vec<f64> {
    let mut ends = Vec::with_capacity(stretches.len() * 2);
    for stretch in stretches {
        ends.push(stretch.from);
        ends.extend(stretch.until);
    }
    ends
}

/// Compare one output by change of value and report it.
///
/// # Errors
/// `compare-refused` with the envelope's code as its cause.
pub(crate) fn compare_change(
    plan: &ChangePlan<'_>,
    comparing: &Comparing,
) -> Result<(OutputReport, OutputCurves), VerifyError> {
    let (start, stop) = plan.window;
    let report = empty_output_report(plan.path, plan.value_type, plan.tolerances, comparing.role);
    let empty = |report: OutputReport| {
        (
            report,
            OutputCurves {
                output: plan.path.to_owned(),
                errors: Curve::from_columns(Vec::new(), Vec::new()),
                bounds: None,
            },
        )
    };
    let ends = stretch_ends(plan.stretches);
    let mut lists: Vec<&[f64]> = vec![plan.engine.times, plan.trend.times, &ends];
    lists.extend(plan.indicators.iter().map(|series| series.times.as_slice()));
    let (instants, _) = series_instants(start, stop, &lists);
    let (Some(engine), Some(trend)) = (
        held_indexes(plan.engine.times, &instants),
        held_indexes(plan.trend.times, &instants),
    ) else {
        return Ok(empty(report));
    };
    let fates = fates_of(
        &active_at(plan.indicators, &instants),
        &uncovered_at(plan.stretches, &instants).0,
    );
    match (plan.engine.values, plan.trend.values) {
        (Values::Real(engine_values), Values::Real(trend_values)) => compare_real(
            plan,
            comparing,
            Held {
                instants,
                fates,
                engine: held(&engine, engine_values),
                trend: held(&trend, trend_values),
            },
            report,
        ),
        (Values::Discrete(engine_values), Values::Discrete(trend_values)) => {
            let exceedances = exact(
                &fates,
                &held(&engine, engine_values),
                &held(&trend, trend_values),
            );
            Ok(finish(
                plan.path,
                report,
                &instants,
                &fates,
                exceedances,
                None,
            ))
        }
        // Bind gives one type to both series.
        _ => Ok(empty(report)),
    }
}

/// Both series of a Real output held at its series instants, with each instant's fate.
struct Held {
    instants: Vec<f64>,
    fates: Vec<Fate>,
    engine: Vec<f64>,
    trend: Vec<f64>,
}

/// The report's counts and verdict over `instants` and their `exceedances`, and the curves.
fn finish(
    path: &str,
    mut report: OutputReport,
    instants: &[f64],
    fates: &[Fate],
    exceedances: Vec<f64>,
    bounds: Option<OutputBounds>,
) -> (OutputReport, OutputCurves) {
    report.compared = count(fates, Fate::Compared);
    report.masked = count(fates, Fate::Masked);
    report.uncovered = count(fates, Fate::Uncovered);
    let verdict = verdict(instants, fates, &exceedances);
    report.status = verdict.status;
    if report.worst.is_none() || report.status != super::report::OutputStatus::Fail {
        report.worst = verdict.worst;
    }
    report.first_exceedance_t = verdict.first;
    report.exceedance_time_s = verdict.exceedance_time_s;
    (report.masked_intervals, report.masked_intervals_total) = verdict.masked_intervals;
    let curves = OutputCurves {
        output: path.to_owned(),
        errors: Curve::from_columns(instants.to_vec(), exceedances),
        bounds,
    };
    (report, curves)
}

/// A Real output: the envelope around the reference's step series, the instants with the bound
/// corners, and the largest error over each stretch.
fn compare_real(
    plan: &ChangePlan<'_>,
    comparing: &Comparing,
    held: Held,
    mut report: OutputReport,
) -> Result<(OutputReport, OutputCurves), VerifyError> {
    let Held {
        instants,
        fates,
        engine,
        trend,
    } = held;
    let engine = zeroed(&fates, engine);
    let trend = zeroed(&fates, trend);
    if count(&fates, Fate::Compared) == 0 {
        // No envelope is built for an output with nothing compared.
        let errors = vec![f64::NAN; instants.len()];
        return Ok(finish(plan.path, report, &instants, &fates, errors, None));
    }
    if let Some(errors) = non_finite_engine(&instants, &fates, &engine, &mut report) {
        return Ok(finish(plan.path, report, &instants, &fates, errors, None));
    }
    let (reference, test) = match comparing.role {
        FunnelReference::Engine => (&engine, &trend),
        FunnelReference::Trend => (&trend, &engine),
    };
    let (reference_x, reference_y) = step_series(&instants, reference);
    let (test_x, test_y) = step_series(&instants, test);
    let result = comparing
        .envelope
        .compare(
            Series::new(&reference_x, &reference_y),
            Series::new(&test_x, &test_y),
            &plan.tolerances.resolved(),
            &comparing.limits,
        )
        .map_err(|error| compare_refused(plan.path, comparing.envelope, &error))?;
    report.half_width = Some(result.half_width());
    report.half_height = Some(result.half_height());
    let (start, stop) = plan.window;
    let (pieces, origin) = with_corners(
        &instants,
        &result.lower_bound,
        &result.upper_bound,
        start,
        stop,
    );
    let piece_fates: Vec<Fate> = origin.iter().map(|index| fates[*index]).collect();
    let piece_test: Vec<f64> = origin.iter().map(|index| test[*index]).collect();
    let errors = piece_exceedances(
        &pieces,
        &piece_test,
        &result.lower_bound,
        &result.upper_bound,
    );
    let mut masked_exceedance = Vec::new();
    let exceedances: Vec<f64> = piece_fates
        .iter()
        .zip(errors)
        .zip(&pieces)
        .map(|((fate, error), t)| match fate {
            Fate::Compared => error,
            Fate::Masked => {
                if error > 0.0 {
                    masked_exceedance.push(Exceedance {
                        t: *t,
                        exceedance: error,
                    });
                }
                error
            }
            Fate::Uncovered => f64::NAN,
        })
        .collect();
    (report.masked_exceedance, report.masked_exceedance_total) =
        capped(masked_exceedance, MAX_MASKED_EXCEEDANCE);
    let bounds = comparing.bounds.then_some(OutputBounds {
        reference: result.reference,
        test: result.test,
        lower: result.lower_bound,
        upper: result.upper_bound,
    });
    Ok(finish(
        plan.path,
        report,
        &pieces,
        &piece_fates,
        exceedances,
        bounds,
    ))
}

#[cfg(test)]
#[path = "compare_change_tests.rs"]
mod tests;
