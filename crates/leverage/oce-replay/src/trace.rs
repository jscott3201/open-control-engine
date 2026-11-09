//! The trace (decisions 3, 18, 20 and 21): columnar, typed, raw bits, one row per attempted frame.
//!
//! Per row: the time (`f64` bits), the status byte, one cell per output, the dense substitution
//! mask while any input's policy can substitute, and one cell per input when inputs are retained.
//! Beside the rows: sparse diagnostic and refusal tables, opt-in state snapshots, and metadata.
//!
//! **Bytes per frame** (excluding the sparse tables and snapshots):
//!
//! ```text
//! bytes = 8 (time) + 1 (status) + Σ width(output)
//!       + [ceil(N_in / 8)   if a substituting policy is in force]
//!       + [Σ width(input)   if inputs are retained]
//! width: Real 8, Integer 8, Boolean 1, Enum 4
//! ```
//!
//! [`Trace::bytes_per_frame`] computes it from the metadata and [`Trace::frame_bytes`] from the
//! columns' actual lengths; the two agree for every run (`rows × bytes_per_frame`), which the
//! crate's tests assert for every corpus fixture. Sparse rows count as follows toward
//! `max_trace_bytes` ([`Trace::byte_size`]): a diagnostic row 24 bytes plus its two strings, a
//! refusal row 16 bytes plus its message (or input path), a snapshot 16 bytes plus its encoding.

use oce_api::{AssertLevel, EngineStateSnapshot, NonFiniteInputPolicy, OcError, Value};

use crate::{
    DuplicateTime, FrameStatus, MissingPolicy, OnRefusal, ReplayError, ReplayLimits, TimeBase,
    TimeOrigin,
};

/// The value kind of a trace column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceKind {
    /// `f64` bits, 8 bytes.
    Real,
    /// `i64`, 8 bytes.
    Integer,
    /// `u8` 0 or 1, 1 byte (numpy takes it as `bool`).
    Boolean,
    /// `u32` one-based ordinal, 4 bytes; the class is in the metadata.
    Enum,
}

impl TraceKind {
    /// Bytes per cell.
    #[must_use]
    pub fn width(self) -> u64 {
        match self {
            Self::Real | Self::Integer => 8,
            Self::Boolean => 1,
            Self::Enum => 4,
        }
    }

    /// The canonical tag: Real 0, Integer 1, Boolean 2, Enum 3.
    #[must_use]
    pub fn tag(self) -> u8 {
        self as u8
    }

    pub(crate) fn of(value: &Value) -> Option<Self> {
        match value {
            Value::Real(_) => Some(Self::Real),
            Value::Integer(_) => Some(Self::Integer),
            Value::Boolean(_) => Some(Self::Boolean),
            Value::Enum { .. } => Some(Self::Enum),
            Value::String(_) => None,
        }
    }
}

/// One typed column. Rows that did not execute (outputs) or resolve (inputs) hold zero bits,
/// masked by the status column.
#[derive(Clone, Debug, PartialEq)]
pub enum TraceColumn {
    /// Real values; compare with `to_bits`.
    Real(Vec<f64>),
    /// Integer values.
    Integer(Vec<i64>),
    /// Boolean values as 0 or 1.
    Boolean(Vec<u8>),
    /// Enum ordinals.
    Enum(Vec<u32>),
}

impl TraceColumn {
    /// `zero_rows` zero cells with room for `room` more, in one allocation.
    pub(crate) fn new(kind: TraceKind, zero_rows: usize, room: usize) -> Self {
        fn zeros<T: Copy>(zero: T, rows: usize, room: usize) -> Vec<T> {
            let mut v = Vec::with_capacity(rows + room);
            v.resize(rows, zero);
            v
        }
        match kind {
            TraceKind::Real => Self::Real(zeros(0.0, zero_rows, room)),
            TraceKind::Integer => Self::Integer(zeros(0, zero_rows, room)),
            TraceKind::Boolean => Self::Boolean(zeros(0, zero_rows, room)),
            TraceKind::Enum => Self::Enum(zeros(0, zero_rows, room)),
        }
    }

