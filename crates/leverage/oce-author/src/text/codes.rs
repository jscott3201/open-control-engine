//! The text form's refusal codes and parse bounds (authoring note, "Text form", "Bounds").
//!
//! Every refusal is one [`crate::ParseError`] with one of these exact codes; a malformed construct
//! has exactly one code. Each bound is accepted at its limit and refused one past it.

/// The input is not UTF-8 ([`crate::parse_text_bytes`] only).
pub const NOT_UTF8: &str = "not-utf8";
/// The input starts with a byte-order mark.
pub const BYTE_ORDER_MARK: &str = "byte-order-mark";
/// A control character: C0 other than line feed (tab and carriage return included), DEL, C1, or a
/// bidirectional override or isolate (U+202A to U+202E, U+2066 to U+2069), anywhere.
pub const CONTROL_CHARACTER: &str = "control-character";
/// The input is larger than [`MAX_TEXT_BYTES`].
pub const INPUT_TOO_LARGE: &str = "input-too-large";
/// The input has more than [`MAX_TEXT_LINES`] lines.
pub const TOO_MANY_LINES: &str = "too-many-lines";
/// A line is longer than [`MAX_LINE_BYTES`].
pub const LINE_TOO_LONG: &str = "line-too-long";
/// A name, number, suffix, atom or provenance id is longer than [`MAX_TOKEN_BYTES`].
pub const TOKEN_TOO_LONG: &str = "token-too-long";
/// A string is longer than [`MAX_STRING_BYTES`] as written.
pub const STRING_TOO_LONG: &str = "string-too-long";
/// A comment is longer than [`MAX_COMMENT_BYTES`] after its `#`.
pub const COMMENT_TOO_LONG: &str = "comment-too-long";
/// Lists nest deeper than [`MAX_LIST_DEPTH`].
pub const NESTING_TOO_DEEP: &str = "nesting-too-deep";
/// More than [`MAX_STATEMENTS_PER_KIND`] statements of one kind, or assignments in one block.
pub const TOO_MANY_STATEMENTS: &str = "too-many-statements";
/// A string has no closing quote on its line.
pub const UNTERMINATED_STRING: &str = "unterminated-string";
/// A backslash escape other than `\"`, `\\` or `\n`.
pub const BAD_ESCAPE: &str = "bad-escape";
/// A number token that is not `-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?`: a leading `+`, a
/// leading zero, `1.`, `.5`, `1e`, a letter glued to the digits.
pub const MALFORMED_NUMBER: &str = "malformed-number";
/// An integer token outside the `i64` range.
pub const INTEGER_OUT_OF_RANGE: &str = "integer-out-of-range";
/// `NaN`, `inf` or `Infinity` (any case, any sign) written as a value, or a real that overflows
/// binary64.
pub const NON_FINITE_NUMBER: &str = "non-finite-number";
/// A unit suffix that is not a unit expression (`m^2`, `K/`, `(kg)`).
pub const MALFORMED_SUFFIX: &str = "malformed-suffix";
/// A well-formed unit suffix that is not a base unit (`h`, `degC`, `kPa`; decision 13).
pub const UNIT_NOT_BASE: &str = "unit-not-base";
/// A tag's atom or the `extends` provenance id holds a byte other than printable ASCII (`!` to
/// `~`). An atom stops at `#`; a provenance id may contain one (decision 5's grammar).
pub const MALFORMED_ATOM: &str = "malformed-atom";
/// A token where the grammar allows none, or a missing one (`expected ... found ...`).
pub const STRAY_TOKEN: &str = "stray-token";
/// A line starts with a word that is not a statement keyword and is not a connection.
pub const UNKNOWN_KEYWORD: &str = "unknown-keyword";
/// A boundary tag other than the twelve the grammar names.
pub const UNKNOWN_TAG: &str = "unknown-tag";
/// A boundary tag other than `@haystack` written twice on one statement.
pub const DUPLICATE_TAG: &str = "duplicate-tag";
/// A boundary type other than `Real`, `Integer` or `Boolean` (a sequence parameter's type may
/// also be an enumeration type name, which the validator resolves).
pub const UNKNOWN_TYPE: &str = "unknown-type";
/// The file does not start with a `sequence` statement.
pub const MISSING_SEQUENCE: &str = "missing-sequence";
/// The `sequence` statement is not followed by an `extends` statement (`extends none` is
/// explicit; decision 5).
pub const MISSING_EXTENDS: &str = "missing-extends";
/// A second `sequence` or `extends` statement.
pub const MISPLACED_STATEMENT: &str = "misplaced-statement";
/// A value the position cannot take: a reference or a list as a sequence parameter's value.
pub const MISPLACED_VALUE: &str = "misplaced-value";
/// A list that is empty, has an empty row, mixes scalars and rows, or has rows of different
/// lengths.
pub const MALFORMED_LIST: &str = "malformed-list";
/// A `(` or `[` that is never closed.
pub const UNCLOSED_BRACKET: &str = "unclosed-bracket";
/// A comment after the last item of a block's `(...)` or a list's `[...]`: it precedes nothing
/// the model can attach it to.
pub const COMMENT_BEFORE_CLOSE: &str = "comment-before-close";

