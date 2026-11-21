//! The mapping reader on its own cursor: the pair and side shapes and the repeat rule.

use super::*;

fn read(text: &str) -> Read<PointMapping> {
    PointMapping::read(&mut Reader::reporting(text))
}

fn pair_text(cdl: &str, device: &str) -> String {
    format!(
        r#"{{"cdl": {{"name": "{cdl}", "type": "int"}}, "device": {{"name": "{device}", "type": "bool"}}}}"#
    )
}

#[test]
fn a_side_keeps_its_name_its_optional_unit_and_its_type() {
    let mapping = read(
        r#"[{"cdl": {"type": "float", "unit": "K", "name": "TSup"},
             "device": {"name": "SAT", "type": "int"}}]"#,
    )
    .expect("read");
    let pair = &mapping.points()[0];
    assert_eq!(
        (
            pair.cdl().name(),
            pair.cdl().unit(),
            pair.cdl().point_type()
        ),
        ("TSup", Some("K"), PointType::Float)
    );
    assert_eq!(
        (
            pair.device().name(),
            pair.device().unit(),
            pair.device().point_type()
        ),
        ("SAT", None, PointType::Int)
    );
    // The sides are not swapped by the order they are written in.
    let swapped = read(
        r#"[{"device": {"name": "SAT", "type": "int"},
             "cdl": {"type": "float", "unit": "K", "name": "TSup"}}]"#,
    )
    .expect("read");
    assert_eq!(swapped, mapping);
}

#[test]
fn a_repeat_is_reported_once_at_the_first_place_it_occurs() {
    let place = |text: &str| {
        let refusal = read(text).expect_err("refused");
        assert_eq!(refusal.kind, SpecErrorKind::DuplicatePoint);
        (refusal.detail, refusal.at.line, refusal.at.column)
    };
    let lines = |pairs: &[String]| format!("[{}]", pairs.join(",\n"));
    // A CDL repeat on line 3 and a device repeat on line 2: the device one is first.
    let text = lines(&[
        pair_text("a", "X"),
        pair_text("b", "X"),
        pair_text("a", "Y"),
    ]);
    assert_eq!(
        place(&text),
        (
            "an earlier point already has the device name `X`".to_owned(),
            2,
            58
        )
    );
    // The other way round.
    let text = lines(&[
        pair_text("a", "X"),
        pair_text("a", "Y"),
        pair_text("b", "X"),
    ]);
    assert_eq!(
        place(&text),
        (
            "an earlier point already has the cdl name `a`".to_owned(),
            2,
            18
        )
    );
    // Both in one pair: the CDL side is the one named, wherever the device side is written.
    let text = lines(&[pair_text("a", "X"), pair_text("a", "X")]);
    assert_eq!(
        place(&text).0,
        "an earlier point already has the cdl name `a`"
    );
    let device_first = format!(
        "[{},\n{}]",
        pair_text("a", "X"),
        r#"{"device": {"name": "X", "type": "bool"}, "cdl": {"name": "a", "type": "int"}}"#
    );
    assert_eq!(
        place(&device_first).0,
        "an earlier point already has the cdl name `a`"
    );
    // No repeat: the mapping is the pairs in file order.
    let mapping = read(&lines(&[pair_text("b", "Y"), pair_text("a", "X")])).expect("read");
    let names: Vec<&str> = mapping
        .points()
        .iter()
        .map(|pair| pair.cdl().name())
        .collect();
    assert_eq!(names, ["b", "a"]);
}

#[test]
fn the_canonical_value_writes_a_unit_only_where_one_was_written() {
    let mapping = read(
        r#"[{"cdl": {"name": "u", "type": "int"}, "device": {"name": "U", "unit": "", "type": "bool"}}]"#,
    )
    .expect("read");
    assert_eq!(
        mapping.to_canonical_json(),
        "[\n  {\n    \"cdl\": {\n      \"name\": \"u\",\n      \"type\": \"int\"\n    },\n    \"device\": {\n      \"name\": \"U\",\n      \"type\": \"bool\",\n      \"unit\": \"\"\n    }\n  }\n]\n"
    );
    assert_eq!(mapping.to_json().to_document(), mapping.to_canonical_json());
}

#[test]
fn the_typed_pass_refuses_text_after_the_mapping_standing_alone() {
    assert!(PointMapping::document(" [] \n").is_ok());
    for (text, column) in [("[] 1", 4), ("[][]", 3), ("[],", 3)] {
        let refusal = PointMapping::document(text).expect_err("text after the mapping");
        assert_eq!(refusal.kind, SpecErrorKind::JsonSyntax, "{text}");
        assert_eq!((refusal.at.line, refusal.at.column), (1, column), "{text}");
        assert_eq!(refusal.detail, "text after the end of the document");
    }
}

#[test]
fn the_file_reader_requires_the_whole_text_to_be_the_mapping() {
    let limits = SpecLimits::default();
    assert!(PointMapping::from_json_str(" [] ", &limits).is_ok());
    for (text, code) in [
        ("[] 1", "json-syntax"),
        ("[] []", "json-syntax"),
        ("{}", "wrong-type"),
    ] {
        let error = PointMapping::from_json_str(text, &limits).expect_err("refused");
        assert_eq!(error.code(), code, "{text}");
    }
    // A lowered byte bound applies to a mapping file as to a spec, and a raised one is the
    // maximum.
    let tight = SpecLimits {
        max_bytes: 3,
        ..SpecLimits::MAX
    };
    assert!(PointMapping::from_json_bytes(b"[ ]", &tight).is_ok());
    assert_eq!(
        PointMapping::from_json_bytes(b"[  ]", &tight)
            .expect_err("refused")
            .code(),
        "input-too-large"
    );
    assert_eq!(
        PointMapping::from_json_str("[  ]", &tight)
            .expect_err("refused")
            .code(),
        "input-too-large"
    );
}
