//! Every object of the spec is closed: an unknown key is refused at every level, a required key
//! cannot be absent, a value of another JSON type is refused, and any tag but
//! `oce/commissioning-spec/v1` is refused by name before the shape is judged. The four Scenario
//! 1 fields are the one place a value is not judged, and even there the text is strict JSON.

use crate::{code_of, fixture_text, load, loaded, one_reference, refusal, top_level};

/// A spec with every object the format has, so each can gain a key.
fn full() -> String {
    r#"{"schema": "oce/commissioning-spec/v1", "time": {"origin": "2018-06-07T00:00:00Z"},
 "tolerances": {"atolx": 1}, "sampling": 5, "modelJsonDirectory": "d", "controller": {"deviceId": 1},
 "references": [{"model": "M", "sequence": "s", "cxf": "s.json", "controllerOutput": "o.csv",
   "generateJson": false, "runController": false, "sampling": 1, "envelope": "funnel",
   "funnelReference": "engine", "window": {"start": 0, "stop": 1}, "parameters": {"k": 1},
   "outputs": {"s.y": {"atoly": 1}}, "indicators": {"s.y": ["s.u"]},
   "pointNameMapping": [{"cdl": {"name": "u", "unit": "K", "type": "float"},
                         "device": {"name": "U", "unit": "degF", "type": "float"}}],
   "inputs": [{"path": "i.csv", "format": "csv", "timeColumn": "t", "columns": {"a": "U"},
               "table": "tab", "hold": {"maxAge": 10}}],
   "expected": [], "indicatorSources": []}]}"#
        .to_owned()
}

/// `document` with `addition` written as the first member of the first object that starts with
/// `anchor`.
fn with_member(document: &str, anchor: &str, addition: &str) -> String {
    assert!(document.contains(anchor), "{anchor}");
    document.replacen(anchor, &format!("{{{addition}, {}", &anchor[1..]), 1)
}

#[test]
fn the_full_document_loads() {
    let spec = loaded(&full());
    assert_eq!(spec.references().len(), 1);
    assert_eq!(spec.findings().len(), 4);
}

#[test]
fn an_unknown_key_is_refused_in_every_object() {
    // (the object's first bytes, a key that object does not define)
    let objects = [
        ("{\"schema\"", "version"),
        ("{\"origin\"", "zone"),
        ("{\"atolx\"", "variable"),
        ("{\"model\"", "tolerances"),
        ("{\"start\"", "step"),
        ("{\"atoly\"", "atol"),
        ("{\"cdl\"", "comment"),
        ("{\"name\": \"u\"", "description"),
        ("{\"name\": \"U\"", "address"),
        ("{\"path\"", "delimiter"),
        ("{\"maxAge\"", "policy"),
    ];
    for (anchor, key) in objects {
        let document = with_member(&full(), anchor, &format!("\"{key}\": 1"));
        let refused = load(&document).expect_err("refused");
        assert_eq!(refused.code(), "unknown-key", "{key} in {anchor}");
        assert_eq!(refused.detail(), format!("unknown key `{key}`"));
        // The place is the key itself.
        let at = refused.at().expect("a place");
        let line = document
            .lines()
            .nth(usize::try_from(at.line).expect("fits") - 1)
            .expect("the line");
        let column = usize::try_from(at.column).expect("fits") - 1;
        assert!(line[column..].starts_with(&format!("\"{key}\"")), "{line}");
    }
    // The same member is refused whatever its value: nothing unknown is skipped.
    for value in ["null", "[]", "{}", "\"x\"", "true", "[[[1]]]"] {
        let document = with_member(&full(), "{\"model\"", &format!("\"extra\": {value}"));
        assert_eq!(code_of(&document), "unknown-key", "{value}");
    }
    // Keys are compared exactly: case and surrounding space matter.
    for near in [
        "Schema",
        "schema ",
        "REFERENCES",
        "Tolerances",
        "generatejson",
    ] {
        let document = with_member(&full(), "{\"schema\"", &format!("\"{near}\": 1"));
        assert_eq!(code_of(&document), "unknown-key", "{near}");
    }
    for near in [
        "Model",
        "pointnamemapping",
        "PointNameMapping",
        "runcontroller",
        "CXF",
    ] {
        let document = with_member(&full(), "{\"model\"", &format!("\"{near}\": 1"));
        assert_eq!(code_of(&document), "unknown-key", "{near}");
    }
}

