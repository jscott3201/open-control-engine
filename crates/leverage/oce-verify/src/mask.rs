//! A window's mask: the AND of its `when` signals at each of its ticks (vectors note, "Tolerance
//! windows", `when`; decision 46).
//!
//! At a tick the mask is true when every signal is true in that frame: an output's value, or
//! the value the frame submitted for an input (the trace keeps the inputs for a scenario whose
//! `when` names one). The cost is the signals times the ticks, which the 16-name bound of
//! [`crate::VerifyLimits::max_when_signals`] and the tick bound hold.

use oce_replay::{Trace, TraceColumn};

use crate::bind_window::Signal;
use crate::verify_error::{VerifyError, VerifyErrorKind};

/// The mask of `signals` over the ticks `first..=last`, one entry per tick; `None` when there is
/// no signal, so every tick counts.
///
/// # Errors
/// `replay-failed` with the cause `output-set-changed` when a signal's column is missing, is not
/// a Boolean column or is shorter than the ticks: binding resolved every signal to a Boolean
/// port and the run keeps the inputs it names, so that is a trace that is not the one binding
/// expects.
pub(crate) fn mask(
    trace: &Trace,
    signals: &[Signal],
    first: u64,
    last: u64,
    subject: &str,
) -> Result<Option<Vec<bool>>, VerifyError> {
    if signals.is_empty() {
        return Ok(None);
    }
    let changed = |detail: String| {
        VerifyError::new(VerifyErrorKind::ReplayFailed, subject, detail)
            .with_cause("output-set-changed")
    };
    let (Ok(start), Ok(end)) = (usize::try_from(first), usize::try_from(last)) else {
        return Err(changed(
            "a window's ticks do not fit this platform".to_owned(),
        ));
    };
    let mut mask = vec![true; end - start + 1];
    for signal in signals {
        let column = match *signal {
            Signal::Input(slot) => trace.inputs().and_then(|inputs| inputs.get(slot)),
            Signal::Output(slot) => trace.outputs().get(slot),
        };
        let Some((path, TraceColumn::Boolean(values))) = column else {
            return Err(changed(format!(
                "the trace has no Boolean column for the when signal {signal:?}"
            )));
        };
        let Some(cells) = values.get(start..=end) else {
            return Err(changed(format!(
                "the trace's column for {path} ends before tick {last}"
            )));
        };
        for (held, cell) in mask.iter_mut().zip(cells) {
            *held &= *cell != 0;
        }
    }
    Ok(Some(mask))
}
