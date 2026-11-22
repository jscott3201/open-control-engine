//! One tolerance window judged under the set's envelope (vectors note, "Tolerance windows";
//! decisions 38, 40, 41, 44, 45 and 48).
//!
//! Both series are sampled at the window's compared ticks and nowhere else, `x` the tick's time
//! in seconds:
//!
//! - the **reference** is `of` by zero-order hold: at a tick, the value of the last step at or
//!   before it;
//! - the **test** is the engine's output at the tick.
//!
//! Then, in this order:
//!
//! 1. **The mask.** At a masked tick both series are zero (OBC §12.7.2 step 8); the tick counts
//!    as `masked`, never `checked`.
//! 2. **An out-of-range output** at an unmasked tick (a NaN, an infinity, or above 2^1000 in
//!    magnitude) fails that instant with the exceedance positive infinity, and the test carries
//!    the reference's value there, so every other tick is judged as it would be had that one
//!    passed.
//! 3. **Self-enclosure.** One call of the envelope with the reference as both series. A
//!    [`crate::CompareError`] from it, a bound past 2^1000 in magnitude
//!    (`non-finite-envelope`) or any error above zero (`reference-outside-envelope`) is
//!    `compare-refused`, an error with no receipt. It reads the reference and the mask only,
//!    never the output's values.
//! 4. **The judgement.** A second call, with the test. An instant's exceedance is the error the
//!    envelope reports at its tick: exactly zero passes, anything above fails it as
//!    `tolerance-exceeded`. Under `aligned-grid` the errors it also reports between ticks are not
//!    read. With every bound and every test value inside 2^1000 each error is a difference of
//!    two such numbers, so no output can make this call refuse; if it did, that is
//!    `compare-refused` too, a defence no loaded set reaches.

use oce_replay::{Trace, TraceColumn};

use crate::compare::{Caps, Counted, Envelopes, Keyed, time_of, typed_cell};
use crate::envelope::{FunnelResult, Series, Tolerances};
use crate::receipt::{Failure, ReceiptValue};
use crate::verify_error::{
    REFERENCE_OUTSIDE_ENVELOPE, VerifyError, VerifyErrorKind, compare_refusal_cause,
};

/// 2^1000: the largest magnitude an output, and a bound, may have and still be judged.
pub(crate) const RANGE: f64 = f64::from_bits(2023 << 52);

/// Whether `value` is judged: finite and at most 2^1000 in magnitude.
pub(crate) fn in_range(value: f64) -> bool {
    value.abs() <= RANGE
}

/// One tolerance window over its compared ticks, and what it is compared with.
pub(crate) struct WithinWindow<'a> {
    /// The scenario's trace.
    pub(crate) trace: &'a Trace,
    /// The output's column.
    pub(crate) values: &'a TraceColumn,
    /// The output's canonical path.
    pub(crate) output: &'a str,
    /// The window's tolerances.
    pub(crate) tolerances: &'a Tolerances,
    /// The expected signal's steps, `(t, value)` in time order.
    pub(crate) of: &'a [(f64, f64)],
    /// The first compared tick.
    pub(crate) first: u64,
    /// The last compared tick.
    pub(crate) last: u64,
    /// The mask over `first..=last`, or `None` for no mask.
    pub(crate) masked: Option<&'a [bool]>,
    /// The expectation's index in the scenario.
    pub(crate) expectation: usize,
}

/// What each compared tick holds before the envelope is called.
enum Instant {
    /// A `when` excluded the tick.
    Masked,
    /// An out-of-range output: the actual value, which fails the instant.
    OutOfRange(f64),
    /// An output to judge.
    Judged(f64),
}

