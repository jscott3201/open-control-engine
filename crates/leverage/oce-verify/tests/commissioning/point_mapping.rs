//! The point-mapping reader (commissioning note, "Point mapping"; decisions 5 and 27): Listing
//! 12.2's shape and nothing else, the three types, names kept as written with no prefix, and no
//! name twice on a side, from a file's bytes and inline in a spec alike.

use oce_verify::{Location, PointMapping, PointNameMapping, PointType, SpecLimits};

use crate::{load, load_mapping, loaded, one_reference};

/// A mapping of the given pairs, each written `cdl name / device name`.
fn mapping_of(pairs: &[(&str, &str)]) -> String {
    let entries: Vec<String> = pairs
        .iter()
        .map(|(cdl, device)| {
            format!(
                "{{\"cdl\": {{\"name\": \"{cdl}\", \"type\": \"float\"}}, \"device\": {{\"name\": \"{device}\", \"type\": \"float\"}}}}"
            )
        })
        .collect();
    format!("[{}]", entries.join(",\n "))
}

fn mapping_code(text: &str) -> &'static str {
    match load_mapping(text) {
        Ok(_) => "loaded",
        Err(error) => error.code(),
    }
}

#[test]
fn a_mapping_is_a_list_of_pairs_and_may_be_empty() {
    let empty = load_mapping("[]").expect("an empty mapping loads");
    assert!(empty.points().is_empty());
    assert_eq!(empty.to_canonical_json(), "[]\n");
    let one = load_mapping(&mapping_of(&[("TSup", "Supply Air Temp")])).expect("loads");
    assert_eq!(one.points().len(), 1);
    assert_eq!(one.points()[0].cdl().name(), "TSup");
    assert_eq!(one.points()[0].device().name(), "Supply Air Temp");
    assert_eq!(one.points()[0].cdl().unit(), None);
    assert_eq!(one.points()[0].cdl().point_type(), PointType::Float);
    for (root, expected) in [
        ("{}", "wrong-type"),
        ("{\"points\": []}", "wrong-type"),
        ("\"points.json\"", "wrong-type"),
        ("null", "wrong-type"),
        ("7", "wrong-type"),
        ("[[]]", "wrong-type"),
        ("[null]", "wrong-type"),
        ("[\"TSup\"]", "wrong-type"),
        ("[] []", "json-syntax"),
        ("[],", "json-syntax"),
        ("", "json-syntax"),
    ] {
        assert_eq!(mapping_code(root), expected, "{root}");
    }
    // A mapping file carries no schema tag: it is Listing 12.2's list, and a key is unknown.
    assert_eq!(
        mapping_code("[{\"schema\": \"oce/commissioning-spec/v1\"}]"),
        "unknown-key"
    );
}

#[test]
fn a_pair_has_both_sides_and_a_side_its_name_and_type() {
    let side = "{\"name\": \"x\", \"type\": \"int\"}";
    for (pair, expected) in [
        (format!("{{\"cdl\": {side}, \"device\": {side}}}"), "loaded"),
        (format!("{{\"device\": {side}, \"cdl\": {side}}}"), "loaded"),
        (format!("{{\"cdl\": {side}}}"), "missing-key"),
        (format!("{{\"device\": {side}}}"), "missing-key"),
        ("{}".to_owned(), "missing-key"),
        (
            format!("{{\"cdl\": {side}, \"device\": {side}, \"note\": \"x\"}}"),
            "unknown-key",
        ),
        (
            format!("{{\"cdl\": {side}, \"Device\": {side}}}"),
            "unknown-key",
        ),
        (
            format!("{{\"cdl\": {side}, \"device\": {side}, \"cdl\": {side}}}"),
            "duplicate-key",
        ),
        (
            format!("{{\"cdl\": \"x\", \"device\": {side}}}"),
            "wrong-type",
        ),
        (
            format!("{{\"cdl\": {side}, \"device\": [{side}]}}"),
            "wrong-type",
        ),
        (
            format!("{{\"cdl\": {side}, \"device\": null}}"),
            "wrong-type",
        ),
    ] {
        assert_eq!(mapping_code(&format!("[{pair}]")), expected, "{pair}");
    }
    let other = "{\"name\": \"y\", \"type\": \"int\"}";
    for (side, expected) in [
        ("{\"name\": \"x\", \"type\": \"float\"}", "loaded"),
        ("{\"type\": \"bool\", \"name\": \"x\"}", "loaded"),
        (
            "{\"name\": \"x\", \"unit\": \"K\", \"type\": \"float\"}",
            "loaded",
        ),
        // A point without a unit writes no `unit` key: an empty unit is neither.
        (
            "{\"name\": \"x\", \"unit\": \"\", \"type\": \"float\"}",
            "empty-name",
        ),
        // A unit that is written is kept as written and judged by the unit table, not here.
        (
            "{\"name\": \"x\", \"unit\": \"furlongs\", \"type\": \"float\"}",
            "loaded",
        ),
        ("{\"name\": \"x\"}", "missing-key"),
        ("{\"type\": \"int\"}", "missing-key"),
        ("{\"unit\": \"K\"}", "missing-key"),
        ("{\"name\": \"\", \"type\": \"int\"}", "empty-name"),
        ("{\"name\": 1, \"type\": \"int\"}", "wrong-type"),
        ("{\"name\": \"x\", \"type\": 1}", "wrong-type"),
        (
            "{\"name\": \"x\", \"unit\": null, \"type\": \"int\"}",
            "wrong-type",
        ),
        (
            "{\"name\": \"x\", \"unit\": 1, \"type\": \"int\"}",
            "wrong-type",
        ),
        (
            "{\"name\": \"x\", \"type\": \"int\", \"units\": \"K\"}",
            "unknown-key",
        ),
        (
            "{\"name\": \"x\", \"type\": \"int\", \"address\": 1}",
            "unknown-key",
        ),
        (
            "{\"name\": \"x\", \"type\": \"int\", \"type\": \"int\"}",
            "duplicate-key",
        ),
    ] {
        for document in [
            format!("[{{\"cdl\": {side}, \"device\": {other}}}]"),
            format!("[{{\"cdl\": {other}, \"device\": {side}}}]"),
        ] {
            assert_eq!(mapping_code(&document), expected, "{document}");
        }
    }
}

