//! The bounds a commissioning run is checked against before its replay runs (commissioning note,
//! decisions 22 and 42; the review of GitHub #744).
//!
//! With a `sampling`, every comparison of a run is at the reference's instants, one count for
//! every output ([`super::prepare::Instants::count`]). Without one, each output's count is an
//! upper bound from what is known before the replay ([`change_of_value_count`]): the frames, its
//! trend samples and its indicator files' samples within the window. Each bound is arithmetic on
//! those counts, the outputs and their indicators, decided before anything that size is built,
//! so the envelope's own `too-many-points` is never reached:
//!
//! - **per output**, the points at most the envelope's limit: `max_aligned_grid_points` for a Real
//!   output under `aligned-grid`, `max_funnel_points` for every other (an exact comparison is
//!   linear too); `too-many-points`, with the output's path;
//! - **the run's work**, `instants × (1 + indicators)` summed over the compared outputs, at most
//!   [`MAX_RUN_POINTS`] (decision 42): the comparison is linear in the count, and masking an
//!   output reads each of its indicators once per instant; `too-many-points`;
//! - **the grid envelope's quadratic work**, `points²` summed over the Real outputs under
//!   `aligned-grid`, at most [`CommissionOptions::max_total_aligned_grid_work`], the bound a
//!   vector set's tolerance windows are held to; `too-many-points`;
//! - **the trace**, `frames × bytes per frame` at most the replay's `max_trace_bytes`, so a run
//!   too long for its trace is refused before the trace is allocated, as `replay-failed` with the
//!   cause `trace-too-large`, the word replay itself gives.
//!
//! [`CommissionOptions::max_total_aligned_grid_work`]: super::CommissionOptions::max_total_aligned_grid_work

use oce_api::{InputDefinition, ValueType};

use super::bind_reference::BoundReference;
use super::report::MAX_RUN_POINTS;
use crate::compare_limits::CompareLimits;
use crate::envelope::Envelope;
use crate::verify_error::{VerifyError, VerifyErrorKind as Kind};

/// What the point bounds are checked against.
pub(crate) struct Caps<'a> {
    /// Each compared output's counts, in the order of the bound reference's `compared`.
    pub(crate) outputs: &'a [OutputCount],
    /// Whether the reference gives no `sampling`, so its outputs are compared by change of
    /// value ([`change_of_value_count`]).
    pub(crate) change_of_value: bool,
    /// The reference's envelope.
    pub(crate) envelope: Envelope,
    /// The comparison's limits, already effective.
    pub(crate) limits: &'a CompareLimits,
    /// The bound on the grid envelope's work, already lowered to its maximum.
    pub(crate) aligned_grid_work: u64,
}

/// What one output's comparison can reach, decided before the replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OutputCount {
    /// The instants its fates and its indicators are read at.
    pub(crate) instants: u64,
    /// The points the envelope is handed per series for a Real output, or the instants an exact
    /// comparison reads: what the per-output cap holds.
    pub(crate) points: u64,
}

impl OutputCount {
    /// A grid output: `count` instants, and one point per instant in each series.
    pub(crate) fn grid(count: u64) -> Self {
        Self {
            instants: count,
            points: count,
        }
    }
}

/// The most a change-of-value comparison of one output can reach, from `samples`: the frames,
/// the trend samples and the samples of the output's indicator files within the window, all
/// known before the replay. Its series instants are at most `samples + 2` (with `start` and
/// `stop`; the engine's executed frames, a trace indicator's samples and every end of an
/// uncovered stretch are frame times). A Real output hands the envelope two step series of at
/// most one point per instant and one carry per change after the first, `2 × instants - 1`; a
/// Boolean or Integer output is compared at its instants.
pub(crate) fn change_of_value_count(samples: u64, value_type: ValueType) -> OutputCount {
    let instants = samples.saturating_add(2);
    let points = if value_type == ValueType::Real {
        instants.saturating_mul(2) - 1
    } else {
        instants
    };
    OutputCount { instants, points }
}

/// The work one output adds to the run: its instants once for the comparison and once per
/// indicator for the mask.
pub(crate) fn output_points(count: u64, indicators: usize) -> u64 {
    count.saturating_mul(
        u64::try_from(indicators)
            .unwrap_or(u64::MAX)
            .saturating_add(1),
    )
}

