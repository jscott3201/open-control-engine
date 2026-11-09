//! Wire-form refusals: closed objects (unknown key, missing key, wrong type) and the hostile
//! corpus (oversized document, deep nesting, huge lists, long strings, non-UTF-8, a byte-order
//! mark, duplicate keys, non-finite numbers, control characters), each a typed `WireError` and
//! never a panic. Every bound is pinned at its boundary: accepted at the limit, refused one past.

mod support;

use oce_author::{
    Location, MAX_WIRE_BYTES, MAX_WIRE_DEPTH, MAX_WIRE_LIST_LEN, MAX_WIRE_STRING_BYTES, WireError,
    from_wire_bytes, from_wire_str, to_wire_string,
};

use support::econ_enable;

fn canonical() -> String {
    to_wire_string(&econ_enable())
}

/// A minimal valid document with `connections` replaced by `connections`.
fn with_connections(connections: &str) -> String {
    format!(
        r#"{{"sequence":"s","extends":"","params":[],"inputs":[],"outputs":[],"blocks":[],"connections":{connections}}}"#
    )
}

/// A minimal valid document whose one block has `value` as its one assignment's value.
fn with_value(value: &str) -> String {
    format!(
        r#"{{"sequence":"s","extends":"","params":[],"inputs":[],"outputs":[],"blocks":[{{"name":"b","class":"CDL.Reals.Sources.Constant","params":[{{"name":"k","value":{value}}}]}}],"connections":[]}}"#
    )
}

