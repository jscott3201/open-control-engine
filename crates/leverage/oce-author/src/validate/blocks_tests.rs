//! The mapping of `class_ports` diagnostics onto findings: one case per engine code the load
//! gate's parameter checks emit, each obtained from a real `class_ports` refusal, plus the
//! backtick reader, the withheld filter and the fallback for a code outside the stage-1 table.

use std::collections::{BTreeMap, BTreeSet};

use oce_api::{ClassPortsError, DiagCode, Diagnostic, Value, class_ports};

use super::{EngineFinding, backticked, engine_finding};
use crate::counted::Counted;
use crate::identifier::Identifier;
use crate::model::{Assignment, Block, Literal};
use crate::validate::{Anchor, STAGE_ONE_CODES};

#[test]
fn backticked_tokens_are_read_in_order() {
    assert_eq!(
        backticked("block `CDL.Reals.Hysteresis` is missing required parameter `uLow`"),
        vec!["CDL.Reals.Hysteresis", "uLow"]
    );
    assert!(backticked("no tokens").is_empty());
    assert_eq!(backticked("unclosed `a"), vec!["a"]);
}

/// A `hys` block assigning `names` (values are irrelevant to the mapping).
fn hysteresis(names: &[&str]) -> Block {
    let mut block = Block::new(Identifier::unchecked("hys"), "CDL.Reals.Hysteresis");
    for name in names {
        block.assignments.push(Assignment::new(
            Identifier::unchecked(*name),
            Literal::Real(0.0),
        ));
    }
    block
}

fn refusal(table: &[(&str, Value)]) -> Vec<Diagnostic> {
    match class_ports("CDL.Reals.Hysteresis", table) {
        Err(ClassPortsError::InvalidParameters { diagnostics, .. }) => diagnostics,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

fn map(block: &Block, withheld: &[&str], diagnostic: &Diagnostic) -> Option<EngineFinding> {
    let mut assigned = BTreeMap::new();
    for (j, a) in block.assignments.iter().enumerate() {
        assigned.entry(Counted(a.name.as_str())).or_insert(j);
    }
    let withheld: BTreeSet<Counted<&str>> = withheld.iter().copied().map(Counted).collect();
    engine_finding(block, 3, &assigned, &withheld, diagnostic)
}

fn only(diagnostics: &[Diagnostic], code: DiagCode) -> &Diagnostic {
    let matching: Vec<_> = diagnostics.iter().filter(|d| d.code == code).collect();
    assert_eq!(matching.len(), 1, "{diagnostics:?}");
    matching[0]
}

/// An engine code, the table that provokes it, the block's assignments, the expected subject and
/// anchor.
type Case = (
    DiagCode,
    Vec<(&'static str, Value)>,
    &'static [&'static str],
    &'static str,
    Anchor,
);

#[test]
fn each_engine_code_maps_to_its_code_and_subject() {
    let real = |v: f64| Value::Real(v);
    let cases: [Case; 5] = [
        (
            DiagCode::MissingRequiredParameter,
            vec![("uHigh", real(1.0))],
            &["uHigh"],
            "hys",
            Anchor::Block(3, None),
        ),
        (
            DiagCode::ParameterKindMismatch,
            vec![("uLow", Value::Boolean(true)), ("uHigh", real(1.0))],
            &["uLow", "uHigh"],
            "hys.uLow",
            Anchor::Block(3, Some(0)),
        ),
        (
            DiagCode::DuplicateParameter,
            vec![
                ("uLow", real(0.0)),
                ("uLow", real(0.0)),
                ("uHigh", real(1.0)),
            ],
            &["uHigh", "uLow"],
            "hys.uLow",
            Anchor::Block(3, Some(1)),
        ),
        (
            DiagCode::ParameterOutOfRange,
            vec![("uLow", real(2.0)), ("uHigh", real(1.0))],
            &["uLow", "uHigh"],
            "hys.uLow",
            Anchor::Block(3, Some(0)),
        ),
        (
            DiagCode::NonFiniteParameter,
            vec![("uLow", real(f64::NAN)), ("uHigh", real(1.0))],
            &["uLow", "uHigh"],
            "hys.uLow",
            Anchor::Block(3, Some(0)),
        ),
    ];
    for (code, table, names, subject, anchor) in cases {
        let diagnostics = refusal(&table);
        let diagnostic = only(&diagnostics, code);
        let mapped = map(&hysteresis(names), &[], diagnostic).expect("mapped");
        assert_eq!(mapped.code, code.as_str(), "{code:?}");
        assert!(STAGE_ONE_CODES.contains(&mapped.code));
        assert_eq!(mapped.subject, subject, "{code:?}: {}", diagnostic.message);
        assert_eq!(mapped.anchor, anchor, "{code:?}");
        assert!(
            mapped.detail.starts_with("block `hys`: "),
            "{}",
            mapped.detail
        );
        assert!(mapped.detail.ends_with(&diagnostic.message));
    }
}

#[test]
fn a_diagnostic_naming_a_withheld_assignment_is_dropped() {
    let diagnostics = refusal(&[("uHigh", Value::Real(1.0))]);
    let missing = only(&diagnostics, DiagCode::MissingRequiredParameter);
    assert!(map(&hysteresis(&["uLow", "uHigh"]), &["uLow"], missing).is_none());
    assert!(map(&hysteresis(&["uHigh"]), &[], missing).is_some());
}

#[test]
fn an_engine_code_outside_stage_one_is_reported_as_out_of_range_naming_it() {
    let diagnostic = Diagnostic::error(
        DiagCode::ResourceLimitExceeded,
        "parameter `uLow` is too large",
    );
    assert!(!STAGE_ONE_CODES.contains(&DiagCode::ResourceLimitExceeded.as_str()));
    let mapped = map(&hysteresis(&["uLow"]), &[], &diagnostic).expect("mapped");
    assert_eq!(mapped.code, "parameter-out-of-range");
    assert_eq!(mapped.subject, "hys.uLow");
    assert!(
        mapped
            .detail
            .contains("engine code `resource-limit-exceeded`"),
        "{}",
        mapped.detail
    );
}
