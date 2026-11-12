//! Hostile input for the CXF importer: the panics the #473 review found (a multi-byte root
//! `@id`, an overflowing `sizeOfDimensions`), lexical traps `serde_json` would resolve silently
//! (a duplicate key, an integer past `i64`), structural traps (giant lists, a datatype that
//! disagrees with its value, an unchecked `@type`), and a bounded deterministic mutation sweep
//! over every document the importers read (`import_sweep.rs`). Every case returns a typed result
//! with its exact kind; none panics.

use crate::support;

use oce_author::{ImportError, ImportErrorKind, ImportOptions, Imported, StrictJson, import_cxf};

use support::fixture;

fn import(bytes: &[u8]) -> Result<Imported, ImportError> {
    import_cxf(bytes, &ImportOptions::new())
}

fn code(bytes: &[u8]) -> &'static str {
    match import(bytes) {
        Err(error) => error.code(),
        Ok(_) => "imported",
    }
}

fn econ() -> String {
    std::fs::read_to_string(fixture("emit/econ_enable.cxf.json")).expect("golden")
}

fn edited(from: &str, to: &str) -> Vec<u8> {
    let text = econ();
    assert!(text.contains(from), "{from}");
    text.replacen(from, to, 1).into_bytes()
}

/// Assert the strict reader refused `bytes` and its message mentions `detail`.
fn not_strict(bytes: &[u8], detail: &str) {
    match import(bytes).map(|_| ()) {
        Err(ImportError {
            kind: ImportErrorKind::NotStrictJson(why),
            ..
        }) => assert!(why.to_string().contains(detail), "{why}"),
        other => panic!("expected not-strict-json `{detail}`, got {other:?}"),
    }
}

#[test]
fn a_multi_byte_root_id_is_refused_without_panicking() {
    // The review's reproduction: the last delimiter search used to slice inside `é`.
    let bytes =
        br#"{"@graph":[{"@id":"urn:x:\u00e9","@type":"S231:Block","S231:containsBlock":[]}]}"#;
    let raw = "{\"@graph\":[{\"@id\":\"urn:x:\u{e9}\",\"@type\":\"S231:Block\",\"S231:containsBlock\":[]}]}";
    for doc in [&bytes[..], raw.as_bytes()] {
        assert_eq!(code(doc), "invalid-name");
    }
    // A multi-byte character at every position of the root `@id`.
    let root = "urn:oce:author:econ_enable";
    for at in 0..=root.len() {
        for ch in ['\u{e9}', '\u{20ac}', '\u{1f321}'] {
            let mut id = root.to_owned();
            id.insert(at, ch);
            let doc = econ().replace(&format!("\"{root}\""), &format!("\"{id}\""));
            // Either the name is no identifier, or the members are no longer under the root (so
            // `dTHys` names no sequence parameter and the precedence reports the expression).
            assert!(
                matches!(
                    code(doc.as_bytes()),
                    "invalid-name" | "unresolved-reference" | "expression-value"
                ),
                "{id}: {}",
                code(doc.as_bytes())
            );
        }
    }
}

#[test]
fn a_multi_byte_character_at_every_position_of_a_member_id_is_typed() {
    let member = "urn:oce:author:econ_enable.hys.uHigh";
    for at in 0..=member.len() {
        let mut id = member.to_owned();
        id.insert(at, '\u{e9}');
        let doc = econ().replace(&format!("\"{member}\""), &format!("\"{id}\""));
        // Both the relation and the node move together, so this may read; it must not panic.
        let _ = import(doc.as_bytes());
        let one = econ().replacen(&format!("\"{member}\""), &format!("\"{id}\""), 1);
        assert!(import(one.as_bytes()).is_err(), "{at}");
    }
}

/// The `uLow` value of the worked example's golden.
const LOW_VALUE: &str = "\"S231:value\": {\n        \"@value\": \"0.0\",\n        \"@type\": \"http://www.w3.org/2001/XMLSchema#double\"\n      }";

#[test]
fn hostile_array_sizes_are_refused_with_a_typed_error() {
    // Each refuses before any element is read; the first used to overflow `usize` (a panic in
    // debug, an empty array accepted in release).
    let cases = [
        ("(4294967296, 4294967296)", "2"),
        ("(18446744073709551615, 2)", "2"),
        ("(99999999999999999999999)", "1"),
        ("(0)", "1"),
        ("(-1)", "1"),
        ("(+2)", "1"),
        ("(2, +1)", "2"),
        ("(2, 0)", "2"),
        ("()", "0"),
        ("(1, 1, 1)", "3"),
        ("(2", "1"),
    ];
    for (size, rank) in cases {
        let text = String::from_utf8(econ().into_bytes()).expect("utf8");
        let node = "\"@id\": \"urn:oce:author:econ_enable.hys.uLow\",\n      \"@type\": \"S231:Parameter\",";
        assert!(text.contains(node));
        let doc = text.replacen(
            node,
            &format!(
                "{node}\n      \"S231:isArray\": true, \"S231:numberDimensions\": {rank}, \"S231:sizeOfDimensions\": \"{size}\","
            ),
            1,
        );
        // A list value, so the refusal is the size check itself, not "not a list".
        let doc = doc.replacen(LOW_VALUE, "\"S231:value\": [1.0, 2.0]", 1);
        assert_eq!(code(doc.as_bytes()), "array-shape", "{size}");
    }
}

