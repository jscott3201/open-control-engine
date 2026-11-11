//! The v0 graph importer refuses hostile and out-of-shape input with its exact kind (importer
//! note, "The v0 graph JSON importer", "Refused"; "Test plan", hostile corpus): the wire form's
//! bounds pinned at the limit and one past it, duplicate keys, unknown and missing keys, a type
//! other than the three, a string, `null`, object or list value, and a malformed connection.

use crate::support;

use oce_author::{
    ImportError, ImportErrorKind, ImportOptions, Imported, Limit, Literal, MAX_WIRE_BYTES,
    MAX_WIRE_DEPTH, MAX_WIRE_LIST_LEN, MAX_WIRE_STRING_BYTES, ParamValue, SourceForm, StrictJson,
    import_graph_v0,
};

fn import(bytes: &[u8]) -> Result<Imported, ImportError> {
    import_graph_v0(bytes, &ImportOptions::new())
}

/// The refusal of `text` as `(code, subject)`, or `("imported", "")`.
fn refusal(text: &str) -> (&'static str, String) {
    match import(text.as_bytes()) {
        Err(error) => {
            assert_eq!(error.source, SourceForm::GraphV0);
            (error.code(), error.subject)
        }
        Ok(_) => ("imported", String::new()),
    }
}

/// A small valid document; `{block}` and `{param}` are spliced per case.
fn document(param_value: &str) -> String {
    format!(
        "{{\"name\": \"s\", \"inputs\": [{{\"name\": \"u\", \"type\": \"real\", \"unit\": \"1\", \"description\": \"in\"}}], \"outputs\": [{{\"name\": \"y\", \"type\": \"real\", \"description\": \"out\"}}], \"blocks\": [{{\"id\": \"g\", \"class\": \"Reals.MultiplyByParameter\", \"params\": [{{\"name\": \"k\", \"value\": {param_value}}}]}}], \"connections\": [\"u -> g.u\", \"g.y -> y\"]}}"
    )
}

fn kind(text: &str) -> ImportErrorKind {
    import(text.as_bytes())
        .map(|_| ())
        .expect_err("refused")
        .kind
}

#[test]
fn the_small_document_imports() {
    let sequence = import(document("2").as_bytes()).expect("imports").sequence;
    assert_eq!(
        sequence.blocks[0].assignments[0].value,
        ParamValue::Literal(Literal::Integer(2))
    );
    assert_eq!(
        sequence.blocks[0].class_path,
        "CDL.Reals.MultiplyByParameter"
    );
}

#[test]
fn number_tokens_keep_their_kind() {
    for (token, literal) in [
        ("2", Literal::Integer(2)),
        ("-0", Literal::Integer(0)),
        ("2.0", Literal::Real(2.0)),
        ("-0.0", Literal::Real(-0.0)),
        ("2e3", Literal::Real(2000.0)),
        ("-9223372036854775808", Literal::Integer(i64::MIN)),
        ("true", Literal::Boolean(true)),
    ] {
        let sequence = import(document(token).as_bytes()).expect(token).sequence;
        assert_eq!(
            sequence.blocks[0].assignments[0].value,
            ParamValue::Literal(literal),
            "{token}"
        );
    }
}

#[test]
fn a_parameter_value_that_is_not_a_number_or_a_boolean_is_refused() {
    assert_eq!(
        refusal(&document("\"PI\"")),
        ("string-value", "blocks[0].params[0]".to_owned())
    );
    for value in ["null", "{}", "[1]", "{\"member\": \"SimpleController.PI\"}"] {
        assert_eq!(
            kind(&document(value)),
            ImportErrorKind::WrongShape {
                key: "value".to_owned()
            },
            "{value}"
        );
    }
}

#[test]
fn a_type_other_than_bool_real_and_int_is_refused() {
    for spelled in ["boolean", "Real", "integer", "string", ""] {
        let text =
            document("2").replacen("\"type\": \"real\"", &format!("\"type\": \"{spelled}\""), 1);
        assert_eq!(
            refusal(&text),
            ("unknown-datatype", "inputs[0]".to_owned()),
            "{spelled}"
        );
    }
    let text = document("2").replacen("\"type\": \"real\"", "\"type\": 1", 1);
    assert_eq!(
        kind(&text),
        ImportErrorKind::WrongShape {
            key: "type".to_owned()
        }
    );
}

