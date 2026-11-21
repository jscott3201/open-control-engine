//! The small readers: what each accepts, what each refuses and where.

use super::*;

fn reader(text: &str) -> Reader<'_> {
    Reader::reporting(text)
}

fn code_and_place<T: std::fmt::Debug>(read: Read<T>) -> (&'static str, u32, u32) {
    let refusal = read.expect_err("refused");
    (refusal.kind.code(), refusal.at.line, refusal.at.column)
}

#[test]
fn the_first_repeat_is_the_earliest_second_occurrence() {
    assert_eq!(first_repeat(&[]), None);
    assert_eq!(first_repeat(&["a"]), None);
    assert_eq!(first_repeat(&["a", "b", "c"]), None);
    assert_eq!(first_repeat(&["a", "a"]), Some(1));
    assert_eq!(first_repeat(&["a", "b", "a"]), Some(2));
    // Two names repeat: the one whose second occurrence comes first is reported.
    assert_eq!(first_repeat(&["a", "b", "b", "a"]), Some(2));
    assert_eq!(first_repeat(&["b", "a", "c", "a", "b"]), Some(3));
    // Three of one: its second, not its third.
    assert_eq!(first_repeat(&["a", "x", "a", "a"]), Some(2));
    // Compared by bytes: case, space and the empty name are names of their own.
    assert_eq!(first_repeat(&["a", "A", "a ", " a", ""]), None);
    assert_eq!(first_repeat(&["", "x", ""]), Some(2));
    // A long list is sorted, not scanned pair by pair: 200,000 names, the last one a repeat.
    let mut names: Vec<String> = (0..200_000).map(|index| index.to_string()).collect();
    names.push("7".to_owned());
    let borrowed: Vec<&str> = names.iter().map(String::as_str).collect();
    assert_eq!(first_repeat(&borrowed), Some(200_000));
}

#[test]
fn a_key_written_twice_is_refused_by_the_typed_reader_standing_alone() {
    // The strict pass refuses a repeat before any typed reader runs; each typed reader still
    // refuses one itself, at the second place.
    let mut slot = None;
    assert_eq!(put(&mut slot, 1, "start", Location::new(1, 2)), Ok(()));
    let refusal = put(&mut slot, 2, "start", Location::new(1, 14)).expect_err("a repeat");
    assert_eq!(refusal.kind, SpecErrorKind::DuplicateKey);
    assert_eq!((refusal.at.line, refusal.at.column), (1, 14));
    assert_eq!(refusal.detail, "duplicate key `start`");
    assert_eq!(slot, Some(1), "the first value is kept");
    // A required key that never arrived is named at its object.
    let refusal = required(None::<u8>, "stop", Location::new(4, 3)).expect_err("missing");
    assert_eq!(refusal.kind, SpecErrorKind::MissingKey);
    assert_eq!((refusal.at.line, refusal.at.column), (4, 3));
    assert_eq!(refusal.detail, "missing key `stop`");
    assert_eq!(required(Some(7), "stop", Location::new(4, 3)), Ok(7));
    // The count bound is inclusive.
    assert!(!exceeds(0, 0) && exceeds(1, 0));
    assert!(!exceeds(256, 256) && exceeds(257, 256));
    assert!(!exceeds(usize::MAX, u32::MAX) || usize::BITS > 32);
    // A name may be anything but empty.
    assert!(not_empty(" ", Location::new(1, 1), "a key").is_ok());
    let refusal = not_empty("", Location::new(2, 9), "an indicator name").expect_err("empty");
    assert_eq!(refusal.kind, SpecErrorKind::EmptyName);
    assert_eq!(refusal.detail, "an indicator name must not be empty");
}

#[test]
fn an_expectation_names_what_was_wanted_and_what_was_found() {
    assert_eq!(
        expect(
            &mut reader("  [1]"),
            Kind::Array,
            "the references",
            "a list"
        )
        .map(|at| (at.line, at.column)),
        Ok((1, 3))
    );
    let refusal = expect(
        &mut reader("\n {}"),
        Kind::Array,
        "the references",
        "a list",
    )
    .expect_err("an object is no list");
    assert_eq!(refusal.kind, SpecErrorKind::WrongType);
    assert_eq!((refusal.at.line, refusal.at.column), (2, 2));
    assert_eq!(
        refusal.detail,
        "expected the references, a list, found an object"
    );
    // A value of the right type costs no allocation.
    let text = "\"x\"";
    let info = allocation_counter::measure(|| {
        let _ = expect(&mut reader(text), Kind::String, "the model", "a string");
    });
    assert_eq!(info.count_total, 0);
    // What is not a value at all is the cursor's refusal, not a wrong type.
    assert_eq!(
        code_and_place(expect(&mut reader("NaN"), Kind::Number, "x", "a number")),
        ("non-finite-number", 1, 1)
    );
    assert_eq!(
        code_and_place(expect(&mut reader(""), Kind::Number, "x", "a number")),
        ("json-syntax", 1, 1)
    );
}

