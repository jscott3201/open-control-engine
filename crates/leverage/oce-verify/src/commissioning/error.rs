//! Why a commissioning spec or a point mapping was refused (commissioning note, "Rust API
//! sketch": the codes of a commissioning run, first row).
//!
//! A [`SpecError`] is one closed [`SpecErrorKind`] with a stable kebab-case
//! [`SpecErrorKind::code`], the place the refusal sits on and a sentence for people. The code is
//! the contract: a test, a tool or an agent branches on it, never on the sentence.
//! [`SPEC_ERROR_CODES`] lists every code in declaration order and a test pins it.
//!
//! The first fourteen codes are the strict-JSON refusals, spelled as the vector reader and the
//! authoring wire form spell them, and `unknown-schema`, `too-many-parameters`, `empty-name` and
//! `invalid-window` are the vector reader's words for the same defects, so one word names one
//! refusal across the tools. `invalid-tolerance` and `work-limit-exceeded` are the envelope
//! comparison's words for a tolerance below zero and for a work counter that ran out.
//!
//! Every sentence and every subject is bounded and printable: [`MAX_SPEC_DETAIL_BYTES`] bytes at
//! most, ASCII only, a line feed written `\n` and any other character outside printable ASCII
//! as `\u{…}`. A refused document cannot put a megabyte, a control character or a terminal
//! escape into a log through its own refusal.

use std::fmt;

use crate::error::Location;
use crate::strict::{Refusal, StrictKind};

/// What a load, or a key selection at bind, refused. Each kind has one stable code
/// ([`SpecErrorKind::code`]); `#[non_exhaustive]`, so consumers branch on the code.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpecErrorKind {
    /// The bytes are not UTF-8.
    NotUtf8,
    /// The document starts with a byte-order mark.
    ByteOrderMark,
    /// The text, with its comments removed, is not one JSON document. A trailing comma is one
    /// such defect, and so is a `/` that starts no comment.
    JsonSyntax,
    /// The document is larger than [`crate::SpecLimits::max_bytes`].
    InputTooLarge,
    /// Objects and arrays nest deeper than [`crate::MAX_DEPTH`].
    NestingTooDeep,
    /// A list has more than [`crate::MAX_LIST_LEN`] entries.
    ListTooLong,
    /// A string is longer than [`crate::MAX_STRING_BYTES`] as written.
    StringTooLong,
    /// An object writes one key twice.
    DuplicateKey,
    /// An object carries a key the format does not define: refused, never ignored. The four
    /// fields of OBC §12.7 Scenario 1 that the verifier does not act on are defined, and are
    /// findings instead ([`crate::LBL_FIELD_IGNORED`]).
    UnknownKey,
    /// An object lacks a required key.
    MissingKey,
    /// A value has the wrong JSON type for its position.
    WrongType,
    /// `NaN` or `Infinity` spelled as a number, or a real that overflows binary64.
    NonFiniteNumber,
    /// An integer token (no fraction or exponent) outside the 64-bit signed range; or, where the
    /// format reads a real, outside the range in which every integer is a binary64
    /// ([`crate::MAX_TIME_INTEGER`]); or a column index that is not from 1 to 4,294,967,295.
    IntegerOutOfRange,
    /// A string carries a control character, raw or escaped: anything in C0, DEL or C1. The one
    /// exception is a line feed written as the escape `\n`; a raw line feed inside a string is
    /// refused like the rest.
    ControlCharacter,
    /// A `/*` comment that is never closed.
    UnterminatedComment,
    /// `schema` is a string other than [`crate::COMMISSIONING_SPEC_SCHEMA`].
    UnknownSchema,
    /// A reference's `parameters` map has more entries than
    /// [`crate::SpecLimits::max_parameters`].
    TooManyParameters,
    /// An `outputs`, `indicators` or `columns` map has more keys than
    /// [`crate::SpecLimits::max_keys`].
    TooManyKeys,
    /// A name is empty: a `model`, a `sequence`, an `outputs` or `indicators` key, an indicator
    /// name, a parameter path, a point's name or its unit, a `table`, a named `timeColumn`, or
    /// a key or a value of a `columns` map.
    EmptyName,
    /// A string that must be one of a closed list is not: a source's `format`, an `envelope`, a
    /// `funnelReference` or a point's `type`.
    UnknownValue,
    /// A path that is absolute, has an empty, `.` or `..` segment, or holds a backslash: a path
    /// that could leave the spec's directory, or name one file two ways.
    PathInvalid,
    /// `time.origin` is not an RFC 3339 instant with an explicit `Z` or `±hh:mm` offset.
    InvalidTimeOrigin,
    /// A tolerance below zero.
    InvalidTolerance,
    /// A `sampling` that is not above zero.
    InvalidSampling,
    /// A window whose `start` is after its `stop`.
    InvalidWindow,
    /// A `hold.maxAge` below zero.
    InvalidHold,
    /// Two points of one mapping share a name on the same side, `cdl` or `device`.
    DuplicatePoint,
    /// An `outputs` or `indicators` key that selects no output of the sequence: it lacks the
    /// reference's prefix, it is a glob that matches nothing, or it is an exact name that names
    /// nothing. Found when the keys are matched against a sequence's outputs, at bind, never by
    /// a load.
    UnmatchedKey,
    /// An exact `outputs` or `indicators` key whose rest names more than one output of the
    /// sequence by the replay name rule. Found at bind, never by a load; the vector bind's word
    /// for the same ambiguity.
    AmbiguousOutput,
    /// Two `outputs` keys of one kind (two globs, or two exact names) select one output and
    /// give one tolerance field different values, so neither takes precedence. Found at bind,
    /// never by a load.
    ConflictingOverride,
    /// Matching a reference's keys against a sequence's outputs needs more than
    /// [`crate::SpecLimits::max_key_match_work`] units. Found at bind, never by a load.
    WorkLimitExceeded,
}

