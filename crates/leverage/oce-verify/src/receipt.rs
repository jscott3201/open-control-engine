//! The receipt: the header shared with the commissioning verifier and the closed body (vectors
//! note, "The receipt"; decisions 21 to 25, 31 to 33 and 37).
//!
//! A [`Receipt`] is plain data and evidence, never a gate: nothing here decides whether a
//! sequence may load or run. Its canonical bytes ([`Receipt::to_canonical_json`]) follow the
//! vector file's canonical rules (keys sorted by their UTF-8 bytes at every depth, two-space
//! indentation, ASCII, one final line feed), arrays keep the order each field states, and its
//! identity, `receipt_id`, is `receipt:fnv1a128:<32 hex>` over those bytes with `timings` and
//! `receipt_id` removed, so two runs on the same inputs give one id whatever they measured.
//!
//! Every field of the note's tables is here, in its stated type and bound; the values a receipt
//! records are typed by the port or parameter they belong to ([`ReceiptValue`]) and spelled
//! exactly, a negative zero, a NaN or an infinity as the bits of the binary64.
//!
//! The types are `#[non_exhaustive]` so the commissioning body can arrive in place; they are
//! built only by [`crate::run_vectors`] and read by everyone.

use oce_replay::EngineRefusal;

use crate::clock::Clock;

/// The format tag every receipt carries in its `schema` key, for every kind (decision 37).
pub const RECEIPT_SCHEMA: &str = "oce/receipt/v1";

/// Which verifier issued a receipt. The commissioning verifier adds its kind in place.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReceiptKind {
    /// A vector set replayed against a subject (`vectors`).
    Vectors,
}

impl ReceiptKind {
    /// The spelling in the receipt's `kind` key.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Vectors => "vectors",
        }
    }
}

/// How outputs were compared. Tolerance mode adds its mode in place.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ComparisonMode {
    /// Every expected value equals the output exactly; a Real compares by its bits (`exact`).
    Exact,
}

impl ComparisonMode {
    /// The spelling in the receipt's `comparison.mode` key.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
        }
    }
}

/// The header's `comparison` object: the mode and, for a tolerance mode, its envelope. Exact mode
/// has no envelope, so the object is `{ "mode": "exact" }` (decision 32).
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Comparison {
    /// The comparison mode.
    pub mode: ComparisonMode,
}

/// Durations a caller measured, in whole milliseconds. `oce-verify` measures nothing (decision
/// 31): a receipt carries what the caller passes in [`crate::VerifyOptions::timings`], or zeros.
/// Outside the hashed part, so they never move `receipt_id`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Timings {
    /// Loading the subject and binding the set.
    pub load_ms: u64,
    /// Running every scenario.
    pub run_ms: u64,
    /// Comparing the traces with the expectations.
    pub compare_ms: u64,
}

/// The header shared with the commissioning verifier. Every field is always present.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceiptHeader {
    /// [`RECEIPT_SCHEMA`].
    pub schema: String,
    /// Which verifier issued the receipt.
    pub kind: ReceiptKind,
    /// The subject's canonical content id (`oce:canon1:…`) as loaded, before any parameter edit.
    pub content_id: String,
    /// The engine version, `major.minor.patch`: [`oce_api::ENGINE_VERSION`], the version this
    /// build stamps in every compiled-schedule blob and state snapshot header and the only one it
    /// accepts there (decision 33, amended 2026-11-12). A pre-release suffix of the Cargo version
    /// is not part of it, so `0.6.0-rc.1` and `0.6.0` write the same field. Not a build identity,
    /// and inside the hashed part, so a bump of the major, minor or patch moves every receipt id.
    pub engine_version: String,
    /// The vector set's `spec_id` (`spec:fnv1a128:…`).
    pub spec_id: String,
    /// How outputs were compared.
    pub comparison: Comparison,
    /// Whether the body's status is `pass`.
    pub passed: bool,
    /// `receipt:fnv1a128:<32 hex>` over the canonical receipt with `timings` and `receipt_id`
    /// removed.
    pub receipt_id: String,
    /// What the caller measured, or zeros.
    pub timings: Timings,
}

