//! Why a vector set was refused at load (vectors note, "Error codes"; decision 18).
//!
//! A [`VectorError`] is one closed [`VectorErrorKind`] with a stable kebab-case
//! [`VectorErrorKind::code`], the place the refusal sits on and a sentence for people. The code is
//! the contract: a test, a tool or an agent branches on it, never on the sentence.
//! [`VECTOR_ERROR_CODES`] lists every code in declaration order and a test pins it.
//!
//! The first fourteen codes are the strict-JSON refusals, spelled as the authoring wire form's
//! are, so one word names one refusal across the tools.

use std::fmt;

/// A 1-based line and column (in bytes) in a vector document. A document within the byte bound
/// always fits in `u32`; a larger count saturates.
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

/// What a load refused. Each kind has one stable code ([`VectorErrorKind::code`]);
/// `#[non_exhaustive]`, so consumers branch on the code.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VectorErrorKind {
    /// The bytes are not UTF-8.
    NotUtf8,
    /// The document starts with a byte-order mark.
    ByteOrderMark,
    /// The text is not one JSON document.
    JsonSyntax,
    /// The document is larger than [`crate::VerifyLimits::max_bytes`].
    InputTooLarge,
    /// Objects and arrays nest deeper than [`crate::MAX_DEPTH`].
    NestingTooDeep,
    /// A list has more than [`crate::MAX_LIST_LEN`] entries (a step list included).
    ListTooLong,
    /// A string is longer than [`crate::MAX_STRING_BYTES`] as written.
    StringTooLong,
    /// An object writes one key twice.
    DuplicateKey,
    /// An object carries a key the format does not define. `within`, `of` and `when` are such
    /// keys: refused, never ignored.
    UnknownKey,
    /// An object lacks a required key.
    MissingKey,
    /// A value has the wrong JSON type for its position.
    WrongType,
    /// `NaN` or `Infinity` spelled as a number, or a real that overflows binary64.
    NonFiniteNumber,
    /// An integer token (no fraction or exponent) outside the 64-bit signed range, or, where the
    /// format reads a time, outside the range in which every integer is a binary64
    /// ([`crate::MAX_TIME_INTEGER`]).
    IntegerOutOfRange,
    /// A string carries a control character (anything in C0 but line feed, DEL or C1), raw or
    /// escaped.
    ControlCharacter,
    /// `schema` is a string other than [`crate::VECTORS_SCHEMA`].
    UnknownSchema,
    /// `step_s` is not positive, or `horizon_s` is negative.
    InvalidClock,
    /// No integer `k` gives `k * step_s == horizon_s`.
    HorizonOffCadence,
    /// The clock has more ticks than [`crate::VerifyLimits::max_ticks`].
    TooManyTicks,
    /// More scenarios than [`crate::VerifyLimits::max_scenarios`].
    TooManyScenarios,
    /// A scenario has more expectations than [`crate::VerifyLimits::max_expectations`].
    TooManyExpectations,
    /// More mutants than [`crate::VerifyLimits::max_mutants`].
    TooManyMutants,
    /// A `params` map has more entries than [`crate::VerifyLimits::max_parameters`].
    TooManyParameters,
    /// Two scenarios share a name.
    DuplicateScenario,
    /// A scenario name, input name, parameter path, output name or diagnostic block is empty.
    EmptyName,
    /// A step list has no step.
    StepListEmpty,
    /// A step list's first `t` is not `0`.
    StepListNotFromZero,
    /// A step's `t` is not greater than the one before it.
    StepTimesNotIncreasing,
    /// A step's `t` is after `horizon_s`.
    StepAfterHorizon,
    /// A window's `from_s` is after its `to_s`.
    InvalidWindow,
    /// A window starts before `0` or ends after `horizon_s`.
    WindowOutsideClock,
    /// A window holds no tick of the clock.
    WindowUncovered,
    /// An expectation is not exactly one of the window form and the instant form.
    AmbiguousExpectation,
    /// A diagnostic `at_s` is not a tick of the clock.
    InstantOffCadence,
    /// A mutant path that is absolute, has an empty, `.` or `..` segment, or holds a backslash:
    /// a path that could leave the vector file's directory, or name one file two ways.
    MutantPathInvalid,
    /// Two mutants share a path.
    DuplicateMutant,
}

