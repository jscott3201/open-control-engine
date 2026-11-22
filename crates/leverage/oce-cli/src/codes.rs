//! Every code the tool reports, with a one-line meaning: the tool's own error codes, and the
//! meaning tables `vocab` prints for `oce_author::WIRE_CODES`, `oce_author::PARSE_CODES`,
//! `oce_author::STAGE_ONE_CODES` and `oce_author::IMPORT_ERROR_CODES` (pinned to those lists, in
//! order, by the unit tests below).

// --- The tool's own error codes (`errors[].code`, exit 2). ---

/// Reading stdin (`-`) without `--from`.
pub(crate) const MISSING_FROM: &str = "missing-from";
/// A file whose name gives no form (`.oce`, `.json`; for `id` and `diff` also `.jsonld`,
/// `.cxf.json`), without `--from`.
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
/// `--namespace` (`emit`, `id`) is malformed.
pub(crate) const INVALID_NAMESPACE: &str = "invalid-namespace";
/// The model cannot be spelled in the form asked for: as text (`fmt` of a wire file, `import`),
/// or in the wire form so that its reader reads it back (`import --wire`).
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
/// A `verify` subject whose name gives no form, without `--subject-form`.
pub(crate) const UNKNOWN_SUBJECT_FORM: &str = "unknown-subject-form";
/// A mutant path that resolves, through a symbolic link or a drive prefix, to a file outside the
/// vector file's directory (owner ruling of 2026-11-11).
pub(crate) const MUTANT_OUTSIDE_DIRECTORY: &str = "mutant-outside-directory";
/// `id --namespace` beside a CXF document, which is never emitted.
pub(crate) const NAMESPACE_NEEDS_AUTHORED: &str = "namespace-needs-authored";
/// The input of `id`, or a CXF side of `diff`, does not load: the engine refused the document,
/// or (`id`) the authored sequence has an error finding and was not emitted.
pub(crate) const NOT_LOADABLE: &str = "not-loadable";
/// The facade could not name a model it loaded (never expected).
pub(crate) const NO_CONTENT_ID: &str = "no-content-id";

/// `verify --commission` over a spec with several references and no `--reference` (commissioning
/// note, decision 35).
pub(crate) const REFERENCE_REQUIRED: &str = "reference-required";
/// A path a commissioning spec names (its CXF, its mapping or a trend source) that resolves,
/// through a symbolic link or a drive prefix, to a file outside the spec's directory (issue
/// #664, the rule of `mutant-outside-directory`).
pub(crate) const FILE_OUTSIDE_DIRECTORY: &str = "file-outside-directory";

/// `import -o` names a file `oce` would read in another form than the one written.
pub(crate) const OUTPUT_FORM_MISMATCH: &str = "output-form-mismatch";
/// `emit -o` or `import -o` names the input file itself, or `verify --commission --out DIR` would
/// write `DIR/receipt.json` over a file the run reads.
pub(crate) const OUTPUT_IS_INPUT: &str = "output-is-input";
/// The two sides of `diff` have names that read as two forms, and `--from` was not given.
pub(crate) const MIXED_FORMS: &str = "mixed-forms";
/// Both sides of `diff` are stdin (`-`), which can be read once.
pub(crate) const STDIN_TWICE: &str = "stdin-twice";

