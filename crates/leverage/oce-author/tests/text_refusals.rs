//! One typed refusal per malformed construct (issue #482; authoring note test plan): each case
//! asserts the exact code and the exact line and column, and every code in `PARSE_CODES` is
//! covered here or, for the bounds, in `text_bounds.rs`. A semantic defect is the validator's,
//! never the parser's.

use oce_author::{
    BAD_ESCAPE, BYTE_ORDER_MARK, COMMENT_BEFORE_CLOSE, CONTROL_CHARACTER, DUPLICATE_NAME,
    DUPLICATE_TAG, INTEGER_OUT_OF_RANGE, Literal, MALFORMED_ATOM, MALFORMED_LIST, MALFORMED_NUMBER,
    MALFORMED_SUFFIX, MISPLACED_STATEMENT, MISPLACED_VALUE, MISSING_EXTENDS, MISSING_SEQUENCE,
    NESTING_TOO_DEEP, NON_FINITE_NUMBER, NOT_UTF8, PARSE_CODES, STRAY_TOKEN, UNCLOSED_BRACKET,
    UNIT_NOT_BASE, UNKNOWN_KEYWORD, UNKNOWN_TAG, UNKNOWN_TYPE, UNTERMINATED_STRING, parse_text,
    parse_text_bytes, validate_text_author_side,
};

const HEAD: &str = "sequence s\nextends none\n";

/// Assert `text` is refused with `code` at `line`:`column`, and return the detail.
fn refused(text: &str, code: &str, line: u32, column: u32) -> String {
    let error = parse_text(text).expect_err(&format!("{code} expected for {text:?}"));
    assert_eq!(
        (error.code, error.location.line, error.location.column),
        (code, line, column),
        "{text:?}: {error}"
    );
    assert!(error.to_string().starts_with(code));
    error.detail
}

/// `HEAD` then `body`: the body's first line is line 3.
fn body(text: &str) -> String {
    format!("{HEAD}{text}")
}

#[test]
fn the_refusal_codes_are_pinned_in_order() {
    // The list is a contract like `STAGE_ONE_CODES`: a code added, removed, renamed or reordered
    // moves this test.
    assert_eq!(
        PARSE_CODES,
        [
            "not-utf8",
            "byte-order-mark",
            "control-character",
            "input-too-large",
            "too-many-lines",
            "line-too-long",
            "token-too-long",
            "string-too-long",
            "comment-too-long",
            "nesting-too-deep",
            "too-many-statements",
            "unterminated-string",
            "bad-escape",
            "malformed-number",
            "integer-out-of-range",
            "non-finite-number",
            "malformed-suffix",
            "unit-not-base",
            "malformed-atom",
            "stray-token",
            "unknown-keyword",
            "unknown-tag",
            "duplicate-tag",
            "unknown-type",
            "missing-sequence",
            "missing-extends",
            "misplaced-statement",
            "misplaced-value",
            "malformed-list",
            "unclosed-bracket",
            "comment-before-close",
        ]
    );
}

#[test]
fn an_unterminated_string_is_refused_at_its_opening_quote() {
    refused(
        &body("in u : Real @name \"open\n"),
        UNTERMINATED_STRING,
        3,
        19,
    );
    refused(
        &body("in u : Real @name \"open"),
        UNTERMINATED_STRING,
        3,
        19,
    );
    // A backslash at the end of the line does not continue the string.
    refused(
        &body("in u : Real @name \"open\\\n\"\n"),
        UNTERMINATED_STRING,
        3,
        19,
    );
}

#[test]
fn a_bad_escape_is_refused_at_its_backslash() {
    for escape in ["\\t", "\\u0041", "\\'", "\\/", "\\r", "\\\u{e9}"] {
        let detail = refused(
            &body(&format!("in u : Real @name \"ab{escape}\"\n")),
            BAD_ESCAPE,
            3,
            22,
        );
        assert!(detail.contains("only"), "{detail}");
    }
    // The three escapes the note allows read back as their characters.
    let model = parse_text(&body("in u : Real @name \"q\\\" b\\\\ n\\n\"\n")).expect("parses");
    assert_eq!(
        model.inputs[0].semantic.name.as_deref(),
        Some("q\" b\\ n\n")
    );
}

