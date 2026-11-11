//! One test per stage-1 finding code, each with the exact code and subject (issue #463
//! acceptance; authoring note, "Finding codes").

use crate::support;

use oce_author::{
    Block, Boundary, Connection, Endpoint, Finding, Identifier, Literal, MemberRef, Origin,
    ParamValue, ProvenanceId, STAGE_ONE_CODES, Sequence, Severity, SignalType, validate,
    validate_author_side,
};

use support::{codes, econ_enable, id};

fn pair(code: &'static str, subject: &str) -> (&'static str, String) {
    (code, subject.to_owned())
}

fn hys(sequence: &mut Sequence) -> &mut Block {
    sequence
        .blocks
        .iter_mut()
        .find(|b| b.name.as_str() == "hys")
        .expect("hys")
}

fn set(block: &mut Block, name: &str, value: ParamValue) {
    match block
        .assignments
        .iter_mut()
        .find(|a| a.name.as_str() == name)
    {
        Some(assignment) => assignment.value = value,
        None => block.assignments.push(oce_author::Assignment::new(
            Identifier::unchecked(name),
            value,
        )),
    }
}

fn real(value: f64) -> ParamValue {
    ParamValue::Literal(Literal::Real(value))
}

/// A one-block sequence: a `CDL.Reals.Sources.Constant` named `src` driving output `y`, plus the
/// given block (with no connections of its own).
fn with_block(block: Block) -> Sequence {
    let mut s = Sequence::new(id("one"));
    s.outputs.push(Boundary::new(id("y"), SignalType::Real));
    let mut source = Block::new(id("src"), "CDL.Reals.Sources.Constant");
    source
        .assignments
        .push(oce_author::Assignment::new(id("k"), real(1.0)));
    s.blocks.push(source);
    s.blocks.push(block);
    s.connections.push(Connection::new(
        Endpoint::parse("src.y"),
        Endpoint::parse("y"),
    ));
    s
}

fn assigned(block: &str, class: &str, assignments: &[(&str, ParamValue)]) -> Block {
    let mut b = Block::new(id(block), class);
    for (name, value) in assignments {
        b.assignments.push(oce_author::Assignment::new(
            Identifier::unchecked(*name),
            value.clone(),
        ));
    }
    b
}

#[test]
fn the_stage_one_code_table_is_the_notes() {
    assert_eq!(
        STAGE_ONE_CODES,
        [
            "empty-sequence",
            "invalid-name",
            "duplicate-name",
            "class-not-found",
            "class-not-authorable",
            "unknown-parameter",
            "parameter-not-settable",
            "duplicate-parameter",
            "missing-required-parameter",
            "parameter-kind-mismatch",
            "parameter-out-of-range",
            "non-finite-parameter",
            "unit-suffix-mismatch",
            "unknown-enum-literal",
            "unknown-enum-type",
            "unknown-parameter-reference",
            "malformed-connection",
            "unknown-source",
            "unknown-target",
            "unknown-port",
            "invalid-semantic-point",
            "invalid-bound",
            "non-finite-bound",
            "crossed-bounds",
            "invalid-provenance-id",
            "single-assignment",
        ]
    );
    // Every code that names an engine defect uses the engine's spelling.
    for engine in [
        oce_api::DiagCode::ClassNotFound,
        oce_api::DiagCode::DuplicateParameter,
        oce_api::DiagCode::MissingRequiredParameter,
        oce_api::DiagCode::ParameterKindMismatch,
        oce_api::DiagCode::ParameterOutOfRange,
        oce_api::DiagCode::NonFiniteParameter,
        oce_api::DiagCode::UnknownEnumLiteral,
        oce_api::DiagCode::UnknownEnumType,
        oce_api::DiagCode::NonFiniteBound,
        oce_api::DiagCode::CrossedBounds,
    ] {
        assert!(STAGE_ONE_CODES.contains(&engine.as_str()), "{engine}");
    }
}