#[test]
fn the_keys_the_format_does_not_have_are_refused_never_ignored() {
    // `data` is the caller's option, not a spec field (decision 41); a per-reference
    // `tolerances` and its `variable` are Listing 12.3's spelling, which this format does not
    // read; a vector set's keys belong to another format.
    for (anchor, member) in [
        ("{\"model\"", r#""data": "synthetic""#),
        ("{\"schema\"", r#""data": "trended""#),
        ("{\"model\"", r#""tolerances": {"atoly": 0.5}"#),
        ("{\"atolx\"", r#""variable": "TSup*""#),
        (
            "{\"schema\"",
            r#""clock": {"step_s": 60, "horizon_s": 1800}"#,
        ),
        ("{\"schema\"", r#""scenarios": []"#),
        ("{\"model\"", r#""within": {"atoly": 0.5}"#),
        (
            "{\"model\"",
            r#""time": {"origin": "2018-06-07T00:00:00Z"}"#,
        ),
        ("{\"schema\"", r#""window": {"start": 0, "stop": 1}"#),
        ("{\"schema\"", r#""generateJson": true"#),
        ("{\"model\"", r#""modelJsonDirectory": "d""#),
        ("{\"model\"", r#""controller": {}"#),
        ("{\"path\"", r#""maxAge": 10"#),
    ] {
        let document = with_member(&full(), anchor, member);
        assert_eq!(code_of(&document), "unknown-key", "{member}");
    }
    assert_eq!(code_of(&full()), "loaded");
}

#[test]
fn a_key_written_twice_is_refused_in_every_object() {
    for (anchor, member) in [
        ("{\"schema\"", r#""sampling": 5"#),
        ("{\"origin\"", r#""origin": "2018-06-07T00:00:00Z""#),
        ("{\"atolx\"", r#""atolx": 1"#),
        ("{\"model\"", r#""sequence": "s""#),
        ("{\"model\"", r#""generateJson": false"#),
        ("{\"start\"", r#""stop": 1"#),
        ("{\"cdl\"", r#""device": {"name": "V", "type": "int"}"#),
        ("{\"name\": \"u\"", r#""type": "float""#),
        ("{\"path\"", r#""format": "csv""#),
        ("{\"maxAge\"", r#""maxAge": 10"#),
        ("{\"k\"", r#""k": 1"#),
        ("{\"s.y\": {", r#""s.y": {}"#),
        ("{\"s.y\": [", r#""s.y": []"#),
        ("{\"a\"", r#""a": "U""#),
        ("{\"deviceId\"", r#""deviceId": 1"#),
    ] {
        let document = with_member(&full(), anchor, member);
        let refused = load(&document).expect_err("refused");
        assert_eq!(refused.code(), "duplicate-key", "{member} in {anchor}");
        assert!(refused.at().is_some());
    }
    // Compared after unescaping: `"a"` and `"a"` are one key.
    let document = one_reference("\"outputs\": {\"s.a\": {}, \"s.\\u0061\": {}}");
    assert_eq!(code_of(&document), "duplicate-key");
    // The second place is the one refused.
    let document = one_reference("\"outputs\": {\"s.a\": {},\n \"s.b\": {},\n \"s.a\": {}}");
    assert_eq!(refusal(&document), ("duplicate-key", 3, 2));
}

#[test]
fn any_other_schema_tag_is_refused_by_name_before_the_shape_is_judged() {
    for tag in [
        "oce/vectors/v1",
        "oce/commissioning-spec/v2",
        "oce/commissioning-spec/v1 ",
        "OCE/commissioning-spec/v1",
        "oce/commissioning-spec",
        "",
    ] {
        // Whatever else the document holds, and wherever the tag is written.
        for document in [
            format!("{{\"schema\": \"{tag}\"}}"),
            format!("{{\"clock\": {{}}, \"scenarios\": [], \"schema\": \"{tag}\"}}"),
            format!("{{\"references\": 7, \"schema\": \"{tag}\"}}"),
        ] {
            let refused = load(&document).expect_err("refused");
            assert_eq!(refused.code(), "unknown-schema", "{document}");
            let at = refused.at().expect("the tag's place");
            let column = usize::try_from(at.column).expect("fits") - 1;
            assert!(
                document[column..].starts_with(&format!("\"{tag}\"")),
                "{document}"
            );
        }
    }
    let refused = load("{\"schema\": \"oce/vectors/v1\"}").expect_err("refused");
    assert_eq!(
        refused.detail(),
        "the schema is `oce/vectors/v1`; this reader reads `oce/commissioning-spec/v1` only"
    );
    // The tag is a string; a nested `schema` is not the document's tag.
    assert_eq!(code_of(&top_level("\"schema\": 1")), "wrong-type");
    assert_eq!(code_of(&top_level("\"schema\": null")), "wrong-type");
    assert_eq!(
        code_of(&top_level(
            "\"controller\": {\"schema\": \"oce/vectors/v1\"}"
        )),
        "loaded"
    );
    // A lexical defect is found before the tag is looked at.
    assert_eq!(
        code_of("{\"schema\": \"oce/vectors/v1\", \"a\": NaN}"),
        "non-finite-number"
    );
    // A vector set is not a spec, by its tag.
    let vectors = String::from_utf8(crate::support::library_vectors("AHU-0016")).expect("UTF-8");
    assert_eq!(code_of(&vectors), "unknown-schema");
    // And a spec is not a vector set.
    let spec = fixture_text("building_33.json");
    let as_vectors = oce_verify::VectorSet::from_json_bytes(
        spec.as_bytes(),
        &oce_verify::VerifyLimits::default(),
    );
    assert_eq!(as_vectors.expect_err("refused").code(), "unknown-schema");
}

#[test]
fn a_value_of_another_json_type_is_refused_at_every_key() {
    // (the reference member, a value of the wrong type for it)
    for (key, wrong) in [
        ("model", "1"),
        ("sequence", "null"),
        ("cxf", "[\"a.json\"]"),
        ("controllerOutput", "true"),
        ("pointNameMapping", "{}"),
        ("outputs", "[]"),
        ("indicators", "[]"),
        ("sampling", "\"60\""),
        ("parameters", "[]"),
        ("inputs", "{}"),
        ("expected", "\"x\""),
        ("indicatorSources", "1"),
        ("window", "[0, 1]"),
        ("envelope", "1"),
        ("funnelReference", "false"),
    ] {
        let members = if matches!(key, "model" | "sequence") {
            let other = if key == "model" { "sequence" } else { "model" };
            format!("{{\"references\": [{{\"{other}\": \"x\", \"{key}\": {wrong}}}]}}")
        } else {
            one_reference(&format!("\"{key}\": {wrong}"))
        };
        let refused = load(&members).expect_err("refused");
        assert_eq!(refused.code(), "wrong-type", "{key}: {wrong}");
        assert!(
            refused.detail().starts_with("expected "),
            "{}",
            refused.detail()
        );
        assert!(
            refused.detail().contains(", found "),
            "{}",
            refused.detail()
        );
    }
    for (member, expected) in [
        ("\"time\": []", "wrong-type"),
        ("\"tolerances\": 0.5", "wrong-type"),
        ("\"sampling\": {}", "wrong-type"),
        ("\"outputs\": {}", "unknown-key"),
    ] {
        assert_eq!(code_of(&top_level(member)), expected, "{member}");
    }
    // `null` is a value of another type everywhere a value is read: nothing is optional by null.
    for member in [
        "\"cxf\": null",
        "\"window\": null",
        "\"parameters\": null",
        "\"outputs\": {\"s.y\": null}",
        "\"indicators\": {\"s.y\": null}",
        "\"parameters\": {\"k\": null}",
        "\"inputs\": [null]",
        "\"pointNameMapping\": [null]",
    ] {
        assert_eq!(code_of(&one_reference(member)), "wrong-type", "{member}");
    }
    assert_eq!(code_of("{\"references\": [null]}"), "wrong-type");
    assert_eq!(code_of("null"), "wrong-type");
    assert_eq!(code_of("\"spec\""), "wrong-type");
}

#[test]
fn an_ignored_field_takes_any_value_and_the_text_is_still_strict() {
    for value in [
        "null",
        "true",
        "0",
        "-0.0",
        "\"test\"",
        "[]",
        "{}",
        "[1, [2, [3]]]",
        "{\"networkAddress\": \"192.168.0.115/24\", \"deviceId\": 240001}",
    ] {
        for document in [
            top_level(&format!("\"modelJsonDirectory\": {value}")),
            top_level(&format!("\"controller\": {value}")),
            one_reference(&format!("\"generateJson\": {value}")),
            one_reference(&format!("\"runController\": {value}")),
        ] {
            let spec = loaded(&document);
            assert_eq!(spec.findings().len(), 1, "{document}");
            assert_eq!(loaded(&spec.to_canonical_json()), spec, "{document}");
        }
    }
    // Whatever the value, the text around and inside it is strict JSON.
    for (value, expected) in [
        ("{\"a\": 1, \"a\": 2}", "duplicate-key"),
        ("{\"a\": {\"b\": 1, \"b\": 1}}", "duplicate-key"),
        ("[NaN]", "non-finite-number"),
        ("[1e999]", "non-finite-number"),
        ("[9223372036854775808]", "integer-out-of-range"),
        ("[1,]", "json-syntax"),
        ("{\"a\": 1,}", "json-syntax"),
        ("\"a\\tb\"", "control-character"),
        ("tru", "json-syntax"),
        ("undefined", "json-syntax"),
    ] {
        let document = top_level(&format!("\"controller\": {value}"));
        assert_eq!(code_of(&document), expected, "{value}");
    }
    // An ignored field is ignored, not optional by position: it belongs to its own object.
    assert_eq!(code_of(&one_reference("\"controller\": {}")), "unknown-key");
    assert_eq!(
        code_of(&top_level("\"runController\": true")),
        "unknown-key"
    );
    // `controllerOutput` is honoured, so it is judged: it is a path.
    assert_eq!(
        code_of(&one_reference("\"controllerOutput\": \"/var/out.csv\"")),
        "path-invalid"
    );
}
