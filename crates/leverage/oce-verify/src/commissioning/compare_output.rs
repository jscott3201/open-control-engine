//! One compared output: masking, the comparison and its report (commissioning note,
//! "Envelope", "Indicators" and "Report and receipt"; decisions 2, 6, 8, 11, 23, 28 and 32).
//!
//! Both series are held at the reference's comparison instants ([`super::prepare`]), and each
//! instant is, first match wins: **masked** when an indicator is false there; **uncovered** when
//! it lies in a refused stretch (decision 28); else **compared**. Masked and uncovered instants
//! are zeroed in both series **before the envelope is built**, "so the funnel spans the step at a
//! mask edge as LBL's tool does" (the zero fill widens the band next to a mask edge; issue #745
//! records it), and are out of the verdict. `compared + masked + uncovered` is the number of
//! instants.
//!
//! - **A Real output** goes through its reference's envelope, with the engine series as the
//!   reference by default and the trend's under `funnelReference: "trend"` (decision 11). An
//!   instant's exceedance is the envelope's error there ([`crate::FunnelResult::errors_at_test`]),
//!   never at a breakpoint between two (decision 32). An error above zero at a masked instant is
//!   reported in `masked_exceedance` and never fails the output (decision 23); with both series
//!   zeroed at the same instants it happens only by rounding. There is no self-enclosure refusal.
//! - **A non-finite engine sample.** When the engine series holds a NaN, an infinity or a value
//!   above 2^1000 in magnitude at an instant that is neither masked nor uncovered, no envelope
//!   is built: the output fails, `worst` is the earliest such instant with the exceedance
//!   positive infinity, and every compared instant's exceedance is positive infinity (owner
//!   ruling of 2026-11-21).
//! - **A Boolean or Integer output** never enters an envelope (decision 8): a mismatch is an
//!   exceedance of exactly `1`.
//!
//! An output whose trend or engine series has no sample at or before the window's start is
//! `no_data` with no instant at all.

use oce_api::ValueType;

use super::choices::FunnelReference;
use super::prepare::{held, held_indexes};
use super::report::{
    Exceedance, MAX_MASKED_EXCEEDANCE, MAX_MASKED_INTERVALS, MaskedInterval, OutputReport,
    OutputStatus, OutputType, capped,
};
use super::run_options::{OutputBounds, OutputCurves};
use super::tolerances::PartialTolerances;
use crate::compare_error::CompareError;
use crate::compare_limits::CompareLimits;
use crate::envelope::{Curve, Envelope, Series};
use crate::verify_error::{VerifyError, VerifyErrorKind as Kind, compare_refusal_cause};

/// 2^1000: an engine sample above it in magnitude is treated as non-finite, so the envelope's
/// arithmetic stays finite.
pub(crate) const MAX_ENGINE_MAGNITUDE: f64 = 1.071_508_607_186_267_3e301;

/// The values of one series, by type.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Values<'a> {
    /// Real samples.
    Real(&'a [f64]),
    /// Integer or Boolean samples, a Boolean as 0 or 1.
    Discrete(&'a [i64]),
}

/// One series: its sample times and values.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Sampled<'a> {
    /// The sample times, non-decreasing.
    pub(crate) times: &'a [f64],
    /// The values.
    pub(crate) values: Values<'a>,
}

/// What one output is compared under.
pub(crate) struct OutputPlan<'a> {
    /// The canonical path.
    pub(crate) path: &'a str,
    /// The output's type.
    pub(crate) value_type: ValueType,
    /// The engine's series.
    pub(crate) engine: Sampled<'a>,
    /// The trend's series.
    pub(crate) trend: Sampled<'a>,
    /// The reference's comparison instants.
    pub(crate) instants: &'a [f64],
    /// Its tolerances after the overrides.
    pub(crate) tolerances: PartialTolerances,
    /// At each instant, whether every indicator of the output is true there.
    pub(crate) active: &'a [bool],
    /// At each instant, whether it lies in a refused stretch.
    pub(crate) uncovered: &'a [bool],
}