#[test]
fn every_stage_one_finding_is_an_author_error_without_a_span() {
    let mut s = econ_enable();
    s.name = Identifier::unchecked("1bad");
    set(
        hys(&mut s),
        "uLow",
        ParamValue::Literal(Literal::Boolean(true)),
    );
    let findings = validate_author_side(&s);
    assert!(!findings.is_empty());
    for finding in &findings {
        assert_eq!(finding.severity, Severity::Error);
        assert_eq!(finding.origin, Origin::Author);
        assert_eq!(finding.span, None);
        assert!(STAGE_ONE_CODES.contains(&finding.code), "{finding}");
    }
    assert_eq!(
        validate(&s),
        findings,
        "stage 2 runs only when stage 1 found no error"
    );
}

#[test]
fn empty_sequence() {
    let s = Sequence::new(id("nothing"));
    assert_eq!(
        codes(&validate(&s)),
        vec![pair("empty-sequence", "sequence")]
    );
}

#[test]
fn invalid_name() {
    let mut s = econ_enable();
    s.name = Identifier::unchecked("econ.enable");
    s.inputs[0].name = Identifier::unchecked("in");
    assert_eq!(
        codes(&validate(&s)),
        vec![
            pair("invalid-name", "sequence"),
            pair("invalid-name", "in"),
            // `TOut -> sub.u2` now names an undeclared input.
            pair("unknown-source", "connection[1]"),
        ]
    );
}

#[test]
fn duplicate_name() {
    let mut s = econ_enable();
    s.outputs
        .push(Boundary::new(id("dTHys"), SignalType::Boolean));
    s.blocks.push(Block::new(id("TOut"), "CDL.Reals.Abs"));
    let findings = validate(&s);
    assert_eq!(
        codes(&findings),
        vec![
            pair("duplicate-name", "dTHys"),
            pair("duplicate-name", "TOut")
        ]
    );
    assert!(
        findings[0]
            .detail
            .contains("output `dTHys` repeats the name of an earlier parameter"),
        "{}",
        findings[0].detail
    );
    assert!(
        findings[1]
            .detail
            .contains("block `TOut` repeats the name of an earlier input")
    );
}

#[test]
fn class_not_found() {
    let mut s = econ_enable();
    s.blocks[0].class_path = "CDL.Reals.Subtrakt".to_owned();
    // The block's ports are unknown, so its connections are not judged.
    assert_eq!(codes(&validate(&s)), vec![pair("class-not-found", "sub")]);
    let mut prefixed = econ_enable();
    prefixed.blocks[0].class_path = "Buildings.Controls.OBC.CDL.Reals.Subtract".to_owned();
    assert_eq!(
        codes(&validate(&prefixed)),
        vec![pair("class-not-found", "sub")]
    );
}

#[test]
fn class_not_authorable() {
    let block = Block::new(id("pass"), "urn:oce:lowering#PassThrough.Real");
    assert_eq!(
        codes(&validate(&with_block(block))),
        vec![pair("class-not-authorable", "pass")]
    );
}

#[test]
fn unknown_parameter() {
    let mut s = econ_enable();
    set(hys(&mut s), "uMid", real(0.5));
    set(hys(&mut s), "k_1", real(0.5));
    assert_eq!(
        codes(&validate(&s)),
        vec![
            pair("unknown-parameter", "hys.uMid"),
            pair("unknown-parameter", "hys.k_1")
        ]
    );
}

#[test]
fn parameter_not_settable() {
    let block = assigned(
        "pass",
        "urn:oce:lowering#PassThrough.Boolean",
        &[("k", real(1.0))],
    );
    assert_eq!(
        codes(&validate(&with_block(block))),
        vec![
            pair("class-not-authorable", "pass"),
            pair("parameter-not-settable", "pass.k")
        ]
    );
}

#[test]
fn duplicate_parameter() {
    let mut s = econ_enable();
    hys(&mut s)
        .assignments
        .push(oce_author::Assignment::new(id("uLow"), real(0.25)));
    let findings = validate(&s);
    assert_eq!(
        codes(&findings),
        vec![pair("duplicate-parameter", "hys.uLow")]
    );
}

