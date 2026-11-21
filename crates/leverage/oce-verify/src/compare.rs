//! One scenario's trace compared with its expectations (vectors note, "After a scenario's run"
//! step 4 and "The receipt"; decisions 8, 12, 22 and 23).
//!
//! - **Exact windows** are exact for every type: an Integer or a Boolean by value, a Real by its
//!   bits, so `0` on a Real output is `+0.0` and does not equal `-0.0`, and no finite literal
//!   equals a NaN. One window tick is one instant.
//! - **Tolerance windows** are judged under the set's envelope ([`crate::tolerance`]), one
//!   instant per tick.
//! - **A mask** (`when`, [`crate::mask`]) excludes each tick where one of its signals is false:
//!   the tick is counted as `masked`, never checked; in an exact window it is skipped, in a
//!   tolerance window both series are zero there.
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

use crate::bind::{BoundCheck, BoundDiagnostic, BoundExpectation, BoundWindow};
use crate::compare_limits::CompareLimits;
use crate::envelope::Envelope;
use crate::mask::mask;
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

/// The envelope a set's tolerance windows are compared under, and its limits (already
/// effective).
#[derive(Clone, Copy)]
pub(crate) struct Envelopes {
    /// The set's envelope.
    pub(crate) envelope: Envelope,
    /// The limits of each comparison.
    pub(crate) limits: CompareLimits,
}

/// A failure with the key it is ordered by: its tick, then its expectation index.
pub(crate) struct Keyed {
    pub(crate) tick: u64,
    pub(crate) expectation: usize,
    pub(crate) failure: Failure,
}

/// What comparing one window found: its instants and its earliest failure records.
#[derive(Default)]
pub(crate) struct Counted {
    /// Instants compared.
    pub(crate) checked: u64,
    /// Instants that failed.
    pub(crate) failed: u64,
    /// Ticks a `when` excluded.
    pub(crate) masked: u64,
    /// The earliest failure records, at most the cap.
    pub(crate) earliest: Vec<Keyed>,
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
    envelopes: Envelopes,
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
        let counted = match bound {
            BoundExpectation::Window(window) => compare_window(
                trace,
                window,
                expectation,
                compared,
                outputs,
                caps,
                envelopes,
                subject,
            )?,
            BoundExpectation::Diagnostic(diagnostic) => {
                compare_diagnostic(trace, diagnostic, expectation, compared, &pairing, subject)?
            }
        };
        instants.checked += counted.checked;
        instants.failed += counted.failed;
        instants.masked += counted.masked;
        failures_total += counted.failed;
        kept.extend(counted.earliest);
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
pub(crate) fn time_of(trace: &Trace, row: u64, subject: &str) -> Result<f64, VerifyError> {
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
pub(crate) fn column<'a>(
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
pub(crate) fn cell(column: &TraceColumn, row: usize) -> Option<ReceiptValue> {
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

/// The ticks of `window` that are compared when the ticks before `compared` ran: the first and
/// the last, or `None` when none ran.
pub(crate) fn compared_ticks(window: &BoundWindow, compared: u64) -> Option<(u64, u64)> {
    // Ticks at and after `compared` did not run, or ran no further than a refused frame.
    let last = compared.checked_sub(1)?.min(window.last_tick);
    (window.first_tick <= last).then_some((window.first_tick, last))
}

/// One window, of either form: its instants, its failures, its masked ticks and its earliest
/// `caps.failures` failure records.
#[expect(
    clippy::too_many_arguments,
    reason = "one window's comparison reads the trace, the window and the run's three settings"
)]
fn compare_window(
    trace: &Trace,
    window: &BoundWindow,
    expectation: usize,
    compared: u64,
    outputs: &[String],
    caps: Caps,
    envelopes: Envelopes,
    subject: &str,
) -> Result<Counted, VerifyError> {
    let Some((first, last)) = compared_ticks(window, compared) else {
        return Ok(Counted::default());
    };
    let values = column(trace, window.output, outputs, subject)?;
    let masked = mask(trace, &window.when, first, last, subject)?;
    let equals = match &window.check {
        BoundCheck::Equals(equals) => *equals,
        BoundCheck::Within { tolerances, of } => {
            return crate::tolerance::compare_within(
                &crate::tolerance::WithinWindow {
                    trace,
                    values,
                    output: &outputs[window.output],
                    tolerances,
                    of,
                    first,
                    last,
                    masked: masked.as_deref(),
                    expectation,
                },
                caps,
                envelopes,
                subject,
            );
        }
    };
    let mut counted = Counted::default();
    for tick in first..=last {
        let row = usize::try_from(tick).unwrap_or(usize::MAX);
        if let Some(mask) = &masked
            && !mask[usize::try_from(tick - first).unwrap_or(usize::MAX)]
        {
            counted.masked += 1;
            continue;
        }
        let actual = typed_cell(values, row, &outputs[window.output], subject)?;
        counted.checked += 1;
        if actual.bit_eq(&equals) {
            continue;
        }
        counted.failed += 1;
        if counted.earliest.len() < caps.failures {
            counted.earliest.push(Keyed {
                tick,
                expectation,
                failure: Failure::ValueMismatch {
                    expectation: u64::try_from(expectation).unwrap_or(u64::MAX),
                    output: outputs[window.output].clone(),
                    t: time_of(trace, tick, subject)?,
                    expected: equals,
                    actual,
                },
            });
        }
    }
    Ok(counted)
}

/// The value of `values` at `row`.
///
/// # Errors
/// `replay-failed` with the cause `output-set-changed`: every row before `compared` holds one
/// cell per output, of the output's declared type, so a cell that cannot be read means the
/// columns are not the ones binding typed.
pub(crate) fn typed_cell(
    values: &TraceColumn,
    row: usize,
    output: &str,
    subject: &str,
) -> Result<ReceiptValue, VerifyError> {
    cell(values, row).ok_or_else(|| {
        VerifyError::new(
            VerifyErrorKind::ReplayFailed,
            subject,
            format!(
                "the trace's column for {output} holds no value of its declared type at row {row}"
            ),
        )
        .with_cause("output-set-changed")
    })
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
) -> Result<Counted, VerifyError> {
    if diagnostic.tick >= compared {
        return Ok(Counted::default());
    }
    if pairing.matched.get(expectation).copied().unwrap_or(false) {
        return Ok(Counted {
            checked: 1,
            ..Counted::default()
        });
    }
    let failure = Failure::DiagnosticMissing {
        expectation: u64::try_from(expectation).unwrap_or(u64::MAX),
        t: time_of(trace, diagnostic.tick, subject)?,
        block: diagnostic.block.clone(),
        message: diagnostic.message.clone(),
    };
    Ok(Counted {
        checked: 1,
        failed: 1,
        masked: 0,
        earliest: vec![Keyed {
            tick: diagnostic.tick,
            expectation,
            failure,
        }],
    })
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
