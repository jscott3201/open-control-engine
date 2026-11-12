//! The stage-1 / engine parity test (authoring note, "Finding codes"; the table there is this
//! test's fixture, row for row in [`PARITY`]).
//!
//! Each defect is emitted with stage 1 bypassed ([`emit_unchecked`]) and loaded in a throwaway
//! engine; the engine's findings are mapped back through the same code stage 2 uses. A row whose
//! owner is the engine asserts the engine reports the stage-1 code at the stage-1 subject; an
//! author-only row that pre-empts an engine code asserts that code at the same subject; the
//! `unknown-parameter` row pins that the engine reports nothing at all (GitHub #467). The
//! `single-assignment` row (author and engine) is same-code only: stage 1 reports the repeating
//! connection, the engine the input, which stage 2 maps to the first connection driving it.

use crate::emit::{EmitOptions, emit_unchecked};
use crate::identifier::Identifier;
use crate::model::{
    Assignment, Block, Bound, Boundary, Connection, Endpoint, Literal, MemberRef, ParamValue,
    ParameterType, Sequence, SequenceParameter, SignalType,
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
    /// The engine reports this very code, at the input it names rather than at the repeated
    /// connection: stage 1 points at the second driver, the engine's subject maps to the first.
    SameCode,
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
    // The suffix never reaches the engine, which loads the base-unit value as written.
    ("unit-suffix-mismatch", EngineSays::Silent),
    (
        "unknown-enum-literal",
        EngineSays::Preempts("grounding-failed"),
    ),
    ("unknown-enum-type", EngineSays::Same),
    (
        "unknown-parameter-reference",
        EngineSays::Preempts("grounding-failed"),
    ),
    ("invalid-bound", EngineSays::Preempts("malformed-document")),
    ("non-finite-bound", EngineSays::Same),
    ("crossed-bounds", EngineSays::Same),
    (
        "unknown-source",
        EngineSays::Preempts("unresolved-reference"),
    ),
    (
        "unknown-target",
        EngineSays::Preempts("unresolved-reference"),
    ),
    ("unknown-port", EngineSays::Preempts("unresolved-reference")),
    ("single-assignment", EngineSays::SameCode),
];

/// The note's finding-code table for every stage-1 code, as `(code, owner, pre-empted engine
/// code)`: owner `engine` or `author`, the pre-empted column `None` for "—". Kept in the note's
/// row order and checked against [`super::super::STAGE_ONE_CODES`] and [`PARITY`] below, so a
/// row added to one and not the others fails here.
const NOTE_TABLE: &[(&str, &str, Option<&str>)] = &[
    ("empty-sequence", "author", Some("malformed-document")),
    ("invalid-name", "author", None),
    ("duplicate-name", "author", Some("duplicate-id")),
    ("class-not-found", "engine", None),
    ("class-not-authorable", "author", None),
    ("unknown-parameter", "author", Some("none")),
    ("parameter-not-settable", "author", None),
    ("duplicate-parameter", "author", Some("duplicate-id")),
    ("missing-required-parameter", "engine", None),
    ("parameter-kind-mismatch", "engine", None),
    ("parameter-out-of-range", "engine", None),
    ("non-finite-parameter", "engine", None),
    ("unit-suffix-mismatch", "author", Some("none")),
    ("unknown-enum-literal", "author", Some("grounding-failed")),
    ("unknown-enum-type", "engine", None),
    (
        "unknown-parameter-reference",
        "author",
        Some("grounding-failed"),
    ),
    ("malformed-connection", "author", None),
    ("unknown-source", "author", Some("unresolved-reference")),
    ("unknown-target", "author", Some("unresolved-reference")),
    ("unknown-port", "author", Some("unresolved-reference")),
    ("invalid-semantic-point", "author", None),
    ("invalid-bound", "author", Some("malformed-document")),
    ("non-finite-bound", "engine", None),
    ("crossed-bounds", "engine", None),
    ("invalid-provenance-id", "author", None),
    (
        "single-assignment",
        "author and engine",
        Some("single-assignment"),
    ),
];

