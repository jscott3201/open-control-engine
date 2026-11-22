//! Why a run gave no receipt: the bind and run refusals (vectors note, "Error codes"; decision
//! 18).
//!
//! A [`VerifyError`] is one closed [`VerifyErrorKind`] with a stable kebab-case
//! [`VerifyErrorKind::code`], the place in the set or the subject it sits on, and a sentence for
//! people; some kinds carry more, each through its own accessor: the candidates of an ambiguous
//! name, the canonical path a refusal concerns, the engine's diagnostic codes of a subject that
//! did not load, and a cause from a closed table this crate owns. The code is the contract: a
//! test, a tool or an agent branches on it, never on the sentence. [`VERIFY_ERROR_CODES`] lists
//! every code in declaration order and a test pins it. Every one is exit 2 with no receipt: a
//! failed expectation is never an error, it is a failed scenario inside a receipt.
//!
//! The four budget rows (`too-many-frames`, `too-many-instants`, `too-many-load-bytes`,
//! `too-many-frame-bytes`) are bind codes from one step, after the subject's first load and its
//! names and before every other load (the work-budget note, rulings 6 and 8): each refuses a run
//! whose work, counted as an upper bound, passes its limit.
//!
//! The mutant rows (`mutant-missing` and the seven after it) are bind codes: an invalid or
//! missing mutant is an error with no receipt, never a score.

use std::fmt;

