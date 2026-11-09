//! Stage 2 (authoring note, "Validator"; decisions 4 and 14): every engine diagnostic of a
//! sequence stage 1 passes is an error finding with the engine's code, mapped onto the model
//! through the IRI scheme, with `origin = engine`.

mod support;

use oce_author::{
    Block, Boundary, Connection, Endpoint, Finding, Literal, Origin, ParamValue, Sequence,
    Severity, SignalType, validate, validate_author_side,
};

use support::{econ_enable, id};

fn pairs(findings: &[Finding]) -> Vec<(&'static str, String)> {
    findings
        .iter()
        .map(|f| (f.code, f.subject.clone()))
        .collect()
}

fn pair(code: &'static str, subject: &str) -> (&'static str, String) {
    (code, subject.to_owned())
}

fn connect(s: &mut Sequence, from: &str, to: &str) {
    s.connections
        .push(Connection::new(Endpoint::parse(from), Endpoint::parse(to)));
}

/// Stage 1 passes `s`, so every finding `validate` returns is the engine's.
fn engine_only(s: &Sequence) -> Vec<Finding> {
    assert_eq!(validate_author_side(s), vec![], "stage 1 must pass");
    let findings = validate(s);
    for finding in &findings {
        assert_eq!(finding.origin, Origin::Engine, "{finding}");
        assert_eq!(finding.severity, Severity::Error, "{finding}");
    }
    findings
}

#[test]
fn a_clean_sequence_has_no_findings_in_either_stage() {
    assert_eq!(engine_only(&econ_enable()), vec![]);
    assert_eq!(engine_only(&support::every_element()), vec![]);
}

#[test]
fn an_unused_boundary_input_is_an_error_at_the_input() {
    let mut s = econ_enable();
    s.inputs.push(Boundary::new(id("spare"), SignalType::Real));
    assert_eq!(
        pairs(&engine_only(&s)),
        vec![pair("unused-boundary-input", "spare")]
    );
}

#[test]
fn an_undriven_boundary_output_is_an_error_at_the_output() {
    let mut s = econ_enable();
    s.outputs.push(Boundary::new(id("spare"), SignalType::Real));
    assert_eq!(
        pairs(&engine_only(&s)),
        vec![pair("undriven-boundary-output", "spare")]
    );
}

#[test]
fn a_parameter_rule_warning_is_an_error_at_the_assignment() {
    // `uLow == uHigh` on a hysteresis is an engine warning; decision 14 makes it an error.
    let mut s = econ_enable();
    s.blocks[1].assignments[0].value = ParamValue::Literal(Literal::Real(1.0));
    let findings = engine_only(&s);
    assert_eq!(
        pairs(&findings),
        vec![pair("parameter-out-of-range", "hys.uLow")]
    );
    assert!(
        findings[0].detail.contains("are equal"),
        "{}",
        findings[0].detail
    );
}

#[test]
fn a_type_mismatch_is_reported_at_the_connection() {
    let mut s = econ_enable();
    s.blocks.push(Block::new(id("not1"), "CDL.Logical.Not"));
    connect(&mut s, "TOut", "not1.u");
    connect(&mut s, "not1.y", "ecoEna");
    assert_eq!(
        pairs(&engine_only(&s)),
        vec![
            pair("single-assignment", "ecoEna"),
            pair("type-mismatch", "connection[4]")
        ]
    );
}

#[test]
fn two_block_outputs_on_one_input_break_single_assignment() {
    let mut s = econ_enable();
    s.blocks.push(Block::new(id("abs"), "CDL.Reals.Abs"));
    connect(&mut s, "TOut", "abs.u");
    connect(&mut s, "abs.y", "hys.u");
    assert_eq!(
        pairs(&engine_only(&s)),
        vec![pair("single-assignment", "connection[2]")]
    );
}

#[test]
fn a_mixed_double_driver_is_not_yet_refused_by_the_engine() {
    // A block input driven by a block output and a boundary input loads warning-free today
    // (GitHub #472). Stage 2 reports whatever the engine reports; flip this when #472 lands.
    let mut s = econ_enable();
    connect(&mut s, "TOut", "hys.u");
    assert_eq!(engine_only(&s), vec![]);
}

#[test]
fn an_algebraic_loop_reports_one_finding_per_member() {
    let mut s = econ_enable();
    s.blocks.push(Block::new(id("a"), "CDL.Reals.Add"));
    s.blocks.push(Block::new(id("b"), "CDL.Reals.Abs"));
    s.outputs.push(Boundary::new(id("yb"), SignalType::Real));
    connect(&mut s, "TOut", "a.u1");
    connect(&mut s, "a.y", "b.u");
    connect(&mut s, "b.y", "a.u2");
    connect(&mut s, "b.y", "yb");
    let findings = engine_only(&s);
    let loops: Vec<&Finding> = findings
        .iter()
        .filter(|f| f.code == "algebraic-loop")
        .collect();
    assert_eq!(loops.len(), 4, "{findings:#?}");
    assert!(
        loops.iter().all(|f| f.subject.starts_with("connection[")),
        "{loops:#?}"
    );
    let mut details: Vec<&str> = loops.iter().map(|f| f.detail.as_str()).collect();
    details.sort_unstable();
    for (member, detail) in details.iter().enumerate() {
        assert!(
            detail.starts_with(&format!("algebraic loop member {} of 4", member + 1)),
            "{detail}"
        );
    }
}

#[test]
fn stage_two_runs_only_when_stage_one_is_clean() {
    // An unused input (stage 2) beside a stage-1 error: only the stage-1 finding is reported.
    let mut s = econ_enable();
    s.inputs.push(Boundary::new(id("spare"), SignalType::Real));
    s.blocks[0].class_path = "CDL.Reals.Subtrakt".to_owned();
    let findings = validate(&s);
    assert_eq!(pairs(&findings), vec![pair("class-not-found", "sub")]);
    assert!(findings.iter().all(|f| f.origin == Origin::Author));
}

#[test]
fn stage_two_findings_are_deterministic() {
    let mut s = econ_enable();
    s.inputs.push(Boundary::new(id("spare"), SignalType::Real));
    s.outputs
        .push(Boundary::new(id("spare2"), SignalType::Real));
    let first = validate(&s);
    assert_eq!(first.len(), 2);
    for _ in 0..3 {
        assert_eq!(validate(&s), first);
    }
    // Sorted by authored position: inputs before outputs.
    assert_eq!(first[0].subject, "spare");
    assert_eq!(first[1].subject, "spare2");
}