#[test]
fn the_parity_table_covers_exactly_the_notes_engine_rows() {
    let codes: Vec<&str> = NOTE_TABLE.iter().map(|(c, _, _)| *c).collect();
    assert_eq!(codes, super::super::STAGE_ONE_CODES);
    let expected: Vec<(&str, String)> = NOTE_TABLE
        .iter()
        .filter(|(_, owner, preempted)| *owner == "engine" || preempted.is_some())
        .map(|(code, owner, preempted)| {
            let says = match (*owner, preempted) {
                ("author and engine", _) => "SameCode".to_owned(),
                ("engine", _) => "Same".to_owned(),
                (_, Some("none")) => "Silent".to_owned(),
                (_, Some(engine)) => format!("Preempts({engine:?})"),
                (_, None) => unreachable!(),
            };
            (*code, says)
        })
        .collect();
    let actual: Vec<(&str, String)> = PARITY
        .iter()
        .map(|(code, says)| (*code, format!("{says:?}")))
        .collect();
    let mut expected = expected;
    let mut actual = actual;
    expected.sort();
    actual.sort();
    assert_eq!(actual, expected);
}

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
        "unit-suffix-mismatch" => {
            // `uLow = 0 h`: Hysteresis declares no unit for `uLow`, so a scaled suffix has
            // nothing to scale to; the value, 0 s, is the worked example's own.
            let assignment = hys(&mut s)
                .assignments
                .iter_mut()
                .find(|a| a.name.as_str() == "uLow")
                .expect("uLow");
            assignment.suffix = Some("h".to_owned());
            assignment.scaled_from = Some(Literal::Integer(0));
        }
        "unknown-enum-literal" => {
            let mut pid = Block::new(id("con"), "CDL.Reals.PID");
            pid.assignments.push(Assignment::new(
                id("controllerType"),
                ParamValue::Literal(Literal::Member(MemberRef::parse("SimpleController.PIX"))),
            ));
            s.blocks.push(pid);
        }
        "unknown-parameter-reference" => {
            set(hys(&mut s), "uHigh", ParamValue::Ref(id("dTHyst")));
        }
        "unknown-enum-type" => s.params.push(SequenceParameter::new(
            id("eneStd"),
            ParameterType::parse("EnergyStandards"),
            Literal::Member(MemberRef::parse("EnergyStandards.ASHRAE90_1")),
        )),
        "invalid-bound" => s.outputs[0].min = Some(Bound::Integer(0)),
        "non-finite-bound" => s.inputs[0].max = Some(Bound::Real(f64::NAN)),
        "crossed-bounds" => {
            s.inputs[0].min = Some(Bound::Real(30.0));
            s.inputs[0].max = Some(Bound::Integer(-10));
        }
        "unknown-source" => s.connections[0].source = Endpoint::parse("TOutCutt"),
        "unknown-target" => s.connections[3].target = Endpoint::parse("ecoEnaa"),
        "unknown-port" => s.connections[0].target = Endpoint::parse("sub.u9"),
        "single-assignment" => {
            s.blocks.push(Block::new(id("abs"), "CDL.Reals.Abs"));
            s.connections.push(Connection::new(
                Endpoint::parse("TOut"),
                Endpoint::parse("abs.u"),
            ));
            s.connections.push(Connection::new(
                Endpoint::parse("abs.y"),
                Endpoint::parse("hys.u"),
            ));
        }
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
            EngineSays::SameCode => assert!(
                engine.iter().any(|(c, _)| *c == code),
                "{code}: the engine reports {engine:?}"
            ),
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

fn connect(s: &mut Sequence, from: &str, to: &str) {
    s.connections
        .push(Connection::new(Endpoint::parse(from), Endpoint::parse(to)));
}

#[test]
fn two_block_outputs_on_one_input_meet_the_engines_single_assignment() {
    let mut s = econ_enable();
    s.blocks.push(Block::new(id("abs"), "CDL.Reals.Abs"));
    connect(&mut s, "TOut", "abs.u");
    connect(&mut s, "abs.y", "hys.u");
    assert_eq!(
        pairs(&validate_author_side(&s)),
        vec![("single-assignment", "connection[5]".to_owned())]
    );
    assert_eq!(
        pairs(&engine(&s)),
        vec![("single-assignment", "connection[2]".to_owned())]
    );
}

#[test]
fn two_boundary_inputs_on_one_input_meet_the_engines_single_assignment() {
    let mut s = econ_enable();
    connect(&mut s, "TOut", "sub.u1");
    assert_eq!(
        pairs(&validate_author_side(&s)),
        vec![("single-assignment", "connection[4]".to_owned())]
    );
    let engine = engine(&s);
    assert_eq!(
        pairs(&engine),
        vec![("single-assignment", "connection[0]".to_owned())]
    );
    assert!(
        engine[0].detail.contains("distinct boundary inputs"),
        "{}",
        engine[0].detail
    );
}

