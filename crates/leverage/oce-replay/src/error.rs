//! Typed errors: mapping ([`MapError`]), sources ([`SourceError`]) and the run ([`ReplayError`]).
//!
//! Nothing in this crate panics on caller or source input; every refusal is one of these.

use std::fmt;

use oce_api::OcError;

use crate::LimitError;

/// A name→slot mapping the crate refuses, at [`crate::InputMap::resolve`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MapError {
    /// The engine's input definitions are not in strictly increasing path order.
    InputsNotCanonical {
        /// The first path out of order.
        path: String,
    },
    /// An engine input has a type no trace column carries (String is metadata, never a signal).
    UnsupportedInputType {
        /// The input path.
        path: String,
    },
    /// Two source column specs share a name.
    DuplicateColumn {
        /// The repeated name.
        column: String,
    },
    /// A map entry names a column the source does not have.
    UnknownColumn {
        /// The column name.
        column: String,
    },
    /// A map entry's target matches no engine input, by exact path or by suffix.
    UnknownTarget {
        /// The source column.
        column: String,
        /// The target as written.
        target: String,
    },
    /// A suffix target matches more than one engine input at a `.` boundary.
    AmbiguousTarget {
        /// The target as written.
        target: String,
        /// Every matching path, in slot order.
        candidates: Vec<String>,
    },
    /// Two map entries (or one column listed twice) supply the same input.
    TwoColumnsOneInput {
        /// The input path.
        input: String,
        /// The column mapped first.
        first: String,
        /// The column mapped second.
        second: String,
    },
    /// A column's kind cannot carry its input's type.
    TypeMismatch {
        /// The source column.
        column: String,
        /// The input path.
        input: String,
    },
    /// An engine input no column supplies (the first in slot order).
    UnmappedInput {
        /// The input path.
        path: String,
    },
    /// `MissingPolicy::Fallback` given as the run-level policy; it exists only per input.
    FallbackIsPerInput,
    /// A `HoldLast` `max_age` that is NaN, infinite or negative.
    InvalidMaxAge {
        /// The input path, or `None` for the run-level policy.
        input: Option<String>,
    },
    /// Two overrides name the same input.
    OverrideTwice {
        /// The input path.
        input: String,
    },
    /// A fallback value whose type is not the input's.
    FallbackType {
        /// The input path.
        input: String,
    },
    /// A fallback value outside the input's declared domain, or NaN.
    FallbackDomain {
        /// The input path.
        input: String,
    },
}

impl fmt::Display for MapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputsNotCanonical { path } => {
                write!(f, "input definitions are not in canonical order at {path}")
            }
            Self::UnsupportedInputType { path } => {
                write!(f, "input {path} has a type no trace column carries")
            }
            Self::DuplicateColumn { column } => write!(f, "source column {column} appears twice"),
            Self::UnknownColumn { column } => write!(f, "the map names unknown column {column}"),
            Self::UnknownTarget { column, target } => {
                write!(
                    f,
                    "column {column} targets {target}, which matches no input"
                )
            }
            Self::AmbiguousTarget { target, candidates } => write!(
                f,
                "target {target} matches {} inputs: {}",
                candidates.len(),
                candidates.join(", ")
            ),
            Self::TwoColumnsOneInput {
                input,
                first,
                second,
            } => write!(f, "columns {first} and {second} both supply input {input}"),
            Self::TypeMismatch { column, input } => {
                write!(f, "column {column} cannot carry the type of input {input}")
            }
            Self::UnmappedInput { path } => write!(f, "no column supplies input {path}"),
            Self::FallbackIsPerInput => write!(f, "Fallback is a per-input override only"),
            Self::InvalidMaxAge { input } => match input {
                Some(input) => write!(f, "HoldLast max_age for {input} is not finite and >= 0"),
                None => write!(f, "run-level HoldLast max_age is not finite and >= 0"),
            },
            Self::OverrideTwice { input } => write!(f, "two overrides name input {input}"),
            Self::FallbackType { input } => {
                write!(f, "fallback value type differs from input {input}")
            }
            Self::FallbackDomain { input } => {
                write!(f, "fallback value is outside the domain of input {input}")
            }
        }
    }
}

impl std::error::Error for MapError {}

