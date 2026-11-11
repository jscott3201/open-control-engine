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
//! The kinds of the mutant rows (`mutant-missing` and the five after it) arrive with mutant mode,
//! in place.

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
    /// A parameter edit the engine still refused when a pass applied nothing (decision 14);
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
    /// The frames the set needs, `(N + 1) * scenarios`, are more than
    /// [`crate::VerifyLimits::max_total_frames`], or one scenario's `N + 1` more than the replay
    /// limits' `max_frames`.
    TooManyFrames,
    /// The engine refused a frame for a reason about the run rather than the frame
    /// (`EngineRefusal::Run`): later frames would be refused the same way.
    RunRefused,
    /// A replay ended with an error rather than a verdict; [`VerifyError::cause`] is its class
    /// from [`REPLAY_FAILURE_CAUSES`], never a Rust variant name.
    ReplayFailed,
    /// A window held no executed tick after a run in which no frame was refused. Load already
    /// proves every window holds a tick of the clock, and a run that refused no frame executed
    /// every tick, so this is a defence that no loaded set reaches.
    WindowUncovered,
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
        Self::RunRefused,
        Self::ReplayFailed,
        Self::WindowUncovered,
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
            Self::RunRefused => "run-refused",
            Self::ReplayFailed => "replay-failed",
            Self::WindowUncovered => "window-uncovered",
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
    "run-refused",
    "replay-failed",
    "window-uncovered",
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
/// parameter, which needs a reload). Any other refusal of `set_param` is not about the edit's
/// value and is `replay-failed` with the cause `engine-setup`.
pub const PARAM_REFUSAL_CLASSES: &[&str] = &["param-range", "param-type", "param-structural"];

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

    /// The cause of `replay-failed` (one of [`REPLAY_FAILURE_CAUSES`]) or the class of
    /// `param-refused` (one of [`PARAM_REFUSAL_CLASSES`]); `None` for every other kind.
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