#[test]
fn an_integer_beyond_i64_is_refused_not_guessed_as_a_real() {
    for token in ["9223372036854775808", "18446744073709551615"] {
        let doc = edited(
            "\"S231:value\": \"dTHys\"",
            &format!("\"S231:value\": {token}"),
        );
        assert_eq!(
            import(&doc).map(|_| ()).map_err(|e| e.kind),
            Err(ImportErrorKind::NotStrictJson(
                StrictJson::IntegerOutOfRange {
                    token: token.to_owned()
                }
            ))
        );
    }
    // `i64::MIN` and `i64::MAX` themselves read.
    for edge in ["-9223372036854775808", "9223372036854775807"] {
        let doc = edited(
            "\"S231:value\": \"dTHys\"",
            &format!("\"S231:value\": {edge}"),
        );
        assert!(import(&doc).is_ok(), "{edge}");
    }
}

#[test]
fn a_key_written_twice_is_refused() {
    for (from, to, key) in [
        (
            "\"S231:value\": \"dTHys\"",
            "\"S231:value\": 1.0, \"S231:value\": \"dTHys\"",
            "S231:value",
        ),
        (
            "\"@type\": \"S231:Block\",",
            "\"@type\": \"S231:Block\", \"@type\": \"S231:Block\",",
            "@type",
        ),
    ] {
        assert_eq!(
            import(&edited(from, to)).map(|_| ()).map_err(|e| e.kind),
            Err(ImportErrorKind::NotStrictJson(StrictJson::DuplicateKey {
                key: key.to_owned()
            }))
        );
    }
}

#[test]
fn a_value_that_disagrees_with_its_datatype_is_refused() {
    let low = "\"@id\": \"urn:oce:author:econ_enable.hys.uLow\",\n      \"@type\": \"S231:Parameter\",\n      \"S231:isOfDataType\": {\n        \"@id\": \"S231:Real\"";
    for (datatype, expected) in [
        ("S231:Boolean", "datatype-mismatch"),
        ("S231:String", "datatype-mismatch"),
        ("S231:Integer", "datatype-mismatch"),
        (
            "http://example.org#Buildings.Controls.OBC.CDL.Types.SimpleController",
            "datatype-mismatch",
        ),
        (
            "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.Types.OperationModes",
            "datatype-mismatch",
        ),
        (
            "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.Types.NoSuchType",
            "unknown-datatype",
        ),
        (
            "http://example.org#Modelica.Units.SI.Temperature",
            "unknown-datatype",
        ),
        ("S231:Bogus", "unknown-datatype"),
    ] {
        let to = low.replace("S231:Real", datatype);
        assert_eq!(code(&edited(low, &to)), expected, "{datatype}");
    }
    // A reference agrees with any datatype; a Real takes an Integer literal.
    let high = "\"@id\": \"urn:oce:author:econ_enable.hys.uHigh\",\n      \"@type\": \"S231:Parameter\",\n      \"S231:isOfDataType\": {\n        \"@id\": \"S231:Real\"";
    assert!(import(&edited(high, &high.replace("S231:Real", "S231:Boolean"))).is_ok());
}

#[test]
fn a_parameter_type_other_than_parameter_is_refused() {
    let at_type = ImportErrorKind::WrongShape {
        key: "@type".to_owned(),
    };
    for (from, to) in [
        (
            "\"@id\": \"urn:oce:author:econ_enable.hys.uLow\",\n      \"@type\": \"S231:Parameter\",",
            "\"@id\": \"urn:oce:author:econ_enable.hys.uLow\",\n      \"@type\": \"S231:Constant\",",
        ),
        (
            "\"@id\": \"urn:oce:author:econ_enable.dTHys\",\n      \"@type\": \"S231:Parameter\",",
            "\"@id\": \"urn:oce:author:econ_enable.dTHys\",\n      \"@type\": [\"S231:Parameter\"],",
        ),
        ("\"@type\": \"S231:Block\",", "\"@type\": [\"S231:Block\"],"),
    ] {
        assert_eq!(
            import(&edited(from, to)).map(|_| ()).map_err(|e| e.kind),
            Err(at_type.clone()),
            "{to}"
        );
    }
}