    /// The column's kind.
    #[must_use]
    pub fn kind(&self) -> TraceKind {
        match self {
            Self::Real(_) => TraceKind::Real,
            Self::Integer(_) => TraceKind::Integer,
            Self::Boolean(_) => TraceKind::Boolean,
            Self::Enum(_) => TraceKind::Enum,
        }
    }

    /// The number of rows.
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Self::Real(v) => v.len(),
            Self::Integer(v) => v.len(),
            Self::Boolean(v) => v.len(),
            Self::Enum(v) => v.len(),
        }
    }

    /// Whether the column has no rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub(crate) fn reserve(&mut self, rows: usize) {
        match self {
            Self::Real(v) => v.reserve(rows),
            Self::Integer(v) => v.reserve(rows),
            Self::Boolean(v) => v.reserve(rows),
            Self::Enum(v) => v.reserve(rows),
        }
    }

    /// Append `value` if its kind is the column's; `false` otherwise (nothing appended).
    pub(crate) fn push(&mut self, value: &Value) -> bool {
        match (self, value) {
            (Self::Real(v), Value::Real(x)) => v.push(*x),
            (Self::Integer(v), Value::Integer(x)) => v.push(*x),
            (Self::Boolean(v), Value::Boolean(x)) => v.push(u8::from(*x)),
            (Self::Enum(v), Value::Enum { ordinal, .. }) => v.push(*ordinal),
            _ => return false,
        }
        true
    }

    pub(crate) fn push_zero(&mut self) {
        match self {
            Self::Real(v) => v.push(0.0),
            Self::Integer(v) => v.push(0),
            Self::Boolean(v) => v.push(0),
            Self::Enum(v) => v.push(0),
        }
    }
}

/// One warning the engine emitted on an accepted frame (the engine emits Warning only).
#[derive(Clone, Debug, PartialEq)]
pub struct DiagnosticRow {
    /// The trace row.
    pub frame: u64,
    /// The producing block's diagnostic source.
    pub block: String,
    /// The message.
    pub message: String,
    /// The model time the engine stamped.
    pub time: f64,
    /// The level.
    pub level: AssertLevel,
}

/// The closed class of an engine refusal, the part of it the canonical bytes record. The facade
/// error's display text is kept beside it in memory only, so a wording change in the facade moves
/// no golden.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EngineRefusal {
    /// `OcError::InputType`: a value of the wrong type (a custom source can lend one).
    InputType,
    /// `OcError::InputNonFinite`: ±∞ under `NonFiniteInputPolicy::Refuse`.
    InputNonFinite,
    /// `OcError::InputDomain`: a value outside the input's declared bounds.
    InputDomain,
    /// `OcError::NonFiniteTime` (replay refuses these first; kept for completeness).
    NonFiniteTime,
    /// `OcError::TimeOutOfRange`: a time beyond the engine's magnitude limit.
    TimeOutOfRange,
    /// `OcError::TimeRegression` (replay refuses these first; kept for completeness).
    TimeRegression,
    /// `OcError::NonFiniteTimeStep`.
    NonFiniteTimeStep,
    /// `OcError::ModelTimeUnrepresentable`.
    ModelTimeUnrepresentable,
    /// Any other refusal: one about the engine or the run, not the frame
    /// (`FrameSequenceExhausted`, a state precondition, a stale slot map, a slot-count mismatch).
    /// Later frames would be refused the same way, so it **stops the run** whatever
    /// [`crate::OnRefusal`] says.
    Run,
}

impl EngineRefusal {
    /// Classify a facade error from `execute_slots`.
    #[must_use]
    pub fn of(error: &OcError) -> Self {
        match error {
            OcError::InputType(_) => Self::InputType,
            OcError::InputNonFinite(_) => Self::InputNonFinite,
            OcError::InputDomain(_) => Self::InputDomain,
            OcError::NonFiniteTime { .. } => Self::NonFiniteTime,
            OcError::TimeOutOfRange { .. } => Self::TimeOutOfRange,
            OcError::TimeRegression { .. } => Self::TimeRegression,
            OcError::NonFiniteTimeStep { .. } => Self::NonFiniteTimeStep,
            OcError::ModelTimeUnrepresentable { .. } => Self::ModelTimeUnrepresentable,
            _ => Self::Run,
        }
    }

    /// The canonical tag: the variants in declaration order from 0.
    #[must_use]
    pub fn tag(self) -> u8 {
        self as u8
    }
}