/// The reference's comparison settings.
pub(crate) struct Comparing {
    /// The envelope.
    pub(crate) envelope: Envelope,
    /// Which series the envelope is built around.
    pub(crate) role: FunnelReference,
    /// The comparison's limits.
    pub(crate) limits: CompareLimits,
    /// Keep the bound curves.
    pub(crate) bounds: bool,
}

/// What happened at each instant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Fate {
    /// In the verdict.
    Compared,
    /// An indicator is false there.
    Masked,
    /// In a refused stretch that no indicator masked.
    Uncovered,
}

/// The fate of each instant of `plan`: masked first, then uncovered.
fn fates(plan: &OutputPlan<'_>) -> Vec<Fate> {
    fates_of(plan.active, plan.uncovered)
}

/// The fate of each instant from whether it is `active` and whether it is `uncovered`: masked
/// first, then uncovered, else compared.
pub(super) fn fates_of(active: &[bool], uncovered: &[bool]) -> Vec<Fate> {
    active
        .iter()
        .zip(uncovered)
        .map(|(active, uncovered)| {
            if !active {
                Fate::Masked
            } else if *uncovered {
                Fate::Uncovered
            } else {
                Fate::Compared
            }
        })
        .collect()
}

pub(super) fn count(fates: &[Fate], fate: Fate) -> u64 {
    u64::try_from(fates.iter().filter(|f| **f == fate).count()).unwrap_or(u64::MAX)
}

fn output_type(value_type: ValueType) -> OutputType {
    match value_type {
        ValueType::Real => OutputType::Real,
        ValueType::Integer => OutputType::Integer,
        // Bind refuses every other type; a Boolean is the one left.
        _ => OutputType::Boolean,
    }
}

/// A report of `plan` with nothing in it: its fields for its type, every count zero.
fn empty_report(plan: &OutputPlan<'_>, role: FunnelReference) -> OutputReport {
    empty_output_report(plan.path, plan.value_type, plan.tolerances, role)
}

/// A report of the output at `path` with nothing in it: its fields for its type, every count
/// zero.
pub(super) fn empty_output_report(
    path: &str,
    value_type: ValueType,
    tolerances: PartialTolerances,
    role: FunnelReference,
) -> OutputReport {
    let real = value_type == ValueType::Real;
    OutputReport {
        output: path.to_owned(),
        output_type: output_type(value_type),
        status: OutputStatus::NoData,
        compared: 0,
        masked: 0,
        uncovered: 0,
        funnel_reference: real.then_some(role),
        tolerances: real.then_some(tolerances),
        half_width: None,
        half_height: None,
        worst: None,
        first_exceedance_t: None,
        exceedance_time_s: 0.0,
        masked_intervals: Vec::new(),
        masked_intervals_total: 0,
        masked_exceedance: Vec::new(),
        masked_exceedance_total: 0,
        last_sample_t: None,
    }
}

