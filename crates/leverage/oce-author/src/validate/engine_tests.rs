//! The stage-1 / engine parity test (authoring note, "Finding codes"; the table there is this
//! test's fixture, row for row in [`PARITY`]).
//!
//! Each defect is emitted with stage 1 bypassed ([`emit_unchecked`]) and loaded in a throwaway
//! engine; the engine's findings are mapped back through the same code stage 2 uses. A row whose
//! owner is the engine asserts the engine reports the stage-1 code at the stage-1 subject; an
//! author-only row that pre-empts an engine code asserts that code at the same subject; the
//! `unknown-parameter` row pins that the engine reports nothing at all (GitHub #467).

use crate::emit::{EmitOptions, emit_unchecked};
use crate::identifier::Identifier;
use crate::model::{
    Assignment, Block, Boundary, Connection, Endpoint, EnumMember, Literal, ParamValue, Sequence,
    SignalType,
};
use crate::validate::{Finding, validate_author_side};

use super::judge;

fn id(name: &str) -> Identifier {
    Identifier::unchecked(name)
}

fn real(value: f64) -> ParamValue {
    ParamValue::Literal(Literal::Real(value))
}

/// The authoring note's worked example.
fn econ_enable() -> Sequence {
    let mut s = Sequence::new(id("econ_enable"));
    s.params.push(crate::model::SequenceParameter::new(
        id("dTHys"),
        SignalType::Real,
        Literal::Integer(1),
    ));
    s.inputs.push(Boundary::new(id("TOut"), SignalType::Real));
    s.inputs
        .push(Boundary::new(id("TOutCut"), SignalType::Real));
    s.outputs
        .push(Boundary::new(id("ecoEna"), SignalType::Boolean));
    s.blocks.push(Block::new(id("sub"), "CDL.Reals.Subtract"));
    let mut hys = Block::new(id("hys"), "CDL.Reals.Hysteresis");
    hys.assignments.push(Assignment::new(id("uLow"), real(0.0)));
    hys.assignments
        .push(Assignment::new(id("uHigh"), ParamValue::Ref(id("dTHys"))));
    s.blocks.push(hys);
    for (from, to) in [
        ("TOutCut", "sub.u1"),
        ("TOut", "sub.u2"),
        ("sub.y", "hys.u"),
        ("hys.y", "ecoEna"),
    ] {
        s.connections
            .push(Connection::new(Endpoint::parse(from), Endpoint::parse(to)));
    }
    s
}

fn hys(s: &mut Sequence) -> &mut Block {
    &mut s.blocks[1]
}

fn set(block: &mut Block, name: &str, value: ParamValue) {
    match block
        .assignments
        .iter_mut()
        .find(|a| a.name.as_str() == name)
    {
        Some(a) => a.value = value,
        None => block.assignments.push(Assignment::new(id(name), value)),
    }
}

fn engine(s: &Sequence) -> Vec<Finding> {
    let options = EmitOptions::default();
    let bytes = emit_unchecked(s, &options).expect("bypass emits");
    judge(s, &bytes, &options.namespace)
}

fn pairs(findings: &[Finding]) -> Vec<(&'static str, String)> {
    findings
        .iter()
        .map(|f| (f.code, f.subject.clone()))
        .collect()
}

