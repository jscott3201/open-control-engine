//! Every [`oce_author::WireError`] code is produced by the wire reader (GitHub issue #495): one
//! small document per code, each asserted to give exactly that code and that location through the
//! public accessors alone, as a host reads them. The case table and [`oce_author::WIRE_CODES`]
//! must equal each other in order, so a new variant needs a document here before the suite
//! passes. (The in-crate test pins the table against the variants; this file can only reach them
//! through a wildcard arm.)

use std::collections::BTreeSet;

use oce_author::{
    Location, MAX_WIRE_BYTES, MAX_WIRE_DEPTH, MAX_WIRE_LIST_LEN, MAX_WIRE_STRING_BYTES, WIRE_CODES,
    WireError, from_wire_bytes, from_wire_str,
};

/// A one-line document with `connections` replaced by `connections`.
fn with_connections(connections: &str) -> String {
    format!(
        r#"{{"sequence":"s","derived_from":"","params":[],"inputs":[],"outputs":[],"blocks":[],"connections":{connections}}}"#
    )
}

/// A one-line document whose one block has `value` as its one assignment's value.
fn with_value(value: &str) -> String {
    format!(
        r#"{{"sequence":"s","derived_from":"","params":[],"inputs":[],"outputs":[],"blocks":[{{"name":"b","class":"CDL.Reals.Sources.Constant","params":[{{"name":"k","value":{value}}}]}}],"connections":[]}}"#
    )
}

/// A one-line document whose one input has `min` as its `min` list.
fn with_min(min: &str) -> String {
    format!(
        r#"{{"sequence":"s","derived_from":"","params":[],"inputs":[{{"name":"u","type":"Real","quantity":"","display_unit":"","min":{min},"max":[],"semantic":{{"name":"","label":"","unit":"","brick":"","s223":"","qudt":"","haystack":[],"provisional":false}}}}],"outputs":[],"blocks":[],"connections":[]}}"#
    )
}

/// The 1-based byte column of the first `needle` in the one-line `text`, plus `past`.
fn column_of(text: &str, needle: &str, past: usize) -> usize {
    text.find(needle).expect("the needle is in the document") + 1 + past
}

/// One refused document per code, in [`WIRE_CODES`] order: the code, the bytes, and whether the
/// refusal carries a location.
fn cases() -> Vec<(&'static str, Vec<u8>, bool)> {
    let text = |code: &'static str, document: String, located: bool| {
        (code, document.into_bytes(), located)
    };
    let mut not_utf8 = with_connections("[]").into_bytes();
    not_utf8[3] = 0xFF;
    let over_size = format!(
        "{}{}",
        with_connections("[]"),
        " ".repeat(MAX_WIRE_BYTES + 1)
    );
    let deep = with_connections(&format!(
        "{}{}",
        "[".repeat(MAX_WIRE_DEPTH),
        "]".repeat(MAX_WIRE_DEPTH)
    ));
    let long_list = with_connections(&format!(
        "[{}]",
        vec!["\"a -> b\""; MAX_WIRE_LIST_LEN + 1].join(",")
    ));
    let long_string = with_connections("[]").replacen(
        "\"s\"",
        &format!("\"{}\"", "a".repeat(MAX_WIRE_STRING_BYTES + 1)),
        1,
    );
    vec![
        ("not-utf8", not_utf8, false),
        text(
            "byte-order-mark",
            format!("\u{feff}{}", with_connections("[]")),
            false,
        ),
        text("input-too-large", over_size, false),
        text("nesting-too-deep", deep, true),
        text("list-too-long", long_list, true),
        text("string-too-long", long_string, true),
        text(
            "duplicate-key",
            with_connections("[]").replacen("{", "{\"sequence\":\"t\",", 1),
            true,
        ),
        text("non-finite-number", with_value("1e999"), true),
        text(
            "integer-out-of-range",
            with_value("9223372036854775808"),
            true,
        ),
        text("control-character", with_value("\"a\\tb\""), true),
        text(
            "unknown-key",
            with_connections("[]").replacen("{", "{\"comment\":\"x\",", 1),
            true,
        ),
        text(
            "missing-key",
            with_connections("[]").replacen("\"derived_from\":\"\",", "", 1),
            true,
        ),
        text("wrong-type", with_connections("{}"), true),
        text("json-syntax", "{".to_owned(), true),
        text("bound-list-too-long", with_min("[0, 1]"), true),
        text(
            "malformed-connection",
            with_connections("[\"a.y b.u\"]"),
            false,
        ),
    ]
}