/// The tool's own error codes and their meanings, as `vocab` prints them.
pub(crate) const TOOL_CODES: &[(&str, &str)] = &[
    (MISSING_FROM, "stdin (-) was given without --from"),
    (
        UNKNOWN_FORM,
        "the name ends in neither .oce nor .json (for id and diff, nor .jsonld or .cxf.json) and --from was not given",
    ),
    (NOT_FOUND, "the input path does not exist"),
    (
        NOT_A_FILE,
        "the input path is a directory or another non-file",
    ),
    (READ_FAILED, "the input could not be read"),
    (
        INPUT_TOO_LARGE,
        "the input is over its limit (4 MiB a sequence, vector, commissioning spec or mapping file, 8 MiB CXF, 1 GiB a trend source); refused before it is read",
    ),
    (
        INVALID_NAMESPACE,
        "--namespace is empty, too long, not printable ASCII or does not end in #, / or :",
    ),
    (
        UNSPELLABLE,
        "the model cannot be written as text, or (import --wire) in the wire form so that it reads back",
    ),
    (
        WRITE_FAILED,
        "the output could not be written or renamed into place, or (verify --out) DIR is not an existing directory, or (verify --commission --out) DIR/outputs already exists",
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
    (
        UNKNOWN_SUBJECT_FORM,
        "a verify subject is not .jsonld, .cxf.json, .oce or .json and --subject-form was not given",
    ),
    (
        MUTANT_OUTSIDE_DIRECTORY,
        "a mutant path resolves, through a link or a drive prefix, outside the vector file's directory",
    ),
    (
        REFERENCE_REQUIRED,
        "verify --commission: the spec has several references and --reference was not given; the detail lists each",
    ),
    (
        FILE_OUTSIDE_DIRECTORY,
        "verify --commission: a CXF, mapping or trend source path resolves, through a link or a drive prefix, outside the spec's directory",
    ),
    (
        NAMESPACE_NEEDS_AUTHORED,
        "id --namespace was given beside a CXF document; only a text or wire file is emitted",
    ),
    (
        NOT_LOADABLE,
        "id or a CXF side of diff: the engine refused the document; id: or the sequence has error findings; there is no content id or diff",
    ),
    (NO_CONTENT_ID, "the facade could not name a model it loaded"),
    (
        OUTPUT_FORM_MISMATCH,
        "import -o or emit -o names a file whose name reads as another form than the one written (.oce text, .json wire, .jsonld and .cxf.json CXF)",
    ),
    (
        OUTPUT_IS_INPUT,
        "emit -o or import -o names the input file itself, by path, link or (on Unix) hard link, or, on Unix, the regular file standard input reads; or verify --commission --out DIR would write DIR/receipt.json over a file the run reads; nothing is written",
    ),
    (
        MIXED_FORMS,
        "diff: the two names read as two forms (text, wire, CXF) and --from was not given; both sides are one form",
    ),
    (
        STDIN_TWICE,
        "diff: both sides are stdin (-), which is read once",
    ),
];

/// The wire form's refusal codes (`oce_author::WIRE_CODES`, in order) and their meanings. The
/// code of a refusal comes from `oce_author::WireError::code`; this table only says what each
/// means.
pub(crate) const WIRE_MEANINGS: &[(&str, &str)] = &[
    ("not-utf8", "the bytes are not UTF-8"),
    (
        "byte-order-mark",
        "the document starts with a byte-order mark",
    ),
    ("input-too-large", "the document is larger than 4 MiB"),
    (
        "nesting-too-deep",
        "objects and arrays nest deeper than 8 levels",
    ),
    ("list-too-long", "a list has more than 65,536 entries"),
    ("string-too-long", "a string is longer than 4,096 bytes"),
    ("duplicate-key", "an object repeats a key"),
    (
        "non-finite-number",
        "NaN or Infinity as a number, or a real that overflows binary64",
    ),
    (
        "integer-out-of-range",
        "an integer token outside the 64-bit signed range",
    ),
    ("control-character", "a string carries a control character"),
    (
        "unknown-key",
        "an object carries a key its schema does not define",
    ),
    ("missing-key", "an object lacks a required key"),
    (
        "wrong-type",
        "a value has the wrong JSON type or an unknown enumerator",
    ),
    ("json-syntax", "the document is not JSON"),
    (
        "bound-list-too-long",
        "a boundary's min or max list has more than one number",
    ),
    (
        "malformed-connection",
        "a connection is not `source -> target` with exactly one ->",
    ),
];

/// The importer's refusal codes (`oce_author::IMPORT_ERROR_CODES`, in order) and their meanings.
/// The code of a refusal comes from `oce_author::ImportError::code`; this table only says what
/// each means. The list is the importer's whole list, so that no refusal can carry a code
/// `vocab` does not print: it holds the three codes of the v0 graph JSON reader, which `import`
/// does not expose, and `too-large`, which the tool's own bounded read pre-empts.
pub(crate) const IMPORT_MEANINGS: &[(&str, &str)] = &[
    (
        "too-large",
        "the document is over the importer's size limit (the tool's bounded read refuses an over-size input first, as input-too-large)",
    ),
    (
        "not-strict-json",
        "the bytes are not UTF-8 or not JSON, nesting is deeper than 128 levels, an object repeats a key, or an integer is outside the 64-bit signed range",
    ),
    (
        "limit-exceeded",
        "v0 graph JSON: nesting, a list or a string over a read bound",
    ),
    (
        "unknown-key",
        "a property, top-level key or @context term the flat CXF subset does not define",
    ),
    (
        "namespace-mismatch",
        "the @context binds the S231 vocabulary in a namespace the engine does not read",
    ),
    (
        "missing-key",
        "a required key is absent: a node without @id, a parameter without a type or a value",
    ),
    (
        "wrong-shape",
        "a value of the wrong JSON shape: a relation that is not references, a label or term that is not a string",
    ),
    ("no-root", "no node contains blocks"),
    ("duplicate-id", "a node @id is declared twice"),
    (
        "unresolved-reference",
        "a reference to no node, or to a node outside the root's IRI",
    ),
    (
        "stray-node",
        "a node no root relation reaches, or one reached twice",
    ),
    (
        "invalid-name",
        "a root @id or local name that breaks the identifier syntax, or a path deeper than block.member",
    ),
    (
        "nested-composite",
        "a second node that contains blocks (outside the subset)",
    ),
    ("instance-member", "hasInstance (outside the subset)"),
    ("constant-member", "hasConstant (outside the subset)"),
    (
        "conditional-component",
        "a conditional component (outside the subset)",
    ),
    (
        "array-connector",
        "an array boundary or port (outside the subset)",
    ),
    (
        "array-sequence-parameter",
        "an array sequence parameter (outside the subset)",
    ),
    (
        "expression-value",
        "a value that is an expression, or a name that is no sequence parameter (outside the subset)",
    ),
    (
        "port-attribute",
        "unit, quantity, displayUnit, min, max or nominal on a block port (not authorable)",
    ),
    (
        "connector-attribute",
        "nominal or unbounded on a boundary (not carried)",
    ),
    (
        "two-spellings",
        "one property written under two spellings on one node (S231:unit beside qudt:hasUnit)",
    ),
    (
        "driving-sink",
        "a boundary output or a block input drives a connection",
    ),
    (
        "class-outside-obc",
        "a block class outside Buildings.Controls.OBC",
    ),
    (
        "datatype-mismatch",
        "a value, bound or connector that disagrees with its declared type",
    ),
    (
        "unknown-datatype",
        "a declared type that is no supported scalar and no named type",
    ),
    (
        "unknown-member-type",
        "a member path whose enumeration or constant package the engine does not know",
    ),
    (
        "malformed-literal",
        "a non-finite or unparsable number, an unknown XSD datatype or a bad string escape",
    ),
    (
        "array-shape",
        "an array's sizes, rank or catalog template disagree with its value",
    ),
    ("string-value", "v0 graph JSON: a string parameter value"),
    (
        "malformed-connection",
        "v0 graph JSON: a connection that is not `source -> target` with exactly one ->",
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
        // Names the scaled symbols, which are accepted only alone (issue #501); `tests/vocabulary.rs`
        // holds the clause to `oce_vocab::SCALE_FACTORS`.
        "a unit suffix that is neither built from base SI units nor one of the scaled symbols min, h and d alone",
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
        "missing-derived-from",
        "the sequence statement is not followed by a derived_from statement",
    ),
    (
        "misplaced-statement",
        "a second sequence or derived_from statement",
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
        "unit-suffix-mismatch",
        "a unit suffix that is not the parameter's declared unit, a scaled one where none is declared, or one that disagrees with its value",
    ),
    (
        "unknown-enum-literal",
        "an enumeration member is not a member of the parameter's type",
    ),
    (
        "unknown-enum-type",
        "a sequence parameter's type names no enumeration, or names a constant package",
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
        "invalid-bound",
        "a boundary bound the boundary's type cannot take",
    ),
    ("non-finite-bound", "a boundary bound is NaN or infinite"),
    ("crossed-bounds", "a boundary's min exceeds its max"),
    (
        "invalid-provenance-id",
        "the derived_from provenance id is malformed",
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
    (
        0,
        "yes: clean, canonical, resolved, every scenario or compared output passed, an id printed, a document imported, nothing differs",
    ),
    (
        1,
        "no: an error finding, a non-canonical file, ports that do not resolve, a failed or no_data verdict, any difference (diff)",
    ),
    (
        2,
        "error: a usage, read, parse, size or I/O failure, a run with no verdict, an input with no content id (id), a document the importer refuses or the output form cannot spell (import), or a side that does not load or cannot be named (diff); the question was not answered",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(table: &[(&'static str, &str)]) -> Vec<&'static str> {
        table.iter().map(|(code, _)| *code).collect()
    }

    #[test]
    fn wire_meanings_cover_wire_codes_in_order() {
        assert_eq!(codes(WIRE_MEANINGS), oce_author::WIRE_CODES);
    }

    #[test]
    fn the_shared_over_size_code_is_spelled_as_the_wire_and_text_forms_spell_it() {
        // One code for an input refused by its size, whoever refuses it: the tool before the
        // read, the wire reader or the text parser.
        assert_eq!(INPUT_TOO_LARGE, oce_author::INPUT_TOO_LARGE);
        assert!(oce_author::WIRE_CODES.contains(&INPUT_TOO_LARGE));
    }

    #[test]
    fn import_meanings_cover_import_codes_in_order() {
        assert_eq!(codes(IMPORT_MEANINGS), oce_author::IMPORT_ERROR_CODES);
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
            WIRE_MEANINGS,
            IMPORT_MEANINGS,
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
