//! Every code the tool reports, with a one-line meaning: the tool's own error codes, the wire-form
//! refusal codes, and the meaning tables `vocab` prints for `oce_author::PARSE_CODES` and
//! `oce_author::STAGE_ONE_CODES` (pinned to those lists, in order, by the unit tests below).

// --- The tool's own error codes (`errors[].code`, exit 2). ---

/// Reading stdin (`-`) without `--from`.
pub(crate) const MISSING_FROM: &str = "missing-from";
/// A file whose extension is neither `.oce` nor `.json`, without `--from`.
pub(crate) const UNKNOWN_FORM: &str = "unknown-form";
/// The input path does not exist.
pub(crate) const NOT_FOUND: &str = "not-found";
/// The input path is a directory or another non-file.
pub(crate) const NOT_A_FILE: &str = "not-a-file";
/// The input could not be read (permissions, an I/O failure).
pub(crate) const READ_FAILED: &str = "read-failed";
/// The input is larger than its form's limit; refused before it is read (shared with the text
/// form's own code).
pub(crate) const INPUT_TOO_LARGE: &str = "input-too-large";
/// `emit --namespace` is malformed.
pub(crate) const INVALID_NAMESPACE: &str = "invalid-namespace";
/// The model cannot be spelled as text (`fmt` of a wire file).
pub(crate) const UNSPELLABLE: &str = "unspellable";
/// The output could not be written or renamed into place.
pub(crate) const WRITE_FAILED: &str = "write-failed";
/// `fmt --write` or `emit -o` asked to rewrite stdin.
pub(crate) const WRITE_NEEDS_FILE: &str = "write-needs-file";
/// `fmt --write` on a wire file, which would replace JSON with text.
pub(crate) const WRITE_NEEDS_TEXT: &str = "write-needs-text";
/// `catalog --param` is not `NAME=VALUE`.
pub(crate) const MALFORMED_PARAM: &str = "malformed-param";
/// `catalog --param` names no parameter of the class.
pub(crate) const UNKNOWN_PARAM: &str = "unknown-param";
/// `catalog --param` names one parameter twice.
pub(crate) const DUPLICATE_PARAM: &str = "duplicate-param";
/// `catalog --param` gives a value that is not of the parameter's type.
pub(crate) const MALFORMED_VALUE: &str = "malformed-value";
/// The facade could not name the ports of an accepted table (never expected).
pub(crate) const PORTS_UNRESOLVABLE: &str = "ports-unresolvable";
/// The emitter refused for a reason this tool does not know (never expected).
pub(crate) const EMIT_REFUSED: &str = "emit-refused";

// --- Wire-form refusal codes (`errors[].code`, exit 2), one per `oce_author::WireError`. ---

/// The wire bytes are not UTF-8.
pub(crate) const WIRE_NOT_UTF8: &str = "not-utf8";
/// The wire document starts with a byte-order mark.
pub(crate) const WIRE_BYTE_ORDER_MARK: &str = "byte-order-mark";
/// Objects and arrays nest too deep.
pub(crate) const WIRE_TOO_DEEP: &str = "nesting-too-deep";
/// A list is too long.
pub(crate) const WIRE_LIST_TOO_LONG: &str = "list-too-long";
/// A string is too long.
pub(crate) const WIRE_STRING_TOO_LONG: &str = "string-too-long";
/// An object repeats a key.
pub(crate) const WIRE_DUPLICATE_KEY: &str = "duplicate-key";
/// `NaN`, `Infinity` or an overflowing real.
pub(crate) const WIRE_NON_FINITE_NUMBER: &str = "non-finite-number";
/// An integer outside `i64`.
pub(crate) const WIRE_INTEGER_OUT_OF_RANGE: &str = "integer-out-of-range";
/// A control character in a string.
pub(crate) const WIRE_CONTROL_CHARACTER: &str = "control-character";
/// A key the schema does not define.
pub(crate) const WIRE_UNKNOWN_KEY: &str = "unknown-key";
/// A required key is missing.
pub(crate) const WIRE_MISSING_KEY: &str = "missing-key";
/// A value of the wrong JSON type or an unknown enumerator.
pub(crate) const WIRE_WRONG_TYPE: &str = "wrong-type";
/// Not JSON.
pub(crate) const WIRE_SYNTAX: &str = "json-syntax";
/// A connection string without exactly one `->`.
pub(crate) const WIRE_MALFORMED_CONNECTION: &str = "malformed-connection";
/// A wire refusal this tool does not know (a newer `WireError` variant).
pub(crate) const WIRE_REFUSED: &str = "wire-refused";