#[test]
fn an_unknown_key_is_refused_with_its_name() {
    let text = canonical().replacen(
        "\"extends\": \"\",",
        "\"extends\": \"\",\n  \"comment\": \"x\",",
        1,
    );
    assert!(matches!(
        from_wire_str(&text),
        Err(WireError::UnknownKey { ref key, .. }) if key == "comment"
    ));
    let nested = canonical().replacen(
        "\"provisional\": false",
        "\"provisional\": false, \"tags\": []",
        1,
    );
    assert!(matches!(
        from_wire_str(&nested),
        Err(WireError::UnknownKey { ref key, .. }) if key == "tags"
    ));
    let value = with_value(r#"{"ref":"x","why":1}"#);
    assert!(matches!(
        from_wire_str(&value),
        Err(WireError::UnknownKey { ref key, .. }) if key == "why"
    ));
    let tagged = with_value(r#"{"reference":"x"}"#);
    assert!(matches!(
        from_wire_str(&tagged),
        Err(WireError::UnknownKey { ref key, .. }) if key == "reference"
    ));
}

#[test]
fn a_missing_key_is_refused_with_its_name() {
    for key in [
        "sequence",
        "extends",
        "params",
        "inputs",
        "outputs",
        "blocks",
        "connections",
    ] {
        let mut value: serde_json::Value = serde_json::from_str(&canonical()).expect("json");
        value.as_object_mut().expect("object").remove(key);
        let text = value.to_string();
        assert!(
            matches!(from_wire_str(&text), Err(WireError::MissingKey { key: ref k, .. }) if k == key),
            "{key}: {:?}",
            from_wire_str(&text)
        );
    }
    let semantic = canonical().replacen("\"provisional\": false", "\"provisional_\": false", 1);
    assert!(matches!(
        from_wire_str(&semantic),
        Err(WireError::UnknownKey { .. })
    ));
    let mut value: serde_json::Value = serde_json::from_str(&canonical()).expect("json");
    value["inputs"][0]["semantic"]
        .as_object_mut()
        .expect("semantic")
        .remove("haystack");
    assert!(matches!(
        from_wire_str(&value.to_string()),
        Err(WireError::MissingKey { ref key, .. }) if key == "haystack"
    ));
}

#[test]
fn a_wrong_type_is_refused() {
    for (from, to) in [
        ("\"sequence\": \"econ_enable\"", "\"sequence\": 7"),
        ("\"provisional\": false", "\"provisional\": \"no\""),
        // A boundary type is one of the three scalars; a sequence parameter's type is any string
        // (an enumeration type name the validator resolves), but a string.
        ("\"type\": \"Boolean\"", "\"type\": \"Float\""),
        ("\"type\": \"Real\"", "\"type\": 7"),
        ("\"haystack\": []", "\"haystack\": \"air\""),
        ("\"connections\": [", "\"connections\": [1, "),
    ] {
        let text = canonical().replacen(from, to, 1);
        assert!(
            matches!(from_wire_str(&text), Err(WireError::WrongType { .. })),
            "{to}: {:?}",
            from_wire_str(&text)
        );
    }
    for value in [
        "null",
        "[1]",
        "{}",
        r#"{"ref":1}"#,
        r#"{"ref":"a","member":"b"}"#,
    ] {
        let result = from_wire_str(&with_value(value));
        assert!(
            matches!(
                result,
                Err(WireError::WrongType { .. } | WireError::UnknownKey { .. })
            ),
            "{value}: {result:?}"
        );
    }
    let reference_in_sequence_param = r#"{"sequence":"s","extends":"","params":[{"name":"p","type":"Real","value":{"ref":"q"}}],"inputs":[],"outputs":[],"blocks":[],"connections":[]}"#;
    assert!(matches!(
        from_wire_str(reference_in_sequence_param),
        Err(WireError::WrongType { .. })
    ));
}

#[test]
fn the_five_value_shapes_read_as_tagged_values() {
    use oce_author::{Literal, ParamValue};
    let read = |value: &str| {
        from_wire_str(&with_value(value)).expect("reads").blocks[0].assignments[0]
            .value
            .clone()
    };
    assert_eq!(read("2"), ParamValue::Literal(Literal::Integer(2)));
    assert_eq!(read("2.0"), ParamValue::Literal(Literal::Real(2.0)));
    assert_eq!(read("2e0"), ParamValue::Literal(Literal::Real(2.0)));
    assert_eq!(read("true"), ParamValue::Literal(Literal::Boolean(true)));
    assert_eq!(
        read("\"PI\""),
        ParamValue::Literal(Literal::Text("PI".to_owned()))
    );
    assert_eq!(read(r#"{"ref":"p"}"#), ParamValue::Ref(support::id("p")));
    assert_eq!(
        read(r#"{"member":"SimpleController.PI"}"#),
        ParamValue::Literal(Literal::Member(oce_author::MemberRef::parse(
            "SimpleController.PI"
        )))
    );
}

#[test]
fn a_connection_without_exactly_one_arrow_is_refused() {
    for text in ["a.y b.u", "a.y -> b.u -> c.u", ""] {
        let result = from_wire_str(&with_connections(&format!("[{}]", serde_json::json!(text))));
        assert_eq!(
            result,
            Err(WireError::MalformedConnection {
                index: 0,
                text: text.to_owned()
            })
        );
    }
}

#[test]
fn the_document_size_is_bounded_at_four_mebibytes() {
    let base = with_connections("[]");
    let pad = |total: usize| format!("{base}{}", " ".repeat(total - base.len()));
    assert!(
        from_wire_str(&pad(MAX_WIRE_BYTES)).is_ok(),
        "accepted at the limit"
    );
    assert_eq!(
        from_wire_str(&pad(MAX_WIRE_BYTES + 1)),
        Err(WireError::TooLarge {
            bytes: MAX_WIRE_BYTES + 1,
            limit: MAX_WIRE_BYTES
        })
    );
    assert!(matches!(
        from_wire_bytes(pad(MAX_WIRE_BYTES + 1).as_bytes()),
        Err(WireError::TooLarge { .. })
    ));
}

#[test]
fn nesting_is_bounded() {
    // The root is level 1 and `connections` level 2; the extra arrays take the depth to the
    // limit, which is accepted by the bound and refused by the typed parse as a wrong type.
    let nest =
        |levels: usize| with_connections(&format!("{}{}", "[".repeat(levels), "]".repeat(levels)));
    let at_limit = nest(MAX_WIRE_DEPTH - 1);
    assert!(
        matches!(from_wire_str(&at_limit), Err(WireError::WrongType { .. })),
        "{:?}",
        from_wire_str(&at_limit)
    );
    assert!(matches!(
        from_wire_str(&nest(MAX_WIRE_DEPTH)),
        Err(WireError::TooDeep {
            limit: MAX_WIRE_DEPTH,
            ..
        })
    ));
    let hostile = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
    assert!(matches!(
        from_wire_str(&hostile),
        Err(WireError::TooDeep { .. })
    ));
    let objects = "{\"a\":".repeat(50_000);
    assert!(matches!(
        from_wire_str(&objects),
        Err(WireError::TooDeep { .. })
    ));
}

#[test]
fn list_lengths_are_bounded() {
    let list = |len: usize| {
        let items = vec!["\"a -> b\""; len].join(",");
        with_connections(&format!("[{items}]"))
    };
    assert_eq!(
        from_wire_str(&list(MAX_WIRE_LIST_LEN))
            .expect("accepted at the limit")
            .connections
            .len(),
        MAX_WIRE_LIST_LEN
    );
    assert!(matches!(
        from_wire_str(&list(MAX_WIRE_LIST_LEN + 1)),
        Err(WireError::ListTooLong {
            limit: MAX_WIRE_LIST_LEN,
            at: Location { line: 1, .. }
        })
    ));
}

#[test]
fn string_lengths_are_bounded() {
    let named = |len: usize| {
        format!(
            r#"{{"sequence":"{}","extends":"","params":[],"inputs":[],"outputs":[],"blocks":[],"connections":[]}}"#,
            "a".repeat(len)
        )
    };
    assert_eq!(
        from_wire_str(&named(MAX_WIRE_STRING_BYTES))
            .expect("accepted at the limit")
            .name
            .as_str()
            .len(),
        MAX_WIRE_STRING_BYTES
    );
    assert!(matches!(
        from_wire_str(&named(MAX_WIRE_STRING_BYTES + 1)),
        Err(WireError::StringTooLong {
            limit: MAX_WIRE_STRING_BYTES,
            ..
        })
    ));
}

#[test]
fn non_utf8_and_a_byte_order_mark_are_refused() {
    let mut bytes = canonical().into_bytes();
    bytes[20] = 0xFF;
    assert_eq!(
        from_wire_bytes(&bytes),
        Err(WireError::NotUtf8 { offset: 20 })
    );
    assert_eq!(
        from_wire_bytes(b"\xC0\x80"),
        Err(WireError::NotUtf8 { offset: 0 })
    );
    let bom = format!("\u{feff}{}", canonical());
    assert_eq!(from_wire_str(&bom), Err(WireError::ByteOrderMark));
    assert_eq!(
        from_wire_bytes(bom.as_bytes()),
        Err(WireError::ByteOrderMark)
    );
    assert_eq!(from_wire_bytes(canonical().as_bytes()), Ok(econ_enable()));
}

#[test]
fn duplicate_keys_are_refused_at_every_level() {
    let root = canonical().replacen(
        "\"extends\": \"\",",
        "\"extends\": \"\",\n  \"extends\": \"\",",
        1,
    );
    assert!(matches!(
        from_wire_str(&root),
        Err(WireError::DuplicateKey { ref key, at: Location { line: 4, .. } }) if key == "extends"
    ));
    let nested = canonical().replacen("\"unit\": \"K\",", "\"unit\": \"K\", \"unit\": \"Pa\",", 1);
    assert!(matches!(
        from_wire_str(&nested),
        Err(WireError::DuplicateKey { ref key, .. }) if key == "unit"
    ));
    let value = with_value(r#"{"ref":"a","ref":"b"}"#);
    assert!(matches!(
        from_wire_str(&value),
        Err(WireError::DuplicateKey { ref key, .. }) if key == "ref"
    ));
    // An escaped spelling of the same key is still the same field to the typed parse.
    let escaped = canonical().replacen(
        "\"extends\": \"\",",
        "\"extends\": \"\",\n  \"\\u0065xtends\": \"\",",
        1,
    );
    assert!(matches!(
        from_wire_str(&escaped),
        Err(WireError::DuplicateKey { .. })
    ));
}

#[test]
fn non_finite_numbers_are_refused() {
    for spelled in ["NaN", "Infinity", "-Infinity", "1e999", "-1e999", "1.0e400"] {
        let result = from_wire_str(&with_value(spelled));
        assert!(
            matches!(result, Err(WireError::NonFiniteNumber { .. })),
            "{spelled}: {result:?}"
        );
    }
}

#[test]
fn integers_outside_i64_are_refused_not_widened() {
    for spelled in [
        "9223372036854775808",
        "18446744073709551615",
        "18446744073709551616",
        "-9223372036854775809",
        "123456789012345678901234567890",
    ] {
        let result = from_wire_str(&with_value(spelled));
        assert!(
            matches!(result, Err(WireError::IntegerOutOfRange { .. })),
            "{spelled}: {result:?}"
        );
    }
}

#[test]
fn control_characters_in_strings_are_refused() {
    for spelled in [
        "\"a\\u0000b\"",
        "\"a\\tb\"",
        "\"a\\rb\"",
        "\"a\\u001fb\"",
        "\"a\\u007fb\"",
        "\"a\\u0085b\"",
        "\"a\u{7f}b\"",
        "\"a\u{85}b\"",
    ] {
        let result = from_wire_str(&with_value(spelled));
        assert!(
            matches!(result, Err(WireError::ControlCharacter { .. })),
            "{spelled:?}: {result:?}"
        );
    }
    let raw = with_value("\"a\u{1}b\"");
    assert!(matches!(
        from_wire_str(&raw),
        Err(WireError::ControlCharacter { .. })
    ));
    // A line feed is text, as the text form's `\n` escape allows.
    assert!(from_wire_str(&with_value("\"a\\nb\"")).is_ok());
}

#[test]
fn broken_syntax_is_a_typed_syntax_error() {
    for text in [
        "",
        "{",
        "{\"sequence\"",
        "[]",
        "{\"sequence\": \"s\",}",
        "nul",
        "\"\\uZZZZ\"",
        "-",
    ] {
        let result = from_wire_str(text);
        assert!(
            matches!(
                result,
                Err(WireError::Syntax { .. } | WireError::WrongType { .. })
            ),
            "{text:?}: {result:?}"
        );
    }
    let trailing = format!("{} x", canonical());
    assert!(matches!(
        from_wire_str(&trailing),
        Err(WireError::Syntax { .. })
    ));
}

#[test]
fn errors_display_their_position() {
    let error = from_wire_str(&with_value("NaN")).expect_err("refused");
    assert!(error.to_string().contains("line 1 column"), "{error}");
}

#[test]
fn a_mutation_sweep_over_the_every_element_bundle_never_panics() {
    // The bundle carries every wire construct, issue #492's enumeration types, bounds and
    // constant members included. Every byte is case-flipped (0x20), replaced by a digit and
    // truncated there (three mutants per byte keep the sweep near ten seconds in a debug build);
    // each mutant is a typed refusal or a model that writes back to bytes reading to the same
    // model.
    let bytes = std::fs::read(support::fixture("wire/every_element.json")).expect("bundle");
    let mut cases = 0usize;
    for at in 0..bytes.len() {
        let mut flipped = bytes.clone();
        flipped[at] ^= 0x20;
        let mut digit = bytes.clone();
        digit[at] = b'9';
        let mutants = [flipped, digit, bytes[..at].to_vec()];
        for mutant in mutants {
            let outcome = std::panic::catch_unwind(|| {
                if let Ok(model) = oce_author::from_wire_bytes(&mutant) {
                    let written = to_wire_string(&model);
                    if let Ok(again) = from_wire_str(&written) {
                        assert_eq!(again, model);
                    }
                }
            });
            assert!(outcome.is_ok(), "panic at byte {at}");
            cases += 1;
        }
    }
    assert!(cases > 5_000, "{cases}");
}
