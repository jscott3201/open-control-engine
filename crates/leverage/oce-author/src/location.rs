//! The one source-location type: a 1-based line and column, used by wire refusals
//! ([`crate::WireError`]) and by finding spans ([`crate::Finding::span`]).

use std::fmt;

/// A 1-based line and column (in bytes) in a source document. A document within the wire and
/// text bounds (4 MiB) always fits in `u32`; a larger count saturates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Location {
    /// 1-based line.
    pub line: u32,
    /// 1-based column, in bytes.
    pub column: u32,
}

impl Location {
    /// A location from `usize` counts, saturating at `u32::MAX`.
    #[must_use]
    pub fn new(line: usize, column: usize) -> Self {
        Self {
            line: u32::try_from(line).unwrap_or(u32::MAX),
            column: u32::try_from(column).unwrap_or(u32::MAX),
        }
    }
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {} column {}", self.line, self.column)
    }
}