#[test]
fn unknown_and_missing_keys_are_refused_where_they_sit() {
    let base = document("2");
    let cases: &[(&str, &str, &str, &str, &str)] = &[
        (
            "{\"name\"",
            "{\"extra\": 1, \"name\"",
            "unknown-key",
            "extra",
            "document",
        ),
        (
            "\"description\": \"in\"}",
            "\"description\": \"in\", \"semantic\": {}}",
            "unknown-key",
            "semantic",
            "inputs[0]",
        ),
        (
            "\"description\": \"out\"}",
            "\"description\": \"out\", \"unit\": \"1\"}",
            "unknown-key",
            "unit",
            "outputs[0]",
        ),
        (
            "\"params\": [",
            "\"label\": \"x\", \"params\": [",
            "unknown-key",
            "label",
            "blocks[0]",
        ),
        (
            "{\"name\": \"k\",",
            "{\"name\": \"k\", \"unit\": \"s\",",
            "unknown-key",
            "unit",
            "blocks[0].params[0]",
        ),
        ("\"unit\": \"1\", ", "", "missing-key", "unit", "inputs[0]"),
        (
            ", \"connections\": [\"u -> g.u\", \"g.y -> y\"]",
            "",
            "missing-key",
            "connections",
            "document",
        ),
        ("\"id\": \"g\", ", "", "missing-key", "id", "blocks[0]"),
    ];
    for (from, to, code, key, subject) in cases {
        assert!(base.contains(from), "{from}");
        let error = import(base.replacen(from, to, 1).as_bytes())
            .map(|_| ())
            .expect_err("refused");
        let expected = if *code == "unknown-key" {
            ImportErrorKind::UnknownKey {
                key: (*key).to_owned(),
            }
        } else {
            ImportErrorKind::MissingKey {
                key: (*key).to_owned(),
            }
        };
        assert_eq!(
            (error.kind, error.subject.as_str()),
            (expected, *subject),
            "{to}"
        );
    }
}

#[test]
fn a_value_of_the_wrong_json_shape_is_refused() {
    let base = document("2");
    for (from, to, key) in [
        ("\"name\": \"s\"", "\"name\": 7", "name"),
        ("\"inputs\": [", "\"inputs\": {\"a\": [", "inputs"),
        (
            "\"connections\": [\"u -> g.u\"",
            "\"connections\": [7",
            "connections",
        ),
        (
            "\"class\": \"Reals.MultiplyByParameter\"",
            "\"class\": null",
            "class",
        ),
        (
            "\"description\": \"in\"",
            "\"description\": [\"in\"]",
            "description",
        ),
    ] {
        let text = base.replacen(from, to, 1);
        let text = if key == "inputs" {
            text.replacen("}], \"outputs\"", "}]}, \"outputs\"", 1)
        } else {
            text
        };
        assert_eq!(
            kind(&text),
            ImportErrorKind::WrongShape {
                key: key.to_owned()
            },
            "{to}"
        );
    }
    assert_eq!(
        kind("[]"),
        ImportErrorKind::WrongShape {
            key: "document".to_owned()
        }
    );
}

#[test]
fn a_connection_without_exactly_one_arrow_is_refused() {
    for spelled in ["u g.u", "u -> g.u -> y", "", "->->"] {
        let text = document("2").replacen("\"u -> g.u\"", &format!("\"{spelled}\""), 1);
        assert_eq!(
            refusal(&text),
            ("malformed-connection", "connections[0]".to_owned()),
            "{spelled}"
        );
    }
}

#[test]
fn the_strict_reader_refuses_what_serde_json_would_resolve() {
    let duplicate =
        document("2").replacen("\"name\": \"s\"", "\"name\": \"s\", \"name\": \"t\"", 1);
    assert_eq!(
        kind(&duplicate),
        ImportErrorKind::NotStrictJson(StrictJson::DuplicateKey {
            key: "name".to_owned()
        })
    );
    assert_eq!(
        kind(&document("9223372036854775808")),
        ImportErrorKind::NotStrictJson(StrictJson::IntegerOutOfRange {
            token: "9223372036854775808".to_owned()
        })
    );
    for text in [
        document("1e999"),
        document("NaN"),
        "{".to_owned(),
        String::new(),
    ] {
        assert!(
            matches!(
                kind(&text),
                ImportErrorKind::NotStrictJson(StrictJson::Syntax { .. })
            ),
            "{text}"
        );
    }
    let mut bytes = document("2").into_bytes();
    bytes[10] = 0xff;
    assert_eq!(
        import(&bytes).map(|_| ()).expect_err("refused").kind,
        ImportErrorKind::NotStrictJson(StrictJson::NotUtf8 { offset: 10 })
    );
}