/// Judge one tolerance window, in the order the module documentation gives.
///
/// # Errors
/// `compare-refused` with a cause from [`crate::COMPARE_REFUSAL_CAUSES`]; `replay-failed` with
/// the cause `output-set-changed` for a trace that is not the one binding expects.
pub(crate) fn compare_within(
    window: &WithinWindow<'_>,
    caps: Caps,
    envelopes: Envelopes,
    subject: &str,
) -> Result<Counted, VerifyError> {
    let place = format!("{subject}.expect[{}]", window.expectation);
    let length = usize::try_from(window.last - window.first + 1).unwrap_or(usize::MAX);
    let mut x = Vec::with_capacity(length);
    let mut reference = Vec::with_capacity(length);
    let mut test = Vec::with_capacity(length);
    let mut instants = Vec::with_capacity(length);
    let mut step = 0;
    for (offset, tick) in (window.first..=window.last).enumerate() {
        let time = time_of(window.trace, tick, subject)?;
        while step + 1 < window.of.len() && window.of[step + 1].0 <= time {
            step += 1;
        }
        let expected = window.of.get(step).map_or(0.0, |(_, value)| *value);
        let masked = window.masked.is_some_and(|mask| !mask[offset]);
        let instant = if masked {
            Instant::Masked
        } else {
            let row = usize::try_from(tick).unwrap_or(usize::MAX);
            match typed_cell(window.values, row, window.output, subject)? {
                ReceiptValue::Real(actual) if in_range(actual) => Instant::Judged(actual),
                ReceiptValue::Real(actual) => Instant::OutOfRange(actual),
                _ => {
                    return Err(VerifyError::new(
                        VerifyErrorKind::ReplayFailed,
                        subject,
                        format!("the trace's column for {} is not Real", window.output),
                    )
                    .with_cause("output-set-changed"));
                }
            }
        };
        let (reference_y, test_y) = match instant {
            Instant::Masked => (0.0, 0.0),
            Instant::OutOfRange(_) => (expected, expected),
            Instant::Judged(actual) => (expected, actual),
        };
        x.push(time);
        reference.push(reference_y);
        test.push(test_y);
        instants.push(instant);
    }
    let refused = |cause: &'static str, detail: String| {
        VerifyError::new(VerifyErrorKind::CompareRefused, place.clone(), detail)
            .with_path(window.output)
            .with_cause(cause)
    };
    let compare = |test: &[f64]| {
        envelopes
            .envelope
            .compare(
                Series::new(&x, &reference),
                Series::new(&x, test),
                window.tolerances,
                &envelopes.limits,
            )
            .map_err(|error| {
                refused(
                    compare_refusal_cause(&error),
                    format!(
                        "the {} envelope gave no result: {error}",
                        envelopes.envelope
                    ),
                )
            })
    };
    let enclosure = compare(&reference)?;
    if !bounds_in_range(&enclosure) {
        return Err(refused(
            "non-finite-envelope",
            "a bound of the window's envelope is past 2^1000 in magnitude".to_owned(),
        ));
    }
    if let Some(at) = enclosure.first_error_x {
        return Err(refused(
            REFERENCE_OUTSIDE_ENVELOPE,
            format!(
                "the expected signal leaves its own {} envelope at {at} s; give the window a y tolerance, an x tolerance that is not a whole number of steps, or name aligned-grid",
                envelopes.envelope
            ),
        ));
    }
    let judged = compare(&test)?;
    let exceedances = at_ticks(&judged, &x).ok_or_else(|| {
        refused(
            "length-mismatch",
            "the envelope reported no error at one of the window's ticks".to_owned(),
        )
    })?;
    let mut counted = Counted::default();
    for (offset, instant) in instants.iter().enumerate() {
        let (actual, exceedance) = match *instant {
            Instant::Masked => {
                counted.masked += 1;
                continue;
            }
            Instant::OutOfRange(actual) => (actual, f64::INFINITY),
            Instant::Judged(actual) => (actual, exceedances[offset]),
        };
        counted.checked += 1;
        if exceedance == 0.0 {
            continue;
        }
        counted.failed += 1;
        if counted.earliest.len() < caps.failures {
            counted.earliest.push(Keyed {
                tick: window.first + u64::try_from(offset).unwrap_or(u64::MAX),
                expectation: window.expectation,
                failure: Failure::ToleranceExceeded {
                    expectation: u64::try_from(window.expectation).unwrap_or(u64::MAX),
                    output: window.output.to_owned(),
                    t: x[offset],
                    expected: ReceiptValue::Real(reference[offset]),
                    actual: ReceiptValue::Real(actual),
                    exceedance,
                },
            });
        }
    }
    Ok(counted)
}

/// Whether every point of both bounds is inside 2^1000 in magnitude.
fn bounds_in_range(result: &FunnelResult) -> bool {
    result
        .lower_bound
        .y()
        .iter()
        .chain(result.upper_bound.y())
        .all(|value| in_range(*value))
}

/// The error `result` reports at each of `ticks`, in order: for `funnel` the errors are the
/// test's instants themselves; for `aligned-grid` they also hold breakpoints between ticks,
/// which are skipped. `None` when a tick has no error, which no envelope gives.
fn at_ticks(result: &FunnelResult, ticks: &[f64]) -> Option<Vec<f64>> {
    let mut errors = result.errors.points();
    ticks
        .iter()
        .map(|tick| {
            errors
                .by_ref()
                .find(|(x, _)| x.to_bits() == tick.to_bits())
                .map(|(_, error)| error)
        })
        .collect()
}

#[cfg(test)]
#[path = "tolerance_tests.rs"]
mod tests;