/// Compare one output and report it.
///
/// # Errors
/// `compare-refused` with the envelope's code as its cause.
pub(crate) fn compare_output(
    plan: &OutputPlan<'_>,
    comparing: &Comparing,
) -> Result<(OutputReport, OutputCurves), VerifyError> {
    let empty = || {
        (
            empty_report(plan, comparing.role),
            OutputCurves {
                output: plan.path.to_owned(),
                errors: Curve::from_columns(Vec::new(), Vec::new()),
                bounds: None,
            },
        )
    };
    let (Some(engine), Some(trend)) = (
        held_indexes(plan.engine.times, plan.instants),
        held_indexes(plan.trend.times, plan.instants),
    ) else {
        return Ok(empty());
    };
    let fates = fates(plan);
    let mut report = empty_report(plan, comparing.role);
    report.compared = count(&fates, Fate::Compared);
    report.masked = count(&fates, Fate::Masked);
    report.uncovered = count(&fates, Fate::Uncovered);
    let (exceedances, bounds) = match (plan.engine.values, plan.trend.values) {
        (Values::Real(engine_values), Values::Real(trend_values)) => compare_real(
            plan,
            comparing,
            &fates,
            (held(&engine, engine_values), held(&trend, trend_values)),
            &mut report,
        )?,
        (Values::Discrete(engine_values), Values::Discrete(trend_values)) => (
            exact(
                &fates,
                &held(&engine, engine_values),
                &held(&trend, trend_values),
            ),
            None,
        ),
        // Bind gives one type to both series.
        _ => return Ok(empty()),
    };
    let verdict = verdict(plan.instants, &fates, &exceedances);
    report.status = verdict.status;
    if report.worst.is_none() || report.status != OutputStatus::Fail {
        report.worst = verdict.worst;
    }
    report.first_exceedance_t = verdict.first;
    report.exceedance_time_s = verdict.exceedance_time_s;
    (report.masked_intervals, report.masked_intervals_total) = verdict.masked_intervals;
    let curves = OutputCurves {
        output: plan.path.to_owned(),
        errors: Curve::from_columns(plan.instants.to_vec(), exceedances),
        bounds,
    };
    Ok((report, curves))
}

/// The figures of a verdict over the test instants `times`, their fates and their exceedances.
pub(super) struct Verdict {
    pub(super) status: OutputStatus,
    pub(super) worst: Option<Exceedance>,
    pub(super) first: Option<f64>,
    pub(super) exceedance_time_s: f64,
    pub(super) masked_intervals: (Vec<MaskedInterval>, u64),
}

pub(super) fn verdict(times: &[f64], fates: &[Fate], exceedances: &[f64]) -> Verdict {
    let mut worst: Option<Exceedance> = None;
    let mut first = None;
    let mut time_s = 0.0;
    let mut any_compared = false;
    for (k, ((t, fate), exceedance)) in times.iter().zip(fates).zip(exceedances).enumerate() {
        if *fate != Fate::Compared {
            continue;
        }
        any_compared = true;
        if *exceedance > 0.0 {
            if first.is_none() {
                first = Some(*t);
            }
            if worst.is_none_or(|w| *exceedance > w.exceedance) {
                worst = Some(Exceedance {
                    t: *t,
                    exceedance: *exceedance,
                });
            }
            if let Some(next) = times.get(k + 1) {
                time_s += next - t;
            }
        }
    }
    let mut intervals = Vec::new();
    let mut open: Option<MaskedInterval> = None;
    for (t, fate) in times.iter().zip(fates) {
        if *fate == Fate::Masked {
            match &mut open {
                Some(run) => run.to = *t,
                None => open = Some(MaskedInterval { from: *t, to: *t }),
            }
        } else if let Some(run) = open.take() {
            intervals.push(run);
        }
    }
    intervals.extend(open);
    let status = if !any_compared {
        OutputStatus::NoData
    } else if first.is_some() {
        OutputStatus::Fail
    } else {
        OutputStatus::Pass
    };
    Verdict {
        status,
        worst: worst.filter(|_| status == OutputStatus::Fail),
        first: first.filter(|_| status == OutputStatus::Fail),
        exceedance_time_s: time_s,
        masked_intervals: capped(intervals, MAX_MASKED_INTERVALS),
    }
}

/// A Boolean or Integer output's exceedances: `1` at a compared instant where the two differ,
/// `0` at a masked one (zeroed in both), a NaN at an uncovered one.
pub(super) fn exact(fates: &[Fate], engine: &[i64], trend: &[i64]) -> Vec<f64> {
    fates
        .iter()
        .zip(engine.iter().zip(trend))
        .map(|(fate, (engine, trend))| match fate {
            Fate::Compared => f64::from(u8::from(engine != trend)),
            Fate::Masked => 0.0,
            Fate::Uncovered => f64::NAN,
        })
        .collect()
}

