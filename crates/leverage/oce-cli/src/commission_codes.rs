//! The codes `oce verify --commission` passes through from `oce-verify`'s commissioning readers,
//! each with a one-line meaning, in the order of `oce_verify::SPEC_ERROR_CODES`,
//! `oce_verify::SOURCE_ERROR_CODES`, `oce_verify::UNIT_ERROR_CODES` and
//! `oce_verify::ENGINE_REFUSAL_CLASSES` (pinned by the unit tests below), so that no refusal the verb prints carries a code `oce vocab` does not list
//! (commissioning note, "Rust API sketch": "Every code ... reaches `oce vocab`").
//!
//! Every one is exit `2`. A spec reader's code is an error's `code` for a spec that does not
//! load; its last four are found when a run binds the keys to the sequence, and arrive as the
//! `cause` of `spec-refused`. A source reader's code is the `cause` of `source-refused`, and a
//! unit code the `cause` of `unit-refused` (and the cause inside a source's `unit-refused`). An
//! engine refusal class is the `cause` of `frame-refused`, which ends a commissioning run with no
//! receipt (decision 28; in a vector receipt the same refusal is a failure record, never an
//! error). The bind and run codes themselves are `verify_codes::VERIFY_MEANINGS`.

/// The commissioning spec's load codes and their meanings, in `oce_verify::SPEC_ERROR_CODES`
/// order.
pub(crate) const SPEC_MEANINGS: &[(&str, &str)] = &[
    ("not-utf8", "the spec is not UTF-8"),
    ("byte-order-mark", "the spec starts with a byte-order mark"),
    (
        "json-syntax",
        "the spec, its // and /* */ comments removed, is not one strict JSON document (a trailing comma included)",
    ),
    (
        "input-too-large",
        "the spec is larger than 4 MiB, comments included",
    ),
    (
        "nesting-too-deep",
        "objects and arrays nest deeper than 8 levels",
    ),
    ("list-too-long", "a list has more than 65,536 entries"),
    (
        "string-too-long",
        "a string is longer than 4,096 bytes as written",
    ),
    ("duplicate-key", "an object writes one key twice"),
    (
        "unknown-key",
        "an object carries a key the format does not define",
    ),
    ("missing-key", "an object lacks a required key"),
    (
        "wrong-type",
        "a value has the wrong JSON type for its position",
    ),
    (
        "non-finite-number",
        "NaN or Infinity as a number, or a real that overflows binary64",
    ),
    (
        "integer-out-of-range",
        "an integer outside 64 bits, or one read as a real above 2^53",
    ),
    (
        "control-character",
        "a string carries a control character, raw or escaped (an escaped \\n excepted)",
    ),
    ("unterminated-comment", "a /* comment is never closed"),
    ("unknown-schema", "schema is not oce/commissioning-spec/v1"),
    (
        "too-many-parameters",
        "a reference's parameters map has more than 256 entries",
    ),
    (
        "too-many-keys",
        "an outputs, indicators or columns map has more than 4,096 keys",
    ),
    (
        "empty-name",
        "an empty model, sequence, key, indicator, parameter path, point name or unit, table, timeColumn or column",
    ),
    (
        "unknown-value",
        "a format, envelope, funnelReference or point type outside its list",
    ),
    (
        "path-invalid",
        "a cxf, mapping, source or controllerOutput path is absolute, has an empty, . or .. segment, or holds a backslash",
    ),
    (
        "invalid-time-origin",
        "time.origin is not an RFC 3339 instant with an explicit offset",
    ),
    ("invalid-tolerance", "a tolerance is negative"),
    ("invalid-sampling", "a sampling is not above zero"),
    ("invalid-window", "a window's start is after its stop"),
    ("invalid-hold", "a hold.maxAge is negative"),
    (
        "duplicate-point",
        "two points of one mapping share a name on one side, cdl or device",
    ),
    (
        "unmatched-key",
        "the cause of spec-refused: an outputs or indicators key selects no output of the sequence",
    ),
    (
        "ambiguous-output",
        "the cause of spec-refused: an exact outputs or indicators key names several outputs",
    ),
    (
        "conflicting-override",
        "the cause of spec-refused: two outputs keys of one kind give one output one tolerance field two values",
    ),
    (
        "work-limit-exceeded",
        "the cause of spec-refused: matching the keys against the outputs needs more than 2^28 work units",
    ),
];

