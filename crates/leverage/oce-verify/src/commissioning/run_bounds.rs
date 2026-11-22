//! The bounds a commissioning run is checked against before its replay runs (commissioning note,
//! decisions 22 and 42; the review of GitHub #744).
//!
//! Every comparison of a run is at the reference's instants, one count for every output
//! ([`super::prepare::Instants::count`]), so each bound is arithmetic on that count, the
//! outputs and their indicators, decided before anything that size is built:
//!
//! - **per output**, the count at most the envelope's limit: `max_aligned_grid_points` for a Real
//!   output under `aligned-grid`, `max_funnel_points` for every other (an exact comparison is
//!   linear too); `too-many-points`, with the output's path;
//! - **the run's work**, `count × (1 + indicators)` summed over the compared outputs, at most
//!   [`MAX_RUN_POINTS`] (decision 42): the comparison is linear in the count, and masking an
//!   output reads each of its indicators once per instant; `too-many-points`;
//! - **the grid envelope's quadratic work**, `count²` summed over the Real outputs under
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
    /// The reference's instant count.
    pub(crate) count: u64,
    /// The reference's envelope.
    pub(crate) envelope: Envelope,
    /// The comparison's limits, already effective.
    pub(crate) limits: &'a CompareLimits,
    /// The bound on the grid envelope's work, already lowered to its maximum.
    pub(crate) aligned_grid_work: u64,
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
    for output in &bound.compared {
        let grid_envelope =
            output.value_type == ValueType::Real && caps.envelope == Envelope::AlignedGrid;
        let cap = if grid_envelope {
            caps.limits.max_aligned_grid_points
        } else {
            caps.limits.max_funnel_points
        };
        let cap = u64::try_from(cap).unwrap_or(u64::MAX);
        if caps.count > cap {
            let how = if output.value_type == ValueType::Real {
                caps.envelope.as_str()
            } else {
                "an exact comparison"
            };
            return Err(too_many(
                place,
                format!(
                    "output {} would be compared on {} instants; at most {cap} under {how}",
                    output.path, caps.count
                ),
            )
            .with_path(output.path.clone()));
        }
        total = total.saturating_add(output_points(caps.count, output.indicators.len()));
        if grid_envelope {
            aligned = aligned.saturating_add(caps.count.saturating_mul(caps.count));
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
