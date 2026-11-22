//! How a commissioning run is carried out, and what it gives back (commissioning note, "Rust API
//! sketch", amended 2026-11-20).

use oce_replay::{ReplayLimits, Trace};

use super::report::TrendData;
use super::trend_limits::TrendLimits;
use crate::compare_limits::CompareLimits;
use crate::envelope::Curve;
use crate::receipt::{Receipt, Timings};

/// The most work the grid envelope may do for one run: the instants squared, summed over the
/// Real outputs compared under `aligned-grid`, 2^32. It is the bound a vector set's tolerance
/// windows are held to ([`crate::VerifyLimits::max_total_aligned_grid_work`]): the envelope tests
/// every grid point against every rectangle.
pub const MAX_TOTAL_ALIGNED_GRID_WORK: u64 = crate::VerifyLimits::MAX.max_total_aligned_grid_work;

/// How a commissioning run is carried out. `Default` is `trended` data, no timings, no bound
/// curves, chunks of 4,096 frames and every limit at its documented maximum.
///
/// The limits are lower-only: each is read through its own `effective`, so a value above a
/// maximum is the maximum. No receipt record cap is here: those are constants of the receipt
/// (decision 42).
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct CommissionOptions {
    /// What the caller says the trend files are; the receipt's `data` (decision 41).
    pub data: TrendData,
    /// Durations the caller measured, copied into the receipt's `timings`; zeros when `None`.
    /// The library measures nothing, and they never move `receipt_id`.
    pub timings: Option<Timings>,
    /// Keep each Real output's reference, test and bound curves in [`OutputCurves::bounds`].
    pub bounds: bool,
    /// Frames per `Replay::advance` call; zero is read as one. Never changes a receipt byte.
    pub chunk_frames: usize,
    /// The bounds of reading the reference's trend sources.
    pub trends: TrendLimits,
    /// The bounds of the replay: its frames and the bytes of its trace.
    pub replay: ReplayLimits,
    /// The bounds of each comparison: the point caps bind applies first (decision 42), and the
    /// funnel's loop-removal work.
    pub compare: CompareLimits,
    /// The grid envelope's work over the run (`too-many-points` past it); lowered to
    /// [`MAX_TOTAL_ALIGNED_GRID_WORK`], never raised.
    pub max_total_aligned_grid_work: u64,
}

impl Default for CommissionOptions {
    fn default() -> Self {
        Self {
            data: TrendData::Trended,
            timings: None,
            bounds: false,
            chunk_frames: 4_096,
            trends: TrendLimits::default(),
            replay: ReplayLimits::default(),
            compare: CompareLimits::default(),
            max_total_aligned_grid_work: MAX_TOTAL_ALIGNED_GRID_WORK,
        }
    }
}

impl CommissionOptions {
    /// These options with `data` as the trend label.
    #[must_use]
    pub fn with_data(mut self, data: TrendData) -> Self {
        self.data = data;
        self
    }
}

/// The four curves of one Real output's envelope, as the comparison saw them: both series after
/// masking (masked and uncovered instants zeroed) and the two bounds.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub struct OutputBounds {
    /// The series the envelope is built around.
    pub reference: Curve,
    /// The series checked against it.
    pub test: Curve,
    /// The lower bound.
    pub lower: Curve,
    /// The upper bound.
    pub upper: Curve,
}

/// The curves of one compared output.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub struct OutputCurves {
    /// The canonical path, as the receipt's `outputs[].output`.
    pub output: String,
    /// One point per test instant, in time order: at a compared instant its exceedance (exactly
    /// `0.0` or `1.0` for a Boolean or Integer output; positive infinity at every compared
    /// instant of an output failed by a non-finite engine sample); at a masked instant the
    /// envelope's error on the zeroed series, reported and never counted; at an uncovered
    /// instant a NaN, because the engine computed nothing there. Empty for an output with no
    /// test instant. Never written into the receipt.
    pub errors: Curve,
    /// The reference, test and bound curves, when [`CommissionOptions::bounds`] is set and an
    /// envelope was built.
    pub bounds: Option<OutputBounds>,
}

/// What a commissioning run produced.
#[non_exhaustive]
#[derive(Debug)]
pub struct CommissionRun {
    /// The receipt.
    pub receipt: Receipt,
    /// The replay's trace, its inputs retained.
    pub trace: Trace,
    /// One entry per compared output, in the order of the receipt's `outputs`.
    pub curves: Vec<OutputCurves>,
}