/// `values` with every instant that is not compared zeroed.
pub(super) fn zeroed(fates: &[Fate], values: Vec<f64>) -> Vec<f64> {
    values
        .into_iter()
        .zip(fates)
        .map(|(y, fate)| if *fate == Fate::Compared { y } else { 0.0 })
        .collect()
}

/// The engine series (zeroed where not compared) at `instants` checked for a non-finite or
/// out-of-range sample: at the first, `report.worst` is that instant with the exceedance positive
/// infinity, and the exceedances are positive infinity at every compared instant and a NaN at
/// every other. `None` when every sample is in range.
pub(super) fn non_finite_engine(
    instants: &[f64],
    fates: &[Fate],
    engine: &[f64],
    report: &mut OutputReport,
) -> Option<Vec<f64>> {
    let t = instants
        .iter()
        .zip(engine)
        .find(|(_, y)| !(y.is_finite() && y.abs() <= MAX_ENGINE_MAGNITUDE))
        .map(|(t, _)| *t)?;
    report.worst = Some(Exceedance {
        t,
        exceedance: f64::INFINITY,
    });
    Some(
        fates
            .iter()
            .map(|fate| match fate {
                Fate::Compared => f64::INFINITY,
                Fate::Masked | Fate::Uncovered => f64::NAN,
            })
            .collect(),
    )
}

/// The envelope's refusal of the output at `path` as the run's `compare-refused`, the envelope's
/// code its cause.
pub(super) fn compare_refused(path: &str, envelope: Envelope, error: &CompareError) -> VerifyError {
    VerifyError::new(
        Kind::CompareRefused,
        path,
        format!("the {envelope} envelope gave no result: {error}"),
    )
    .with_path(path)
    .with_cause(compare_refusal_cause(error))
}

/// A Real output, through the envelope: its exceedances and, when kept, its bound curves. Fills
/// the report's half-sizes and masked exceedances, and its `worst` for a non-finite engine
/// sample.
fn compare_real(
    plan: &OutputPlan<'_>,
    comparing: &Comparing,
    fates: &[Fate],
    (engine, trend): (Vec<f64>, Vec<f64>),
    report: &mut OutputReport,
) -> Result<(Vec<f64>, Option<OutputBounds>), VerifyError> {
    let engine = zeroed(fates, engine);
    let trend = zeroed(fates, trend);
    if report.compared == 0 {
        // No envelope is built for an output with nothing compared.
        return Ok((vec![f64::NAN; plan.instants.len()], None));
    }
    if let Some(exceedances) = non_finite_engine(plan.instants, fates, &engine, report) {
        return Ok((exceedances, None));
    }
    let (reference, test) = match comparing.role {
        FunnelReference::Engine => (&engine, &trend),
        FunnelReference::Trend => (&trend, &engine),
    };
    let result = comparing
        .envelope
        .compare(
            Series::new(plan.instants, reference),
            Series::new(plan.instants, test),
            &plan.tolerances.resolved(),
            &comparing.limits,
        )
        .map_err(|error| compare_refused(plan.path, comparing.envelope, &error))?;
    report.half_width = Some(result.half_width());
    report.half_height = Some(result.half_height());
    let mut masked_exceedance = Vec::new();
    let exceedances: Vec<f64> = fates
        .iter()
        .zip(result.errors_at_test())
        .zip(plan.instants)
        .map(|((fate, error), t)| match fate {
            Fate::Compared => *error,
            Fate::Masked => {
                if *error > 0.0 {
                    masked_exceedance.push(Exceedance {
                        t: *t,
                        exceedance: *error,
                    });
                }
                *error
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
    Ok((exceedances, bounds))
}

#[cfg(test)]
#[path = "compare_output_tests.rs"]
mod tests;
