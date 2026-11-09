//! Replay limits (design note decision 19): the caps every source and every run is held to.
//!
//! Each limit is a default and a documented maximum at once. A caller may lower any of them, never
//! raise one above its maximum: [`ReplayLimits::validate`] refuses such a request with a typed
//! [`LimitError`]. Every source constructor and [`crate::Replay::new`] validate the limits they are
//! given, so an out-of-range request never reaches a reader.

use std::fmt;

/// The caps on one replay: source rows, columns, bytes, cell length and step-list length, frames
/// and trace bytes.
///
/// `Default` is the documented maxima ([`ReplayLimits::MAX`]). Every cap is inclusive: a source or
/// run *at* a cap is accepted, one unit past it is refused with a typed error, never truncated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplayLimits {
    /// Source rows (CSV data rows, columnar rows, the union of step times). Maximum 16,777,216.
    pub max_rows: u64,
    /// Source columns, the time column included. Maximum 4,096.
    pub max_columns: u32,
    /// Bytes in one CSV cell, header cells included. Maximum 256.
    pub max_cell_bytes: u32,
    /// Bytes read from a CSV reader, or held by a columnar source's columns. Maximum 1 GiB.
    pub max_source_bytes: u64,
    /// Steps in one input's step list. Maximum 1,000,000.
    pub max_steps: u64,
    /// Attempted frames (trace rows) in one run. Maximum 16,777,216.
    pub max_frames: u64,
    /// Trace bytes: frame columns, sparse rows and snapshots ([`crate::Trace::byte_size`]).
    /// Maximum 1 GiB.
    pub max_trace_bytes: u64,
}

impl ReplayLimits {
    /// The documented maxima, which are also the defaults.
    pub const MAX: Self = Self {
        max_rows: 16_777_216,
        max_columns: 4_096,
        max_cell_bytes: 256,
        max_source_bytes: 1 << 30,
        max_steps: 1_000_000,
        max_frames: 16_777_216,
        max_trace_bytes: 1 << 30,
    };

    /// Refuse any cap above its documented maximum. Lowering is always allowed, to zero included.
    ///
    /// # Errors
    /// [`LimitError::AboveMaximum`] naming the first cap (in field order) above its maximum.
    pub fn validate(&self) -> Result<(), LimitError> {
        let max = Self::MAX;
        let checks: [(&'static str, u64, u64); 7] = [
            ("max_rows", self.max_rows, max.max_rows),
            (
                "max_columns",
                u64::from(self.max_columns),
                u64::from(max.max_columns),
            ),
            (
                "max_cell_bytes",
                u64::from(self.max_cell_bytes),
                u64::from(max.max_cell_bytes),
            ),
            (
                "max_source_bytes",
                self.max_source_bytes,
                max.max_source_bytes,
            ),
            ("max_steps", self.max_steps, max.max_steps),
            ("max_frames", self.max_frames, max.max_frames),
            ("max_trace_bytes", self.max_trace_bytes, max.max_trace_bytes),
        ];
        for (limit, requested, maximum) in checks {
            if requested > maximum {
                return Err(LimitError::AboveMaximum {
                    limit,
                    requested,
                    maximum,
                });
            }
        }
        Ok(())
    }
}

impl Default for ReplayLimits {
    fn default() -> Self {
        Self::MAX
    }
}

/// A limit request the crate refuses.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LimitError {
    /// A cap was raised above its documented maximum.
    AboveMaximum {
        /// The field name, such as `max_rows`.
        limit: &'static str,
        /// The requested value.
        requested: u64,
        /// The documented maximum.
        maximum: u64,
    },
}

impl fmt::Display for LimitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AboveMaximum {
                limit,
                requested,
                maximum,
            } => write!(
                f,
                "{limit} {requested} is above its documented maximum {maximum}"
            ),
        }
    }
}

impl std::error::Error for LimitError {}