/// A source the crate refuses, at construction or while reading. Row numbers count data rows
/// from 1; column names are the source's.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SourceError {
    /// The limits given to the source are above their maxima.
    Limit(LimitError),
    /// More rows than `max_rows`.
    TooManyRows {
        /// The cap.
        limit: u64,
    },
    /// More columns than `max_columns`.
    TooManyColumns {
        /// The cap.
        limit: u32,
    },
    /// More bytes than `max_source_bytes`.
    TooManyBytes {
        /// The cap.
        limit: u64,
    },
    /// A cell longer than `max_cell_bytes`.
    CellTooLong {
        /// The data row, or 0 for the header.
        row: u64,
        /// The zero-based field index.
        field: usize,
        /// The cap.
        limit: u32,
    },
    /// A step list longer than `max_steps`.
    TooManySteps {
        /// The column.
        column: String,
        /// The cap.
        limit: u64,
    },
    /// A column the map needs (or the time column) is absent from the source.
    MissingColumn {
        /// The column name.
        column: String,
    },
    /// The source carries a column the map does not know.
    UnknownColumn {
        /// The column name.
        column: String,
    },
    /// A column appears twice.
    DuplicateColumn {
        /// The column name.
        column: String,
    },
    /// A column's kind (or enum class) differs from the map's spec for it.
    ColumnKind {
        /// The column name.
        column: String,
    },
    /// Columns of unequal length.
    LengthMismatch {
        /// The column name.
        column: String,
    },
    /// A CSV record with a different field count from the header.
    Ragged {
        /// The data row.
        row: u64,
    },
    /// A cell that is not UTF-8.
    NotUtf8 {
        /// The data row, or 0 for the header.
        row: u64,
        /// The zero-based field index.
        field: usize,
    },
    /// A cell that does not parse as its column's kind.
    BadCell {
        /// The data row.
        row: u64,
        /// The column name.
        column: String,
    },
    /// A row whose time is missing (blank, NaN, NaT); never substituted.
    MissingTime {
        /// The data row.
        row: u64,
    },
    /// A row whose time is infinite, or a time conversion that overflows.
    NonFiniteTime {
        /// The data row.
        row: u64,
    },
    /// A time earlier than the previous row's.
    TimeRegression {
        /// The data row.
        row: u64,
    },
    /// A time equal to the previous row's under `DuplicateTime::Refuse`.
    DuplicateTime {
        /// The data row.
        row: u64,
    },
    /// A step list whose times are not finite and strictly increasing, or a step value that is
    /// NaN or of the wrong type.
    BadStep {
        /// The column.
        column: String,
        /// The zero-based step index.
        step: usize,
    },
    /// A row whose width is not the map's input count (a custom source).
    RowWidth {
        /// The data row.
        row: u64,
    },
    /// An instant that is not the RFC 3339 subset (`YYYY-MM-DDTHH:MM:SS[.f]` with `Z` or
    /// `±hh:mm`).
    BadInstant {
        /// The data row, or 0 for the origin.
        row: u64,
    },
    /// The underlying reader failed (other than the byte cap).
    Read {
        /// The reader's message.
        message: String,
    },
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Limit(error) => write!(f, "{error}"),
            Self::TooManyRows { limit } => write!(f, "source has more than {limit} rows"),
            Self::TooManyColumns { limit } => write!(f, "source has more than {limit} columns"),
            Self::TooManyBytes { limit } => write!(f, "source has more than {limit} bytes"),
            Self::CellTooLong { row, field, limit } => {
                write!(f, "row {row} field {field} is longer than {limit} bytes")
            }
            Self::TooManySteps { column, limit } => {
                write!(f, "step list {column} has more than {limit} steps")
            }
            Self::MissingColumn { column } => write!(f, "source lacks column {column}"),
            Self::UnknownColumn { column } => write!(f, "column {column} is not in the map"),
            Self::DuplicateColumn { column } => write!(f, "column {column} appears twice"),
            Self::ColumnKind { column } => write!(f, "column {column} differs from its spec"),
            Self::LengthMismatch { column } => write!(f, "column {column} has a different length"),
            Self::Ragged { row } => write!(f, "row {row} has a different field count"),
            Self::NotUtf8 { row, field } => write!(f, "row {row} field {field} is not UTF-8"),
            Self::BadCell { row, column } => write!(f, "row {row} column {column} does not parse"),
            Self::MissingTime { row } => write!(f, "row {row} has no time"),
            Self::NonFiniteTime { row } => write!(f, "row {row} time is not finite"),
            Self::TimeRegression { row } => write!(f, "row {row} time is earlier than row before"),
            Self::DuplicateTime { row } => write!(f, "row {row} time equals the row before"),
            Self::BadStep { column, step } => write!(f, "step {step} of {column} is invalid"),
            Self::RowWidth { row } => write!(f, "row {row} width differs from the map"),
            Self::BadInstant { row } => write!(f, "row {row} instant is not RFC 3339 subset"),
            Self::Read { message } => write!(f, "source read failed: {message}"),
        }
    }
}