#[test]
fn a_malformed_number_is_refused_not_normalized() {
    for (value, column) in [
        ("+1", 18),
        ("01", 18),
        ("-01", 18),
        ("1.", 18),
        (".5", 18),
        ("1e", 18),
        ("1e+", 18),
        ("1x", 18),
        ("0x10", 18),
        ("1.5.3", 18),
        ("1_000", 18),
    ] {
        refused(
            &body(&format!("param p : Real = {value}\n")),
            MALFORMED_NUMBER,
            3,
            column,
        );
    }
}

#[test]
fn an_integer_outside_i64_is_refused_and_the_extremes_are_kept() {
    refused(
        &body("param p : Integer = 9223372036854775808\n"),
        INTEGER_OUT_OF_RANGE,
        3,
        21,
    );
    refused(
        &body("param p : Integer = -9223372036854775809\n"),
        INTEGER_OUT_OF_RANGE,
        3,
        21,
    );
    let model = parse_text(&body(
        "param a : Integer = 9223372036854775807\nparam b : Integer = -9223372036854775808\n",
    ))
    .expect("the i64 extremes parse");
    assert_eq!(model.params[0].value, Literal::Integer(i64::MAX));
    assert_eq!(model.params[1].value, Literal::Integer(i64::MIN));
}

#[test]
fn nan_and_infinity_spelled_as_numbers_are_refused() {
    for value in [
        "NaN", "nan", "inf", "Inf", "Infinity", "INFINITY", "-inf", "-NaN", "1e309", "-1e400",
    ] {
        refused(
            &body(&format!("param p : Real = {value}\n")),
            NON_FINITE_NUMBER,
            3,
            18,
        );
        refused(
            &body(&format!("block h = CDL.Reals.Hysteresis(uLow = {value})\n")),
            NON_FINITE_NUMBER,
            3,
            39,
        );
    }
    // A real that underflows reads as zero, as binary64 does.
    let model = parse_text(&body("param p : Real = 1e-400\n")).expect("parses");
    assert_eq!(model.params[0].value, Literal::Real(0.0));
}

#[test]
fn a_malformed_suffix_is_refused_at_the_suffix() {
    for suffix in ["m^2", "K/", "1", "/s", "m--1", "kg.", "m-", "%", "K/(kg.K)"] {
        refused(
            &body(&format!("param p : Real = 1 {suffix}\n")),
            MALFORMED_SUFFIX,
            3,
            20,
        );
    }
}

#[test]
fn a_suffix_that_is_not_a_base_unit_is_refused_naming_vocabulary_v1() {
    // Decision 13: `1 h` is refused until vocabulary v1 supplies scale factors.
    let detail = refused(&body("param p : Real = 1 h\n"), UNIT_NOT_BASE, 3, 20);
    assert!(
        detail.contains("`h`") && detail.contains("vocabulary v1"),
        "{detail}"
    );
    for suffix in ["degC", "kPa", "min", "g", "l/s", "W/m2.h"] {
        refused(
            &body(&format!("param p : Real = 1 {suffix}\n")),
            UNIT_NOT_BASE,
            3,
            20,
        );
    }
    for suffix in [
        "K",
        "s",
        "m3/s",
        "kg/s",
        "J.kg-1.K-1",
        "1/s",
        "Pa",
        "W",
        "Ohm",
    ] {
        let model = parse_text(&body(&format!("param p : Real = 1 {suffix}\n"))).expect(suffix);
        assert_eq!(model.params[0].suffix.as_deref(), Some(suffix));
    }
}

