//! The run's time base and the missing-sample vocabulary (design note decisions 2, 5 and 11).
//!
//! These are host policy applied offline, never engine policy: the engine still requires every
//! input on every frame, and replay decides, per input and per frame, what an absent sample
//! becomes and writes that decision into the trace.

use oce_api::Value;

/// How frame times are chosen. There is no default: a run always declares one (decision 5).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TimeBase {
    /// One frame per source row, at the row's time, passed through bit for bit.
    Recorded,
    /// Frame `k` at `t0 + (k as f64) * step`, computed by multiply and never by accumulating
    /// `step`. Each input takes the latest sample at or before the tick (zero-order hold); a sample
    /// is current only for the interval `(tick(k - 1), tick(k)]` it arrived in, and carrying it
    /// further is a hold governed by [`MissingPolicy`] (decision 11). Ticks run to the last
    /// sample time. `t0` must be finite and `step` finite and positive.
    Grid {
        /// The time of frame 0, model seconds.
        t0: f64,
        /// The tick spacing, seconds.
        step: f64,
    },
}

impl TimeBase {
    /// The time of grid frame `k`: `t0 + (k as f64) * step`, one rounding of the product and one
    /// of the sum, exactly as documented. `None` for [`TimeBase::Recorded`], whose times are the
    /// rows'. Does not validate `t0` or `step`.
    #[must_use]
    pub fn tick(&self, k: u64) -> Option<f64> {
        match *self {
            Self::Recorded => None,
            // `as f64` is the documented conversion: exact up to 2^53, round-to-nearest beyond.
            #[allow(clippy::cast_precision_loss)]
            Self::Grid { t0, step } => Some(t0 + (k as f64) * step),
        }
    }
}

/// What an equal timestamp on consecutive source rows does (decision 5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DuplicateTime {
    /// A row whose time equals the previous row's is a typed
    /// [`crate::SourceError::DuplicateTime`] before the engine sees it. The default.
    #[default]
    Refuse,
    /// Each equal-time row is its own frame and its own trace row under
    /// [`TimeBase::Recorded`] (a legal zero-elapsed HostTick transition); under
    /// [`TimeBase::Grid`] the later row is the latest sample. Rows are never collapsed.
    Allow,
}

/// What an absent sample becomes (decision 2). The run-level policy applies to every input
/// without an override; [`MissingPolicy::Fallback`] exists only as a per-input override.
#[derive(Clone, Debug, Default)]
pub enum MissingPolicy {
    /// The frame is refused by replay before the engine: status
    /// [`FrameStatus::RefusedMissing`] and a refusal row naming the first refusing input. The
    /// default.
    #[default]
    Refuse,
    /// The last *observed* sample, if `t - t_sample <= max_age`; otherwise as `Refuse`. A hold
    /// never holds a substituted value, and age is measured from the observed sample's time.
    /// `max_age` must be finite and not negative.
    HoldLast {
        /// The oldest a held sample may be, seconds.
        max_age: f64,
    },
    /// A value the caller declared for the input, type and domain checked when the map is
    /// resolved. Per-input overrides only.
    Fallback(Value),
    /// The frame is not submitted and the run continues: status [`FrameStatus::Skipped`].
    ///
    /// Precedence: if another input missing on the same frame cannot be substituted (`Refuse`, or
    /// `HoldLast` with an expired or absent sample), the frame is `RefusedMissing` instead, naming
    /// the first such input in slot order, and [`OnRefusal`] applies. Refusal dominates skipping.
    SkipFrame,
}

impl MissingPolicy {
    /// Whether this policy can feed the engine a value no sensor reported at that instant.
    #[must_use]
    pub fn can_substitute(&self) -> bool {
        matches!(self, Self::HoldLast { .. } | Self::Fallback(_))
    }
}

/// What a refused frame does to the run (decision 2). Refusal is atomic in the engine, so
/// continuing needs no rollback.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum OnRefusal {
    /// The run ends with [`crate::Progress::Stopped`] after writing the refused row. The default.
    #[default]
    Stop,
    /// The refused row is written and the run moves on.
    RecordAndContinue,
}

/// Whether a source's cells are samples or signals (decision 11).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SourceKind {
    /// Observations at instants: under [`TimeBase::Grid`] a sample is current only for the tick
    /// interval it arrived in. CSV and columnar sources.
    #[default]
    Samples,
    /// Values defined on every interval they cover: under [`TimeBase::Grid`] the latest value is
    /// always current, never a hold. Step lists.
    Signals,
}

/// The status byte of one trace row (decision 18).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum FrameStatus {
    /// Every input observed; the engine accepted the frame.
    Executed = 0,
    /// At least one input held or defaulted; the engine accepted the frame.
    Substituted = 1,
    /// A `SkipFrame` input was missing and no missing input refused the frame; nothing was
    /// submitted. The substitution mask still records any input substituted on the row (as on a
    /// refused row): the mask is per input and per row, whatever the status.
    Skipped = 2,
    /// A missing input could not be substituted; replay refused the frame before the engine.
    RefusedMissing = 3,
    /// The engine refused the frame (preparation or execution); its state is unchanged.
    RefusedEngine = 4,
}

impl FrameStatus {
    /// The status for a status byte, or `None` for a byte no status uses.
    #[must_use]
    pub fn from_byte(byte: u8) -> Option<Self> {
        Some(match byte {
            0 => Self::Executed,
            1 => Self::Substituted,
            2 => Self::Skipped,
            3 => Self::RefusedMissing,
            4 => Self::RefusedEngine,
            _ => return None,
        })
    }

    /// Whether the engine accepted the frame (its output columns hold values).
    #[must_use]
    pub fn executed(self) -> bool {
        matches!(self, Self::Executed | Self::Substituted)
    }
}