impl std::error::Error for SourceError {}

impl From<LimitError> for SourceError {
    fn from(error: LimitError) -> Self {
        Self::Limit(error)
    }
}

/// A run the crate refuses or ends with an error. Refused *frames* are not errors: they are trace
/// rows (see [`crate::OnRefusal`]).
#[derive(Debug)]
#[non_exhaustive]
pub enum ReplayError {
    /// The limits are above their maxima.
    Limit(LimitError),
    /// A source failed or was refused.
    Source(SourceError),
    /// The engine refused a setup call (`input_definitions`, `frame_slots`, `state_snapshot`).
    Engine(OcError),
    /// The map no longer matches the engine's input definitions (paths, types, bounds, order),
    /// as after a reload of another model.
    StaleMap,
    /// The source was bound to another map.
    SourceNotBound,
    /// `TimeBase::Grid` with a non-finite `t0`, or a `step` that is not finite and positive.
    InvalidTimeBase,
    /// `TraceOptions::snapshot_every` of zero.
    InvalidTraceOptions,
    /// The options and the source carry different wall-clock origins.
    OriginConflict,
    /// The run reached `max_frames` with frames still to attempt.
    TooManyFrames {
        /// The cap.
        limit: u64,
    },
    /// The trace would exceed `max_trace_bytes`.
    TraceTooLarge {
        /// The cap.
        limit: u64,
    },
    /// An accepted frame's outputs differ from the set fixed by the first accepted frame
    /// (decision 16).
    OutputSetChanged {
        /// The trace row.
        frame: u64,
    },
    /// The first accepted frame has an output no trace column carries (a String).
    UnsupportedOutputType {
        /// The boundary-output identity.
        path: String,
    },
    /// A coverage window that is not finite with `from <= to`.
    InvalidWindow {
        /// The window index.
        index: usize,
    },
    /// A coverage window contains no executed tick (decision 7).
    Uncovered {
        /// The window index.
        index: usize,
    },
    /// `advance` called after an earlier call returned an error.
    Failed,
}

impl fmt::Display for ReplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Limit(error) => write!(f, "{error}"),
            Self::Source(error) => write!(f, "{error}"),
            Self::Engine(error) => write!(f, "engine: {error}"),
            Self::StaleMap => write!(f, "the input map does not match the engine's inputs"),
            Self::SourceNotBound => write!(f, "the source is bound to another input map"),
            Self::InvalidTimeBase => write!(f, "grid t0 must be finite and step finite and > 0"),
            Self::InvalidTraceOptions => write!(f, "snapshot_every must be at least 1"),
            Self::OriginConflict => write!(f, "options and source carry different origins"),
            Self::TooManyFrames { limit } => write!(f, "the run has more than {limit} frames"),
            Self::TraceTooLarge { limit } => write!(f, "the trace would exceed {limit} bytes"),
            Self::OutputSetChanged { frame } => write!(f, "frame {frame} changed the output set"),
            Self::UnsupportedOutputType { path } => {
                write!(f, "output {path} has a type no trace column carries")
            }
            Self::InvalidWindow { index } => write!(f, "coverage window {index} is invalid"),
            Self::Uncovered { index } => write!(f, "coverage window {index} has no executed tick"),
            Self::Failed => write!(f, "the replay already failed"),
        }
    }
}

impl std::error::Error for ReplayError {}

impl From<LimitError> for ReplayError {
    fn from(error: LimitError) -> Self {
        Self::Limit(error)
    }
}

impl From<SourceError> for ReplayError {
    fn from(error: SourceError) -> Self {
        Self::Source(error)
    }
}

impl From<OcError> for ReplayError {
    fn from(error: OcError) -> Self {
        Self::Engine(error)
    }
}