#[test]
fn a_stray_token_is_refused_where_it_stands() {
    refused(&body("param p : Real = 1 K extra\n"), STRAY_TOKEN, 3, 22);
    refused("sequence s t\nextends none\n", STRAY_TOKEN, 1, 12);
    refused(&body("param p Real = 1\n"), STRAY_TOKEN, 3, 9);
    refused(&body("$\n"), STRAY_TOKEN, 3, 1);
    refused(&body("a.y -> b.u $\n"), STRAY_TOKEN, 3, 12);
    refused(&body("a.y b.u\n"), STRAY_TOKEN, 3, 5);
    refused(&body("param p : Real = true K\n"), STRAY_TOKEN, 3, 23);
    refused(&body("block h = C(k = 1,)\n"), STRAY_TOKEN, 3, 19);
    refused(&body("block h = C(k = true j = 2)\n"), STRAY_TOKEN, 3, 22);
    // After a number, the next run is its unit suffix, so a missing `,` reads as a suffix.
    refused(&body("block h = C(k = 1 j = 2)\n"), UNIT_NOT_BASE, 3, 19);
    refused(&body("block h = C(k = [1,])\n"), STRAY_TOKEN, 3, 20);
    refused(&body("in u : Real @unit\n"), STRAY_TOKEN, 3, 18);
    refused(&body("in u : Real @name K\n"), STRAY_TOKEN, 3, 19);
    refused(&body("in u : Real \u{e9}\n"), STRAY_TOKEN, 3, 13);
    refused("sequence s\nextends\n", STRAY_TOKEN, 2, 8);
    refused(&body("block h = C(k = -x)\n"), STRAY_TOKEN, 3, 17);
}

#[test]
fn an_unknown_keyword_is_refused_at_the_word() {
    let detail = refused(&body("blok h = CDL.Reals.Abs\n"), UNKNOWN_KEYWORD, 3, 1);
    assert!(detail.contains("`blok`"), "{detail}");
    refused(&body("Param p : Real = 1\n"), UNKNOWN_KEYWORD, 3, 1);
}

#[test]
fn an_unknown_tag_type_or_repeated_tag_is_refused() {
    refused(&body("in u : Real @colour red\n"), UNKNOWN_TAG, 3, 13);
    refused(&body("in u : Real @unit K @unit K\n"), DUPLICATE_TAG, 3, 21);
    refused(
        &body("in u : Real @provisional @provisional\n"),
        DUPLICATE_TAG,
        3,
        26,
    );
    refused(&body("in u : Float\n"), UNKNOWN_TYPE, 3, 8);
    refused(&body("in u : real\n"), UNKNOWN_TYPE, 3, 8);
    // `@haystack` repeats.
    let model = parse_text(&body("in u : Real @haystack a @haystack b\n")).expect("parses");
    assert_eq!(model.inputs[0].semantic.haystack, ["a", "b"]);
}

#[test]
fn the_header_is_sequence_then_an_explicit_extends() {
    refused("", MISSING_SEQUENCE, 1, 1);
    refused("# only a comment\n\n", MISSING_SEQUENCE, 3, 1);
    refused("param p : Real = 1\n", MISSING_SEQUENCE, 1, 1);
    refused("extends none\nsequence s\n", MISSING_SEQUENCE, 1, 1);
    refused("sequence s\n", MISSING_EXTENDS, 2, 1);
    refused("sequence s", MISSING_EXTENDS, 1, 11);
    refused("sequence s\nparam p : Real = 1\n", MISSING_EXTENDS, 2, 1);
    refused(&body("sequence t\n"), MISPLACED_STATEMENT, 3, 1);
    refused(
        &body("param p : Real = 1\nextends none\n"),
        MISPLACED_STATEMENT,
        4,
        1,
    );
    // A comment between the two folds into the header; blank lines are allowed.
    let model = parse_text("\n# a\nsequence s\n\n# b\nextends none\n").expect("parses");
    assert_eq!(model.annotations.comments, [" a", " b"]);
    assert_eq!(model.extends, None);
}