/// The tool's own error codes and their meanings, as `vocab` prints them.
pub(crate) const TOOL_CODES: &[(&str, &str)] = &[
    (MISSING_FROM, "stdin (-) was given without --from text|wire"),
    (
        UNKNOWN_FORM,
        "the extension is neither .oce nor .json and --from was not given",
    ),
    (NOT_FOUND, "the input path does not exist"),
    (
        NOT_A_FILE,
        "the input path is a directory or another non-file",
    ),
    (READ_FAILED, "the input could not be read"),
    (
        INPUT_TOO_LARGE,
        "the input is over its form's 4 MiB limit; refused before it is read",
    ),
    (
        INVALID_NAMESPACE,
        "--namespace is empty, too long, not printable ASCII or does not end in #, / or :",
    ),
    (UNSPELLABLE, "the model cannot be written as text"),
    (
        WRITE_FAILED,
        "the output could not be written or renamed into place",
    ),
    (WRITE_NEEDS_FILE, "--write or -o cannot rewrite stdin"),
    (
        WRITE_NEEDS_TEXT,
        "fmt --write rewrites text files only; redirect a wire file's text instead",
    ),
    (MALFORMED_PARAM, "a --param is not NAME=VALUE"),
    (UNKNOWN_PARAM, "a --param names no parameter of the class"),
    (DUPLICATE_PARAM, "a --param names one parameter twice"),
    (
        MALFORMED_VALUE,
        "a --param value is not of the parameter's type",
    ),
    (
        PORTS_UNRESOLVABLE,
        "the facade could not name the ports of an accepted table",
    ),
    (
        EMIT_REFUSED,
        "the emitter refused for a reason this tool does not know",
    ),
];

/// The wire-form refusal codes and their meanings, in `WireError` order.
pub(crate) const WIRE_CODES: &[(&str, &str)] = &[
    (WIRE_NOT_UTF8, "the bytes are not UTF-8"),
    (
        WIRE_BYTE_ORDER_MARK,
        "the document starts with a byte-order mark",
    ),
    (INPUT_TOO_LARGE, "the document is larger than 4 MiB"),
    (
        WIRE_TOO_DEEP,
        "objects and arrays nest deeper than 8 levels",
    ),
    (WIRE_LIST_TOO_LONG, "a list has more than 65,536 entries"),
    (WIRE_STRING_TOO_LONG, "a string is longer than 4,096 bytes"),
    (WIRE_DUPLICATE_KEY, "an object repeats a key"),
    (
        WIRE_NON_FINITE_NUMBER,
        "NaN or Infinity as a number, or a real that overflows binary64",
    ),
    (
        WIRE_INTEGER_OUT_OF_RANGE,
        "an integer token outside the 64-bit signed range",
    ),
    (
        WIRE_CONTROL_CHARACTER,
        "a string carries a control character",
    ),
    (
        WIRE_UNKNOWN_KEY,
        "an object carries a key its schema does not define",
    ),
    (WIRE_MISSING_KEY, "an object lacks a required key"),
    (
        WIRE_WRONG_TYPE,
        "a value has the wrong JSON type or an unknown enumerator",
    ),
    (WIRE_SYNTAX, "the document is not JSON"),
    (
        WIRE_MALFORMED_CONNECTION,
        "a connection is not `source -> target` with exactly one ->",
    ),
    (
        WIRE_REFUSED,
        "a wire refusal this version of oce does not name",
    ),
];