/// What a bind or a run refused. Each kind has one stable code ([`VerifyErrorKind::code`]);
/// `#[non_exhaustive]`, so consumers branch on the code.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VerifyErrorKind {
    /// The engine refused to load the subject. [`VerifyError::diagnostic_codes`] lists the
    /// `DiagCode` spellings of every diagnostic the refusal carries, in the engine's stage order.
    SubjectLoadFailed,
    /// An input name matches no boundary input, by exact path or by suffix.
    UnknownInput,
    /// An input suffix matches more than one boundary input; [`VerifyError::candidates`] lists
    /// them.
    AmbiguousInput,
    /// Two input names of one scenario resolve to the same boundary input.
    DuplicateInput,
    /// A boundary input that no name of the scenario supplies (the first in slot order).
    InputNotSupplied,
    /// An output name matches no boundary output.
    UnknownOutput,
    /// An output suffix matches more than one boundary output; [`VerifyError::candidates`] lists
    /// them.
    AmbiguousOutput,
    /// A parameter key matches no parameter.
    ParamPathUnknown,
    /// A parameter suffix matches more than one parameter; [`VerifyError::candidates`] lists them.
    ParamPathAmbiguous,
    /// Two keys of one `params` map resolve to the same parameter.
    ParamPathDuplicate,
    /// A parameter edit the engine still refused when a pass applied nothing (decision 14),
    /// found at bind, where every scenario's edits are tried before any frame runs;
    /// [`VerifyError::cause`] is the refusal's class from [`PARAM_REFUSAL_CLASSES`].
    ParamRefused,
    /// A value whose JSON kind its port's or parameter's declared type does not take: a real or
    /// a boolean on an Integer, a number on a Boolean, a boolean on a Real, anything on a String
    /// or Enum parameter.
    ValueTypeMismatch,
    /// An integer token on a Real port or parameter that no binary64 holds exactly.
    ValueNotRepresentable,
    /// The subject has an Enum boundary input or output: no vector value spells an enumeration
    /// member yet (until #465).
    EnumPortUnsupported,
    /// The subject has a String boundary input or output: a String is metadata, never a signal.
    UnsupportedPortType,
    /// The frames the set may need, `(N + 1) * scenarios * (1 + mutants)`, are more than
    /// [`crate::VerifyLimits::max_total_frames`], or one scenario's `N + 1` more than the replay
    /// limits' `max_frames`. The first budget checked.
    TooManyFrames,
    /// A `when` name matches no boundary input or output, by exact path or by suffix.
    WhenUnknown,
    /// A `when` suffix matches more than one boundary input or output;
    /// [`VerifyError::candidates`] lists them.
    WhenAmbiguous,
    /// A `when` name resolves to a port that is not a Boolean.
    WhenNotBoolean,
    /// A tolerance window (`within` and `of`) on a Boolean or Integer output: the envelope is
    /// type-blind, so a value between two levels could pass.
    ToleranceOnDiscreteOutput,
    /// More comparison points than the envelope's limit allows ([`crate::CompareLimits`]:
    /// `max_funnel_points` or `max_aligned_grid_points`), or, under `aligned-grid`, the ticks or
    /// instants squared past their bound. A vector set: a tolerance window's ticks, or the
    /// windows' ticks squared past [`crate::VerifyLimits::max_total_aligned_grid_work`], checked
    /// at bind's budget step, straight after the instant budget. A commissioning run: an
    /// output's instants, the run's `instants × (1 + indicators)` past 16,777,216 (decision 42),
    /// or the instants squared past [`crate::MAX_TOTAL_ALIGNED_GRID_WORK`], all before the replay
    /// runs, so a comparison never returns the port's code of the same spelling.
    TooManyPoints,
    /// The instants the set may compare, `(1 + mutants)` times its scenarios' window ticks and
    /// diagnostic expectations, are more than [`crate::VerifyLimits::max_total_instants`].
    TooManyInstants,
    /// The bytes the run may load, the subject's and the supplied mutants' together times
    /// `1 + e + s`, are more than [`crate::VerifyLimits::max_total_load_bytes`].
    TooManyLoadBytes,
    /// The byte-frames the run may execute, `(N + 1) * s` times the subject's and the supplied
    /// mutants' bytes, are more than [`crate::VerifyLimits::max_total_frame_bytes`].
    TooManyFrameBytes,
    /// A mutant the set lists whose bytes the run was not given.
    MutantMissing,
    /// Mutant bytes the run was given under a path the set does not list.
    MutantUnlisted,
    /// Mutant bytes given twice under one path: one refusal, spelled as the load code for a path
    /// the set lists twice.
    DuplicateMutant,
    /// The mutants' bytes together pass [`crate::VerifyLimits::max_mutant_bytes`].
    TooManyMutantBytes,
    /// The engine refused to load a mutant. [`VerifyError::diagnostic_codes`] lists the
    /// `DiagCode` spellings of the refusal, as for `subject-load-failed`.
    MutantLoadFailed,
    /// A mutant's boundary differs from the subject's: an input path or type, or an output path
    /// or type (input bounds are not compared). [`VerifyError::path`] is the first port that
    /// differs.
    MutantBoundaryDiffers,
    /// A mutant has the subject's canonical content id: it promises the subject's outputs, so it
    /// proves nothing.
    MutantIdentical,
    /// A mutant refuses a parameter edit the subject admits, so a scenario could not run on it
    /// (owner ruling of 2026-11-11): found at bind, where every distinct edit list is tried on
    /// each mutant. [`VerifyError::path`] is the parameter, [`VerifyError::cause`] one of
    /// [`MUTANT_PARAM_REFUSAL_CAUSES`].
    MutantParamRefused,
    /// The engine refused a frame for a reason about the run rather than the frame
    /// (`EngineRefusal::Run`): later frames would be refused the same way.
    RunRefused,
    /// The envelope gave a window or an output no result: [`VerifyError::cause`] is one of
    /// [`COMPARE_REFUSAL_CAUSES`]. For a vector set it is found by the self-enclosure call, the
    /// expected signal compared against itself, so it never depends on the values of the output
    /// the window judges. A commissioning run has no self-enclosure call: its refusal comes from
    /// comparing the output itself.
    CompareRefused,
    /// A replay ended with an error rather than a verdict; [`VerifyError::cause`] is its class
    /// from [`REPLAY_FAILURE_CAUSES`], never a Rust variant name.
    ReplayFailed,
    /// A window held no executed tick after a run in which no frame was refused. Load already
    /// proves every window holds a tick of the clock, and a run that refused no frame executed
    /// every tick, so this is a defence that no loaded set reaches.
    WindowUncovered,
    /// A commissioning run named a reference index past the end of the spec's `references`
    /// (decision 35).
    ReferenceOutOfRange,
    /// A reference that loads but cannot run: it has no `cxf`, or no `window` (decision 16).
    /// [`VerifyError::subject`] names the missing field.
    ReferenceIncomplete,
    /// A reference's `outputs` or `indicators` keys could not be matched against the sequence's
    /// outputs: [`VerifyError::cause`] is the spec reader's code, `unmatched-key`,
    /// `ambiguous-output`, `conflicting-override` or `work-limit-exceeded`.
    SpecRefused,
    /// An indicator name that is neither one signal of the sequence (`<sequence>.<path>`,
    /// resolved against its inputs and outputs) nor the `cdl.name` of a point an
    /// `indicatorSources` source reads (decision 27). A name with the prefix that matches several
    /// signals is refused the same way, with [`VerifyError::candidates`].
    UnknownIndicator,
    /// An indicator whose value at a sample is neither a Boolean nor exactly `0` or `1`
    /// (decision 2; owner ruling of 2026-11-21): a Real at `0.5`, a NaN, an Integer at `2`.
    IndicatorNotBoolean,
    /// Two mapped names of the expected outputs resolve to one boundary output.
    DuplicateOutput,
    /// The engine refused a submitted frame for a reason about the frame (decision 28): a value
    /// outside an input's domain, a non-finite value, a time it cannot take. The run ends with no
    /// receipt. [`VerifyError::cause`] is the class ([`crate::ENGINE_REFUSAL_CLASSES`]);
    /// [`VerifyError::frame_index`], [`VerifyError::frame_time`] and
    /// [`VerifyError::engine_message`] say where and what.
    FrameRefused,
    /// A reference reads more trend sources than [`crate::TrendLimits::max_sources`] (decision
    /// 42).
    TooManySources,
    /// A reference compares more outputs than 4,096 (decision 42).
    TooManyOutputs,
    /// A trend source, or the point mapping, was refused: [`VerifyError::cause`] is the
    /// [`crate::SourceError`] code.
    SourceRefused,
    /// A mapped point's units do not fit the sequence port it lands on: [`VerifyError::cause`] is
    /// the [`crate::UnitError`] code (`port-unit-mismatch`, `absolute-for-difference`,
    /// `difference-for-absolute`).
    UnitRefused,
    /// A commissioning reference under the `aligned-grid` envelope with no `sampling`, its own or
    /// the spec's (owner ruling of 2026-11-22, narrowing the ruling of 2026-11-21). Without a
    /// grid the reference is compared by change of value, and that envelope's band, a straight
    /// line between its grid points, would cover a step's old level across a gap and pass a
    /// pulse the trend never logged. A `funnel` reference needs no `sampling`. Issue #792 asks
    /// for a step-shaped grid band, which lifts it.
    SamplingRequired,
    /// A commissioning reference whose indicator rules give more than
    /// [`crate::MAX_INDICATORS`] distinct names.
    TooManyIndicators,
}

