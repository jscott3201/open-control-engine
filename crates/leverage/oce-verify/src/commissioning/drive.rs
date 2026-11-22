//! The one replay of a commissioning run, and what it leaves uncovered (commissioning note,
//! "Sources, sampling and time" and decision 28).
//!
//! Replay runs over the merged trend inputs with the recorded times, equal times allowed, the
//! sources' holds, `OnRefusal::RecordAndContinue` and a trace that retains its inputs (an
//! indicator may name one), driven by `Replay::advance` in chunks. After each chunk the refusal
//! rows the chunk added are read in order:
//!
//! - a row whose cause is a missing sample (no hold, or a hold older than `maxAge`) is the
//!   trend log's gap: it is counted in `frames.refused` and the run goes on;
//! - the first row whose cause is the engine ends the run with no receipt: `run-refused` for a
//!   run-class refusal, `frame-refused` for any other, carrying the class as its cause and the
//!   row, its time and the engine's message cut to 256 bytes.
//!
//! A chunk may run past an engine refusal before it is read; the error is the same whatever the
//! chunk size, since the first engine row is the same row.
//!
//! **Uncovered stretches.** From a frame refused for a missing sample, the engine has no value
//! until the next executed frame (status `Executed` or `Substituted`): the stretch is
//! `[refused time, next executed time)`, empty when the two times are equal, and runs to the
//! window's `stop`, included, when no executed frame follows.

use oce_api::{Engine, InputDefinition};
use oce_replay::{
    EngineRefusal, FrameStatus, InputMap, OnRefusal, Progress, RefusalCause, Replay, ReplayError,
    ReplayLimits, ReplayOptions, Trace, TraceOptions,
};

use super::trend_types::TrendInputs;
use crate::refusal_classes::{engine_refusal_class, replay_failure_cause};
use crate::verify_error::{VerifyError, VerifyErrorKind as Kind};

/// The most bytes of the engine's message a `frame-refused` refusal carries.
pub(crate) const MAX_ENGINE_MESSAGE_BYTES: usize = 256;

/// A stretch of model time the engine has no value for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Stretch {
    /// The time of the refused frame that opened it.
    pub(crate) from: f64,
    /// The time of the executed frame that closed it, excluded; `None` when none did, and the
    /// stretch runs to the window's stop, included.
    pub(crate) until: Option<f64>,
}

impl Stretch {
    /// Whether the instant `t` is in the stretch.
    pub(crate) fn holds(&self, t: f64) -> bool {
        t >= self.from && self.until.is_none_or(|until| t < until)
    }
}

