//! Finding order: by the subject's authored position, then code, then detail; deduplicated; and
//! a pure function of the model, so permuting the model permutes the findings with it and nothing
//! else.

mod support;

use oce_author::{
    Boundary, Endpoint, Finding, Identifier, Literal, ParamValue, ProvenanceId, Sequence,
    SignalType, validate,
};

use support::econ_enable;

/// A sequence with defects at every level, several on one subject.
fn defective() -> Sequence {
    let mut s = econ_enable();
    s.name = Identifier::unchecked("bad name");
    s.extends = Some(ProvenanceId::unchecked("X:1"));
    s.params[0].value = Literal::Text("one".to_owned());
    s.inputs[0].semantic.unit = Some("deg C".to_owned());
    s.inputs[0].semantic.brick = Some(String::new());
    s.inputs[1].name = Identifier::unchecked("hys");
    let hys = &mut s.blocks[1];
    hys.assignments[0].value = ParamValue::Literal(Literal::Boolean(true));
    hys.assignments.push(oce_author::Assignment::new(
        Identifier::unchecked("zeta"),
        Literal::Real(1.0),
    ));
    hys.assignments.push(oce_author::Assignment::new(
        Identifier::unchecked("alpha"),
        Literal::Real(1.0),
    ));
    s.connections[0].target = Endpoint::parse("sub.u9");
    s.connections[2].source = Endpoint::parse("nope");
    s
}

fn summary(findings: &[Finding]) -> Vec<String> {
    findings
        .iter()
        .map(|f| format!("{} {} | {}", f.subject, f.code, f.detail))
        .collect()
}

#[test]
fn findings_follow_authored_position_then_code_then_detail() {
    let findings = validate(&defective());
    let order: Vec<(&str, &str)> = findings
        .iter()
        .map(|f| (f.subject.as_str(), f.code))
        .collect();
    assert_eq!(
        order,
        vec![
            ("sequence", "invalid-name"),
            ("extends", "invalid-provenance-id"),
            ("dTHys", "parameter-kind-mismatch"),
            ("TOut", "invalid-semantic-point"),
            ("TOut", "invalid-semantic-point"),
            // `hys` the input is declared before `hys` the block.
            ("hys", "duplicate-name"),
            ("hys.uLow", "parameter-kind-mismatch"),
            ("hys.zeta", "unknown-parameter"),
            ("hys.alpha", "unknown-parameter"),
            // Code order within one connection: `unknown-port` before `unknown-source`.
            ("connection[0]", "unknown-port"),
            ("connection[0]", "unknown-source"),
            ("connection[2]", "unknown-source"),
        ],
        "{:#?}",
        summary(&findings)
    );
    // Two findings on one subject with one code sort by detail.
    assert!(
        findings[3].detail < findings[4].detail,
        "{:?}",
        summary(&findings[3..5])
    );
    // `connection[0]` (`TOutCut -> sub.u9`) names the renamed input: it is now undeclared.
    assert!(
        findings[10].detail.contains("`TOutCut` is not declared"),
        "{}",
        findings[10].detail
    );
}

#[test]
fn validation_is_deterministic_across_runs_and_clones() {
    let s = defective();
    let first = validate(&s);
    for _ in 0..16 {
        assert_eq!(validate(&s.clone()), first);
    }
}

#[test]
fn findings_are_deduplicated() {
    let mut s = econ_enable();
    // The same defect twice on one subject: two identical `invalid-semantic-point` details.
    s.inputs[0].semantic.haystack = vec!["a b".to_owned()];
    s.inputs[0].semantic.qudt = Some("a b".to_owned());
    let findings = validate(&s);
    assert_eq!(findings.len(), 2, "{:#?}", summary(&findings));
    let mut seen = std::collections::BTreeSet::new();
    for f in &findings {
        assert!(
            seen.insert((f.subject.clone(), f.code, f.detail.clone())),
            "duplicate {f}"
        );
    }
}

#[test]
fn permuting_the_model_permutes_the_findings_and_nothing_else() {
    let base = defective();
    let base_findings = validate(&base);
    // Reverse the blocks, the inputs and the block assignments.
    let mut permuted = base.clone();
    permuted.blocks.reverse();
    permuted.inputs.reverse();
    for block in &mut permuted.blocks {
        block.assignments.reverse();
    }
    let findings = validate(&permuted);
    let as_set =
        |f: &[Finding]| -> std::collections::BTreeSet<String> { summary(f).into_iter().collect() };
    assert_eq!(
        as_set(&findings),
        as_set(&base_findings),
        "the same findings"
    );
    let subjects: Vec<&str> = findings.iter().map(|f| f.subject.as_str()).collect();
    assert_eq!(
        subjects,
        vec![
            "sequence",
            "extends",
            "dTHys",
            "TOut",
            "TOut",
            "hys",
            "hys.alpha",
            "hys.zeta",
            "hys.uLow",
            "connection[0]",
            "connection[0]",
            "connection[2]",
        ],
        "in the permuted authored order"
    );
    // Permuting only connections renumbers their subjects and keeps their order by index.
    let mut connections = base.clone();
    connections.connections.reverse();
    let renumbered: Vec<String> = validate(&connections)
        .into_iter()
        .filter(|f| f.subject.starts_with("connection"))
        .map(|f| format!("{} {}", f.subject, f.code))
        .collect();
    assert_eq!(
        renumbered,
        vec![
            "connection[1] unknown-source",
            "connection[3] unknown-port",
            "connection[3] unknown-source"
        ]
    );
}

#[test]
fn an_input_order_permutation_of_a_clean_sequence_stays_clean() {
    let mut s = econ_enable();
    s.inputs.swap(0, 1);
    s.connections.swap(0, 1);
    s.outputs
        .push(Boundary::new(support::id("spare"), SignalType::Real));
    assert_eq!(validate(&s), vec![], "unused boundaries are stage 2's");
}