#[test]
fn missing_required_parameter() {
    let mut s = econ_enable();
    hys(&mut s)
        .assignments
        .retain(|a| a.name.as_str() != "uLow");
    let findings = validate(&s);
    assert_eq!(
        codes(&findings),
        vec![pair("missing-required-parameter", "hys")]
    );
    assert!(
        findings[0].detail.contains("`uLow`"),
        "{}",
        findings[0].detail
    );
}

#[test]
fn parameter_kind_mismatch() {
    let mut s = econ_enable();
    set(
        hys(&mut s),
        "uLow",
        ParamValue::Literal(Literal::Text("0".to_owned())),
    );
    s.params[0].value = Literal::Boolean(true);
    assert_eq!(
        codes(&validate(&s)),
        vec![
            pair("parameter-kind-mismatch", "dTHys"),
            pair("parameter-kind-mismatch", "hys.uLow")
        ]
    );
    let sum = assigned("sum", "CDL.Integers.MultiSum", &[("nin", real(2.0))]);
    assert_eq!(
        codes(&validate(&with_block(sum))),
        vec![pair("parameter-kind-mismatch", "sum.nin")]
    );
    let pid = assigned(
        "con",
        "CDL.Reals.PID",
        &[(
            "controllerType",
            ParamValue::Literal(Literal::Text("PI".to_owned())),
        )],
    );
    assert_eq!(
        codes(&validate(&with_block(pid))),
        vec![pair("parameter-kind-mismatch", "con.controllerType")]
    );
}

#[test]
fn parameter_out_of_range() {
    let mut s = econ_enable();
    set(hys(&mut s), "uLow", real(2.0));
    let findings = validate(&s);
    assert_eq!(
        codes(&findings),
        vec![pair("parameter-out-of-range", "hys.uLow")]
    );
    let sum = assigned(
        "sum",
        "CDL.Reals.MultiSum",
        &[("nin", ParamValue::Literal(Literal::Integer(-1)))],
    );
    assert_eq!(
        codes(&validate(&with_block(sum))),
        vec![pair("parameter-out-of-range", "sum.nin")]
    );
}

#[test]
fn non_finite_parameter() {
    let mut s = econ_enable();
    set(hys(&mut s), "uLow", real(f64::NEG_INFINITY));
    s.params[0].value = Literal::Real(f64::NAN);
    // The reference to the broken `dTHys` is withheld, not reported twice.
    assert_eq!(
        codes(&validate(&s)),
        vec![
            pair("non-finite-parameter", "dTHys"),
            pair("non-finite-parameter", "hys.uLow")
        ]
    );
}

#[test]
fn unit_suffix_mismatch() {
    // A suffix that is not the declared unit (TrueDelay's `delayTime` is in `s`), a scaled one
    // where none is declared (Hysteresis's `uLow`), and a scaled one on a sequence parameter.
    let mut s = econ_enable();
    let mut delay = Block::new(id("del"), "CDL.Logical.TrueDelay");
    let mut delay_time = oce_author::Assignment::new(id("delayTime"), real(5.0));
    delay_time.suffix = Some("K".to_owned());
    delay.assignments.push(delay_time);
    s.blocks.push(delay);
    let low = hys(&mut s)
        .assignments
        .iter_mut()
        .find(|a| a.name.as_str() == "uLow")
        .expect("uLow");
    low.suffix = Some("min".to_owned());
    low.scaled_from = Some(Literal::Integer(0));
    s.params[0].suffix = Some("h".to_owned());
    s.params[0].scaled_from = Some(Literal::Integer(1));
    s.params[0].value = Literal::Real(3_600.0);
    let findings = validate(&s);
    assert_eq!(
        codes(&findings),
        vec![
            pair("unit-suffix-mismatch", "dTHys"),
            pair("unit-suffix-mismatch", "hys.uLow"),
            pair("unit-suffix-mismatch", "del.delayTime"),
        ]
    );
    assert!(findings.iter().all(|f| f.origin == Origin::Author));
    // The block reading the broken `dTHys` is withheld, not reported twice: `hys.uHigh` has no
    // finding of its own.
    assert!(
        findings[2].detail.contains("declared in `s`"),
        "{}",
        findings[2]
    );
}

