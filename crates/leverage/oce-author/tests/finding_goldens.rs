//! Finding goldens: the full finding list (code, subject and detail, in order) of every wire
//! bundle, compared bit for bit, so a change to any message or to the order is deliberate. The
//! `defects.json` bundle carries a defect of every stage-1 kind the wire form can express.

use crate::support;

use oce_author::{
    Block, Boundary, Endpoint, Identifier, Literal, MemberRef, ParamValue, ProvenanceId,
    SignalType, from_wire_str, to_wire_string, validate,
};

use support::{compare_or_bless, econ_enable, fixture, id};

/// A sequence with one defect per kind the wire form can carry.
fn defects() -> oce_author::Sequence {
    let mut s = econ_enable();
    s.name = Identifier::unchecked("defects");
    s.derived_from = Some(ProvenanceId::unchecked("CXF:abc"));
    s.params.push(oce_author::SequenceParameter::new(
        id("nStage"),
        SignalType::Integer,
        Literal::Real(2.5),
    ));
    s.params.push(oce_author::SequenceParameter::new(
        id("eneStd"),
        oce_author::ParameterType::parse("OperationModes"),
        Literal::Member(MemberRef::parse("OperationModes.occupied")),
    ));
    s.inputs[0].semantic.unit = Some("deg C".to_owned());
    s.inputs[0].min = Some(oce_author::Bound::Real(30.0));
    s.inputs[0].max = Some(oce_author::Bound::Integer(-10));
    s.outputs[0].max = Some(oce_author::Bound::Integer(1));
    s.outputs.push(Boundary::new(
        Identifier::unchecked("sub"),
        SignalType::Real,
    ));
    let hys = &mut s.blocks[1];
    hys.assignments
        .push(oce_author::Assignment::new(id("uLow"), Literal::Real(0.5)));
    hys.assignments
        .push(oce_author::Assignment::new(id("uMid"), Literal::Real(0.5)));
    hys.assignments.push(oce_author::Assignment::new(
        id("pre_y_start"),
        ParamValue::Ref(id("dTHysX")),
    ));
    let mut pid = Block::new(id("con"), "CDL.Reals.PID");
    pid.assignments.push(oce_author::Assignment::new(
        id("controllerType"),
        Literal::Member(MemberRef::parse("SimpleController.PIX")),
    ));
    pid.assignments
        .push(oce_author::Assignment::new(id("k"), Literal::Real(-1.0)));
    s.blocks.push(pid);
    s.blocks
        .push(Block::new(Identifier::unchecked("end"), "CDL.Reals.Nope"));
    let mut pass = Block::new(id("pass"), "urn:oce:lowering#PassThrough.Real");
    pass.assignments
        .push(oce_author::Assignment::new(id("k"), Literal::Integer(1)));
    s.blocks.push(pass);
    s.blocks.push(Block::new(id("limit"), "CDL.Reals.Limiter"));
    s.connections.push(oce_author::Connection::new(
        Endpoint::parse("hys.y.z"),
        Endpoint::parse("ecoEna"),
    ));
    s.connections.push(oce_author::Connection::new(
        Endpoint::parse("TOut"),
        Endpoint::parse("sub.u1"),
    ));
    s.connections.push(oce_author::Connection::new(
        Endpoint::parse("ecoEna"),
        Endpoint::parse("hys.u"),
    ));
    s.connections.push(oce_author::Connection::new(
        Endpoint::parse("sub.y"),
        Endpoint::parse("TOut"),
    ));
    s.connections.push(oce_author::Connection::new(
        Endpoint::parse("TOut"),
        Endpoint::parse("sub.u9"),
    ));
    s
}

fn report(sequence: &oce_author::Sequence) -> String {
    validate(sequence)
        .iter()
        .map(|f| format!("{}\t{}\t{}\n", f.code, f.subject, f.detail))
        .collect()
}

#[test]
fn the_defects_bundle_is_checked_in() {
    compare_or_bless(&fixture("wire/defects.json"), &to_wire_string(&defects()));
}

#[test]
fn every_bundles_findings_match_their_golden() {
    let mut bundles: Vec<_> = std::fs::read_dir(fixture("wire"))
        .expect("read fixtures/wire")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    bundles.sort();
    for path in bundles {
        let text = std::fs::read_to_string(&path).expect("read");
        let model = from_wire_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let stem = path.file_stem().and_then(|s| s.to_str()).expect("stem");
        compare_or_bless(&fixture(&format!("findings/{stem}.tsv")), &report(&model));
    }
}

#[test]
fn the_defects_bundle_covers_every_wire_expressible_code() {
    let codes: std::collections::BTreeSet<&str> =
        validate(&defects()).iter().map(|f| f.code).collect();
    let expected: std::collections::BTreeSet<&str> = oce_author::STAGE_ONE_CODES
        .iter()
        .copied()
        // A sequence with blocks is not empty; a non-finite Real has no wire spelling, as a
        // parameter value or as a bound.
        .filter(|c| !["empty-sequence", "non-finite-parameter", "non-finite-bound"].contains(c))
        .collect();
    assert_eq!(codes, expected);
}

#[test]
fn every_emitted_code_is_a_stage_one_code() {
    let mut models = vec![defects()];
    for entry in std::fs::read_dir(fixture("wire")).expect("fixtures") {
        let text = std::fs::read_to_string(entry.expect("entry").path()).expect("read");
        models.push(from_wire_str(&text).expect("parse"));
    }
    for model in &models {
        for finding in validate(model) {
            assert!(
                oce_author::STAGE_ONE_CODES.contains(&finding.code),
                "{}: {finding}",
                model.name
            );
        }
    }
}