#[test]
fn the_types_are_float_int_and_bool_spelled_so() {
    for (written, expected) in [
        ("float", Some(PointType::Float)),
        ("int", Some(PointType::Int)),
        ("bool", Some(PointType::Bool)),
        ("Float", None),
        ("real", None),
        ("double", None),
        ("integer", None),
        ("boolean", None),
        ("enum", None),
        ("string", None),
        ("", None),
        ("int ", None),
    ] {
        let document = format!(
            "[{{\"cdl\": {{\"name\": \"u\", \"type\": \"{written}\"}}, \"device\": {{\"name\": \"U\", \"type\": \"bool\"}}}}]"
        );
        match expected {
            Some(kind) => {
                let mapping = load_mapping(&document).expect("loads");
                assert_eq!(mapping.points()[0].cdl().point_type(), kind);
                assert_eq!(mapping.points()[0].device().point_type(), PointType::Bool);
                assert_eq!(kind.as_str(), written);
            }
            None => {
                let error = load_mapping(&document).expect_err("refused");
                assert_eq!(error.code(), "unknown-value", "{written:?}");
                assert_eq!(error.at(), Some(Location::new(1, 32)), "the type's value");
            }
        }
    }
    let error = load_mapping(
        "[{\"cdl\": {\"name\": \"u\", \"type\": \"real\"}, \"device\": {\"name\": \"U\", \"type\": \"bool\"}}]",
    )
    .expect_err("refused");
    assert_eq!(
        error.detail(),
        "a point type is `real`; expected `float`, `int` or `bool`"
    );
}

#[test]
fn a_name_is_kept_as_written_and_carries_no_prefix() {
    let mapping = load_mapping(&mapping_of(&[
        ("TZonCooSetOcc", "Occupied Cooling Setpoint_1"),
        ("setPoiVAV.TSup", "a.b.c"),
        ("  spaced  ", "VFD \u{b0} Cmd"),
        ("\\u0075Fan", "caf\\u00e9"),
        ("fanSta.y", "indicator 1"),
        ("*", "?"),
    ]))
    .expect("loads");
    let names: Vec<(&str, &str)> = mapping
        .points()
        .iter()
        .map(|pair| (pair.cdl().name(), pair.device().name()))
        .collect();
    assert_eq!(
        names,
        [
            ("TZonCooSetOcc", "Occupied Cooling Setpoint_1"),
            // Nothing is stripped, added or resolved: a dotted name is a name.
            ("setPoiVAV.TSup", "a.b.c"),
            ("  spaced  ", "VFD \u{b0} Cmd"),
            ("uFan", "caf\u{e9}"),
            ("fanSta.y", "indicator 1"),
            ("*", "?"),
        ]
    );
    // The canonical form writes each name back as it reads it.
    let again = load_mapping(&mapping.to_canonical_json()).expect("loads");
    assert_eq!(again, mapping);
}