impl VerifyErrorKind {
    /// Every kind, in declaration order.
    const ALL: &'static [Self] = &[
        Self::SubjectLoadFailed,
        Self::UnknownInput,
        Self::AmbiguousInput,
        Self::DuplicateInput,
        Self::InputNotSupplied,
        Self::UnknownOutput,
        Self::AmbiguousOutput,
        Self::ParamPathUnknown,
        Self::ParamPathAmbiguous,
        Self::ParamPathDuplicate,
        Self::ParamRefused,
        Self::ValueTypeMismatch,
        Self::ValueNotRepresentable,
        Self::EnumPortUnsupported,
        Self::UnsupportedPortType,
        Self::TooManyFrames,
        Self::WhenUnknown,
        Self::WhenAmbiguous,
        Self::WhenNotBoolean,
        Self::ToleranceOnDiscreteOutput,
        Self::TooManyPoints,
        Self::TooManyInstants,
        Self::TooManyLoadBytes,
        Self::TooManyFrameBytes,
        Self::MutantMissing,
        Self::MutantUnlisted,
        Self::DuplicateMutant,
        Self::TooManyMutantBytes,
        Self::MutantLoadFailed,
        Self::MutantBoundaryDiffers,
        Self::MutantIdentical,
        Self::MutantParamRefused,
        Self::RunRefused,
        Self::CompareRefused,
        Self::ReplayFailed,
        Self::WindowUncovered,
        Self::ReferenceOutOfRange,
        Self::ReferenceIncomplete,
        Self::SpecRefused,
        Self::UnknownIndicator,
        Self::IndicatorNotBoolean,
        Self::DuplicateOutput,
        Self::FrameRefused,
        Self::TooManySources,
        Self::TooManyOutputs,
        Self::SourceRefused,
        Self::UnitRefused,
        Self::SamplingRequired,
        Self::TooManyIndicators,
    ];

    /// The stable kebab-case code of this kind: the contract a caller branches on.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::SubjectLoadFailed => "subject-load-failed",
            Self::UnknownInput => "unknown-input",
            Self::AmbiguousInput => "ambiguous-input",
            Self::DuplicateInput => "duplicate-input",
            Self::InputNotSupplied => "input-not-supplied",
            Self::UnknownOutput => "unknown-output",
            Self::AmbiguousOutput => "ambiguous-output",
            Self::ParamPathUnknown => "param-path-unknown",
            Self::ParamPathAmbiguous => "param-path-ambiguous",
            Self::ParamPathDuplicate => "param-path-duplicate",
            Self::ParamRefused => "param-refused",
            Self::ValueTypeMismatch => "value-type-mismatch",
            Self::ValueNotRepresentable => "value-not-representable",
            Self::EnumPortUnsupported => "enum-port-unsupported",
            Self::UnsupportedPortType => "unsupported-port-type",
            Self::TooManyFrames => "too-many-frames",
            Self::WhenUnknown => "when-unknown",
            Self::WhenAmbiguous => "when-ambiguous",
            Self::WhenNotBoolean => "when-not-boolean",
            Self::ToleranceOnDiscreteOutput => "tolerance-on-discrete-output",
            Self::TooManyPoints => "too-many-points",
            Self::TooManyInstants => "too-many-instants",
            Self::TooManyLoadBytes => "too-many-load-bytes",
            Self::TooManyFrameBytes => "too-many-frame-bytes",
            Self::MutantMissing => "mutant-missing",
            Self::MutantUnlisted => "mutant-unlisted",
            Self::DuplicateMutant => "duplicate-mutant",
            Self::TooManyMutantBytes => "too-many-mutant-bytes",
            Self::MutantLoadFailed => "mutant-load-failed",
            Self::MutantBoundaryDiffers => "mutant-boundary-differs",
            Self::MutantIdentical => "mutant-identical",
            Self::MutantParamRefused => "mutant-param-refused",
            Self::RunRefused => "run-refused",
            Self::CompareRefused => "compare-refused",
            Self::ReplayFailed => "replay-failed",
            Self::WindowUncovered => "window-uncovered",
            Self::ReferenceOutOfRange => "reference-out-of-range",
            Self::ReferenceIncomplete => "reference-incomplete",
            Self::SpecRefused => "spec-refused",
            Self::UnknownIndicator => "unknown-indicator",
            Self::IndicatorNotBoolean => "indicator-not-boolean",
            Self::DuplicateOutput => "duplicate-output",
            Self::FrameRefused => "frame-refused",
            Self::TooManySources => "too-many-sources",
            Self::TooManyOutputs => "too-many-outputs",
            Self::SourceRefused => "source-refused",
            Self::UnitRefused => "unit-refused",
            Self::SamplingRequired => "sampling-required",
            Self::TooManyIndicators => "too-many-indicators",
        }
    }
}