impl SpecErrorKind {
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
        Self::UnterminatedComment,
        Self::UnknownSchema,
        Self::TooManyParameters,
        Self::TooManyKeys,
        Self::EmptyName,
        Self::UnknownValue,
        Self::PathInvalid,
        Self::InvalidTimeOrigin,
        Self::InvalidTolerance,
        Self::InvalidSampling,
        Self::InvalidWindow,
        Self::InvalidHold,
        Self::DuplicatePoint,
        Self::UnmatchedKey,
        Self::AmbiguousOutput,
        Self::ConflictingOverride,
        Self::WorkLimitExceeded,
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
            Self::UnterminatedComment => "unterminated-comment",
            Self::UnknownSchema => "unknown-schema",
            Self::TooManyParameters => "too-many-parameters",
            Self::TooManyKeys => "too-many-keys",
            Self::EmptyName => "empty-name",
            Self::UnknownValue => "unknown-value",
            Self::PathInvalid => "path-invalid",
            Self::InvalidTimeOrigin => "invalid-time-origin",
            Self::InvalidTolerance => "invalid-tolerance",
            Self::InvalidSampling => "invalid-sampling",
            Self::InvalidWindow => "invalid-window",
            Self::InvalidHold => "invalid-hold",
            Self::DuplicatePoint => "duplicate-point",
            Self::UnmatchedKey => "unmatched-key",
            Self::AmbiguousOutput => "ambiguous-output",
            Self::ConflictingOverride => "conflicting-override",
            Self::WorkLimitExceeded => "work-limit-exceeded",
        }
    }
}

impl StrictKind for SpecErrorKind {
    const JSON_SYNTAX: Self = Self::JsonSyntax;
    const NESTING_TOO_DEEP: Self = Self::NestingTooDeep;
    const LIST_TOO_LONG: Self = Self::ListTooLong;
    const STRING_TOO_LONG: Self = Self::StringTooLong;
    const DUPLICATE_KEY: Self = Self::DuplicateKey;
    const WRONG_TYPE: Self = Self::WrongType;
    const NON_FINITE_NUMBER: Self = Self::NonFiniteNumber;
    const INTEGER_OUT_OF_RANGE: Self = Self::IntegerOutOfRange;
    const CONTROL_CHARACTER: Self = Self::ControlCharacter;
}