/// Why a frame was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RefusalCause {
    /// Replay refused it: the first input (in slot order) that was missing and could not be
    /// substituted.
    MissingInput {
        /// The input slot.
        slot: usize,
        /// The input path.
        path: String,
    },
    /// The engine refused it.
    Engine {
        /// The closed class of the refusal (in the canonical bytes).
        refusal: EngineRefusal,
        /// `OcError`'s display text (in memory only, never in the canonical bytes).
        message: String,
    },
}

/// One refused frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefusalRow {
    /// The trace row.
    pub frame: u64,
    /// The cause.
    pub cause: RefusalCause,
}

/// One input column's metadata.
#[derive(Clone, Debug)]
pub struct TraceInput {
    /// The canonical input path.
    pub path: String,
    /// The column kind.
    pub kind: TraceKind,
    /// The enum class path the source declared, for an Enum input: a source-declared,
    /// **unverified** label. The facade exposes no lookup from the engine's enum class to its
    /// path, so replay cannot check it; verification waits for that lookup.
    pub enum_class: Option<String>,
    /// The slot's missing-sample policy.
    pub policy: MissingPolicy,
}

/// One output column's metadata, fixed by the first accepted frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceOutput {
    /// The boundary-output identity.
    pub path: String,
    /// The column kind.
    pub kind: TraceKind,
    /// The facade's in-memory enum class index, for an Enum output: process-local, kept in
    /// memory only and never written to the canonical bytes, which carry an empty class path for
    /// every output until the facade exposes a class-path lookup (see the crate docs).
    pub enum_class_index: Option<u32>,
}

/// The run's metadata, written once.
#[derive(Clone, Debug)]
pub struct TraceMeta {
    /// Input columns in slot order.
    pub inputs: Vec<TraceInput>,
    /// Output columns in output-slot order (empty until the first accepted frame).
    pub outputs: Vec<TraceOutput>,
    /// The time base.
    pub time: TimeBase,
    /// The duplicate-time policy.
    pub duplicate_time: DuplicateTime,
    /// The refusal policy.
    pub on_refusal: OnRefusal,
    /// The non-finite input policy the replay set on its engine (decision 12).
    pub non_finite: NonFiniteInputPolicy,
    /// Whether input columns are retained.
    pub retains_inputs: bool,
    /// Snapshot cadence, every N rows.
    pub snapshot_every: Option<u64>,
    /// Snapshot rows.
    pub snapshot_at: Vec<u64>,
    /// The limits.
    pub limits: ReplayLimits,
    /// The wall-clock origin, header only.
    pub origin: Option<TimeOrigin>,
    /// Source columns the map and the source ignored.
    pub ignored_columns: Vec<String>,
}

/// The record of one run. Accessors borrow; none allocates except [`Trace::canonical_bytes`].
#[derive(Clone, Debug)]
pub struct Trace {
    pub(crate) meta: TraceMeta,
    pub(crate) times: Vec<f64>,
    pub(crate) status: Vec<u8>,
    pub(crate) outputs: Vec<(String, TraceColumn)>,
    pub(crate) mask: Option<Vec<u8>>,
    pub(crate) inputs: Option<Vec<(String, TraceColumn)>>,
    pub(crate) diagnostics: Vec<DiagnosticRow>,
    pub(crate) refusals: Vec<RefusalRow>,
    pub(crate) snapshots: Vec<(u64, EngineStateSnapshot)>,
    pub(crate) sparse_bytes: u64,
}

impl Trace {
    /// The run's metadata.
    #[must_use]
    pub fn meta(&self) -> &TraceMeta {
        &self.meta
    }

    /// Frame times, one per row, bit for bit as submitted (or ticked).
    #[must_use]
    pub fn times(&self) -> &[f64] {
        &self.times
    }

    /// Status bytes, one per row ([`FrameStatus`]).
    #[must_use]
    pub fn status(&self) -> &[u8] {
        &self.status
    }

    /// The number of rows.
    #[must_use]
    pub fn rows(&self) -> usize {
        self.times.len()
    }

    /// Output columns in output-slot order; empty until the first accepted frame.
    #[must_use]
    pub fn outputs(&self) -> &[(String, TraceColumn)] {
        &self.outputs
    }

