//! The body of a commissioning receipt (commissioning note, "Report and receipt", whose two
//! tables are normative: decision 31).
//!
//! Plain data, built only by [`crate::run_commissioning`] and written by the receipt's one
//! canonical writer. Every field of the note's body and output tables is here, in its type and
//! with its presence rule: a field the table marks "when …" is an `Option`. Each capped list
//! carries its `_total` beside it, and every cap is a constant of this module, never a caller's
//! option (decision 42), so one run has one receipt.

use oce_replay::TimeOrigin;

use super::choices::FunnelReference;
use super::tolerances::PartialTolerances;
use super::trend_types::{MappingRecord, SourceRecord, UnitColumn};
use crate::envelope::HalfSize;
use crate::number::{Literal, Number};
use crate::receipt::{FrameCounts, UnmatchedWarning};

/// The most `units.columns` rows a receipt lists; `units.columns_total` counts them all.
pub const MAX_UNIT_COLUMNS: usize = 256;
/// The most run-time warnings a commissioning receipt lists; `warnings_total` counts them all.
pub const MAX_RUN_WARNINGS: usize = 16;
/// The most findings a receipt lists; `findings_total` counts them all.
pub const MAX_FINDINGS: usize = 64;
/// The most unverified outputs a receipt lists; `unverified_outputs_total` counts them all.
pub const MAX_UNVERIFIED_OUTPUTS: usize = 64;
/// The most masked intervals one output lists; `masked_intervals_total` counts them all.
pub const MAX_MASKED_INTERVALS: usize = 64;
/// The most masked exceedances one output lists; `masked_exceedance_total` counts them all.
pub const MAX_MASKED_EXCEEDANCE: usize = 16;
/// The most outputs one run compares (decision 42): a complete list, refused past it at bind as
/// `too-many-outputs`.
pub const MAX_COMPARED_OUTPUTS: usize = 4_096;
/// The most comparison points one run holds over all its outputs (decision 42), refused past it
/// at bind as `too-many-points`.
pub const MAX_RUN_POINTS: u64 = 16_777_216;

/// What the caller says the trend files are (decisions 4 and 41): a claim inside the hashed part
/// of the receipt, never checked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TrendData {
    /// Recorded by a real controller (`trended`). The default.
    #[default]
    Trended,
    /// Generated, for instance from the engine and perturbed (`synthetic`).
    Synthetic,
}

impl TrendData {
    /// `trended` or `synthetic`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Trended => "trended",
            Self::Synthetic => "synthetic",
        }
    }
}

/// A verdict of one output, and of the run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OutputStatus {
    /// At least one instant was compared and none exceeded.
    Pass,
    /// A compared instant exceeded, or the engine series held a non-finite or out-of-range value
    /// at an instant neither masked nor uncovered.
    Fail,
    /// Nothing was compared: "zero compared points is `no_data`, never `pass`".
    NoData,
}

impl OutputStatus {
    /// `pass`, `fail` or `no_data`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::NoData => "no_data",
        }
    }
}

/// The type of a compared output: a Real goes through the envelope, the other two are compared
/// exactly (decision 8).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OutputType {
    /// `real`.
    Real,
    /// `integer`.
    Integer,
    /// `boolean`.
    Boolean,
}

impl OutputType {
    /// `real`, `integer` or `boolean`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Real => "real",
            Self::Integer => "integer",
            Self::Boolean => "boolean",
        }
    }
}

/// A number as the spec wrote it (a window end, a sampling): written in the receipt as the
/// spec's canonical form writes it, so `5` stays `5` and `5.0` stays `5.0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpecNumber(pub(crate) Number);

impl SpecNumber {
    /// The value, in seconds or in its own unit.
    #[must_use]
    pub fn value(&self) -> f64 {
        self.0.seconds()
    }

    /// The spec's canonical spelling of the number.
    #[must_use]
    pub fn canonical(&self) -> String {
        self.0.canonical()
    }
}

/// One parameter edit of the reference: the canonical parameter path and the value as the spec
/// wrote it, in CDL units and never converted (decision 24).
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct CommissioningParam {
    /// The canonical parameter path.
    pub path: String,
    /// The value as the spec wrote it.
    pub value: Literal,
}

/// The body's `time` object.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct TimeRecord {
    /// `time.origin`, when the spec gives one: the instant model seconds count from.
    pub origin: Option<TimeOrigin>,
    /// The window's start, as the spec wrote it.
    pub window_start: SpecNumber,
    /// The window's stop, as the spec wrote it.
    pub window_stop: SpecNumber,
    /// The sampling that applies to the reference, its own or the spec's: a run needs one.
    pub sampling: SpecNumber,
}

/// One finding of the spec loader that concerns the top level or this reference.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindingRecord {
    /// The finding's code, `lbl-field-ignored`.
    pub code: String,
    /// The field it is about (`references[1].runController`).
    pub at: String,
}

/// An instant and its exceedance: a `worst` or a `masked_exceedance` entry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Exceedance {
    /// The instant, in model seconds.
    pub t: f64,
    /// How far the test lay outside the band there, in the output's CDL unit; positive
    /// infinity for a non-finite or out-of-range engine sample.
    pub exceedance: f64,
}