/// A verdict: of one scenario, and of the set (decision 26).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScenarioStatus {
    /// Every instant checked passed, at least one was checked, and no frame was refused.
    Pass,
    /// A frame was refused or an instant failed; for the set, any scenario failed.
    Fail,
    /// Nothing was checked and nothing failed; for the set, it has no scenario or a scenario is
    /// `no_data` and none failed.
    NoData,
}

impl ScenarioStatus {
    /// The spelling in a receipt: `pass`, `fail` or `no_data`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::NoData => "no_data",
        }
    }
}

/// A typed value a receipt records: an expected or actual output, or a parameter edit. A receipt
/// never holds a String or an Enum, since binding refuses both kinds of port.
///
/// Spelled exactly (the note's "Values"): a Boolean as `true` or `false`, an Integer as a JSON
/// integer, a finite Real other than negative zero as a JSON number in the canonical real
/// spelling, and negative zero, a NaN or an infinity as the string `"0x"` and the 16 lowercase
/// hex digits of its bits.
#[derive(Clone, Copy, Debug)]
pub enum ReceiptValue {
    /// A Real, compared and written by its bits.
    Real(f64),
    /// An Integer.
    Integer(i64),
    /// A Boolean.
    Boolean(bool),
}

impl ReceiptValue {
    /// The engine value: what a frame submits and `set_param` sets.
    pub(crate) fn engine_value(self) -> oce_api::Value {
        match self {
            Self::Real(x) => oce_api::Value::Real(x),
            Self::Integer(x) => oce_api::Value::Integer(x),
            Self::Boolean(x) => oce_api::Value::Boolean(x),
        }
    }

    /// Whether two values are the same kind and the same value, a Real by its bits: `0.0` is not
    /// `-0.0`, and a NaN equals a NaN of the same bits.
    #[must_use]
    pub fn bit_eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Real(a), Self::Real(b)) => a.to_bits() == b.to_bits(),
            (Self::Integer(a), Self::Integer(b)) => a == b,
            (Self::Boolean(a), Self::Boolean(b)) => a == b,
            _ => false,
        }
    }
}

/// Counts of one scenario's frames, or the sums over the set, by the trace's status byte.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FrameCounts {
    /// Frames the engine accepted with every input observed.
    pub executed: u64,
    /// Frames the engine accepted with an input held or defaulted; zero under the replay
    /// defaults a vector run keeps.
    pub substituted: u64,
    /// Frames refused, by replay or by the engine; at most one per scenario, since a refusal
    /// stops the scenario's run.
    pub refused: u64,
}

/// Counts of instants: one window tick, or one diagnostic expectation at its tick.
/// `checked == passed + failed`.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct InstantCounts {
    /// Instants compared.
    pub checked: u64,
    /// Instants that held.
    pub passed: u64,
    /// Instants that did not.
    pub failed: u64,
    /// Instants masked by a `when` indicator: zero until `when` exists.
    pub masked: u64,
}

/// Scenario counts by verdict.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ScenarioCounts {
    /// Scenarios in the set.
    pub total: u64,
    /// Scenarios that passed.
    pub pass: u64,
    /// Scenarios that failed.
    pub fail: u64,
    /// Scenarios that checked nothing.
    pub no_data: u64,
}

/// The body's `totals`: sums over every scenario.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Totals {
    /// Scenario counts by verdict.
    pub scenarios: ScenarioCounts,
    /// Frame counts summed over scenarios.
    pub frames: FrameCounts,
    /// Instant counts summed over scenarios.
    pub instants: InstantCounts,
    /// The sum of every scenario's `failures_total`.
    pub failures: u64,
    /// The sum of every scenario's `warnings_total`.
    pub warnings: u64,
}

/// One effective parameter edit of a scenario, by canonical path, with its typed value.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct ParamEdit {
    /// The canonical parameter path.
    pub path: String,
    /// The value set, typed by the parameter's declared type.
    pub value: ReceiptValue,
}