/// Every parse refusal code, in the order above.
pub const PARSE_CODES: &[&str] = &[
    NOT_UTF8,
    BYTE_ORDER_MARK,
    CONTROL_CHARACTER,
    INPUT_TOO_LARGE,
    TOO_MANY_LINES,
    LINE_TOO_LONG,
    TOKEN_TOO_LONG,
    STRING_TOO_LONG,
    COMMENT_TOO_LONG,
    NESTING_TOO_DEEP,
    TOO_MANY_STATEMENTS,
    UNTERMINATED_STRING,
    BAD_ESCAPE,
    MALFORMED_NUMBER,
    INTEGER_OUT_OF_RANGE,
    NON_FINITE_NUMBER,
    MALFORMED_SUFFIX,
    UNIT_NOT_BASE,
    MALFORMED_ATOM,
    STRAY_TOKEN,
    UNKNOWN_KEYWORD,
    UNKNOWN_TAG,
    DUPLICATE_TAG,
    UNKNOWN_TYPE,
    MISSING_SEQUENCE,
    MISSING_EXTENDS,
    MISPLACED_STATEMENT,
    MISPLACED_VALUE,
    MALFORMED_LIST,
    UNCLOSED_BRACKET,
    COMMENT_BEFORE_CLOSE,
];

/// The largest text document, in bytes (4 MiB, as the wire form).
pub const MAX_TEXT_BYTES: usize = 4 * 1024 * 1024;
/// The most lines in a document (a final line feed does not open a line).
pub const MAX_TEXT_LINES: usize = 100_000;
/// The longest line, in bytes, without its line feed (amended 2026-11-09: the longest string the
/// wire form carries fits on one line with its assignment).
pub const MAX_LINE_BYTES: usize = 8_192;
/// The longest name, number, unit suffix, atom or provenance id, in bytes.
pub const MAX_TOKEN_BYTES: usize = 256;
/// The longest string, in bytes as written between its quotes (escapes counted as written), as
/// the wire form's [`crate::MAX_WIRE_STRING_BYTES`].
pub const MAX_STRING_BYTES: usize = 4_096;
/// The longest comment, in bytes after its `#`: the line bound less the `#` and the deepest
/// indent the printer gives a comment (two spaces), so a comment never formats past the line bound.
pub const MAX_COMMENT_BYTES: usize = MAX_LINE_BYTES - 3;
/// The deepest list nesting: vectors and matrices.
pub const MAX_LIST_DEPTH: usize = 2;
/// The most statements of one kind (parameters, inputs, outputs, blocks, connections), and the
/// most assignments in one block, array elements counted one by one.
pub const MAX_STATEMENTS_PER_KIND: usize = 65_536;