impl VectorErrorKind {
    /// Every kind, in declaration order.
    const ALL: &'static [Self] = &[
        Self::NotUtf8,
        Self::ByteOrderMark,
        Self::JsonSyntax,
        Self::InputTooLarge,
        Self::NestingTooDeep,
        Self::ListTooLong,
        Self::StringTooLong,
        Self::DuplicateKey,
        Self::UnknownKey,
        Self::MissingKey,
        Self::WrongType,
        Self::NonFiniteNumber,
        Self::IntegerOutOfRange,
        Self::ControlCharacter,
        Self::UnknownSchema,
        Self::InvalidClock,
        Self::HorizonOffCadence,
        Self::TooManyTicks,
        Self::TooManyScenarios,
        Self::TooManyExpectations,
        Self::TooManyMutants,
        Self::TooManyParameters,
        Self::DuplicateScenario,
        Self::EmptyName,
        Self::StepListEmpty,
        Self::StepListNotFromZero,
        Self::StepTimesNotIncreasing,
        Self::StepAfterHorizon,
        Self::InvalidWindow,
        Self::WindowOutsideClock,
        Self::WindowUncovered,
        Self::AmbiguousExpectation,
        Self::InstantOffCadence,
        Self::MutantPathInvalid,
        Self::DuplicateMutant,
    ];

    /// The stable kebab-case code of this kind: the contract a caller branches on.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::NotUtf8 => "not-utf8",
            Self::ByteOrderMark => "byte-order-mark",
            Self::JsonSyntax => "json-syntax",
            Self::InputTooLarge => "input-too-large",
            Self::NestingTooDeep => "nesting-too-deep",
            Self::ListTooLong => "list-too-long",
            Self::StringTooLong => "string-too-long",
            Self::DuplicateKey => "duplicate-key",
            Self::UnknownKey => "unknown-key",
            Self::MissingKey => "missing-key",
            Self::WrongType => "wrong-type",
            Self::NonFiniteNumber => "non-finite-number",
            Self::IntegerOutOfRange => "integer-out-of-range",
            Self::ControlCharacter => "control-character",
            Self::UnknownSchema => "unknown-schema",
            Self::InvalidClock => "invalid-clock",
            Self::HorizonOffCadence => "horizon-off-cadence",
            Self::TooManyTicks => "too-many-ticks",
            Self::TooManyScenarios => "too-many-scenarios",
            Self::TooManyExpectations => "too-many-expectations",
            Self::TooManyMutants => "too-many-mutants",
            Self::TooManyParameters => "too-many-parameters",
            Self::DuplicateScenario => "duplicate-scenario",
            Self::EmptyName => "empty-name",
            Self::StepListEmpty => "step-list-empty",
            Self::StepListNotFromZero => "step-list-not-from-zero",
            Self::StepTimesNotIncreasing => "step-times-not-increasing",
            Self::StepAfterHorizon => "step-after-horizon",
            Self::InvalidWindow => "invalid-window",
            Self::WindowOutsideClock => "window-outside-clock",
            Self::WindowUncovered => "window-uncovered",
            Self::AmbiguousExpectation => "ambiguous-expectation",
            Self::InstantOffCadence => "instant-off-cadence",
            Self::MutantPathInvalid => "mutant-path-invalid",
            Self::DuplicateMutant => "duplicate-mutant",
        }
    }
}

/// Every [`VectorErrorKind`] code, in declaration order.
pub const VECTOR_ERROR_CODES: &[&str] = &[
    "not-utf8",
    "byte-order-mark",
    "json-syntax",
    "input-too-large",
    "nesting-too-deep",
    "list-too-long",
    "string-too-long",
    "duplicate-key",
    "unknown-key",
    "missing-key",
    "wrong-type",
    "non-finite-number",
    "integer-out-of-range",
    "control-character",
    "unknown-schema",
    "invalid-clock",
    "horizon-off-cadence",
    "too-many-ticks",
    "too-many-scenarios",
    "too-many-expectations",
    "too-many-mutants",
    "too-many-parameters",
    "duplicate-scenario",
    "empty-name",
    "step-list-empty",
    "step-list-not-from-zero",
    "step-times-not-increasing",
    "step-after-horizon",
    "invalid-window",
    "window-outside-clock",
    "window-uncovered",
    "ambiguous-expectation",
    "instant-off-cadence",
    "mutant-path-invalid",
    "duplicate-mutant",
];

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

// The list and the kinds cannot drift apart: a build fails unless `VECTOR_ERROR_CODES` is the
// code of every kind in `VectorErrorKind::ALL`, in order.
const _: () = {
    assert!(VECTOR_ERROR_CODES.len() == VectorErrorKind::ALL.len());
    let mut index = 0;
    while index < VECTOR_ERROR_CODES.len() {
        assert!(same(
            VECTOR_ERROR_CODES[index],
            VectorErrorKind::ALL[index].code()
        ));
        index += 1;
    }
};

/// A refused vector set: the kind, where it sits and a sentence. Never a panic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VectorError {
    kind: VectorErrorKind,
    at: Option<Location>,
    subject: String,
    detail: String,
}

impl VectorError {
    /// A refusal found while reading the text, at a line and column.
    pub(crate) fn in_text(kind: VectorErrorKind, at: Location, detail: impl Into<String>) -> Self {
        Self {
            kind,
            at: Some(at),
            subject: at.to_string(),
            detail: detail.into(),
        }
    }

