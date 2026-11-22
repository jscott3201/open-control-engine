//! Indicators and the mask they give (commissioning note, "Indicators"; decision 2, OBC §12.7.2
//! step 8).
//!
//! An indicator is a signal of the sequence read from the trace (an input, which the trace
//! retains, or an output), sampled at the executed frames, or a column of an `indicatorSources`
//! file. Its values are Boolean or exactly `0` or `1`; any other sample, a NaN included, is
//! `indicator-not-boolean` (owner ruling of 2026-11-21), every sample checked whether or not the
//! window reaches it. An indicator is "held from each sample to the next, and false before the
//! first", and several for one output are ANDed: an instant is compared only where all are true.

use oce_replay::{Trace, TraceColumn};

use super::bind_reference::{BoundIndicator, IndicatorSignal};
use super::trend_types::{SeriesValues, TrendSeries};
use crate::verify_error::{VerifyError, VerifyErrorKind as Kind};

/// One indicator as a Boolean step series.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct IndicatorSeries {
    /// The sample times, non-decreasing.
    pub(crate) times: Vec<f64>,
    /// The value at each.
    pub(crate) values: Vec<bool>,
}

/// A sample that is no Boolean value: what an `indicator-not-boolean` refusal names.
fn not_boolean(name: &str, place: &str, t: f64, value: &str) -> VerifyError {
    VerifyError::new(
        Kind::IndicatorNotBoolean,
        place,
        format!(
            "indicator `{name}` is {value} at t = {t:?} s; an indicator is Boolean, or exactly 0 or 1"
        ),
    )
}

/// A Real indicator sample as a Boolean: exactly `0` (either sign) or `1`.
fn real_flag(x: f64) -> Option<bool> {
    if x == 0.0 {
        Some(false)
    } else if x == 1.0 {
        Some(true)
    } else {
        None
    }
}

/// An Integer indicator sample as a Boolean: exactly `0` or `1`.
fn integer_flag(x: i64) -> Option<bool> {
    match x {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

/// The values of `column` at `rows`, as flags, or the first sample that is none.
fn trace_flags(
    column: &TraceColumn,
    rows: &[usize],
    times: &[f64],
) -> Result<Vec<bool>, (f64, String)> {
    let mut flags = Vec::with_capacity(rows.len());
    for &row in rows {
        let flag = match column {
            TraceColumn::Boolean(values) => Some(values[row] == 1),
            TraceColumn::Integer(values) => integer_flag(values[row]),
            TraceColumn::Real(values) => real_flag(values[row]),
            TraceColumn::Enum(values) => {
                return Err((
                    times[row],
                    format!("the enumeration ordinal {}", values[row]),
                ));
            }
        };
        let Some(flag) = flag else {
            let shown = match column {
                TraceColumn::Integer(values) => values[row].to_string(),
                TraceColumn::Real(values) => format!("{:?}", values[row]),
                TraceColumn::Boolean(_) | TraceColumn::Enum(_) => String::new(),
            };
            return Err((times[row], shown));
        };
        flags.push(flag);
    }
    Ok(flags)
}

/// The series of `indicator`, from the trace's executed `rows` or from its file.
///
/// # Errors
/// `indicator-not-boolean` for its first sample that is not Boolean or exactly `0` or `1`.
pub(crate) fn indicator_series(
    indicator: &BoundIndicator,
    trace: &Trace,
    rows: &[usize],
    files: &[TrendSeries],
    place: &str,
) -> Result<IndicatorSeries, VerifyError> {
    let refused = |(t, value): (f64, String)| not_boolean(&indicator.name, place, t, &value);
    match &indicator.signal {
        IndicatorSignal::Input(path) | IndicatorSignal::Output(path) => {
            let columns = match &indicator.signal {
                IndicatorSignal::Input(_) => trace.inputs().unwrap_or_default(),
                _ => trace.outputs(),
            };
            // A trace whose engine accepted no frame has no column yet: no sample, so the
            // indicator is false throughout.
            let Some((_, column)) = columns.iter().find(|(name, _)| name == path) else {
                return Ok(IndicatorSeries {
                    times: Vec::new(),
                    values: Vec::new(),
                });
            };
            let values = trace_flags(column, rows, trace.times()).map_err(refused)?;
            Ok(IndicatorSeries {
                times: rows.iter().map(|row| trace.times()[*row]).collect(),
                values,
            })
        }
        IndicatorSignal::File(index) => {
            let series = &files[*index];
            let values = match &series.values {
                SeriesValues::Boolean(values) => values.clone(),
                SeriesValues::Integer(values) => values
                    .iter()
                    .zip(&series.times)
                    .map(|(x, t)| integer_flag(i64::from(*x)).ok_or((*t, x.to_string())))
                    .collect::<Result<_, _>>()
                    .map_err(refused)?,
                SeriesValues::Real(values) => values
                    .iter()
                    .zip(&series.times)
                    .map(|(x, t)| real_flag(*x).ok_or((*t, format!("{x:?}"))))
                    .collect::<Result<_, _>>()
                    .map_err(refused)?,
            };
            Ok(IndicatorSeries {
                times: series.times.clone(),
                values,
            })
        }
    }
}

/// Whether each of `instants` (non-decreasing) is compared: every indicator in `indicators` true
/// there, each held from its sample to the next and false before its first. With no indicator
/// every instant is.
pub(crate) fn active_at(indicators: &[&IndicatorSeries], instants: &[f64]) -> Vec<bool> {
    let mut active = vec![true; instants.len()];
    for indicator in indicators {
        let mut next = 0;
        let mut value = false;
        for (slot, t) in active.iter_mut().zip(instants) {
            while next < indicator.times.len() && indicator.times[next] <= *t {
                value = indicator.values[next];
                next += 1;
            }
            *slot &= value;
        }
    }
    active
}

#[cfg(test)]
#[path = "indicator_mask_tests.rs"]
mod tests;