/// A run of masked test instants: the first and the last.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaskedInterval {
    /// The first masked instant of the run, in model seconds.
    pub from: f64,
    /// The last masked instant of the run, in model seconds.
    pub to: f64,
}

/// One compared output's report (the note's output table).
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct OutputReport {
    /// The canonical path.
    pub output: String,
    /// The output's type.
    pub output_type: OutputType,
    /// The verdict.
    pub status: OutputStatus,
    /// The test instants compared.
    pub compared: u64,
    /// The test instants an indicator excluded.
    pub masked: u64,
    /// The test instants in a refused stretch that no indicator masked (decision 28).
    pub uncovered: u64,
    /// Which series the envelope is built around; for a Real output only.
    pub funnel_reference: Option<FunnelReference>,
    /// The tolerances after the overrides; for a Real output only. A name nothing gives is
    /// written `0`.
    pub tolerances: Option<PartialTolerances>,
    /// The half-widths applied, when an envelope was built.
    pub half_width: Option<HalfSize>,
    /// The half-heights applied, when an envelope was built.
    pub half_height: Option<HalfSize>,
    /// The largest exceedance over the compared instants and the earliest instant that has it,
    /// when the status is `fail`.
    pub worst: Option<Exceedance>,
    /// The earliest compared instant with an exceedance above zero, when the status is `fail`.
    pub first_exceedance_t: Option<f64>,
    /// The time in exceedance, in seconds: the sum over the compared instants with an exceedance
    /// above zero of the step to the next test instant.
    pub exceedance_time_s: f64,
    /// The runs of masked instants, in time order; at most [`MAX_MASKED_INTERVALS`].
    pub masked_intervals: Vec<MaskedInterval>,
    /// The number of runs before the cap.
    pub masked_intervals_total: u64,
    /// The masked instants whose envelope error is above zero, in time order; at most
    /// [`MAX_MASKED_EXCEEDANCE`]. Reported, never counted (decision 23).
    pub masked_exceedance: Vec<Exceedance>,
    /// The number of such instants before the cap.
    pub masked_exceedance_total: u64,
}

/// The body of a commissioning receipt (the note's body table, in its order).
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct CommissioningBody {
    /// The run's verdict: `fail` when an output failed; else `no_data` when no output was
    /// compared or one is `no_data`; else `pass`.
    pub status: OutputStatus,
    /// The zero-based index of the reference in the spec's `references`.
    pub reference: u64,
    /// `model`, as the reference writes it; a label.
    pub model: String,
    /// `sequence`, as the reference writes it; a label.
    pub sequence: String,
    /// What the caller says the trends are.
    pub data: TrendData,
    /// The parameter edits, sorted by path bytes; empty when there are none.
    pub parameters: Vec<CommissioningParam>,
    /// `Engine::content_id` after the edits, the id of what ran (decision 30); `None` when there
    /// are no edits. The header's `content_id` is the CXF's as loaded, before them.
    pub content_id: Option<String>,
    /// The time base, origin, window and sampling.
    pub time: TimeRecord,
    /// The mapping file, when the mapping is one.
    pub mapping: Option<MappingRecord>,
    /// The converted points, sorted by `device`; at most [`MAX_UNIT_COLUMNS`].
    pub unit_columns: Vec<UnitColumn>,
    /// The number of converted points before the cap.
    pub unit_columns_total: u64,
    /// One record per trend source, in role then declaration order. Complete.
    pub sources: Vec<SourceRecord>,
    /// The run's frames by status byte; `refused` counts missing-sample refusals only.
    pub frames: FrameCounts,
    /// The CXF's load warnings as `DiagCode` spellings in load order; at most
    /// [`crate::MAX_LOAD_WARNINGS`].
    pub load_warnings: Vec<String>,
    /// The number of load warnings before the cap.
    pub load_warnings_total: u64,
    /// The run-time warnings of the trace in row then emission order; at most
    /// [`MAX_RUN_WARNINGS`].
    pub warnings: Vec<UnmatchedWarning>,
    /// The number of run-time warnings before the cap.
    pub warnings_total: u64,
    /// One report per compared output, sorted by canonical path bytes. Complete.
    pub outputs: Vec<OutputReport>,
    /// The boundary outputs no `expected` source supplies, sorted; at most
    /// [`MAX_UNVERIFIED_OUTPUTS`]. They change no verdict (decision 40).
    pub unverified_outputs: Vec<String>,
    /// The number of unverified outputs before the cap.
    pub unverified_outputs_total: u64,
    /// The spec loader's findings about the top level and this reference; at most
    /// [`MAX_FINDINGS`].
    pub findings: Vec<FindingRecord>,
    /// The number of findings before the cap.
    pub findings_total: u64,
}

/// The first `cap` of `items`, and how many there were.
pub(crate) fn capped<T>(mut items: Vec<T>, cap: usize) -> (Vec<T>, u64) {
    let total = u64::try_from(items.len()).unwrap_or(u64::MAX);
    items.truncate(cap);
    (items, total)
}