fn too_many(place: &str, detail: String) -> VerifyError {
    VerifyError::new(Kind::TooManyPoints, format!("{place}.outputs"), detail)
}

/// The three point bounds of the module documentation, in its order.
///
/// # Errors
/// `too-many-points` for the first bound passed.
pub(crate) fn check_points(
    bound: &BoundReference,
    caps: &Caps<'_>,
    place: &str,
) -> Result<(), VerifyError> {
    let mut total: u64 = 0;
    let mut aligned: u64 = 0;
    for (output, count) in bound.compared.iter().zip(caps.outputs) {
        let grid_envelope =
            output.value_type == ValueType::Real && caps.envelope == Envelope::AlignedGrid;
        let cap = if grid_envelope {
            caps.limits.max_aligned_grid_points
        } else {
            caps.limits.max_funnel_points
        };
        let cap = u64::try_from(cap).unwrap_or(u64::MAX);
        if count.points > cap {
            let how = if output.value_type == ValueType::Real {
                caps.envelope.as_str()
            } else {
                "an exact comparison"
            };
            let detail = if caps.change_of_value {
                format!(
                    "output {} would be compared by change of value on up to {} points; at most {cap} under {how}",
                    output.path, count.points
                )
            } else {
                format!(
                    "output {} would be compared on {} instants; at most {cap} under {how}",
                    output.path, count.points
                )
            };
            return Err(too_many(place, detail).with_path(output.path.clone()));
        }
        total = total.saturating_add(output_points(count.instants, output.indicators.len()));
        if grid_envelope {
            aligned = aligned.saturating_add(count.points.saturating_mul(count.points));
        }
    }
    run_points_within(total, place)?;
    if aligned > caps.aligned_grid_work {
        return Err(too_many(
            place,
            format!(
                "the aligned-grid envelope would do {aligned} units of work over the outputs (the instants squared); at most {}",
                caps.aligned_grid_work
            ),
        ));
    }
    Ok(())
}

/// The run's work together against [`MAX_RUN_POINTS`] (decision 42), inclusive.
///
/// # Errors
/// `too-many-points` past it.
pub(crate) fn run_points_within(total: u64, place: &str) -> Result<(), VerifyError> {
    if total > MAX_RUN_POINTS {
        return Err(too_many(
            place,
            format!(
                "the outputs and their indicators would be compared on {total} points together; at most {MAX_RUN_POINTS} in one run"
            ),
        ));
    }
    Ok(())
}

/// The bytes one trace row takes: the time (8) and the status (1), every output and every
/// retained input at its kind's width (8 a Real or an Integer, 1 a Boolean, 4 an Enum ordinal),
/// and one substitution bit per input when an input can be held: replay's own formula
/// (`Trace::bytes_per_frame`).
pub(crate) fn bytes_per_frame(
    inputs: &[InputDefinition],
    output_types: &[ValueType],
    holds: bool,
) -> u64 {
    let width = |value_type: ValueType| match value_type {
        ValueType::Boolean => 1,
        ValueType::Enum(_) => 4,
        ValueType::Real | ValueType::Integer | ValueType::String => 8,
    };
    let outputs: u64 = output_types.iter().map(|t| width(*t)).sum();
    let retained: u64 = inputs.iter().map(|d| width(d.value_type)).sum();
    let mask = if holds {
        u64::try_from(inputs.len().div_ceil(8)).unwrap_or(u64::MAX)
    } else {
        0
    };
    9 + outputs + retained + mask
}

/// The trace bound of the module documentation: `frames` rows of `row_bytes` within
/// `max_trace_bytes`.
///
/// # Errors
/// `replay-failed` with the cause `trace-too-large`.
pub(crate) fn check_trace_bytes(
    frames: usize,
    row_bytes: u64,
    max_trace_bytes: u64,
    place: &str,
) -> Result<(), VerifyError> {
    let bytes = u64::try_from(frames)
        .unwrap_or(u64::MAX)
        .saturating_mul(row_bytes);
    if bytes > max_trace_bytes {
        return Err(VerifyError::new(
            Kind::ReplayFailed,
            place,
            format!(
                "the trace of {frames} frames would take {bytes} bytes at {row_bytes} a row; at most {max_trace_bytes}"
            ),
        )
        .with_cause("trace-too-large"));
    }
    Ok(())
}

#[cfg(test)]
#[path = "run_bounds_tests.rs"]
mod tests;
