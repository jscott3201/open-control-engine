//! The text form (authoring note, "Text form"; decisions 1, 3, 6, 12, 13 and 15): a terse line
//! grammar with a hand-written recursive-descent parser and a canonical printer.
//!
//! [`parse_text`] reads a document into the model and refuses anything malformed with one typed
//! [`ParseError`] per construct (its `code` is one of [`PARSE_CODES`]), bounded at parse and never
//! panicking. [`print_text`] writes the model canonically, and [`fmt_text`] is parse then print,
//! idempotent. [`parse_text_with_spans`] also returns a [`SourceMap`], which places validator
//! findings on the text ([`validate_text`]).
//!
//! What the model cannot tell apart is decided once, here: a semantic judgement (a duplicate
//! name, an unknown class or parameter, a wrong kind) is the validator's, never the parser's; a
//! sequence parameter or boundary keeps its section, not its line; and a comment travels with the
//! statement that follows it.

mod codes;
mod lexer;
mod parser;
mod printer;
mod spans;
mod units;

use std::fmt;

use crate::location::Location;
use crate::model::Sequence;
use crate::validate::{Finding, validate, validate_author_side};

pub use codes::*;
pub use printer::MAX_ONE_LINE_COLUMNS;
pub use spans::SourceMap;
pub use units::BASE_UNIT_SYMBOLS;

/// Why a text document was refused: one exact `code` from [`PARSE_CODES`], where, and what.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// The exact refusal code (`unterminated-string`, …).
    pub code: &'static str,
    /// Where the refused construct starts (1-based line and byte column).
    pub location: Location,
    /// What is wrong, in the author's terms.
    pub detail: String,
}

impl ParseError {
    pub(crate) fn new(code: &'static str, location: Location, detail: impl Into<String>) -> Self {
        Self {
            code,
            location,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {}: {}", self.code, self.location, self.detail)
    }
}

impl std::error::Error for ParseError {}

/// Parse a text document into the model.
///
/// # Errors
///
/// The first malformed construct or exceeded bound, as a [`ParseError`].
pub fn parse_text(text: &str) -> Result<Sequence, ParseError> {
    parse_text_with_spans(text).map(|(sequence, _)| sequence)
}

/// Parse raw bytes: refuse non-UTF-8 ([`NOT_UTF8`], at the first bad byte), then [`parse_text`].
///
/// # Errors
///
/// As [`parse_text`], plus [`NOT_UTF8`].
pub fn parse_text_bytes(bytes: &[u8]) -> Result<Sequence, ParseError> {
    match std::str::from_utf8(bytes) {
        Ok(text) => parse_text(text),
        Err(error) => {
            let valid = &bytes[..error.valid_up_to()];
            let line = valid.iter().filter(|&&b| b == b'\n').count() + 1;
            let line_start = valid
                .iter()
                .rposition(|&b| b == b'\n')
                .map_or(0, |at| at + 1);
            Err(ParseError::new(
                NOT_UTF8,
                Location::new(line, valid.len() - line_start + 1),
                format!("byte {} is not UTF-8", error.valid_up_to()),
            ))
        }
    }
}

/// Parse a text document and keep where each statement, assignment and connection was written.
///
/// # Errors
///
/// As [`parse_text`].
pub fn parse_text_with_spans(text: &str) -> Result<(Sequence, SourceMap), ParseError> {
    lexer::prescan(text)?;
    parser::Parser::new(text).run()
}

/// Write the model in the canonical text spelling: the header comments, `sequence`, `extends`,
/// then parameters, inputs, outputs, blocks and connections in that section order with authored
/// order kept inside each section (decision 12), Reals in the shortest round-trip spelling,
/// Integers bare, suffixes re-emitted, comments and blank lines from the annotations, two-space
/// indentation and one trailing line feed. Infallible; for every model [`parse_text`] returns,
/// `parse_text(&print_text(m)) == m`.
#[must_use]
pub fn print_text(sequence: &Sequence) -> String {
    printer::print(sequence)
}

/// Parse then print: the canonical spelling of a text document. Idempotent:
/// `fmt_text(&fmt_text(t)?) == fmt_text(t)`.
///
/// # Errors
///
/// As [`parse_text`].
pub fn fmt_text(text: &str) -> Result<String, ParseError> {
    parse_text(text).map(|sequence| print_text(&sequence))
}

/// Parse, then both validator stages ([`crate::validate()`]), with every finding's
/// [`Finding::span`] set from the text.
///
/// # Errors
///
/// As [`parse_text`]; a document that parses always yields its findings.
pub fn validate_text(text: &str) -> Result<Vec<Finding>, ParseError> {
    let (sequence, spans) = parse_text_with_spans(text)?;
    let mut findings = validate(&sequence);
    spans.apply(&sequence, &mut findings);
    Ok(findings)
}

/// Parse, then stage 1 only ([`crate::validate_author_side`]), with spans set as
/// [`validate_text`] does.
///
/// # Errors
///
/// As [`parse_text`].
pub fn validate_text_author_side(text: &str) -> Result<Vec<Finding>, ParseError> {
    let (sequence, spans) = parse_text_with_spans(text)?;
    let mut findings = validate_author_side(&sequence);
    spans.apply(&sequence, &mut findings);
    Ok(findings)
}