/// The text form's refusal codes (`oce_author::PARSE_CODES`, in order) and their meanings.
pub(crate) const PARSE_MEANINGS: &[(&str, &str)] = &[
    ("not-utf8", "the input is not UTF-8"),
    ("byte-order-mark", "the input starts with a byte-order mark"),
    (
        "control-character",
        "a tab, carriage return, other control character or bidirectional override",
    ),
    ("input-too-large", "the input is larger than 4 MiB"),
    ("too-many-lines", "the input has more than 100,000 lines"),
    ("line-too-long", "a line is longer than 8,192 bytes"),
    (
        "token-too-long",
        "a name, number, suffix, atom or provenance id is longer than 256 bytes",
    ),
    (
        "string-too-long",
        "a string is longer than 4,096 bytes as written",
    ),
    (
        "comment-too-long",
        "a comment is longer than 8,189 bytes after its #",
    ),
    ("nesting-too-deep", "lists nest deeper than 2"),
    (
        "too-many-statements",
        "more than 65,536 statements of one kind or assignments in one block",
    ),
    (
        "unterminated-string",
        "a string has no closing quote on its line",
    ),
    (
        "bad-escape",
        "a backslash escape other than \\\", \\\\ or \\n",
    ),
    (
        "malformed-number",
        "a number with a leading + or zero, a bare dot, an empty exponent or glued letters",
    ),
    (
        "integer-out-of-range",
        "an integer outside the 64-bit signed range",
    ),
    (
        "non-finite-number",
        "NaN, inf or Infinity as a value, or a real that overflows binary64",
    ),
    (
        "malformed-suffix",
        "a unit suffix that is not a unit expression",
    ),
    (
        "unit-not-base",
        "a unit suffix that is not built from base SI units",
    ),
    (
        "malformed-atom",
        "a tag atom or provenance id with a byte outside printable ASCII",
    ),
    (
        "stray-token",
        "a token where the grammar allows none, or a missing one",
    ),
    (
        "unknown-keyword",
        "a line that starts with no statement keyword and is not a connection",
    ),
    ("unknown-tag", "a boundary tag the grammar does not name"),
    (
        "duplicate-tag",
        "a boundary tag other than @haystack written twice on one statement",
    ),
    ("unknown-type", "a type other than Real, Integer or Boolean"),
    (
        "missing-sequence",
        "the file does not start with a sequence statement",
    ),
    (
        "missing-extends",
        "the sequence statement is not followed by an extends statement",
    ),
    (
        "misplaced-statement",
        "a second sequence or extends statement",
    ),
    (
        "misplaced-value",
        "a reference or a list as a sequence parameter's value",
    ),
    ("malformed-list", "an empty, ragged or mixed list"),
    ("unclosed-bracket", "a ( or [ that is never closed"),
    (
        "comment-before-close",
        "a comment after the last item of a (...) or [...]",
    ),
];