/// The trend source reader's codes, the cause of `source-refused`, and their meanings, in
/// `oce_verify::SOURCE_ERROR_CODES` order.
pub(crate) const SOURCE_MEANINGS: &[(&str, &str)] = &[
    (
        "reference-out-of-range",
        "the reference index is past the end of the spec's references",
    ),
    (
        "too-many-sources",
        "the reference names more than 256 trend sources",
    ),
    (
        "duplicate-file",
        "the run was handed one path's bytes twice",
    ),
    (
        "file-unlisted",
        "the run was handed bytes under a path the reference does not name",
    ),
    (
        "file-missing",
        "the reference names a mapping or trend source the run was handed no bytes for",
    ),
    (
        "too-many-total-bytes",
        "the mapping and the trend sources are more than 1 GiB together (each path once, the CXF not counted); oce refuses it from the files' metadata before reading them",
    ),
    (
        "mapping-required",
        "the reference reads trend sources and names no pointNameMapping",
    ),
    (
        "mapping-refused",
        "the point-mapping file does not load (the detail says where and why)",
    ),
    (
        "column-reference-invalid",
        "a timeColumn, columns key or table its format does not take, or the time column mapped to a point",
    ),
    (
        "unmapped-point",
        "a column names a device point the mapping does not have",
    ),
    (
        "point-read-twice",
        "two columns of one role name one device point",
    ),
    (
        "type-pair-refused",
        "no rule converts the point's device type to its CDL type",
    ),
    (
        "unit-refused",
        "the point's two units have no conversion, or a reading overflows",
    ),
    (
        "unit-on-discrete-point",
        "a unit conversion other than the identity on an int or bool CDL point",
    ),
    (
        "format-tag-missing",
        "a combiTimeTable file does not start with #1",
    ),
    (
        "table-syntax",
        "a combiTimeTable file is not the format (a matrix header, a cell, a separator)",
    ),
    (
        "table-required",
        "a combiTimeTable file holds several matrices and the source names no table",
    ),
    (
        "table-not-found",
        "no matrix of the file has the source's table name",
    ),
    (
        "duplicate-table",
        "two matrices of the file have the source's table name",
    ),
    (
        "size-mismatch",
        "a matrix holds fewer or more values than its declared size",
    ),
    (
        "bad-number",
        "a combiTimeTable value is not a decimal number in C syntax",
    ),
    (
        "non-finite-number",
        "an infinity in a csv cell or time, or a combiTimeTable value beyond the largest binary64",
    ),
    (
        "bad-cell",
        "a csv cell its column's device type does not read",
    ),
    (
        "cell-refused",
        "a value the point's type rule refuses: a 0.5 Boolean, a non-integral Integer",
    ),
    (
        "column-missing",
        "a column the source names, or its time column, is not in the file",
    ),
    ("duplicate-column", "a csv header names one column twice"),
    ("no-header", "a controllerOutput file has no header row"),
    (
        "no-point-columns",
        "a controllerOutput file's header names no column but the time",
    ),
    (
        "ragged-row",
        "a csv row has a different number of fields from the header",
    ),
    ("not-utf8", "a csv cell or header is not UTF-8"),
    (
        "cell-too-long",
        "a csv cell, combiTimeTable number or matrix name is longer than 256 bytes",
    ),
    (
        "too-many-rows",
        "more than 16,777,216 rows in one file or frames in the merged inputs",
    ),
    (
        "too-many-columns",
        "more than 4,096 columns in one file or in the merged inputs, the time included",
    ),
    (
        "too-many-bytes",
        "more than 1 GiB in one file or held by a source's or the merged inputs' columns",
    ),
    ("time-missing", "a csv row's time cell is empty, NA or NaN"),
    (
        "bad-time",
        "a time cell is neither seconds nor an instant, or not the first row's kind",
    ),
    (
        "time-origin-required",
        "a csv time column of instants in a spec with no time.origin",
    ),
    (
        "time-regression",
        "a time earlier than the row before it in one file",
    ),
    (
        "unreadable",
        "a replay reader refusal with no word of its own (never expected)",
    ),
];

/// The unit table's codes, the cause of `unit-refused`, and their meanings, in
/// `oce_verify::UNIT_ERROR_CODES` order.
pub(crate) const UNIT_MEANINGS: &[(&str, &str)] = &[
    (
        "unknown-pair",
        "units/v1 lists no conversion from the device's unit to the CDL point's",
    ),
    (
        "one-sided",
        "one side of a mapped point has a unit and the other none",
    ),
    ("empty-unit", "a unit is written as the empty string"),
    (
        "overflow",
        "a finite reading's conversion is beyond the largest binary64",
    ),
    (
        "absolute-for-difference",
        "an absolute temperature pair on a TemperatureDifference port",
    ),
    (
        "difference-for-absolute",
        "a temperature difference pair on a ThermodynamicTemperature port",
    ),
    (
        "port-unit-mismatch",
        "the mapping's cdl.unit is not the unit the sequence port declares",
    ),
];

/// The classes of an engine refusal of a frame, the cause of `frame-refused`, and their
/// meanings, in `oce_verify::ENGINE_REFUSAL_CLASSES` order.
pub(crate) const ENGINE_REFUSAL_MEANINGS: &[(&str, &str)] = &[
    (
        "input-type",
        "an input value of the wrong type for its port",
    ),
    ("input-non-finite", "an infinite input value"),
    (
        "input-domain",
        "an input value outside its port's declared bounds (a wrong unit or mapping, most often)",
    ),
    ("non-finite-time", "a frame time that is not finite"),
    (
        "time-out-of-range",
        "a frame time beyond the engine's magnitude limit",
    ),
    (
        "time-regression",
        "a frame time earlier than the one before",
    ),
    ("non-finite-time-step", "a time step that is not finite"),
    (
        "model-time-unrepresentable",
        "a model time the engine cannot represent",
    ),
    (
        "run",
        "a refusal about the engine or the run, not the frame (run-refused carries it)",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(table: &[(&'static str, &str)]) -> Vec<&'static str> {
        table.iter().map(|(code, _)| *code).collect()
    }

    #[test]
    fn the_tables_are_oce_verifys_code_lists_in_order() {
        assert_eq!(codes(SPEC_MEANINGS), oce_verify::SPEC_ERROR_CODES);
        assert_eq!(codes(SOURCE_MEANINGS), oce_verify::SOURCE_ERROR_CODES);
        assert_eq!(codes(UNIT_MEANINGS), oce_verify::UNIT_ERROR_CODES);
        assert_eq!(
            codes(ENGINE_REFUSAL_MEANINGS),
            oce_verify::ENGINE_REFUSAL_CLASSES
        );
    }

    #[test]
    fn every_meaning_is_one_nonempty_line() {
        for (code, meaning) in SPEC_MEANINGS
            .iter()
            .chain(SOURCE_MEANINGS)
            .chain(UNIT_MEANINGS)
            .chain(ENGINE_REFUSAL_MEANINGS)
        {
            assert!(!meaning.is_empty() && !meaning.contains('\n'), "{code}");
        }
    }
}