/// Every [`VerifyErrorKind`] code, in declaration order.
pub const VERIFY_ERROR_CODES: &[&str] = &[
    "subject-load-failed",
    "unknown-input",
    "ambiguous-input",
    "duplicate-input",
    "input-not-supplied",
    "unknown-output",
    "ambiguous-output",
    "param-path-unknown",
    "param-path-ambiguous",
    "param-path-duplicate",
    "param-refused",
    "value-type-mismatch",
    "value-not-representable",
    "enum-port-unsupported",
    "unsupported-port-type",
    "too-many-frames",
    "when-unknown",
    "when-ambiguous",
    "when-not-boolean",
    "tolerance-on-discrete-output",
    "too-many-points",
    "too-many-instants",
    "too-many-load-bytes",
    "too-many-frame-bytes",
    "mutant-missing",
    "mutant-unlisted",
    "duplicate-mutant",
    "too-many-mutant-bytes",
    "mutant-load-failed",
    "mutant-boundary-differs",
    "mutant-identical",
    "mutant-param-refused",
    "run-refused",
    "compare-refused",
    "replay-failed",
    "window-uncovered",
    "reference-out-of-range",
    "reference-incomplete",
    "spec-refused",
    "unknown-indicator",
    "indicator-not-boolean",
    "duplicate-output",
    "frame-refused",
    "too-many-sources",
    "too-many-outputs",
    "source-refused",
    "unit-refused",
    "sampling-required",
    "too-many-indicators",
];