/// The stage-1 finding codes (`oce_author::STAGE_ONE_CODES`, in order) and their meanings.
pub(crate) const STAGE_ONE_MEANINGS: &[(&str, &str)] = &[
    ("empty-sequence", "the sequence has no blocks"),
    (
        "invalid-name",
        "a declared name is not an identifier, or is a reserved word or keyword",
    ),
    (
        "duplicate-name",
        "a name is declared twice across parameters, inputs, outputs and blocks",
    ),
    (
        "class-not-found",
        "a block's class path is not a registered class",
    ),
    (
        "class-not-authorable",
        "a block's class is an engine-reserved lowering identity",
    ),
    (
        "unknown-parameter",
        "an assignment names no parameter of the block's class",
    ),
    (
        "parameter-not-settable",
        "an assignment names a parameter no author may set",
    ),
    ("duplicate-parameter", "a block assigns one parameter twice"),
    (
        "missing-required-parameter",
        "a required parameter is not assigned",
    ),
    (
        "parameter-kind-mismatch",
        "a value's kind does not fit the parameter",
    ),
    (
        "parameter-out-of-range",
        "a value breaks a catalog rule, widths included",
    ),
    ("non-finite-parameter", "a Real value is NaN or infinite"),
    (
        "unknown-enum-literal",
        "an enumeration member is not a member of the parameter's type",
    ),
    (
        "unknown-parameter-reference",
        "a reference names no sequence parameter",
    ),
    (
        "malformed-connection",
        "a connection endpoint is not name or block.port",
    ),
    (
        "unknown-source",
        "a connection source is neither a boundary input nor a block output",
    ),
    (
        "unknown-target",
        "a connection target is neither a boundary output nor a block input",
    ),
    (
        "unknown-port",
        "a connection names a port the configured block does not have on that side",
    ),
    (
        "invalid-semantic-point",
        "a semantic, quantity or displayUnit field is malformed",
    ),
    (
        "invalid-provenance-id",
        "the extends provenance id is malformed",
    ),
    (
        "single-assignment",
        "two connections drive one block input or boundary output",
    ),
];

/// The stage-2 codes the validator adds beside the engine's own `DiagCode` spellings.
pub(crate) const STAGE_TWO_MEANINGS: &[(&str, &str)] = &[
    (
        "document-too-large",
        "the emitted document is larger than the engine loads",
    ),
    (
        "load-failed",
        "the throwaway engine refused the emitted document",
    ),
];

/// The boundary tags of the text form, in the grammar's order.
pub(crate) const TEXT_FORM_TAGS: &[&str] = &[
    "@unit",
    "@quantity",
    "@displayUnit",
    "@brick",
    "@s223",
    "@qudt",
    "@haystack",
    "@name",
    "@label",
    "@provisional",
];

/// The exit statuses and their meanings.
pub(crate) const EXIT_MEANINGS: &[(u8, &str)] = &[
    (0, "yes: clean, canonical, resolved"),
    (
        1,
        "no: an error finding, a non-canonical file, ports that do not resolve",
    ),
    (
        2,
        "error: a usage, read, parse, size or I/O failure; the question was not answered",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(table: &[(&'static str, &str)]) -> Vec<&'static str> {
        table.iter().map(|(code, _)| *code).collect()
    }

    #[test]
    fn parse_meanings_cover_parse_codes_in_order() {
        assert_eq!(codes(PARSE_MEANINGS), oce_author::PARSE_CODES);
    }

    #[test]
    fn stage_one_meanings_cover_stage_one_codes_in_order() {
        assert_eq!(codes(STAGE_ONE_MEANINGS), oce_author::STAGE_ONE_CODES);
    }

    #[test]
    fn stage_two_meanings_name_the_validator_constants() {
        assert_eq!(
            codes(STAGE_TWO_MEANINGS),
            [oce_author::DOCUMENT_TOO_LARGE, oce_author::LOAD_FAILED]
        );
    }

    #[test]
    fn every_meaning_is_one_nonempty_line_and_codes_are_unique_per_table() {
        for table in [
            TOOL_CODES,
            WIRE_CODES,
            PARSE_MEANINGS,
            STAGE_ONE_MEANINGS,
            STAGE_TWO_MEANINGS,
        ] {
            let mut seen = std::collections::BTreeSet::new();
            for (code, meaning) in table {
                assert!(seen.insert(*code), "{code} twice");
                assert!(!meaning.is_empty() && !meaning.contains('\n'), "{code}");
                assert!(
                    code.bytes()
                        .all(|b| b.is_ascii_lowercase() || b == b'-' || b.is_ascii_digit())
                );
            }
        }
    }
}