#[test]
fn a_path_is_refused_with_its_reason_and_kept_otherwise() {
    assert_eq!(
        path(&mut reader("\"a/b.json\""), "the cxf path"),
        Ok("a/b.json".to_owned())
    );
    for (written, reason) in [
        (
            "\"a\\\\b\"",
            "the cxf path `a\\b` holds a backslash; segments are separated by `/` only",
        ),
        (
            "\"/a\"",
            "the cxf path `/a` is absolute; it must be relative to the spec file",
        ),
        ("\"\"", "the cxf path `` has an empty segment"),
        ("\"a//b\"", "the cxf path `a//b` has an empty segment"),
        ("\"./a\"", "the cxf path `./a` has a `.` segment"),
        (
            "\"a/../b\"",
            "the cxf path `a/../b` has a `..` segment; it must stay inside the spec file's directory",
        ),
    ] {
        let refusal = path(&mut reader(written), "the cxf path").expect_err("refused");
        assert_eq!(refusal.kind, SpecErrorKind::PathInvalid, "{written}");
        assert_eq!(refusal.detail, reason);
        assert_eq!((refusal.at.line, refusal.at.column), (1, 1));
    }
    assert_eq!(
        code_and_place(path(&mut reader(" 7"), "the cxf path")),
        ("wrong-type", 1, 2)
    );
}

#[test]
fn a_real_keeps_its_kind_and_an_integer_token_is_exact_or_refused() {
    let (number, at) = real(&mut reader(" 10"), "a tolerance").expect("read");
    assert_eq!((number, at.column), (Number::Integer(10), 2));
    assert_eq!(
        real(&mut reader("1e1"), "a tolerance").map(|read| read.0),
        Ok(Number::Real(10.0))
    );
    assert_eq!(
        real(&mut reader("9007199254740992"), "x").map(|read| read.0),
        Ok(Number::Integer(1 << 53))
    );
    assert_eq!(
        real(&mut reader("-9007199254740992"), "x").map(|read| read.0),
        Ok(Number::Integer(-(1 << 53)))
    );
    for past in [
        "9007199254740993",
        "-9007199254740993",
        "9223372036854775807",
    ] {
        let refusal = real(&mut reader(past), "x").expect_err("refused");
        assert_eq!(refusal.kind, SpecErrorKind::IntegerOutOfRange, "{past}");
        assert_eq!(
            refusal.detail,
            "an integer beyond 9007199254740992 is not exact in binary64; write it as a real"
        );
    }
    assert_eq!(
        code_and_place(real(&mut reader("true"), "the sampling in seconds")),
        ("wrong-type", 1, 1)
    );
}

#[test]
fn a_literal_is_a_number_or_a_boolean_with_the_kind_it_was_written_as() {
    for (written, expected) in [
        ("1", Literal::Integer(1)),
        ("-0", Literal::Integer(0)),
        ("1.0", Literal::Real(1.0)),
        ("-0.0", Literal::Real(-0.0)),
        ("true", Literal::Boolean(true)),
        ("false", Literal::Boolean(false)),
        ("9223372036854775807", Literal::Integer(i64::MAX)),
    ] {
        assert_eq!(literal(&mut reader(written)), Ok(expected), "{written}");
    }
    for wrong in ["\"1\"", "null", "[]", "{}"] {
        let refusal = literal(&mut reader(wrong)).expect_err("refused");
        assert_eq!(refusal.kind, SpecErrorKind::WrongType, "{wrong}");
        assert!(
            refusal
                .detail
                .starts_with("expected a value (a number or a boolean), found ")
        );
    }
}