#[test]
fn a_block_output_beside_a_boundary_input_breaks_the_engines_single_assignment() {
    // A block input driven by a block output and a boundary input: stage 1 refuses the repeated
    // target, and with stage 1 bypassed the engine refuses it too at the input (GitHub #472,
    // fixed in GitHub #477), mapped to the first connection driving it.
    let mut s = econ_enable();
    connect(&mut s, "TOut", "hys.u");
    assert_eq!(
        pairs(&validate_author_side(&s)),
        vec![("single-assignment", "connection[4]".to_owned())]
    );
    let findings = engine(&s);
    assert_eq!(
        pairs(&findings),
        vec![("single-assignment", "connection[2]".to_owned())]
    );
    assert_eq!(findings[0].origin, crate::validate::Origin::Engine);
    // The engine's subject is the input `hys.u`; stage 2 names the first connection driving it.
    assert!(
        findings[0]
            .detail
            .contains("a block output and a boundary input"),
        "{}",
        findings[0].detail
    );
}

#[test]
fn a_refusal_without_a_coded_diagnostic_is_load_failed() {
    let s = econ_enable();
    let findings = judge(&s, b"{\"@graph\": [", "urn:oce:author:");
    assert_eq!(
        pairs(&findings),
        vec![("load-failed", "sequence".to_owned())]
    );
    assert_eq!(findings[0].origin, crate::validate::Origin::Engine);
    assert!(!findings[0].detail.is_empty());
}

#[test]
fn a_diagnostic_on_the_root_maps_to_the_sequence() {
    // A second top composite: `composite/root-count` names the first candidate, the root.
    let s = econ_enable();
    let options = EmitOptions::default();
    let text = String::from_utf8(emit_unchecked(&s, &options).expect("emit")).expect("utf8");
    let doubled = text.replacen(
        "\"@graph\": [",
        "\"@graph\": [{\"@id\": \"urn:oce:other:x\", \"@type\": \"S231:Block\", \"S231:containsBlock\": [{\"@id\": \"urn:oce:other:x.c\"}]}, {\"@id\": \"urn:oce:other:x.c\", \"@type\": \"http://example.org#Buildings.Controls.OBC.CDL.Logical.Sources.Constant\"},",
        1,
    );
    let findings = judge(&s, doubled.as_bytes(), &options.namespace);
    let root: Vec<_> = findings
        .iter()
        .filter(|f| f.code == "malformed-document")
        .collect();
    assert_eq!(root.len(), 1, "{findings:#?}");
    // `urn:oce:other:x` is first in `@graph`, so it is the subject: outside the scheme.
    assert_eq!(root[0].subject, "sequence");
    assert!(
        root[0]
            .detail
            .starts_with("engine subject `urn:oce:other:x`"),
        "{}",
        root[0].detail
    );
}

#[test]
fn subjects_outside_the_scheme_stay_on_the_sequence_with_the_raw_subject() {
    let s = econ_enable();
    let scheme = crate::emit::Scheme::new("urn:oce:author:", "econ_enable");
    for raw in [
        "connector#3",
        "block#1",
        "urn:other:econ_enable.hys",
        "urn:oce:author:econ_enabled.hys",
    ] {
        let (anchor, subject, detail) =
            super::map_subject(&s, &scheme, Some(raw), "message".to_owned());
        assert_eq!(anchor, super::super::Anchor::Sequence, "{raw}");
        assert_eq!(subject, "sequence");
        assert_eq!(detail, format!("engine subject `{raw}`: message"));
    }
    let (_, subject, detail) = super::map_subject(&s, &scheme, None, "m".to_owned());
    assert_eq!((subject.as_str(), detail.as_str()), ("sequence", "m"));
    let cases = [
        ("urn:oce:author:econ_enable", "sequence"),
        ("urn:oce:author:econ_enable.dTHys", "dTHys"),
        ("urn:oce:author:econ_enable.TOut", "TOut"),
        ("urn:oce:author:econ_enable.ecoEna", "ecoEna"),
        ("urn:oce:author:econ_enable.hys", "hys"),
        ("urn:oce:author:econ_enable.hys.uHigh", "hys.uHigh"),
        ("urn:oce:author:econ_enable.hys.u", "connection[2]"),
        ("urn:oce:author:econ_enable.sub.y", "connection[2]"),
    ];
    for (raw, expected) in cases {
        let (_, subject, _) = super::map_subject(&s, &scheme, Some(raw), "m".to_owned());
        assert_eq!(subject, expected, "{raw}");
    }
}