/// One failure record, shaped by its code (decision 23). A failure makes the scenario `fail`
/// (exit 1), never an error.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub enum Failure {
    /// `value-mismatch`: a window's output differed from its expected value at a tick.
    ValueMismatch {
        /// The expectation's index in the scenario's `expect` list.
        expectation: u64,
        /// The output's canonical path.
        output: String,
        /// The tick's time, seconds.
        t: f64,
        /// The value the window expected.
        expected: ReceiptValue,
        /// The value the engine produced.
        actual: ReceiptValue,
    },
    /// `diagnostic-missing`: no warning at the expectation's tick matched its block (and
    /// message, when it gave one).
    DiagnosticMissing {
        /// The expectation's index in the scenario's `expect` list.
        expectation: u64,
        /// The tick's time, seconds.
        t: f64,
        /// The class path the expectation names.
        block: String,
        /// The message the expectation names, when it gave one.
        message: Option<String>,
    },
    /// `frame-refused`: the frame at this tick was refused, which ended the scenario's run. No
    /// expectation was reached.
    FrameRefused {
        /// The refused tick's time, seconds.
        t: f64,
        /// The closed class of the engine's refusal ([`crate::engine_refusal_class`]).
        refusal: EngineRefusal,
    },
}

impl Failure {
    /// The record's code: `value-mismatch`, `diagnostic-missing` or `frame-refused`.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::ValueMismatch { .. } => "value-mismatch",
            Self::DiagnosticMissing { .. } => "diagnostic-missing",
            Self::FrameRefused { .. } => "frame-refused",
        }
    }

    /// The time of the tick the record is about, seconds.
    #[must_use]
    pub const fn t(&self) -> f64 {
        match self {
            Self::ValueMismatch { t, .. }
            | Self::DiagnosticMissing { t, .. }
            | Self::FrameRefused { t, .. } => *t,
        }
    }
}

/// One run-time warning no diagnostic expectation of the scenario matched. Reported, never a
/// failure (decision 12).
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct UnmatchedWarning {
    /// The model time the engine stamped on the warning, seconds.
    pub t: f64,
    /// The producing block's class path (`CDL.Utilities.Assert`): a class, not an instance.
    pub block: String,
    /// The warning's text.
    pub message: String,
}

/// One scenario of a vector receipt.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct ScenarioReceipt {
    /// The scenario's name, as written.
    pub name: String,
    /// The scenario's verdict.
    pub status: ScenarioStatus,
    /// The effective parameter edits, sorted by canonical path bytes; empty when there are none.
    pub params: Vec<ParamEdit>,
    /// `Engine::content_id` after the edits, the id of what ran; `None` when `params` is empty.
    pub content_id: Option<String>,
    /// The scenario's frames.
    pub frames: FrameCounts,
    /// The number of entries in the scenario's `expect` list.
    pub expectations: u64,
    /// The scenario's instants.
    pub instants: InstantCounts,
    /// The failure records, by tick then by expectation index; at most
    /// [`crate::VerifyLimits::max_recorded_failures`].
    pub failures: Vec<Failure>,
    /// The number of failures before the cap.
    pub failures_total: u64,
    /// The unmatched warnings, in row then emission order; at most
    /// [`crate::VerifyLimits::max_recorded_warnings`].
    pub warnings: Vec<UnmatchedWarning>,
    /// The number of unmatched warnings before the cap.
    pub warnings_total: u64,
}

/// What happened to one mutant (decisions 27 and 29).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MutantStatus {
    /// A scenario failed on the mutant: the vectors noticed the defect.
    Caught,
    /// Every scenario passed on the mutant (or checked nothing): the vectors missed the defect.
    Survived,
    /// The mutant was not run, because the subject did not pass.
    NotRun,
}

impl MutantStatus {
    /// The spelling in a receipt: `caught`, `survived` or `not_run`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Caught => "caught",
            Self::Survived => "survived",
            Self::NotRun => "not_run",
        }
    }
}