#[test]
fn unknown_enum_literal() {
    for spelling in [
        "SimpleController.PX",
        "Smoothness.PI",
        "PI",
        "CDL.Types.SimpleController.PI",
    ] {
        let pid = assigned(
            "con",
            "CDL.Reals.PID",
            &[(
                "controllerType",
                ParamValue::Literal(Literal::Member(MemberRef::parse(spelling))),
            )],
        );
        let findings = validate(&with_block(pid));
        assert_eq!(
            codes(&findings),
            vec![pair("unknown-enum-literal", "con.controllerType")],
            "{spelling}"
        );
        assert!(
            findings[0].detail.contains("P, PI, PD, PID"),
            "{}",
            findings[0].detail
        );
    }
    let real_param = assigned(
        "c",
        "CDL.Reals.Sources.Constant",
        &[(
            "k",
            ParamValue::Literal(Literal::Member(MemberRef::parse("SimpleController.PI"))),
        )],
    );
    assert_eq!(
        codes(&validate(&with_block(real_param))),
        vec![pair("parameter-kind-mismatch", "c.k")],
        "an enum member for a Real parameter is a kind mismatch"
    );
}

#[test]
fn unknown_parameter_reference() {
    let mut s = econ_enable();
    set(hys(&mut s), "uHigh", ParamValue::Ref(id("dTHyst")));
    assert_eq!(
        codes(&validate(&s)),
        vec![pair("unknown-parameter-reference", "hys.uHigh")]
    );
}

#[test]
fn malformed_connection() {
    let mut s = econ_enable();
    s.connections[2].source = Endpoint::parse("sub.y.z");
    s.connections[3].target = Endpoint::parse("");
    s.connections.push(Connection::new(
        Endpoint::parse("sub.y"),
        Endpoint::parse("sub.y"),
    ));
    s.connections.push(Connection::new(
        Endpoint::parse("1sub.y"),
        Endpoint::parse("hys.u"),
    ));
    assert_eq!(
        codes(&validate(&s)),
        vec![
            pair("malformed-connection", "connection[2]"),
            pair("malformed-connection", "connection[3]"),
            pair("malformed-connection", "connection[4]"),
            pair("malformed-connection", "connection[5]"),
        ]
    );
}

#[test]
fn unknown_source() {
    let mut s = econ_enable();
    s.connections[0].source = Endpoint::parse("TOutCutt");
    s.connections[1].source = Endpoint::parse("ecoEna");
    s.connections[2].source = Endpoint::parse("subb.y");
    s.connections.push(Connection::new(
        Endpoint::parse("dTHys"),
        Endpoint::parse("hys.u"),
    ));
    let findings = validate(&s);
    assert_eq!(
        codes(&findings),
        vec![
            pair("unknown-source", "connection[0]"),
            pair("unknown-source", "connection[1]"),
            pair("unknown-source", "connection[2]"),
            pair("unknown-source", "connection[4]"),
        ]
    );
    assert!(
        findings[1].detail.contains("boundary output"),
        "{}",
        findings[1].detail
    );
    assert!(
        findings[3].detail.contains("sequence parameter"),
        "{}",
        findings[3].detail
    );
}

#[test]
fn unknown_target() {
    let mut s = econ_enable();
    s.connections[3].target = Endpoint::parse("TOut");
    s.connections.push(Connection::new(
        Endpoint::parse("hys.y"),
        Endpoint::parse("hys"),
    ));
    let findings = validate(&s);
    assert_eq!(
        codes(&findings),
        vec![
            pair("unknown-target", "connection[3]"),
            pair("unknown-target", "connection[4]")
        ]
    );
    assert!(findings[0].detail.contains("boundary input"));
    assert!(findings[1].detail.contains("block"));
}