/// `text` cut to at most `limit` bytes at a character boundary.
pub(crate) fn cut(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_owned();
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

fn replay_failed(subject: &str, error: &ReplayError) -> VerifyError {
    VerifyError::new(
        Kind::ReplayFailed,
        subject,
        format!("the replay failed: {error}"),
    )
    .with_cause(replay_failure_cause(error))
}

/// Run the replay of `inputs` on `engine` (edits applied), in chunks of `chunk` frames.
///
/// # Errors
/// `run-refused` or `frame-refused` for the first engine refusal; `replay-failed` for a replay
/// that ends in an error, or one that could not be set up (cause `engine-setup` or `source`).
pub(crate) fn drive(
    engine: Engine,
    definitions: &[InputDefinition],
    inputs: TrendInputs,
    limits: &ReplayLimits,
    chunk: usize,
    subject: &str,
) -> Result<Trace, VerifyError> {
    let setup = |detail: String| {
        VerifyError::new(Kind::ReplayFailed, subject, detail).with_cause("engine-setup")
    };
    let map = InputMap::resolve(
        definitions,
        &inputs.column_specs(),
        &inputs.column_map(),
        &inputs.missing_policy(),
        &inputs.overrides(),
    )
    .map_err(|error| setup(format!("the input map was refused: {error}")))?;
    let mut options = ReplayOptions::new(inputs.time_base());
    options.duplicate_time = inputs.duplicate_time();
    options.on_refusal = OnRefusal::RecordAndContinue;
    options.trace = TraceOptions {
        inputs: true,
        ..TraceOptions::default()
    };
    options.limits = *limits;
    let source = inputs.into_source(&map, limits).map_err(|error| {
        VerifyError::new(
            Kind::ReplayFailed,
            subject,
            format!("the merged trend inputs were refused as a frame source: {error}"),
        )
        .with_cause("source")
    })?;
    let mut replay = Replay::new(engine, map, source, options)
        .map_err(|error| replay_failed(subject, &error))?;
    let mut read = 0;
    loop {
        let progress = replay
            .advance(chunk.max(1))
            .map_err(|error| replay_failed(subject, &error))?;
        let trace = replay.trace();
        for row in &trace.refusals()[read..] {
            if let RefusalCause::Engine { refusal, message } = &row.cause {
                let time = usize::try_from(row.frame)
                    .ok()
                    .and_then(|frame| trace.times().get(frame))
                    .copied()
                    .unwrap_or(f64::NAN);
                return Err(engine_refused(subject, row.frame, time, *refusal, message));
            }
        }
        read = trace.refusals().len();
        match progress {
            Progress::More { .. } => {}
            Progress::Done { .. } | Progress::Stopped { .. } => break,
        }
    }
    let (_, trace) = replay.finish();
    Ok(trace)
}

/// The error for the engine's refusal of trace row `frame` at `time`.
fn engine_refused(
    subject: &str,
    frame: u64,
    time: f64,
    refusal: EngineRefusal,
    message: &str,
) -> VerifyError {
    let message = cut(message, MAX_ENGINE_MESSAGE_BYTES);
    let kind = if refusal == EngineRefusal::Run {
        Kind::RunRefused
    } else {
        Kind::FrameRefused
    };
    VerifyError::new(
        kind,
        subject,
        format!(
            "the engine refused frame {frame} at t = {time:?} s ({}): {message}",
            engine_refusal_class(refusal)
        ),
    )
    .with_cause(engine_refusal_class(refusal))
    .with_frame(frame, time, message)
}

/// The executed rows of `trace` (status `Executed` or `Substituted`), in row order.
pub(crate) fn executed_rows(trace: &Trace) -> Vec<usize> {
    trace
        .status()
        .iter()
        .enumerate()
        .filter(|(_, byte)| FrameStatus::from_byte(**byte).is_some_and(FrameStatus::executed))
        .map(|(row, _)| row)
        .collect()
}

/// The uncovered stretches of `trace`, in time order (the module documentation's rule).
pub(crate) fn uncovered_stretches(trace: &Trace) -> Vec<Stretch> {
    let mut stretches = Vec::new();
    let mut open: Option<f64> = None;
    for (byte, time) in trace.status().iter().zip(trace.times()) {
        match FrameStatus::from_byte(*byte) {
            Some(FrameStatus::RefusedMissing) => {
                open.get_or_insert(*time);
            }
            Some(status) if status.executed() => {
                if let Some(from) = open.take()
                    && *time > from
                {
                    stretches.push(Stretch {
                        from,
                        until: Some(*time),
                    });
                }
            }
            _ => {}
        }
    }
    if let Some(from) = open {
        stretches.push(Stretch { from, until: None });
    }
    stretches
}

/// Whether each of `instants` (non-decreasing) lies in one of `stretches` (in time order and
/// disjoint, as [`uncovered_stretches`] gives them), and the steps that took: one per instant
/// and one per stretch passed, so the work is linear in the two together, never their product.
pub(crate) fn uncovered_at(stretches: &[Stretch], instants: &[f64]) -> (Vec<bool>, u64) {
    let mut flags = Vec::with_capacity(instants.len());
    let mut next = 0;
    let mut steps: u64 = 0;
    for t in instants {
        // A stretch that closed at or before `t` holds no later instant either.
        while next < stretches.len() && stretches[next].until.is_some_and(|until| until <= *t) {
            next += 1;
            steps += 1;
        }
        steps += 1;
        flags.push(stretches.get(next).is_some_and(|stretch| stretch.holds(*t)));
    }
    (flags, steps)
}

#[cfg(test)]
#[path = "drive_tests.rs"]
mod tests;