/// Whether the mutants were scored: only when the subject passed (decision 29).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScoreStatus {
    /// The subject passed and every mutant was run.
    Scored,
    /// The subject did not pass, so no mutant was run: a mutant that fails the scenario the
    /// subject fails proves nothing.
    NotRun,
}

impl ScoreStatus {
    /// The spelling in a receipt: `scored` or `not_run`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scored => "scored",
            Self::NotRun => "not_run",
        }
    }
}

/// The scenario that caught a mutant, and that scenario's first failure on it.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct MutantCatch {
    /// The first scenario, in file order, that failed on the mutant.
    pub scenario: String,
    /// That scenario's first failure on the mutant, by tick then expectation index, shaped as a
    /// scenario's failure records are.
    pub failure: Failure,
}

/// One listed mutant in a receipt.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct MutantEntry {
    /// The path, as the set writes it.
    pub path: String,
    /// `fnv1a128:<32 hex>` of the bytes the run was given for it: which document was scored,
    /// where the set's `spec_id` covers only its path.
    pub digest: String,
    /// The mutant's canonical content id as loaded (every listed mutant loaded, or there is no
    /// receipt).
    pub content_id: String,
    /// What happened to it.
    pub status: MutantStatus,
    /// The scenario that caught it and its first failure, when it was caught.
    pub caught_by: Option<MutantCatch>,
}

/// The body's `mutants` section, present when the set lists a mutant. The score is two integers,
/// never a ratio (decision 29), and it never changes `passed` (decision 30).
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct MutantSection {
    /// Whether the mutants were scored.
    pub status: ScoreStatus,
    /// The number of mutants the set lists.
    pub total: u64,
    /// The mutants caught; zero when not run.
    pub caught: u64,
    /// The mutants that survived; zero when not run.
    pub survived: u64,
    /// One entry per listed mutant, in file order.
    pub entries: Vec<MutantEntry>,
}

/// The body of a vector receipt: the field table of the note, in its order.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct VectorBody {
    /// The set's verdict.
    pub status: ScenarioStatus,
    /// The set's clock, written as the set writes it.
    pub clock: Clock,
    /// The subject's load warnings as `DiagCode` spellings, in load order; at most
    /// [`crate::MAX_LOAD_WARNINGS`].
    pub load_warnings: Vec<String>,
    /// The number of load warnings before the cap.
    pub load_warnings_total: u64,
    /// Sums over every scenario.
    pub totals: Totals,
    /// One entry per scenario, in file order.
    pub scenarios: Vec<ScenarioReceipt>,
    /// The mutant score, when the set lists at least one mutant.
    pub mutants: Option<MutantSection>,
}

/// The body of a receipt, by kind: a closed enum (decision 21). The commissioning verifier adds
/// its variant in place.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub enum ReceiptBody {
    /// A vector run's body.
    Vectors(VectorBody),
}

/// One receipt: the shared header and the body (decision 21).
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct Receipt {
    /// The header.
    pub header: ReceiptHeader,
    /// The body.
    pub body: ReceiptBody,
}

impl Receipt {
    /// The canonical document of this receipt: the vector file's canonical rules, every header
    /// field and the body, keys sorted at every depth, arrays in the order each field states.
    /// Deterministic: the same receipt always writes the same bytes.
    #[must_use]
    pub fn to_canonical_json(&self) -> String {
        crate::receipt_json::receipt(self, true).to_document()
    }

    /// The identity of this receipt's content: `receipt:fnv1a128:` and the 32 lowercase hex
    /// digits of FNV-1a-128 over [`Self::to_canonical_json`] with the header's `timings` and
    /// `receipt_id` removed. [`crate::run_vectors`] stores it in `header.receipt_id`; recomputing
    /// it checks that a receipt was not edited after it was issued.
    #[must_use]
    pub fn computed_receipt_id(&self) -> String {
        crate::fnv::tagged(
            "receipt:",
            crate::receipt_json::receipt(self, false)
                .to_document()
                .as_bytes(),
        )
    }
}
