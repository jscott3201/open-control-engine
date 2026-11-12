//! One scenario's trace compared with its expectations (vectors note, "After a scenario's run"
//! step 4 and "The receipt"; decisions 8, 12, 22 and 23).
//!
//! - **Windows** are exact for every type: an Integer or a Boolean by value, a Real by its bits,
//!   so `0` on a Real output is `+0.0` and does not equal `-0.0`, and no finite literal equals a
//!   NaN. One window tick is one instant.
//! - **A diagnostic expectation** holds when it is paired with a warning of the frame at its
//!   tick whose `block` equals the expectation's and, when it gives one, whose `message` equals it
//!   byte for byte. Pairing is one to one, in the order [`crate::pairing`] gives. It is one
//!   instant.
//! - **A refused frame** stops the scenario's run: instants before its tick are compared and
//!   counted, instants at and after it are not, and the refusal is one `frame-refused` failure.
//!   It is not an instant.
//! - **Warnings** no diagnostic expectation took are listed and never change a verdict.
//!
//! Failures are recorded by tick, then by expectation index, at most
//! [`crate::VerifyLimits::max_recorded_failures`], with the total kept. A `frame-refused` record
//! is always kept (owner ruling of 2026-11-10): when a frame was refused, the last slot of the
//! list is reserved for it, so earlier mismatches never push it out; with a cap of zero no record
//! is listed. Unmatched warnings are listed in row then emission order, at most
//! [`crate::VerifyLimits::max_recorded_warnings`], with the total kept. Memory is bounded by the
//! caps, not by the run: each expectation keeps at most the cap of its own earliest failures,
//! which holds every record the scenario's first records can be.

use oce_replay::{DiagnosticRow, EngineRefusal, Trace, TraceColumn};

use crate::bind::{BoundDiagnostic, BoundExpectation, BoundWindow};
use crate::pairing::{Pairing, pair};
use crate::receipt::{Failure, InstantCounts, ReceiptValue, UnmatchedWarning};
use crate::verify_error::{VerifyError, VerifyErrorKind};

/// What comparing one scenario found.
pub(crate) struct Compared {
    /// The scenario's instants.
    pub(crate) instants: InstantCounts,
    /// The first failure records, by tick then expectation index.
    pub(crate) failures: Vec<Failure>,
    /// Failures before the cap.
    pub(crate) failures_total: u64,
    /// The first unmatched warnings, in row then emission order.
    pub(crate) warnings: Vec<UnmatchedWarning>,
    /// Unmatched warnings before the cap.
    pub(crate) warnings_total: u64,
}

/// The caps on the recorded lists.
#[derive(Clone, Copy)]
pub(crate) struct Caps {
    /// The failure records kept.
    pub(crate) failures: usize,
    /// The unmatched warnings kept.
    pub(crate) warnings: usize,
}

/// A failure with the key it is ordered by: its tick, then its expectation index.
struct Keyed {
    tick: u64,
    expectation: usize,
    failure: Failure,
}

/// Compare `trace` with `expectations`. `refused` is the refused row and its class when a frame
/// stopped the run; `outputs` are the boundary outputs in slot order; `subject` names the
/// scenario in a refusal.
///
/// # Errors
/// `replay-failed` with the cause `output-set-changed` when the trace's columns are not the slot
/// map's or lack a row binding reached (a defence: replay fixes its columns from the same map and
/// writes one row per tick).
pub(crate) fn compare(
    trace: &Trace,
    refused: Option<(u64, EngineRefusal)>,
    expectations: &[BoundExpectation],
    outputs: &[String],
    caps: Caps,
    subject: &str,
) -> Result<Compared, VerifyError> {
    let rows = u64::try_from(trace.rows()).unwrap_or(u64::MAX);
    // The ticks that are compared: every row before the refused one, or every row.
    let compared = refused.map_or(rows, |(row, _)| row.min(rows));
    let pairing = pair(trace.diagnostics(), expectations, compared);
    let mut instants = InstantCounts::default();
    let mut kept: Vec<Keyed> = Vec::new();
    let mut failures_total = 0_u64;
    for (expectation, bound) in expectations.iter().enumerate() {
        let (checked, failed, earliest) = match bound {
            BoundExpectation::Window(window) => {
                compare_window(trace, window, expectation, compared, outputs, caps, subject)?
            }
            BoundExpectation::Diagnostic(diagnostic) => {
                compare_diagnostic(trace, diagnostic, expectation, compared, &pairing, subject)?
            }
        };
        instants.checked += checked;
        instants.failed += failed;
        failures_total += failed;
        kept.extend(earliest);
    }
    instants.passed = instants.checked - instants.failed;
    kept.sort_by_key(|keyed| (keyed.tick, keyed.expectation));
    // The refusal's record is always kept: when a frame was refused, the earlier records leave
    // the last slot for it.
    let room = match refused {
        Some(_) => caps.failures.saturating_sub(1),
        None => caps.failures,
    };
    let mut failures: Vec<Failure> = kept
        .into_iter()
        .take(room)
        .map(|keyed| keyed.failure)
        .collect();
    if let Some((row, refusal)) = refused {
        failures_total += 1;
        if caps.failures > 0 {
            failures.push(Failure::FrameRefused {
                t: time_of(trace, row, subject)?,
                refusal,
            });
        }
    }
    let (warnings, warnings_total) = unmatched(trace.diagnostics(), &pairing, caps.warnings);
    Ok(Compared {
        instants,
        failures,
        failures_total,
        warnings,
        warnings_total,
    })
}