/// The causes a `replay-failed` refusal carries ([`VerifyError::cause`]), the closed table the
/// vectors note gives:
///
/// - `trace-too-large`: the trace would pass the replay limits' `max_trace_bytes`;
/// - `output-set-changed`: an accepted frame's outputs differ from the first accepted frame's;
/// - `grid-not-increasing`: two consecutive grid ticks rounded to one time;
/// - `engine-setup`: the engine or the replay refused to set a scenario's run up (a fresh load,
///   the parameter halt or resume, the slot map, the content id, `Replay::new`), and any other
///   replay error the table does not name;
/// - `source`: the step source refused its lists (a lowered replay limit on rows, steps or
///   columns).
pub const REPLAY_FAILURE_CAUSES: &[&str] = &[
    "trace-too-large",
    "output-set-changed",
    "grid-not-increasing",
    "engine-setup",
    "source",
];

/// The classes a `param-refused` refusal carries ([`VerifyError::cause`]): the facade's refusal
/// of the edit, `OcError::ParamRange` (outside the declared bounds or a class rule such as
/// `uLow <= uHigh`), `OcError::ParamType` or `OcError::ParamStructural` (a conditional-instance
/// parameter, or a value that changes the block's schedule shape, its feedthrough or which
/// inputs it reads when it fires, such as `PIDWithReset.controllerType` across the modes with an
/// integrator; either needs a reload). Any other refusal of `set_param` is not about the edit's
/// value and is `replay-failed` with the cause `engine-setup`.
pub const PARAM_REFUSAL_CLASSES: &[&str] = &["param-range", "param-type", "param-structural"];

/// The causes a `mutant-param-refused` refusal carries ([`VerifyError::cause`]): the three
/// [`PARAM_REFUSAL_CLASSES`], and `param-path-unknown` for a parameter the subject has and the
/// mutant does not (`OcError::UnknownPoint`).
pub const MUTANT_PARAM_REFUSAL_CAUSES: &[&str] = &[
    "param-range",
    "param-type",
    "param-structural",
    "param-path-unknown",
];

/// The causes a `compare-refused` refusal carries ([`VerifyError::cause`]): the funnel port's
/// ten [`crate::COMPARE_ERROR_CODES`], in their order, then `reference-outside-envelope`, a
/// window's expected signal that leaves its own envelope (an error above zero at a tick of the
/// self-enclosure call). `non-finite-envelope` means the envelope's arithmetic left the finite
/// numbers: for a vector set that includes a bound past 2^1000 in magnitude, which the
/// self-enclosure call refuses; a commissioning run treats an engine sample past 2^1000 as a
/// failed output instead, so it meets the code only for a trend that large. A loaded set can
/// reach four: `work-limit-exceeded`, `loop-removal-undefined`, `non-finite-envelope` and
/// `reference-outside-envelope`; load and bind refuse what would give the others. A
/// commissioning run reaches the first three only: it has no self-enclosure call and refuses
/// the point caps before the replay.
pub const COMPARE_REFUSAL_CAUSES: &[&str] = &[
    "invalid-tolerance",
    "length-mismatch",
    "empty-series",
    "too-many-points",
    "non-finite-sample",
    "x-not-non-decreasing",
    "mismatched-endpoints",
    "work-limit-exceeded",
    "loop-removal-undefined",
    "non-finite-envelope",
    REFERENCE_OUTSIDE_ENVELOPE,
];