#[test]
fn a_reference_or_a_list_is_misplaced_as_a_sequence_parameter_value() {
    refused(&body("param p : Real = q\n"), MISPLACED_VALUE, 3, 18);
    refused(&body("param p : Real = [1, 2]\n"), MISPLACED_VALUE, 3, 18);
    // An enumeration member is a literal, so a sequence parameter may hold it (the validator
    // judges its kind).
    parse_text(&body("param p : Real = SimpleController.PI\n")).expect("parses");
}

#[test]
fn a_malformed_list_is_refused_at_its_bracket() {
    for (list, column) in [
        ("[]", 17),
        ("[[]]", 17),
        ("[[1], [2, 3]]", 17),
        ("[1, [2]]", 17),
        ("[[1], 2]", 17),
    ] {
        refused(
            &body(&format!("block g = C(K = {list})\n")),
            MALFORMED_LIST,
            3,
            column,
        );
    }
    refused(&body("block g = C(K = [[[1]]])\n"), NESTING_TOO_DEEP, 3, 19);
}

#[test]
fn an_unclosed_bracket_is_refused_at_its_opening() {
    refused(&body("block h = C(k = 1\n"), UNCLOSED_BRACKET, 3, 12);
    refused(
        &body("block h = C(\n  k = 1,\n  j = 2\n"),
        UNCLOSED_BRACKET,
        3,
        12,
    );
    refused(&body("block h = C(k = [1, 2\n"), UNCLOSED_BRACKET, 3, 17);
}

#[test]
fn a_comment_before_a_close_is_refused() {
    refused(
        &body("block h = C(\n  k = 1\n  # dangling\n)\n"),
        COMMENT_BEFORE_CLOSE,
        5,
        3,
    );
    refused(
        &body("block h = C(k = [\n  1\n  # dangling\n])\n"),
        COMMENT_BEFORE_CLOSE,
        5,
        3,
    );
    // An end-of-line comment on the last assignment belongs to that assignment.
    let model = parse_text(&body("block h = C(\n  k = 1 # kept\n)\n")).expect("parses");
    assert_eq!(
        model.blocks[0].assignments[0].annotations.comments,
        [" kept"]
    );
}

#[test]
fn lexical_refusals_name_the_byte() {
    refused("\u{feff}sequence s\nextends none\n", BYTE_ORDER_MARK, 1, 1);
    for (ch, column) in [
        ("\t", 9),
        ("\r", 9),
        ("\u{7f}", 9),
        ("\u{85}", 9),
        ("\u{0}", 9),
    ] {
        refused(
            &format!("sequence{ch} s\nextends none\n"),
            CONTROL_CHARACTER,
            1,
            column,
        );
    }
    // A control character inside a comment or a string is refused too.
    refused(&body("# a\tb\n"), CONTROL_CHARACTER, 3, 4);
    refused(
        &body("in u : Real @name \"a\tb\"\n"),
        CONTROL_CHARACTER,
        3,
        21,
    );
    // CRLF line ends are refused at the carriage return.
    refused("sequence s\r\nextends none\r\n", CONTROL_CHARACTER, 1, 11);
    let error = parse_text_bytes(b"sequence s\nextends none\nparam p : Real = 1 \xff\n")
        .expect_err("not UTF-8");
    assert_eq!(
        (error.code, error.location.line, error.location.column),
        (NOT_UTF8, 3, 20)
    );
    // Non-ASCII is fine in strings and comments.
    parse_text(&body(
        "# \u{e9}t\u{e9} \u{1f321}\nin u : Real @label \"\u{b0}C\"\n",
    ))
    .expect("parses");
}

#[test]
fn duplicate_names_and_unknown_classes_are_the_validators_not_the_parsers() {
    let text = body(
        "in x : Real\nout x : Real\nblock b = CDL.Reals.Nope(k = 1, k = 2)\nblock in = CDL.Reals.Abs\n",
    );
    let findings = validate_text_author_side(&text).expect("parses");
    let codes: Vec<&str> = findings.iter().map(|f| f.code).collect();
    assert!(codes.contains(&DUPLICATE_NAME), "{codes:?}");
    assert!(codes.contains(&"class-not-found"), "{codes:?}");
    assert!(codes.contains(&"invalid-name"), "{codes:?}");
}

