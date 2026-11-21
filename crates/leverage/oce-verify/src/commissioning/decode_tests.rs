//! The spec reader on its own cursor: each key lands in its own field.

use super::*;

fn reader(text: &str) -> Reader<'_> {
    Reader::reporting(text)
}

#[test]
fn each_tolerance_name_lands_in_its_own_slot() {
    // Six different values, written in an order that is not the slots' order.
    let read = tolerances(&mut reader(
        r#"{"rtoly": 6, "rtolx": 5, "ltoly": 4, "ltolx": 3, "atoly": 2, "atolx": 1}"#,
    ))
    .expect("read");
    assert_eq!(read.or_zero(), [1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    assert_eq!(
        [
            read.atolx(),
            read.atoly(),
            read.ltolx(),
            read.ltoly(),
            read.rtolx(),
            read.rtoly()
        ],
        [1.0, 2.0, 3.0, 4.0, 5.0, 6.0].map(Some)
    );
    // One name alone sets one slot.
    for (index, name) in TOLERANCE_NAMES.iter().enumerate() {
        let read = tolerances(&mut reader(&format!("{{\"{name}\": 7}}"))).expect("read");
        let mut expected = [None; 6];
        expected[index] = Some(Number::Integer(7));
        assert_eq!(read.written, expected, "{name}");
    }
    let refusal = tolerances(&mut reader("[]")).expect_err("refused");
    assert_eq!(
        refusal.detail,
        "expected the tolerances, an object, found a list"
    );
}

#[test]
fn each_reference_key_lands_in_its_own_field() {
    let limits = SpecLimits::MAX;
    let read = reference(
        &mut reader(
            r#"{"model": "the model", "sequence": "seq", "cxf": "the/cxf", "controllerOutput": "the/output",
                "pointNameMapping": "the/mapping", "sampling": 7, "envelope": "aligned-grid",
                "funnelReference": "trend", "window": {"start": 1, "stop": 2},
                "parameters": {"p": 1}, "outputs": {"seq.o": {}}, "indicators": {"seq.i": []},
                "inputs": [{"path": "in", "format": "csv", "timeColumn": 1, "columns": {}}],
                "expected": [{"path": "ex", "format": "csv", "timeColumn": 1, "columns": {}}],
                "indicatorSources": [{"path": "ind", "format": "csv", "timeColumn": 1, "columns": {}}],
                "generateJson": "g", "runController": "r"}"#,
        ),
        &limits,
    )
    .expect("read");
    assert_eq!((read.model(), read.sequence()), ("the model", "seq"));
    assert_eq!(
        (read.cxf(), read.controller_output()),
        (Some("the/cxf"), Some("the/output"))
    );
    assert_eq!(
        read.point_name_mapping(),
        Some(&PointNameMapping::Path("the/mapping".to_owned()))
    );
    assert_eq!(read.sampling_s(), Some(7.0));
    assert_eq!(read.envelope(), Some(Envelope::AlignedGrid));
    assert_eq!(read.funnel_reference(), Some(FunnelReference::Trend));
    assert_eq!(
        read.window()
            .map(|window| (window.start_s(), window.stop_s())),
        Some((1.0, 2.0))
    );
    assert_eq!(read.parameters().map(|all| all[0].path()), Some("p"));
    assert_eq!(read.outputs()[0].key(), "seq.o");
    assert_eq!(read.indicators()[0].key(), "seq.i");
    // The three source lists are three fields.
    let first = |list: Option<&[TrendSource]>| list.map(|all| all[0].path().to_owned());
    assert_eq!(first(read.inputs()).as_deref(), Some("in"));
    assert_eq!(first(read.expected()).as_deref(), Some("ex"));
    assert_eq!(first(read.indicator_sources()).as_deref(), Some("ind"));
    // And the two ignored fields are two fields.
    assert_eq!(read.generate_json, Some(Json::str("g")));
    assert_eq!(read.run_controller, Some(Json::str("r")));
}

#[test]
fn the_typed_pass_refuses_text_after_the_spec_standing_alone() {
    let limits = SpecLimits::MAX;
    assert!(document(" {\"references\": []} \n", &limits).is_ok());
    for (text, column) in [
        ("{\"references\": []} 1", 20),
        ("{\"references\": []}{}", 19),
        ("{\"references\": []},", 19),
        ("{\"references\": []} x", 20),
    ] {
        let refusal = document(text, &limits).expect_err("text after the spec");
        assert_eq!(refusal.kind, SpecErrorKind::JsonSyntax, "{text}");
        assert_eq!((refusal.at.line, refusal.at.column), (1, column), "{text}");
        assert_eq!(refusal.detail, "text after the end of the document");
    }
}

#[test]
fn each_top_level_key_lands_in_its_own_field() {
    let limits = SpecLimits::MAX;
    let read = spec(
        &mut reader(
            r#"{"schema": "oce/commissioning-spec/v1", "time": {"origin": "2018-06-07T00:00:00Z"},
                "tolerances": {"atoly": 3}, "sampling": 9, "modelJsonDirectory": "m",
                "controller": "c", "references": [{"model": "", "sequence": "s"}]}"#,
        ),
        &limits,
    )
    .expect("read");
    assert_eq!(
        read.time_origin().map(TimeOrigin::as_str),
        Some("2018-06-07T00:00:00Z")
    );
    assert_eq!(
        read.tolerances().and_then(PartialTolerances::atoly),
        Some(3.0)
    );
    assert_eq!(read.sampling_s(), Some(9.0));
    assert_eq!(read.references().len(), 1);
    assert_eq!(read.model_json_directory, Some(Json::str("m")));
    assert_eq!(read.controller, Some(Json::str("c")));
    // The tag is checked where it is written, with its place.
    let refusal =
        spec(&mut reader(r#"{"schema": "oce/vectors/v1"}"#), &limits).expect_err("another format");
    assert_eq!(refusal.kind, SpecErrorKind::UnknownSchema);
    assert_eq!((refusal.at.line, refusal.at.column), (1, 12));
    // The parameter bound of the limits handed in is the one applied.
    let one = SpecLimits {
        max_parameters: 1,
        ..limits
    };
    let two = r#"{"references": [{"model": "", "sequence": "s", "parameters": {"a": 1, "b": 2}}]}"#;
    assert!(spec(&mut reader(two), &limits).is_ok());
    assert_eq!(
        spec(&mut reader(two), &one).expect_err("refused").kind,
        SpecErrorKind::TooManyParameters
    );
}