#[test]
fn unknown_port() {
    let mut s = econ_enable();
    s.connections[0].target = Endpoint::parse("sub.u3");
    s.connections[2].source = Endpoint::parse("sub.u1");
    let findings = validate(&s);
    assert_eq!(
        codes(&findings),
        vec![
            pair("unknown-port", "connection[0]"),
            pair("unknown-port", "connection[2]")
        ]
    );
    assert!(
        findings[0].detail.contains("its inputs are: u1, u2"),
        "{}",
        findings[0].detail
    );
    assert!(
        findings[1].detail.contains("`u1` is an input"),
        "{}",
        findings[1].detail
    );
}

#[test]
fn unknown_port_follows_the_configured_width() {
    let sum = |nin: i64| {
        let mut s = with_block(assigned(
            "sum",
            "CDL.Reals.MultiSum",
            &[("nin", ParamValue::Literal(Literal::Integer(nin)))],
        ));
        s.inputs.push(Boundary::new(id("u"), SignalType::Real));
        s.connections.push(Connection::new(
            Endpoint::parse("u"),
            Endpoint::parse("sum.u3"),
        ));
        s
    };
    assert_eq!(codes(&validate_author_side(&sum(3))), vec![]);
    assert_eq!(
        codes(&validate_author_side(&sum(2))),
        vec![pair("unknown-port", "connection[1]")]
    );
}

#[test]
fn invalid_semantic_point() {
    let mut s = econ_enable();
    s.inputs[0].semantic.unit = Some("deg C".to_owned());
    s.inputs[1].semantic.haystack = vec!["air".to_owned(), "#temp".to_owned()];
    s.outputs[0].quantity = Some(String::new());
    s.outputs[0].semantic.label = Some("bad\u{7}".to_owned());
    let findings = validate(&s);
    assert_eq!(
        codes(&findings),
        vec![
            pair("invalid-semantic-point", "TOut"),
            pair("invalid-semantic-point", "TOutCut"),
            pair("invalid-semantic-point", "ecoEna"),
            pair("invalid-semantic-point", "ecoEna"),
        ]
    );
    let details: Vec<&str> = findings
        .iter()
        .map(|f: &Finding| f.detail.as_str())
        .collect();
    assert!(details[0].starts_with("`unit`"), "{details:?}");
    assert!(details[1].starts_with("`haystack[1]`"), "{details:?}");
    assert!(
        details.iter().any(|d| d.starts_with("`quantity` is empty")),
        "{details:?}"
    );
    assert!(
        details
            .iter()
            .any(|d| d.starts_with("`label` carries a control character")),
        "{details:?}"
    );
}

#[test]
fn invalid_provenance_id() {
    let mut s = econ_enable();
    s.derived_from = Some(ProvenanceId::unchecked("CXF:abc"));
    assert_eq!(
        codes(&validate(&s)),
        vec![pair("invalid-provenance-id", "derived_from")]
    );
    s.derived_from = Some(ProvenanceId::unchecked("cxf:fnv1a128:0123abcd"));
    assert_eq!(codes(&validate(&s)), vec![]);
}

#[test]
fn single_assignment() {
    // A repeated target in the connection list: the second connection is refused at stage 1,
    // before the engine sees it (the engine refuses every double-driver shape too since
    // GitHub #477).
    let mut s = econ_enable();
    s.connections.push(Connection::new(
        Endpoint::parse("TOut"),
        Endpoint::parse("hys.u"),
    ));
    s.connections.push(Connection::new(
        Endpoint::parse("TOutCut"),
        Endpoint::parse("sub.u1"),
    ));
    let findings = validate(&s);
    assert_eq!(
        codes(&findings),
        vec![
            pair("single-assignment", "connection[4]"),
            pair("single-assignment", "connection[5]")
        ]
    );
    assert!(
        findings[0]
            .detail
            .contains("already driven by connection[2]"),
        "{}",
        findings[0].detail
    );
    // A repeated boundary-output target too.
    let mut s = econ_enable();
    s.connections.push(Connection::new(
        Endpoint::parse("sub.y"),
        Endpoint::parse("ecoEna"),
    ));
    assert_eq!(
        codes(&validate(&s)),
        vec![pair("single-assignment", "connection[4]")]
    );
}