#[test]
fn a_dotted_name_has_no_space_around_its_dots() {
    refused(&body("block h = CDL .Reals.Abs\n"), STRAY_TOKEN, 3, 15);
    refused(&body("block h = CDL. Reals.Abs\n"), STRAY_TOKEN, 3, 16);
    refused(&body("a .y -> b.u\n"), STRAY_TOKEN, 3, 3);
    refused(&body("a.y -> b. u\n"), STRAY_TOKEN, 3, 11);
    refused(
        &body("block p = CDL.Reals.PID(controllerType = SimpleController .PI)\n"),
        STRAY_TOKEN,
        3,
        59,
    );
}

#[test]
fn an_atom_is_printable_ascii_and_stops_at_a_hash() {
    refused(&body("in u : Real @unit d\u{e9}g\n"), MALFORMED_ATOM, 3, 19);
    refused(
        &body("in u : Real @haystack \u{1f321}\n"),
        MALFORMED_ATOM,
        3,
        23,
    );
    refused("sequence s\nextends cxf:\u{e9}\n", MALFORMED_ATOM, 2, 9);
    // `#` ends an atom (the rest is a comment) but belongs to a provenance id (decision 5).
    let model =
        parse_text("sequence s\nextends cxf:a#b # c\nin u : Real @unit K#c\n").expect("parses");
    assert_eq!(
        model.extends.as_ref().map(|id| id.as_str()),
        Some("cxf:a#b")
    );
    assert_eq!(model.inputs[0].semantic.unit.as_deref(), Some("K"));
    assert_eq!(model.inputs[0].annotations.comments, ["c"]);
}

#[test]
fn bidirectional_overrides_and_isolates_are_refused_in_comments_and_strings() {
    for ch in [
        '\u{202a}', '\u{202b}', '\u{202c}', '\u{202d}', '\u{202e}', '\u{2066}', '\u{2067}',
        '\u{2068}', '\u{2069}',
    ] {
        refused(&body(&format!("# a{ch}b\n")), CONTROL_CHARACTER, 3, 4);
        refused(
            &body(&format!("in u : Real @name \"a{ch}b\"\n")),
            CONTROL_CHARACTER,
            3,
            21,
        );
    }
    // Neighbours are not refused.
    parse_text(&body("# \u{2029}\u{2065}\u{206a}\u{2028}\n")).expect("parses");
}

#[test]
fn the_suffix_denominator_and_exponent_edges() {
    // One `/`, whose right-hand side is the whole denominator: `kg/m.s2` is kg/(m·s²).
    for suffix in [
        "kg/m.s2",
        "m.s-2",
        "W/m2.K",
        "1/s",
        "J.kg-1.K-1",
        "m-3",
        "H",
        "K10",
    ] {
        let model = parse_text(&body(&format!("param p : Real = 1 {suffix}\n"))).expect(suffix);
        assert_eq!(model.params[0].suffix.as_deref(), Some(suffix));
    }
    for suffix in [
        "K0", "m-0", "m00", "kg/m/s", "m/", "/m", "1", "m2.", ".m", "m-", "m--2", "m+2",
    ] {
        refused(
            &body(&format!("param p : Real = 1 {suffix}\n")),
            MALFORMED_SUFFIX,
            3,
            20,
        );
    }
}