#[test]
fn the_size_bound_is_checked_before_parsing() {
    let over = vec![b' '; MAX_WIRE_BYTES + 1];
    assert_eq!(
        import(&over).map(|_| ()).expect_err("refused").kind,
        ImportErrorKind::TooLarge {
            bytes: MAX_WIRE_BYTES + 1,
            limit: MAX_WIRE_BYTES
        }
    );
    assert!(matches!(
        import(&vec![b' '; MAX_WIRE_BYTES])
            .map(|_| ())
            .expect_err("refused")
            .kind,
        ImportErrorKind::NotStrictJson(_)
    ));
}

#[test]
fn nesting_is_bounded_at_the_wire_depth() {
    // The parameter object sits at depth 5; a value of `k` nested lists reaches depth 5 + k.
    let nested = |k: usize| format!("{}1{}", "[".repeat(k), "]".repeat(k));
    let at_limit = document(&nested(MAX_WIRE_DEPTH - 5));
    assert_eq!(
        kind(&at_limit),
        ImportErrorKind::WrongShape {
            key: "value".to_owned()
        }
    );
    for k in [MAX_WIRE_DEPTH - 4, 200, 100_000] {
        assert_eq!(
            kind(&document(&nested(k))),
            ImportErrorKind::LimitExceeded(Limit::Depth {
                limit: MAX_WIRE_DEPTH
            }),
            "{k}"
        );
    }
}

#[test]
fn a_list_is_bounded_at_the_wire_list_length() {
    let blocks = |n: usize| {
        let one = "{\"id\": \"b\", \"class\": \"Logical.Not\", \"params\": []}";
        let all = vec![one; n].join(",");
        document("2").replacen("\"blocks\": [", &format!("\"blocks\": [{all}, "), 1)
    };
    // 65,535 copies plus the document's own block: exactly the limit, read (and carried, with
    // its duplicate names, for the validator).
    let read = import(blocks(MAX_WIRE_LIST_LEN - 1).as_bytes()).expect("at the limit");
    assert_eq!(read.sequence.blocks.len(), MAX_WIRE_LIST_LEN);
    assert_eq!(
        kind(&blocks(MAX_WIRE_LIST_LEN)),
        ImportErrorKind::LimitExceeded(Limit::ListLength {
            limit: MAX_WIRE_LIST_LEN
        })
    );
}

#[test]
fn a_string_is_bounded_at_the_wire_string_length() {
    let with = |n: usize| {
        document("2").replacen(
            "\"description\": \"in\"",
            &format!("\"description\": \"{}\"", "d".repeat(n)),
            1,
        )
    };
    assert!(import(with(MAX_WIRE_STRING_BYTES).as_bytes()).is_ok());
    assert_eq!(
        kind(&with(MAX_WIRE_STRING_BYTES + 1)),
        ImportErrorKind::LimitExceeded(Limit::StringLength {
            limit: MAX_WIRE_STRING_BYTES
        })
    );
    // Measured after unescaping: 4,096 escaped bytes are 2,048 decoded ones.
    let escaped = document("2").replacen(
        "\"description\": \"in\"",
        &format!("\"description\": \"{}\"", "\\n".repeat(2_048)),
        1,
    );
    assert!(import(escaped.as_bytes()).is_ok());
    // A key is a string too.
    let long_key = document("2").replacen(
        "{\"name\"",
        &format!("{{\"{}\": 1, \"name\"", "k".repeat(4_097)),
        1,
    );
    assert_eq!(
        kind(&long_key),
        ImportErrorKind::LimitExceeded(Limit::StringLength {
            limit: MAX_WIRE_STRING_BYTES
        })
    );
}

#[test]
fn names_and_classes_are_carried_unchecked_for_the_validator() {
    let text = document("2")
        .replacen("\"name\": \"s\"", "\"name\": \"not a name\"", 1)
        .replacen("\"Reals.MultiplyByParameter\"", "\"No.Such.Class\"", 1);
    let sequence = import(text.as_bytes()).expect("imports").sequence;
    assert_eq!(sequence.name.as_str(), "not a name");
    assert_eq!(sequence.blocks[0].class_path, "CDL.No.Such.Class");
    let codes: Vec<&str> = oce_author::validate_author_side(&sequence)
        .iter()
        .map(|f| f.code)
        .collect();
    assert!(
        codes.contains(&"invalid-name") && codes.contains(&"class-not-found"),
        "{codes:?}"
    );
}