#[test]
fn an_ignored_value_is_kept_whole_with_sorted_keys_and_number_kinds() {
    let value = opaque(&mut reader(
        r#"{"b": [1, 1.0, -0, -0.0, 1e16, true, false, null, "sé"], "a": {"z": {}, "y": []}}"#,
    ))
    .expect("read");
    assert_eq!(
        value.to_document(),
        "{\n  \"a\": {\n    \"y\": [],\n    \"z\": {}\n  },\n  \"b\": [\n    1,\n    1.0,\n    0,\n    -0.0,\n    1e16,\n    true,\n    false,\n    null,\n    \"s\\u00e9\"\n  ]\n}\n"
    );
    for (written, expected) in [
        ("null", "null\n"),
        ("false", "false\n"),
        ("\"\"", "\"\"\n"),
        ("[]", "[]\n"),
        ("{}", "{}\n"),
        ("9223372036854775807", "9223372036854775807\n"),
    ] {
        assert_eq!(
            opaque(&mut reader(written)).expect("read").to_document(),
            expected
        );
    }
    // Still strict: a repeated key at any depth, and whatever the cursor refuses.
    assert_eq!(
        code_and_place(opaque(&mut reader(r#"{"a": 1, "a": 2}"#))),
        ("duplicate-key", 1, 10)
    );
    assert_eq!(
        code_and_place(opaque(&mut reader(r#"[{"k": {"a": 1, "a": 2}}]"#))),
        ("duplicate-key", 1, 17)
    );
    assert_eq!(
        code_and_place(opaque(&mut reader("[1,]"))),
        ("json-syntax", 1, 4)
    );
    assert_eq!(
        code_and_place(opaque(&mut reader("nul"))),
        ("json-syntax", 1, 1)
    );
    assert_eq!(
        code_and_place(opaque(&mut reader(&"[".repeat(9)))),
        ("nesting-too-deep", 1, 9)
    );
}

#[test]
fn a_keyed_map_keeps_file_order_and_refuses_the_entry_past_its_bound_before_reading_it() {
    let bound = |limit| KeyBound {
        kind: SpecErrorKind::TooManyKeys,
        limit,
        what: "keys in one outputs map",
    };
    let numbers = |cursor: &mut Reader<'_>| real(cursor, "a number").map(|read| read.0);
    let read = keyed(
        &mut reader(r#"{"b": 1, "a": 2, "c": 3}"#),
        "the map",
        bound(3),
        numbers,
    )
    .expect("read");
    let keys: Vec<&str> = read.iter().map(|(key, _)| key.as_str()).collect();
    assert_eq!(keys, ["b", "a", "c"], "file order, not sorted");
    assert_eq!(
        keyed(&mut reader("{}"), "the map", bound(0), numbers).map(|entries| entries.len()),
        Ok(0)
    );
    // One past the bound: refused at that key, and its value, here no number at all, is not read.
    let refusal = keyed(
        &mut reader(r#"{"b": 1, "a": 2, "c": "x"}"#),
        "the map",
        bound(2),
        numbers,
    )
    .expect_err("refused");
    assert_eq!(refusal.kind, SpecErrorKind::TooManyKeys);
    assert_eq!((refusal.at.line, refusal.at.column), (1, 18));
    assert_eq!(refusal.detail, "more than 2 keys in one outputs map");
    assert_eq!(
        code_and_place(keyed(
            &mut reader(r#"{"a": 1, "": 2}"#),
            "the map",
            bound(9),
            numbers
        )),
        ("empty-name", 1, 10)
    );
    assert_eq!(
        code_and_place(keyed(
            &mut reader(r#"{"a": 1, "b": 2, "a": 3}"#),
            "the map",
            bound(9),
            numbers
        )),
        ("duplicate-key", 1, 18)
    );
    let refusal = keyed(&mut reader("[]"), "the map", bound(9), numbers).expect_err("refused");
    assert_eq!(refusal.detail, "expected the map, an object, found a list");
}

#[test]
fn the_byte_stages_run_in_order_before_any_shape_is_read() {
    let limits = SpecLimits::MAX;
    let (text, tag) = prepared(" {\"schema\": \"x\"} // c", &limits).expect("prepared");
    assert_eq!(tag.as_deref(), Some("x"));
    assert_eq!(text, " {\"schema\": \"x\"}     ");
    assert_eq!(prepared("[1]", &limits).expect("prepared").1, None);
    assert_eq!(
        prepared("{\"schema\": 1}", &limits).expect("prepared").1,
        None
    );
    let code =
        |text: &str, limits: &SpecLimits| prepared(text, limits).expect_err("refused").code();
    assert_eq!(code("\u{feff}/*", &limits), "byte-order-mark");
    assert_eq!(code("/* [1,]", &limits), "unterminated-comment");
    assert_eq!(code("[1,] /* c */", &limits), "json-syntax");
    let two = SpecLimits {
        max_bytes: 2,
        ..limits
    };
    assert_eq!(
        code("\u{feff}", &two),
        "input-too-large",
        "three bytes of mark"
    );
    assert!(prepared("[]", &two).is_ok());
    assert_eq!(utf8(b"[]", &two), Ok("[]"));
    assert_eq!(
        utf8(b"\xff\xff\xff", &two).expect_err("refused").code(),
        "input-too-large"
    );
    assert_eq!(
        utf8(b"\xff\xff", &two).expect_err("refused").code(),
        "not-utf8"
    );
    assert_eq!(
        utf8(b"ab\xff", &limits).expect_err("refused").detail(),
        "byte 2 is not UTF-8"
    );
}