/// The cause of a `compare-refused` refusal whose expected signal leaves its own envelope.
pub(crate) const REFERENCE_OUTSIDE_ENVELOPE: &str = "reference-outside-envelope";

/// The `compare-refused` cause of a port refusal: its code, held to [`COMPARE_REFUSAL_CAUSES`] by
/// an exhaustive match, so a new port refusal does not compile until the table has its row.
pub(crate) fn compare_refusal_cause(error: &crate::CompareError) -> &'static str {
    use crate::CompareError as E;
    let index = match error {
        E::InvalidTolerance { .. } => 0,
        E::LengthMismatch { .. } => 1,
        E::EmptySeries { .. } => 2,
        E::TooManyPoints { .. } => 3,
        E::NonFiniteSample { .. } => 4,
        E::XNotNonDecreasing { .. } => 5,
        E::MismatchedEndpoints { .. } => 6,
        E::WorkLimitExceeded { .. } => 7,
        E::LoopRemovalUndefined => 8,
        E::NonFiniteEnvelope => 9,
    };
    COMPARE_REFUSAL_CAUSES[index]
}

/// Whether two strings are the same bytes, in a constant context.
const fn same(left: &str, right: &str) -> bool {
    let (left, right) = (left.as_bytes(), right.as_bytes());
    if left.len() != right.len() {
        return false;
    }
    let mut index = 0;
    while index < left.len() {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    true
}

// The list and the kinds cannot drift apart: a build fails unless `VERIFY_ERROR_CODES` is the
// code of every kind in `VerifyErrorKind::ALL`, in order.
const _: () = {
    assert!(VERIFY_ERROR_CODES.len() == VerifyErrorKind::ALL.len());
    let mut index = 0;
    while index < VERIFY_ERROR_CODES.len() {
        assert!(same(
            VERIFY_ERROR_CODES[index],
            VerifyErrorKind::ALL[index].code()
        ));
        index += 1;
    }
};

/// A run that gave no receipt: the kind, where it sits, a sentence and the kind's own details.
/// Never a panic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifyError {
    kind: VerifyErrorKind,
    subject: String,
    detail: String,
    // Boxed, so the error stays small beside the values a `Result` carries.
    details: Box<Details>,
}

/// What some kinds carry beyond the place and the sentence.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Details {
    path: Option<String>,
    candidates: Vec<String>,
    cause: Option<&'static str>,
    diagnostic_codes: Vec<&'static str>,
    /// A refused frame's trace row and the bits of its time in model seconds.
    frame: Option<(u64, u64)>,
    /// The engine's message about a refused frame, cut to 256 bytes at a character boundary.
    engine_message: Option<String>,
}

