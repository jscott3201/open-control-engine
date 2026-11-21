//! The wire form's refusal codes (GitHub issue #495).
//!
//! Every [`WireError`] has one stable kebab-case [`WireError::code`], so a host names a refusal by
//! asking the error and never by matching its variants: `WireError` is `#[non_exhaustive]`, and a
//! match outside this crate needs a wildcard arm that would give a later variant a generic name.
//! [`WIRE_CODES`] lists every code in declaration order and an in-crate test pins it against an
//! exhaustive match, so a variant cannot be added without a code. [`WireError::location`] is the
//! position a report prints beside the code.
//!
//! Eight codes are spelled as the text form's code for the same defect (`not-utf8`,
//! `byte-order-mark`, `input-too-large`, `nesting-too-deep`, `string-too-long`,
//! `non-finite-number`, `integer-out-of-range`, `control-character`), and `malformed-connection`
//! is also a stage-1 finding code. The lists are separate contracts: a code is unique within
//! [`WIRE_CODES`], not across the crate.

use super::WireError;
use crate::location::Location;

/// Every [`WireError::code`], in declaration order: sixteen codes, one per variant.
pub const WIRE_CODES: &[&str] = &[
    "not-utf8",
    "byte-order-mark",
    "input-too-large",
    "nesting-too-deep",
    "list-too-long",
    "string-too-long",
    "duplicate-key",
    "non-finite-number",
    "integer-out-of-range",
    "control-character",
    "unknown-key",
    "missing-key",
    "wrong-type",
    "json-syntax",
    "bound-list-too-long",
    "malformed-connection",
];

impl WireError {
    /// The stable kebab-case code, for reports (one of [`WIRE_CODES`]).
    ///
    /// The code depends on the variant alone, never on what it carries. A code is not renamed or
    /// reused once published; a new variant brings a new code. Never panics.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotUtf8 { .. } => "not-utf8",
            Self::ByteOrderMark => "byte-order-mark",
            Self::TooLarge { .. } => "input-too-large",
            Self::TooDeep { .. } => "nesting-too-deep",
            Self::ListTooLong { .. } => "list-too-long",
            Self::StringTooLong { .. } => "string-too-long",
            Self::DuplicateKey { .. } => "duplicate-key",
            Self::NonFiniteNumber { .. } => "non-finite-number",
            Self::IntegerOutOfRange { .. } => "integer-out-of-range",
            Self::ControlCharacter { .. } => "control-character",
            Self::UnknownKey { .. } => "unknown-key",
            Self::MissingKey { .. } => "missing-key",
            Self::WrongType { .. } => "wrong-type",
            Self::Syntax { .. } => "json-syntax",
            Self::BoundListTooLong { .. } => "bound-list-too-long",
            Self::MalformedConnection { .. } => "malformed-connection",
        }
    }

    /// Where in the document the refusal sits: the 1-based line and byte column the variant
    /// carries, or `None` for a refusal with no line and column ([`WireError::NotUtf8`], which
    /// carries a byte offset; [`WireError::ByteOrderMark`] and [`WireError::TooLarge`], which
    /// concern the whole document; and [`WireError::MalformedConnection`], which carries the
    /// connection's index). The position is returned as the reader recorded it: a document that
    /// ends where a value is expected is placed at column 0 today (GitHub issue #560). Never
    /// panics.
    #[must_use]
    pub fn location(&self) -> Option<Location> {
        match self {
            Self::NotUtf8 { .. }
            | Self::ByteOrderMark
            | Self::TooLarge { .. }
            | Self::MalformedConnection { .. } => None,
            Self::TooDeep { at, .. }
            | Self::ListTooLong { at, .. }
            | Self::StringTooLong { at, .. }
            | Self::DuplicateKey { at, .. }
            | Self::NonFiniteNumber { at }
            | Self::IntegerOutOfRange { at }
            | Self::ControlCharacter { at }
            | Self::UnknownKey { at, .. }
            | Self::MissingKey { at, .. }
            | Self::WrongType { at, .. }
            | Self::Syntax { at, .. }
            | Self::BoundListTooLong { at } => Some(*at),
        }
    }
}

#[cfg(test)]
#[path = "codes_tests.rs"]
mod tests;