fn refusal(bytes: &[u8]) -> WireError {
    from_wire_bytes(bytes).expect_err("the document is refused")
}

#[test]
fn every_code_is_produced_by_a_document_and_the_cases_are_the_table_in_order() {
    let mut produced = Vec::new();
    for (code, bytes, _) in cases() {
        assert_eq!(refusal(&bytes).code(), code);
        produced.push(code);
    }
    assert_eq!(produced, WIRE_CODES);
    let unique: BTreeSet<&str> = WIRE_CODES.iter().copied().collect();
    assert_eq!(unique.len(), WIRE_CODES.len(), "a code is listed twice");
}

#[test]
fn the_table_is_the_sixteen_published_codes() {
    assert_eq!(
        WIRE_CODES,
        [
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
        ]
    );
}

#[test]
fn a_refusal_has_a_location_exactly_when_its_message_names_one() {
    for (code, bytes, located) in cases() {
        let error = refusal(&bytes);
        let location = error.location();
        assert_eq!(location.is_some(), located, "{code}");
        let message = error.to_string();
        match location {
            Some(at) => {
                assert!(at.line >= 1 && at.column >= 1, "{code}: {at}");
                assert!(message.contains(&at.to_string()), "{code}: {message}");
            }
            None => assert!(!message.contains(" line "), "{code}: {message}"),
        }
    }
}

#[test]
fn the_location_is_the_exact_line_and_column_of_the_refused_token() {
    let nan = with_value("NaN");
    assert_eq!(
        refusal(nan.as_bytes()).location(),
        Some(Location::new(1, column_of(&nan, "NaN", 0)))
    );
    let integer = with_value("9223372036854775808");
    assert_eq!(
        refusal(integer.as_bytes()).location(),
        Some(Location::new(
            1,
            column_of(&integer, "9223372036854775808", 0)
        ))
    );
    // A refusal on a later line reports that line.
    let lines = with_value("\n\n  NaN");
    assert_eq!(
        refusal(lines.as_bytes()).location(),
        Some(Location::new(3, 3))
    );
    assert_eq!(refusal(b"{").location(), Some(Location::new(1, 1)));
    assert_eq!(refusal(b"").location(), Some(Location::new(1, 0)));
}

#[test]
fn the_bytes_and_the_text_readers_give_one_code_and_location() {
    for (code, bytes, _) in cases() {
        let Ok(text) = std::str::from_utf8(&bytes) else {
            assert_eq!(code, "not-utf8");
            continue;
        };
        let from_bytes = refusal(&bytes);
        let from_text = from_wire_str(text).expect_err("the document is refused");
        assert_eq!(from_bytes, from_text, "{code}");
        assert_eq!(from_text.code(), code);
        assert_eq!(from_bytes.location(), from_text.location(), "{code}");
        // Reading again gives the same refusal: nothing in the code or location depends on a
        // previous read.
        assert_eq!(refusal(&bytes), from_bytes, "{code}");
    }
}

#[test]
fn the_first_refusal_of_a_document_with_two_defects_keeps_its_own_code() {
    // Size is judged before UTF-8, and a byte-order mark before the scan.
    let mut large_and_invalid = vec![b' '; MAX_WIRE_BYTES + 1];
    large_and_invalid[0] = 0xFF;
    assert_eq!(refusal(&large_and_invalid).code(), "input-too-large");
    let marked_and_broken = "\u{feff}{";
    assert_eq!(
        refusal(marked_and_broken.as_bytes()).code(),
        "byte-order-mark"
    );
    // The empty document and a lone value are refusals with a code, never a panic.
    assert_eq!(refusal(b"").code(), "json-syntax");
    assert_eq!(refusal(b"[]").code(), "wrong-type");
    assert_eq!(refusal(b"null").code(), "wrong-type");
}