impl VerifyError {
    /// A refusal of `kind` at `subject` with a sentence and no further detail.
    pub(crate) fn new(
        kind: VerifyErrorKind,
        subject: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            subject: subject.into(),
            detail: detail.into(),
            details: Box::default(),
        }
    }

    /// The same refusal about the canonical engine path `path`.
    pub(crate) fn with_path(mut self, path: impl Into<String>) -> Self {
        self.details.path = Some(path.into());
        self
    }

    /// The same refusal naming the candidates of an ambiguous name.
    pub(crate) fn with_candidates(mut self, candidates: Vec<String>) -> Self {
        self.details.candidates = candidates;
        self
    }

    /// The same refusal with a cause from one of the crate's closed tables.
    pub(crate) fn with_cause(mut self, cause: &'static str) -> Self {
        self.details.cause = Some(cause);
        self
    }

    /// The same refusal carrying the engine's diagnostic codes.
    pub(crate) fn with_diagnostic_codes(mut self, codes: Vec<&'static str>) -> Self {
        self.details.diagnostic_codes = codes;
        self
    }

    /// The same refusal about the trace row `frame`, at `time` model seconds, with the engine's
    /// `message` about it.
    pub(crate) fn with_frame(mut self, frame: u64, time: f64, message: String) -> Self {
        self.details.frame = Some((frame, time.to_bits()));
        self.details.engine_message = Some(message);
        self
    }

    /// The closed kind.
    #[must_use]
    pub fn kind(&self) -> VerifyErrorKind {
        self.kind
    }

    /// The kind's stable code ([`VerifyErrorKind::code`]).
    #[must_use]
    pub fn code(&self) -> &'static str {
        self.kind.code()
    }

    /// The place the refusal sits on: `subject` for the subject document, or a path into the
    /// set such as `params.persist.delayTime`, `scenarios[2].inputs.T`,
    /// `scenarios[0].inputs.x[1].value`, `scenarios[1].expect[0].output` or `scenarios[3]`.
    /// Indexes count from zero in file order; names are written as the set writes them.
    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }

    /// The sentence for people. Not a contract: branch on [`Self::code`].
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }

    /// The canonical engine path the refusal concerns, when it concerns one: the input of
    /// `duplicate-input` and `input-not-supplied`, the port of `enum-port-unsupported` and
    /// `unsupported-port-type`, the resolved target of a value refusal, the parameter of
    /// `param-path-duplicate` and `param-refused`.
    #[must_use]
    pub fn path(&self) -> Option<&str> {
        self.details.path.as_deref()
    }

    /// The canonical paths an ambiguous name matches, in path order; empty for every other
    /// kind.
    #[must_use]
    pub fn candidates(&self) -> &[String] {
        &self.details.candidates
    }

    /// The cause of `replay-failed` (one of [`REPLAY_FAILURE_CAUSES`]), the class of
    /// `param-refused` (one of [`PARAM_REFUSAL_CLASSES`]), the cause of `mutant-param-refused`
    /// (one of [`MUTANT_PARAM_REFUSAL_CAUSES`]), the cause of `compare-refused` (one of
    /// [`COMPARE_REFUSAL_CAUSES`]), the class of `frame-refused` (one of
    /// [`crate::ENGINE_REFUSAL_CLASSES`]), and the lower layer's code of `spec-refused`,
    /// `source-refused` and `unit-refused`; `None` for every other kind.
    #[must_use]
    pub fn cause(&self) -> Option<&'static str> {
        self.details.cause
    }

    /// The `DiagCode` spellings of a `subject-load-failed` refusal, in the engine's stage order
    /// (`OcError::all_diagnostics`); empty when the refusal carried no structured diagnostic
    /// (malformed JSON, a document past the engine's byte limit) and for every other kind.
    #[must_use]
    pub fn diagnostic_codes(&self) -> &[&'static str] {
        &self.details.diagnostic_codes
    }

    /// The trace row of the frame a `frame-refused` refusal is about; `None` for every other
    /// kind.
    #[must_use]
    pub fn frame_index(&self) -> Option<u64> {
        self.details.frame.map(|(frame, _)| frame)
    }

    /// That frame's time, in model seconds; `None` for every other kind.
    #[must_use]
    pub fn frame_time(&self) -> Option<f64> {
        self.details.frame.map(|(_, bits)| f64::from_bits(bits))
    }

    /// The engine's message about that frame, cut to 256 bytes at a character boundary; `None`
    /// for every other kind. For the three input classes it names the input; the verifier does
    /// not parse it, so a `frame-refused` has no [`Self::path`].
    #[must_use]
    pub fn engine_message(&self) -> Option<&str> {
        self.details.engine_message.as_deref()
    }
}

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.subject, self.detail)
    }
}

impl std::error::Error for VerifyError {}

#[cfg(test)]
#[path = "verify_error_tests.rs"]
mod tests;