/// What the engine does with a stage-1 defect when stage 1 is bypassed.
#[derive(Clone, Copy, Debug)]
enum EngineSays {
    /// The engine reports this very code at the same subject (owner: engine).
    Same,
    /// The engine reports this pre-empted code at the same subject (owner: author-only).
    Preempts(&'static str),
    /// The engine reports nothing at that subject.
    Silent,
}

/// The authoring note's finding-code table, restricted to the stage-1 rows that carry an engine
/// code or pre-empt one (author-only rows with no engine counterpart have nothing to compare).
const PARITY: &[(&str, EngineSays)] = &[
    ("empty-sequence", EngineSays::Preempts("malformed-document")),
    ("duplicate-name", EngineSays::Preempts("duplicate-id")),
    ("class-not-found", EngineSays::Same),
    ("unknown-parameter", EngineSays::Silent),
    ("duplicate-parameter", EngineSays::Preempts("duplicate-id")),
    ("missing-required-parameter", EngineSays::Same),
    ("parameter-kind-mismatch", EngineSays::Same),
    ("parameter-out-of-range", EngineSays::Same),
    ("non-finite-parameter", EngineSays::Same),
    (
        "unknown-enum-literal",
        EngineSays::Preempts("grounding-failed"),
    ),
    (
        "unknown-parameter-reference",
        EngineSays::Preempts("grounding-failed"),
    ),
    (
        "unknown-source",
        EngineSays::Preempts("unresolved-reference"),
    ),
    (
        "unknown-target",
        EngineSays::Preempts("unresolved-reference"),
    ),
    ("unknown-port", EngineSays::Preempts("unresolved-reference")),
];

/// One sequence carrying exactly the defect `code` names, built from the worked example.
fn defect(code: &str) -> Sequence {
    let mut s = econ_enable();
    match code {
        "empty-sequence" => return Sequence::new(id("nothing")),
        "duplicate-name" => s.blocks.push(Block::new(id("TOut"), "CDL.Reals.Abs")),
        "class-not-found" => s.blocks[0].class_path = "CDL.Reals.Subtrakt".to_owned(),
        "unknown-parameter" => set(hys(&mut s), "uMid", real(0.5)),
        "duplicate-parameter" => hys(&mut s)
            .assignments
            .push(Assignment::new(id("uLow"), real(0.25))),
        "missing-required-parameter" => hys(&mut s)
            .assignments
            .retain(|a| a.name.as_str() != "uLow"),
        "parameter-kind-mismatch" => set(
            hys(&mut s),
            "uLow",
            ParamValue::Literal(Literal::Text("0".to_owned())),
        ),
        "parameter-out-of-range" => set(hys(&mut s), "uLow", real(2.0)),
        "non-finite-parameter" => set(hys(&mut s), "uLow", real(f64::NEG_INFINITY)),
        "unknown-enum-literal" => {
            let mut pid = Block::new(id("con"), "CDL.Reals.PID");
            pid.assignments.push(Assignment::new(
                id("controllerType"),
                ParamValue::Literal(Literal::EnumMember(EnumMember::parse(
                    "SimpleController.PIX",
                ))),
            ));
            s.blocks.push(pid);
        }
        "unknown-parameter-reference" => {
            set(hys(&mut s), "uHigh", ParamValue::Ref(id("dTHyst")));
        }
        "unknown-source" => s.connections[0].source = Endpoint::parse("TOutCutt"),
        "unknown-target" => s.connections[3].target = Endpoint::parse("ecoEnaa"),
        "unknown-port" => s.connections[0].target = Endpoint::parse("sub.u9"),
        other => panic!("no defect for {other}"),
    }
    s
}

#[test]
fn the_worked_example_is_clean_in_both_stages() {
    let s = econ_enable();
    assert_eq!(validate_author_side(&s), vec![]);
    assert_eq!(engine(&s), vec![]);
}

#[test]
fn every_parity_row_is_a_stage_one_code() {
    for (code, _) in PARITY {
        assert!(super::super::STAGE_ONE_CODES.contains(code), "{code}");
    }
}

#[test]
fn each_stage_one_defect_meets_the_engine_code_the_table_names() {
    for &(code, expected) in PARITY {
        let s = defect(code);
        let author = pairs(&validate_author_side(&s));
        assert_eq!(
            author.len(),
            1,
            "{code}: one stage-1 finding, got {author:?}"
        );
        let (author_code, subject) = &author[0];
        assert_eq!(author_code, &code, "{code}");
        let engine = pairs(&engine(&s));
        let at_subject: Vec<&str> = engine
            .iter()
            .filter(|(_, s)| s == subject)
            .map(|(c, _)| *c)
            .collect();
        match expected {
            EngineSays::Same => assert!(
                at_subject.contains(&code),
                "{code}: the engine reports {engine:?}"
            ),
            EngineSays::Preempts(engine_code) => assert!(
                at_subject.contains(&engine_code),
                "{code}: expected `{engine_code}` at `{subject}`, the engine reports {engine:?}"
            ),
            EngineSays::Silent => assert_eq!(engine, vec![], "{code}"),
        }
    }
}

#[test]
fn every_engine_finding_is_an_error_from_the_engine() {
    for &(code, _) in PARITY {
        for finding in engine(&defect(code)) {
            assert_eq!(
                finding.severity,
                crate::validate::Severity::Error,
                "{finding}"
            );
            assert_eq!(finding.origin, crate::validate::Origin::Engine, "{finding}");
            assert_eq!(finding.span, None, "{finding}");
        }
    }
}