    /// Input columns in slot order, when retained.
    #[must_use]
    pub fn inputs(&self) -> Option<&[(String, TraceColumn)]> {
        self.inputs.as_deref()
    }

    /// The dense substitution mask, `mask_width()` bytes per row, input slot `i` at byte `i / 8`
    /// bit `i % 8` (least significant first); present while any input's policy can substitute.
    #[must_use]
    pub fn substitution_mask(&self) -> Option<&[u8]> {
        self.mask.as_deref()
    }

    /// Mask bytes per row: `ceil(N_in / 8)` when a mask is kept, else 0.
    #[must_use]
    pub fn mask_width(&self) -> usize {
        if self.mask.is_some() {
            self.meta.inputs.len().div_ceil(8)
        } else {
            0
        }
    }

    /// Whether input `slot` was substituted on `row`. `false` without a mask or out of range.
    #[must_use]
    pub fn substituted(&self, row: usize, slot: usize) -> bool {
        let width = self.mask_width();
        self.mask.as_ref().is_some_and(|mask| {
            slot < self.meta.inputs.len()
                && mask
                    .get(row * width + slot / 8)
                    .is_some_and(|byte| byte & (1 << (slot % 8)) != 0)
        })
    }

    /// The warnings of accepted frames, in row then emission order.
    #[must_use]
    pub fn diagnostics(&self) -> &[DiagnosticRow] {
        &self.diagnostics
    }

    /// The refused frames, in row order.
    #[must_use]
    pub fn refusals(&self) -> &[RefusalRow] {
        &self.refusals
    }

    /// State snapshots, each taken after its row.
    #[must_use]
    pub fn snapshots(&self) -> &[(u64, EngineStateSnapshot)] {
        &self.snapshots
    }

    /// Whether every row executed with observed inputs (no substitution, skip or refusal).
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.status
            .iter()
            .all(|&s| s == FrameStatus::Executed as u8)
    }

    /// The bytes-per-frame formula of the module docs, from the metadata.
    #[must_use]
    pub fn bytes_per_frame(&self) -> u64 {
        let outputs: u64 = self.meta.outputs.iter().map(|o| o.kind.width()).sum();
        let inputs: u64 = if self.meta.retains_inputs {
            self.meta.inputs.iter().map(|i| i.kind.width()).sum()
        } else {
            0
        };
        9 + outputs + self.mask_width() as u64 + inputs
    }

    /// The bytes the frame columns hold, from their lengths and element widths.
    #[must_use]
    pub fn frame_bytes(&self) -> u64 {
        let columns = |columns: &[(String, TraceColumn)]| -> u64 {
            columns
                .iter()
                .map(|(_, c)| c.len() as u64 * c.kind().width())
                .sum()
        };
        self.times.len() as u64 * 8
            + self.status.len() as u64
            + columns(&self.outputs)
            + self.mask.as_ref().map_or(0, |m| m.len() as u64)
            + self.inputs.as_deref().map_or(0, columns)
    }

    /// Frame bytes plus the sparse rows and snapshots, as counted against `max_trace_bytes`.
    #[must_use]
    pub fn byte_size(&self) -> u64 {
        self.frame_bytes() + self.sparse_bytes
    }

    /// Refuse unless every `[from, to]` window contains at least one *executed* row time
    /// (decision 7). A row on a bound counts.
    ///
    /// # Errors
    /// [`ReplayError::InvalidWindow`] for a window that is not finite with `from <= to`;
    /// [`ReplayError::Uncovered`] for the first window with no executed row.
    pub fn check_coverage(&self, windows: &[(f64, f64)]) -> Result<(), ReplayError> {
        for (index, &(from, to)) in windows.iter().enumerate() {
            if !(from.is_finite() && to.is_finite() && from <= to) {
                return Err(ReplayError::InvalidWindow { index });
            }
        }
        for (index, &(from, to)) in windows.iter().enumerate() {
            let start = self.times.partition_point(|&t| t < from);
            let end = self.times.partition_point(|&t| t <= to);
            let covered = self.status[start..end]
                .iter()
                .any(|&s| FrameStatus::from_byte(s).is_some_and(FrameStatus::executed));
            if !covered {
                return Err(ReplayError::Uncovered { index });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "trace_tests.rs"]
mod tests;