/// Every [`SpecErrorKind`] code, in declaration order.
pub const SPEC_ERROR_CODES: &[&str] = &[
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
    "unterminated-comment",
    "unknown-schema",
    "too-many-parameters",
    "too-many-keys",
    "empty-name",
    "unknown-value",
    "path-invalid",
    "invalid-time-origin",
    "invalid-tolerance",
    "invalid-sampling",
    "invalid-window",
    "invalid-hold",
    "duplicate-point",
    "unmatched-key",
    "ambiguous-output",
    "conflicting-override",
    "work-limit-exceeded",
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

// The list and the kinds cannot drift apart: a build fails unless `SPEC_ERROR_CODES` is the code
// of every kind in `SpecErrorKind::ALL`, in order.
const _: () = {
    assert!(SPEC_ERROR_CODES.len() == SpecErrorKind::ALL.len());
    let mut index = 0;
    while index < SPEC_ERROR_CODES.len() {
        assert!(same(
            SPEC_ERROR_CODES[index],
            SpecErrorKind::ALL[index].code()
        ));
        index += 1;
    }
};

/// The most bytes a [`SpecError`]'s sentence, and its subject, may have.
pub const MAX_SPEC_DETAIL_BYTES: usize = 512;

/// The most bytes of one name or value quoted inside a sentence ([`shown`]).
const MAX_SHOWN_BYTES: usize = 96;

/// Append `character` to `out` as printable ASCII: itself, `\n` for a line feed, or `\u{…}`.
fn push_printable(out: &mut String, character: char) {
    match character {
        ' '..='~' => out.push(character),
        '\n' => out.push_str("\\n"),
        other => out.push_str(&format!("\\u{{{:x}}}", u32::from(other))),
    }
}

/// `text` as printable ASCII, at most `limit` bytes: a text that fits is written whole, and a
/// longer one is cut between two characters and ends in `...`. Never holds more than `limit`
/// bytes and one character's spelling while it is built.
pub(crate) fn printable(text: &str, limit: usize) -> String {
    let mut out = String::new();
    // The longest prefix written so far that leaves room for the three bytes of the mark.
    let mut room_for_mark = 0;
    for character in text.chars() {
        push_printable(&mut out, character);
        if out.len() + 3 <= limit {
            room_for_mark = out.len();
        }
        if out.len() > limit {
            out.truncate(room_for_mark);
            out.push_str("...");
            return out;
        }
    }
    out
}

/// A name or a value a document wrote, quoted for a sentence: in backticks, printable ASCII
/// only, and cut to a bounded length with `...` when it is longer.
pub(crate) fn shown(text: &str) -> String {
    format!("`{}`", printable(text, MAX_SHOWN_BYTES))
}

/// A refused commissioning spec or point mapping: the kind, where it sits and a sentence. Never
/// a panic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpecError {
    kind: SpecErrorKind,
    at: Option<Location>,
    subject: String,
    detail: String,
}

impl SpecError {
    /// A refusal found while reading the text, at a line and column of the text as written,
    /// comments included.
    pub(crate) fn in_text(kind: SpecErrorKind, at: Location, detail: &str) -> Self {
        Self {
            kind,
            at: Some(at),
            subject: at.to_string(),
            detail: printable(detail, MAX_SPEC_DETAIL_BYTES),
        }
    }

    /// A refusal of the bytes as a whole, or of a value placed by name.
    pub(crate) fn on(kind: SpecErrorKind, subject: &str, detail: &str) -> Self {
        Self {
            kind,
            at: None,
            subject: printable(subject, MAX_SPEC_DETAIL_BYTES),
            detail: printable(detail, MAX_SPEC_DETAIL_BYTES),
        }
    }

    /// The closed kind.
    #[must_use]
    pub fn kind(&self) -> SpecErrorKind {
        self.kind
    }

    /// The kind's stable code ([`SpecErrorKind::code`]).
    #[must_use]
    pub fn code(&self) -> &'static str {
        self.kind.code()
    }

    /// Where in the text the reader stopped: a 1-based line and byte column of the document as
    /// written, its comments included. `None` for a refusal of the bytes as a whole (`not-utf8`,
    /// `byte-order-mark`, `input-too-large`) and for the four found at bind (`unmatched-key`,
    /// `ambiguous-output`, `conflicting-override`, `work-limit-exceeded`).
    #[must_use]
    pub fn at(&self) -> Option<Location> {
        self.at
    }

    /// The place the refusal sits on: `line 3 column 14`, `document`, or the map a key belongs
    /// to (`outputs`, `indicators`). At most [`MAX_SPEC_DETAIL_BYTES`] bytes of printable ASCII.
    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }

    /// The sentence for people: at most [`MAX_SPEC_DETAIL_BYTES`] bytes of printable ASCII. Not a
    /// contract: branch on [`Self::code`].
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl From<Refusal<SpecErrorKind>> for SpecError {
    fn from(refusal: Refusal<SpecErrorKind>) -> Self {
        Self::in_text(refusal.kind, refusal.at, &refusal.detail)
    }
}

impl fmt::Display for SpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.subject, self.detail)
    }
}

impl std::error::Error for SpecError {}

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