#[test]
fn a_mismatched_enumeration_reference_meets_the_engines_kind_check() {
    // An enumeration sequence parameter referenced by a Real block parameter: stage 1 refuses the
    // pairing as `parameter-kind-mismatch`; with stage 1 bypassed the engine refuses the same
    // assignment with the same code (issue #492, the importer note's decision 2).
    let mut s = econ_enable();
    s.params.push(SequenceParameter::new(
        id("conTyp"),
        ParameterType::parse("SimpleController"),
        Literal::Member(MemberRef::parse("SimpleController.PI")),
    ));
    set(hys(&mut s), "uHigh", ParamValue::Ref(id("conTyp")));
    assert_eq!(
        pairs(&validate_author_side(&s)),
        vec![("parameter-kind-mismatch", "hys.uHigh".to_owned())]
    );
    let engine = pairs(&engine(&s));
    assert!(
        engine.contains(&("parameter-kind-mismatch", "hys.uHigh".to_owned())),
        "{engine:?}"
    );
}

#[test]
fn a_real_bound_on_an_integer_boundary_meets_the_engines_malformed_document() {
    let mut s = econ_enable();
    s.inputs
        .push(Boundary::new(id("uStage"), SignalType::Integer));
    s.inputs[2].min = Some(Bound::Real(0.5));
    s.blocks
        .push(Block::new(id("toReal"), "CDL.Conversions.IntegerToReal"));
    s.outputs
        .push(Boundary::new(id("yStage"), SignalType::Real));
    for (from, to) in [("uStage", "toReal.u"), ("toReal.y", "yStage")] {
        s.connections
            .push(Connection::new(Endpoint::parse(from), Endpoint::parse(to)));
    }
    assert_eq!(
        pairs(&validate_author_side(&s)),
        vec![("invalid-bound", "uStage".to_owned())]
    );
    let engine = pairs(&engine(&s));
    assert!(
        engine.contains(&("malformed-document", "uStage".to_owned())),
        "{engine:?}"
    );
}

#[test]
fn an_integer_bound_outside_i32_preempts_a_silent_narrowing() {
    // Kind: pre-empts a silent narrowing. The engine's Integer domain is `i32`; with stage 1
    // bypassed an Integer input declared over the whole `i64` range loads warning-free and
    // reports the `i32` range, so the loaded domain is not the declared one. That is why stage 1
    // refuses the bound as `invalid-bound` (importer note, decision 6, amended by #493).
    let mut s = econ_enable();
    s.inputs
        .push(Boundary::new(id("uStage"), SignalType::Integer));
    s.inputs[2].min = Some(Bound::Integer(i64::MIN));
    s.inputs[2].max = Some(Bound::Integer(i64::from(i32::MAX) + 1));
    s.blocks
        .push(Block::new(id("toReal"), "CDL.Conversions.IntegerToReal"));
    s.outputs
        .push(Boundary::new(id("yStage"), SignalType::Real));
    for (from, to) in [("uStage", "toReal.u"), ("toReal.y", "yStage")] {
        s.connections
            .push(Connection::new(Endpoint::parse(from), Endpoint::parse(to)));
    }
    assert_eq!(
        pairs(&validate_author_side(&s)),
        vec![
            ("invalid-bound", "uStage".to_owned()),
            ("invalid-bound", "uStage".to_owned())
        ]
    );
    assert_eq!(pairs(&engine(&s)), vec![], "the engine says nothing");
    let bytes = emit_unchecked(&s, &EmitOptions::default()).expect("bypass emits");
    let mut loaded = oce_api::Engine::new();
    let (report, _) = loaded.load_cxf(&bytes).expect("the engine loads it");
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let inputs = loaded.input_definitions().expect("definitions");
    let stage = inputs
        .iter()
        .find(|d| d.path.ends_with(".uStage"))
        .expect("uStage");
    assert!(
        matches!(stage.min, Some(oce_api::Value::Integer(v)) if v == i64::from(i32::MIN))
            && matches!(stage.max, Some(oce_api::Value::Integer(v)) if v == i64::from(i32::MAX)),
        "the engine narrows to i32: {stage:?}"
    );
}
