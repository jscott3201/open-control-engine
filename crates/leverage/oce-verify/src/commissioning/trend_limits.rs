//! The bounds the trend sources of one reference are read under (commissioning note, "Caps" and
//! decision 42; issue #660).
//!
//! "Every source is bounded by replay's limits": a `csv` file is read by replay's own
//! [`oce_replay::CsvSource`] under [`ReplayLimits`], a `combiTimeTable` file is held to the same
//! caps by this crate's reader, and the merged inputs to the caps of the
//! [`oce_replay::ColumnarSource`] they become. One bound is added: the number of sources.
//!
//! Each field is a default and a documented maximum at once, as [`crate::CompareLimits`] is: a
//! caller may lower any of them, to zero included, and a value above the maximum is read as the
//! maximum, so no caller can raise one. Every bound is inclusive: an input *at* a bound is read,
//! one past it is refused with a typed [`crate::SourceError`], never truncated.

use oce_replay::ReplayLimits;

/// The bounds on reading the trend sources of one reference.
///
/// `Default` is the documented maxima ([`TrendLimits::MAX`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrendLimits {
    /// Trend sources of one reference, `inputs`, `expected` (or a `controllerOutput`) and
    /// `indicatorSources` together. Maximum 256, the bound decision 42 puts on a receipt's
    /// complete `sources` list; past it the refusal is `too-many-sources`, decided from the spec
    /// before any file is looked at.
    pub max_sources: u32,
    /// The caps of replay, applied to each file and to the merged inputs:
    ///
    /// - `max_source_bytes` (1 GiB): the bytes of one file, decided from its length before a
    ///   byte is read; and the bytes the values one source keeps, or the merged inputs hold,
    ///   take (8 per time and per Real or Integer value, 1 per Boolean and per validity flag),
    ///   decided from the counts before anything is allocated;
    /// - `max_rows` (16,777,216): the data rows of one file, and the frames the inputs merge
    ///   into;
    /// - `max_columns` (4,096): the columns of one file, and of the merged inputs, the time
    ///   column included;
    /// - `max_cell_bytes` (256): one `csv` cell, one `combiTimeTable` number and one matrix
    ///   name.
    ///
    /// The other three caps are the run's, not the reader's.
    pub replay: ReplayLimits,
}

impl TrendLimits {
    /// The documented maxima, which are also the defaults.
    pub const MAX: Self = Self {
        max_sources: 256,
        replay: ReplayLimits::MAX,
    };

    /// These limits with every field above its documented maximum lowered to that maximum: the
    /// limits a read actually applies. Replay's caps are lowered field by field.
    #[must_use]
    pub fn effective(&self) -> Self {
        let max = Self::MAX.replay;
        let given = self.replay;
        Self {
            max_sources: self.max_sources.min(Self::MAX.max_sources),
            replay: ReplayLimits {
                max_rows: given.max_rows.min(max.max_rows),
                max_columns: given.max_columns.min(max.max_columns),
                max_cell_bytes: given.max_cell_bytes.min(max.max_cell_bytes),
                max_source_bytes: given.max_source_bytes.min(max.max_source_bytes),
                max_steps: given.max_steps.min(max.max_steps),
                max_frames: given.max_frames.min(max.max_frames),
                max_trace_bytes: given.max_trace_bytes.min(max.max_trace_bytes),
            },
        }
    }
}

impl Default for TrendLimits {
    fn default() -> Self {
        Self::MAX
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_documented_maxima() {
        let limits = TrendLimits::default();
        assert_eq!(limits, TrendLimits::MAX);
        assert_eq!(limits.max_sources, 256);
        assert_eq!(limits.replay, ReplayLimits::default());
        assert_eq!(limits.replay.max_rows, 16_777_216);
        assert_eq!(limits.replay.max_columns, 4_096);
        assert_eq!(limits.replay.max_cell_bytes, 256);
        assert_eq!(limits.replay.max_source_bytes, 1 << 30);
    }

    #[test]
    fn a_limit_can_be_lowered_and_never_raised() {
        let raised = TrendLimits {
            max_sources: u32::MAX,
            replay: ReplayLimits {
                max_rows: u64::MAX,
                max_columns: u32::MAX,
                max_cell_bytes: u32::MAX,
                max_source_bytes: u64::MAX,
                max_steps: u64::MAX,
                max_frames: u64::MAX,
                max_trace_bytes: u64::MAX,
            },
        };
        assert_eq!(raised.effective(), TrendLimits::MAX);
        assert!(raised.effective().replay.validate().is_ok());
        let lowered = TrendLimits {
            max_sources: 0,
            replay: ReplayLimits {
                max_rows: 1,
                max_columns: 2,
                max_cell_bytes: 3,
                max_source_bytes: 4,
                max_steps: 5,
                max_frames: 6,
                max_trace_bytes: 7,
            },
        };
        assert_eq!(lowered.effective(), lowered);
        // One field raised leaves the others as given.
        let one = TrendLimits {
            max_sources: 300,
            replay: ReplayLimits {
                max_rows: 9,
                ..ReplayLimits::MAX
            },
        };
        assert_eq!(
            one.effective(),
            TrendLimits {
                max_sources: 256,
                replay: ReplayLimits {
                    max_rows: 9,
                    ..ReplayLimits::MAX
                },
            }
        );
    }
}