#[test]
fn every_parse_code_is_produced_by_some_input() {
    // A mechanical cover: one input per code, run through the byte entry; the codes seen must be
    // exactly `PARSE_CODES`. (The tests above pin each code's location; this pins that none is
    // dead.)
    let many = |n: usize, line: &str| format!("{HEAD}{}", line.repeat(n));
    let mut big = format!(
        "{HEAD}{}",
        format!("#{}\n", "x".repeat(4_094)).repeat(1_024)
    );
    big.push_str(&"#".repeat(64));
    let cases: Vec<(Vec<u8>, &str)> = vec![
        (b"sequence s\nextends none\n\xff".to_vec(), NOT_UTF8),
        ("\u{feff}sequence s".into(), BYTE_ORDER_MARK),
        ("sequence\ts".into(), CONTROL_CHARACTER),
        (big.into_bytes(), oce_author::INPUT_TOO_LARGE),
        (many(100_000, "\n").into_bytes(), oce_author::TOO_MANY_LINES),
        (
            body(&format!("#{}\n", "x".repeat(9_000))).into_bytes(),
            oce_author::LINE_TOO_LONG,
        ),
        (
            body(&format!("param {} : Real = 1\n", "a".repeat(257))).into_bytes(),
            oce_author::TOKEN_TOO_LONG,
        ),
        (
            body(&format!("in u : Real @name \"{}\"\n", "x".repeat(4_097))).into_bytes(),
            oce_author::STRING_TOO_LONG,
        ),
        (
            body(&format!("#{}\n", "x".repeat(8_190))).into_bytes(),
            oce_author::COMMENT_TOO_LONG,
        ),
        (
            body("block g = C(K = [[[1]]])\n").into_bytes(),
            NESTING_TOO_DEEP,
        ),
        (
            many(65_537, "u -> y\n").into_bytes(),
            oce_author::TOO_MANY_STATEMENTS,
        ),
        (
            body("in u : Real @name \"x\n").into_bytes(),
            UNTERMINATED_STRING,
        ),
        (body("in u : Real @name \"\\t\"\n").into_bytes(), BAD_ESCAPE),
        (body("param p : Real = 01\n").into_bytes(), MALFORMED_NUMBER),
        (
            body("param p : Integer = 9223372036854775808\n").into_bytes(),
            INTEGER_OUT_OF_RANGE,
        ),
        (
            body("param p : Real = NaN\n").into_bytes(),
            NON_FINITE_NUMBER,
        ),
        (
            body("param p : Real = 1 m^2\n").into_bytes(),
            MALFORMED_SUFFIX,
        ),
        (body("param p : Real = 1 h\n").into_bytes(), UNIT_NOT_BASE),
        (
            body("in u : Real @unit d\u{e9}g\n").into_bytes(),
            MALFORMED_ATOM,
        ),
        (body("$\n").into_bytes(), STRAY_TOKEN),
        (body("blok h = C\n").into_bytes(), UNKNOWN_KEYWORD),
        (body("in u : Real @colour red\n").into_bytes(), UNKNOWN_TAG),
        (
            body("in u : Real @unit K @unit K\n").into_bytes(),
            DUPLICATE_TAG,
        ),
        (body("in u : Float\n").into_bytes(), UNKNOWN_TYPE),
        (b"param p : Real = 1\n".to_vec(), MISSING_SEQUENCE),
        (b"sequence s\n".to_vec(), MISSING_EXTENDS),
        (body("sequence t\n").into_bytes(), MISPLACED_STATEMENT),
        (body("param p : Real = q\n").into_bytes(), MISPLACED_VALUE),
        (body("block g = C(K = [])\n").into_bytes(), MALFORMED_LIST),
        (body("block h = C(k = 1\n").into_bytes(), UNCLOSED_BRACKET),
        (
            body("block h = C(\n  k = 1\n  # x\n)\n").into_bytes(),
            COMMENT_BEFORE_CLOSE,
        ),
    ];
    let mut seen = std::collections::BTreeSet::new();
    for (input, code) in cases {
        let error = parse_text_bytes(&input).expect_err(code);
        assert_eq!(error.code, code, "{error}");
        seen.insert(error.code);
    }
    let all: std::collections::BTreeSet<&str> = PARSE_CODES.iter().copied().collect();
    assert_eq!(seen, all, "a parse code no input produces");
}