    /// A refusal of the bytes as a whole, or of a value the format places by path.
    pub(crate) fn on(
        kind: VectorErrorKind,
        subject: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            at: None,
            subject: subject.into(),
            detail: detail.into(),
        }
    }

    /// The closed kind.
    #[must_use]
    pub fn kind(&self) -> VectorErrorKind {
        self.kind
    }

    /// The kind's stable code ([`VectorErrorKind::code`]).
    #[must_use]
    pub fn code(&self) -> &'static str {
        self.kind.code()
    }

    /// Where in the text the reader stopped, for the refusals found while reading it (the
    /// strict-JSON and shape codes, the count bounds and `ambiguous-expectation`); `None` for a
    /// refusal of the bytes as a whole and for one placed by path.
    #[must_use]
    pub fn at(&self) -> Option<Location> {
        self.at
    }

    /// The place the refusal sits on: `document`, `line 3 column 14`, or a path into the set
    /// such as `clock.horizon_s`, `scenarios[2].inputs.clg_vlv_cmd[1].t` or
    /// `scenarios[0].expect[1]`. Indexes count from zero in file order.
    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }

    /// The sentence for people. Not a contract: branch on [`Self::code`].
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for VectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.subject, self.detail)
    }
}

impl std::error::Error for VectorError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// The position of `kind` in the declaration. The match is exhaustive, so a new variant does
    /// not compile until it has a row here, in `ALL` and in `VECTOR_ERROR_CODES`.
    fn declared_index(kind: VectorErrorKind) -> usize {
        use VectorErrorKind as K;
        match kind {
            K::NotUtf8 => 0,
            K::ByteOrderMark => 1,
            K::JsonSyntax => 2,
            K::InputTooLarge => 3,
            K::NestingTooDeep => 4,
            K::ListTooLong => 5,
            K::StringTooLong => 6,
            K::DuplicateKey => 7,
            K::UnknownKey => 8,
            K::MissingKey => 9,
            K::WrongType => 10,
            K::NonFiniteNumber => 11,
            K::IntegerOutOfRange => 12,
            K::ControlCharacter => 13,
            K::UnknownSchema => 14,
            K::InvalidClock => 15,
            K::HorizonOffCadence => 16,
            K::TooManyTicks => 17,
            K::TooManyScenarios => 18,
            K::TooManyExpectations => 19,
            K::TooManyMutants => 20,
            K::TooManyParameters => 21,
            K::DuplicateScenario => 22,
            K::EmptyName => 23,
            K::StepListEmpty => 24,
            K::StepListNotFromZero => 25,
            K::StepTimesNotIncreasing => 26,
            K::StepAfterHorizon => 27,
            K::InvalidWindow => 28,
            K::WindowOutsideClock => 29,
            K::WindowUncovered => 30,
            K::AmbiguousExpectation => 31,
            K::InstantOffCadence => 32,
            K::MutantPathInvalid => 33,
            K::DuplicateMutant => 34,
        }
    }

    #[test]
    fn the_code_list_is_every_kind_in_declaration_order() {
        assert_eq!(VectorErrorKind::ALL.len(), 35);
        for (index, kind) in VectorErrorKind::ALL.iter().enumerate() {
            assert_eq!(declared_index(*kind), index, "{kind:?}");
        }
        let codes: Vec<&str> = VectorErrorKind::ALL.iter().map(|k| k.code()).collect();
        assert_eq!(codes, VECTOR_ERROR_CODES);
    }

    #[test]
    fn codes_are_unique_lowercase_kebab_case() {
        let mut seen = std::collections::BTreeSet::new();
        for code in VECTOR_ERROR_CODES {
            assert!(seen.insert(*code), "{code} twice");
            assert!(
                code.bytes()
                    .all(|b| b.is_ascii_lowercase() || b == b'-' || b.is_ascii_digit()),
                "{code}"
            );
            assert!(!code.starts_with('-') && !code.ends_with('-'), "{code}");
        }
    }

    #[test]
    fn a_text_refusal_carries_its_location_and_a_path_refusal_none() {
        let text = VectorError::in_text(
            VectorErrorKind::UnknownKey,
            Location::new(3, 14),
            "unknown key `when`",
        );
        assert_eq!(text.at(), Some(Location::new(3, 14)));
        assert_eq!(text.subject(), "line 3 column 14");
        assert_eq!(text.to_string(), "line 3 column 14: unknown key `when`");
        assert_eq!(text.code(), "unknown-key");
        let path = VectorError::on(
            VectorErrorKind::InvalidClock,
            "clock.step_s",
            "not positive",
        );
        assert_eq!(path.at(), None);
        assert_eq!(path.kind(), VectorErrorKind::InvalidClock);
        assert_eq!(path.detail(), "not positive");
    }

    #[test]
    fn a_location_saturates_instead_of_wrapping() {
        let far = Location::new(usize::MAX, usize::MAX);
        assert_eq!((far.line, far.column), (u32::MAX, u32::MAX));
    }
}