/// The time of trace row `row`, seconds.
///
/// # Errors
/// `replay-failed` with the cause `output-set-changed` for a row the trace does not have: every
/// caller asks for a row the run wrote, so that is a trace that is not the one binding expects,
/// and a time is never made up for it.
fn time_of(trace: &Trace, row: u64, subject: &str) -> Result<f64, VerifyError> {
    usize::try_from(row)
        .ok()
        .and_then(|row| trace.times().get(row))
        .copied()
        .ok_or_else(|| {
            VerifyError::new(
                VerifyErrorKind::ReplayFailed,
                subject,
                format!(
                    "the trace has {} rows and no row {row}",
                    trace.times().len()
                ),
            )
            .with_cause("output-set-changed")
        })
}

/// The output column of slot `slot`, checked against the slot map's path.
fn column<'a>(
    trace: &'a Trace,
    slot: usize,
    outputs: &[String],
    subject: &str,
) -> Result<&'a TraceColumn, VerifyError> {
    match (trace.outputs().get(slot), outputs.get(slot)) {
        (Some((path, column)), Some(expected)) if path == expected => Ok(column),
        _ => Err(VerifyError::new(
            VerifyErrorKind::ReplayFailed,
            subject,
            format!("the trace has no output column for output slot {slot}"),
        )
        .with_cause("output-set-changed")),
    }
}

/// The value of `column` at `row`, typed as the column is.
fn cell(column: &TraceColumn, row: usize) -> Option<ReceiptValue> {
    match column {
        TraceColumn::Real(values) => values.get(row).copied().map(ReceiptValue::Real),
        TraceColumn::Integer(values) => values.get(row).copied().map(ReceiptValue::Integer),
        TraceColumn::Boolean(values) => values
            .get(row)
            .map(|byte| ReceiptValue::Boolean(*byte != 0)),
        // Binding refuses every Enum output, so no compared column is one.
        TraceColumn::Enum(_) => None,
    }
}

/// One window: its instants, its failures and its earliest `caps.failures` failure records.
fn compare_window(
    trace: &Trace,
    window: &BoundWindow,
    expectation: usize,
    compared: u64,
    outputs: &[String],
    caps: Caps,
    subject: &str,
) -> Result<(u64, u64, Vec<Keyed>), VerifyError> {
    // Ticks at and after `compared` did not run, or ran no further than a refused frame.
    let Some(last) = compared
        .checked_sub(1)
        .map(|last| last.min(window.last_tick))
    else {
        return Ok((0, 0, Vec::new()));
    };
    if window.first_tick > last {
        return Ok((0, 0, Vec::new()));
    }
    let values = column(trace, window.output, outputs, subject)?;
    let (mut checked, mut failed) = (0_u64, 0_u64);
    let mut earliest = Vec::new();
    for tick in window.first_tick..=last {
        let row = usize::try_from(tick).unwrap_or(usize::MAX);
        // Every row before `compared` holds one cell per output, of the output's declared type;
        // a cell that cannot be read means the columns are not the ones binding typed.
        let Some(actual) = cell(values, row) else {
            return Err(VerifyError::new(
                VerifyErrorKind::ReplayFailed,
                subject,
                format!(
                    "the trace's column for {} holds no value of its declared type at row {row}",
                    outputs[window.output]
                ),
            )
            .with_cause("output-set-changed"));
        };
        checked += 1;
        if actual.bit_eq(&window.equals) {
            continue;
        }
        failed += 1;
        if earliest.len() < caps.failures {
            earliest.push(Keyed {
                tick,
                expectation,
                failure: Failure::ValueMismatch {
                    expectation: u64::try_from(expectation).unwrap_or(u64::MAX),
                    output: outputs[window.output].clone(),
                    t: time_of(trace, tick, subject)?,
                    expected: window.equals,
                    actual,
                },
            });
        }
    }
    Ok((checked, failed, earliest))
}

/// One diagnostic expectation: one instant when its tick ran, passed when the pairing gave it a
/// warning, and its failure record otherwise.
fn compare_diagnostic(
    trace: &Trace,
    diagnostic: &BoundDiagnostic,
    expectation: usize,
    compared: u64,
    pairing: &Pairing,
    subject: &str,
) -> Result<(u64, u64, Vec<Keyed>), VerifyError> {
    if diagnostic.tick >= compared {
        return Ok((0, 0, Vec::new()));
    }
    if pairing.matched.get(expectation).copied().unwrap_or(false) {
        return Ok((1, 0, Vec::new()));
    }
    let failure = Failure::DiagnosticMissing {
        expectation: u64::try_from(expectation).unwrap_or(u64::MAX),
        t: time_of(trace, diagnostic.tick, subject)?,
        block: diagnostic.block.clone(),
        message: diagnostic.message.clone(),
    };
    Ok((
        1,
        1,
        vec![Keyed {
            tick: diagnostic.tick,
            expectation,
            failure,
        }],
    ))
}

/// The warnings no diagnostic expectation took, in row then emission order: the first `cap` and
/// the total.
fn unmatched(
    rows: &[DiagnosticRow],
    pairing: &Pairing,
    cap: usize,
) -> (Vec<UnmatchedWarning>, u64) {
    let mut warnings = Vec::new();
    let mut total = 0_u64;
    for (index, row) in rows.iter().enumerate() {
        if pairing.taken.get(index).copied().unwrap_or(false) {
            continue;
        }
        total += 1;
        if warnings.len() < cap {
            warnings.push(UnmatchedWarning {
                t: row.time,
                block: row.block.clone(),
                message: row.message.clone(),
            });
        }
    }
    (warnings, total)
}