#[test]
fn giant_lists_are_refused_in_bounded_time() {
    let start = std::time::Instant::now();
    // 100,000 references to nodes that do not exist (about 5 MB, under the document bound).
    let refs: Vec<String> = (0..100_000)
        .map(|i| format!("{{\"@id\": \"urn:oce:author:econ_enable.p{i}\"}}"))
        .collect();
    let doc = edited(
        "\"S231:hasParameter\": [",
        &format!("\"S231:hasParameter\": [{},", refs.join(",")),
    );
    assert_eq!(code(&doc), "unresolved-reference");
    // A 1,000,000-element array value whose declared size is (2).
    let items = vec!["1.0"; 1_000_000].join(",");
    let node =
        "\"@id\": \"urn:oce:author:econ_enable.hys.uLow\",\n      \"@type\": \"S231:Parameter\",";
    let text = econ();
    let doc = text.replacen(
        node,
        &format!("{node}\n      \"S231:isArray\": true, \"S231:numberDimensions\": 1, \"S231:sizeOfDimensions\": \"(2)\","),
        1,
    );
    let doc = doc.replacen(LOW_VALUE, &format!("\"S231:value\": [{items}]"), 1);
    assert_eq!(code(doc.as_bytes()), "array-shape");
    assert!(start.elapsed() < std::time::Duration::from_secs(60));
}

#[test]
fn integer_tokens_outside_i64_are_refused_at_any_depth() {
    for token in [
        "-9223372036854775809",
        "9223372036854775808",
        "18446744073709551616",
        "-18446744073709551616",
        "123456789012345678901234567890",
        "-123456789012345678901234567890",
    ] {
        not_strict(
            &edited(
                "\"S231:value\": \"dTHys\"",
                &format!("\"S231:value\": {token}"),
            ),
            "outside the i64 range",
        );
        // Deep in an unread key and inside an array: still refused, lexically.
        not_strict(
            &edited(
                "\"@type\": \"S231:Block\",",
                &format!("\"@type\": \"S231:Block\", \"x:y\": [[{{\"z\": [{token}]}}]],"),
            ),
            "outside the i64 range",
        );
    }
    // A real-shaped token of any size is a Real, and a digit run inside a string is text.
    for real in ["1e400", "123456789012345678901234567890.0", "-1.5e-3"] {
        let doc = edited(
            "\"S231:value\": \"dTHys\"",
            &format!("\"S231:value\": {real}"),
        );
        assert!(
            !matches!(
                import(&doc),
                Err(ImportError {
                    kind: ImportErrorKind::NotStrictJson(StrictJson::IntegerOutOfRange { .. }),
                    ..
                })
            ),
            "{real}"
        );
    }
}

#[test]
fn negative_zero_reads_as_the_integer_zero_as_on_the_wire() {
    let doc = edited("\"S231:value\": \"dTHys\"", "\"S231:value\": -0");
    let sequence = import(&doc).expect("reads").sequence;
    let hys = sequence
        .blocks
        .iter()
        .find(|b| b.name.as_str() == "hys")
        .expect("hys");
    let high = hys
        .assignments
        .iter()
        .find(|a| a.name.as_str() == "uHigh")
        .expect("uHigh");
    assert_eq!(
        high.value,
        oce_author::ParamValue::Literal(oce_author::Literal::Integer(0))
    );
    // `-0.0` is the Real negative zero.
    let doc = edited("\"S231:value\": \"dTHys\"", "\"S231:value\": -0.0");
    let sequence = import(&doc).expect("reads").sequence;
    let value = &sequence.blocks[1].assignments[1].value;
    assert!(
        matches!(value, oce_author::ParamValue::Literal(oce_author::Literal::Real(v)) if v.to_bits() == (-0.0f64).to_bits()),
        "{value:?}"
    );
}

#[test]
fn nesting_is_pinned_at_the_recursion_limit() {
    // Total nesting depth = the top object plus `k` arrays. 127 levels parse (and are then
    // refused for their shape); 128 and 129 hit `serde_json`'s recursion limit. Neither panics.
    let nested = |depth: usize| {
        let k = depth - 1;
        format!("{{\"@graph\": {}{}}}", "[".repeat(k), "]".repeat(k))
    };
    assert_eq!(
        import(nested(127).as_bytes())
            .map(|_| ())
            .map_err(|e| e.kind),
        Err(ImportErrorKind::WrongShape {
            key: "@graph".to_owned()
        })
    );
    not_strict(nested(128).as_bytes(), "recursion limit exceeded");
    not_strict(nested(129).as_bytes(), "recursion limit exceeded");
}