#[test]
fn a_name_twice_on_one_side_is_refused_at_its_second_place() {
    for (pairs, side, name, line) in [
        (&[("u", "A"), ("u", "B")][..], "cdl", "u", 2),
        (&[("u", "A"), ("v", "A")], "device", "A", 2),
        (
            &[("u", "A"), ("v", "B"), ("w", "C"), ("v", "D")],
            "cdl",
            "v",
            4,
        ),
        (
            &[("u", "A"), ("v", "B"), ("w", "B"), ("u", "C")],
            "device",
            "B",
            3,
        ),
        // Both sides repeat in one pair: the CDL side is the one named.
        (&[("u", "A"), ("u", "A")], "cdl", "u", 2),
        // Three times: the second is the place.
        (&[("u", "A"), ("u", "B"), ("u", "C")], "cdl", "u", 2),
    ] {
        let text = mapping_of(pairs);
        let error = load_mapping(&text).expect_err("refused");
        assert_eq!(error.code(), "duplicate-point", "{text}");
        assert_eq!(
            error.detail(),
            format!("an earlier point already has the {side} name `{name}`")
        );
        // The place is the repeated name's own string, on its pair's line.
        let at = error.at().expect("a place");
        assert_eq!(at.line, line, "{text}");
        let written = text
            .lines()
            .nth(usize::try_from(at.line).expect("fits") - 1)
            .expect("the line");
        let column = usize::try_from(at.column).expect("fits") - 1;
        assert!(
            written[column..].starts_with(&format!("\"{name}\"")),
            "{written}"
        );
    }
    // The same name on the two sides is two names: a CDL point may be called as its device is.
    assert_eq!(
        mapping_code(&mapping_of(&[("u", "u"), ("v", "v")])),
        "loaded"
    );
    assert_eq!(
        mapping_code(&mapping_of(&[("u", "v"), ("v", "u")])),
        "loaded"
    );
    // Compared exactly, after unescaping: case and space make another name, an escape does not.
    assert_eq!(
        mapping_code(&mapping_of(&[("u", "A"), ("U", "a"), ("u ", "A ")])),
        "loaded"
    );
    assert_eq!(
        mapping_code(&mapping_of(&[("u", "A"), ("\\u0075", "B")])),
        "duplicate-point"
    );
}

#[test]
fn a_mapping_file_and_an_inline_mapping_are_read_by_one_reader() {
    let text = crate::fixture_text("listing_12_2.json");
    let from_file = load_mapping(&text).expect("loads");
    let spec = loaded(&one_reference(&format!("\"pointNameMapping\": {text}")));
    let Some(PointNameMapping::Inline(inline)) = spec.references()[0].point_name_mapping() else {
        panic!("the mapping is inline");
    };
    assert_eq!(inline, &from_file);
    // The same defects are the same codes either way.
    for (mapping, expected) in [
        (mapping_of(&[("u", "A"), ("u", "B")]), "duplicate-point"),
        ("[{\"cdl\": {\"name\": \"u\", \"type\": \"float\"}}]".to_owned(), "missing-key"),
        ("[{\"cdl\": 1, \"device\": 2}]".to_owned(), "wrong-type"),
        ("[{\"cdl\": {\"name\": \"\", \"type\": \"int\"}, \"device\": {\"name\": \"a\", \"type\": \"int\"}}]".to_owned(), "empty-name"),
    ] {
        assert_eq!(mapping_code(&mapping), expected, "{mapping}");
        let inline = one_reference(&format!("\"pointNameMapping\": {mapping}"));
        assert_eq!(load(&inline).expect_err("refused").code(), expected, "{mapping}");
    }
    // Two references may write the same inline mapping: a repeat is judged within one mapping.
    let both = format!(
        "{{\"references\": [{{\"model\": \"M\", \"sequence\": \"a\", \"pointNameMapping\": {m}}}, {{\"model\": \"M\", \"sequence\": \"b\", \"pointNameMapping\": {m}}}]}}",
        m = mapping_of(&[("u", "A")])
    );
    assert_eq!(crate::code_of(&both), "loaded");
}

#[test]
fn a_mapping_file_passes_the_byte_stages_a_spec_passes() {
    let limits = SpecLimits::default();
    let code = |bytes: &[u8]| match PointMapping::from_json_bytes(bytes, &limits) {
        Ok(_) => "loaded",
        Err(error) => error.code(),
    };
    assert_eq!(code(b"[]"), "loaded");
    assert_eq!(code(b"\xef\xbb\xbf[]"), "byte-order-mark");
    assert_eq!(code(b"[\"\xff\"]"), "not-utf8");
    assert_eq!(code(b"[] // done"), "loaded");
    assert_eq!(code(b"[] /* open"), "unterminated-comment");
    let small = SpecLimits {
        max_bytes: 2,
        ..SpecLimits::MAX
    };
    assert_eq!(
        PointMapping::from_json_bytes(b"[ ]", &small)
            .expect_err("refused")
            .code(),
        "input-too-large"
    );
    assert!(PointMapping::from_json_bytes(b"[]", &small).is_ok());
    let error = PointMapping::from_json_bytes(b"[\"\xff\"]", &limits).expect_err("refused");
    assert_eq!(
        (error.subject(), error.detail()),
        ("document", "byte 2 is not UTF-8")
    );
}
