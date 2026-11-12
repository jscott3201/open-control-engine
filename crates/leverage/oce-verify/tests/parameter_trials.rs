//! The subject's parameter trials at bind (vectors note, part 2's reading 8; work-budget note,
//! ruling 7): each distinct effective edit list is tried once, on a fresh engine, in the order of
//! the first scenario that has it. Trying a list once rather than once per scenario changes no
//! refusal and no receipt byte: the first scenario in file order holding a refused list is the one
//! named either way, and a trial writes nothing to the receipt.
//!
//! Beside them, the work-budget note's bound on the passes those trials take: no class of the
//! catalog chains more than two parameters through a rule `Engine::set_param` checks against
//! another parameter.

mod support;

use oce_api::CatalogRule;
use oce_verify::Receipt;

use support::{receipt, refused, subject};

/// A hysteresis set on a 30-step clock with set-level `params` body `set` and `scenarios`.
fn set(set_params: &str, scenarios: &[&str]) -> String {
    format!(
        r#"{{"schema": "oce/vectors/v1", "clock": {{"step_s": 60, "horizon_s": 1800}}, "params": {{{set_params}}}, "scenarios": [{}]}}"#,
        scenarios.join(", ")
    )
}

/// A scenario holding `u` at `u`, with its own `params` body, expecting `y` to be `y` throughout.
fn scenario(name: &str, params: &str, u: f64, y: bool) -> String {
    format!(
        r#"{{"name": "{name}", "params": {{{params}}}, "inputs": {{"u": {u:?}}}, "expect": [{{"output": "y", "from_s": 0, "to_s": 1800, "equals": {y}}}]}}"#
    )
}

#[test]
fn shared_edit_lists_leave_the_receipt_as_it_was() {
    // Three distinct effective lists over five scenarios: {uHigh 3, uLow 0.25} three times,
    // {uLow 0.5} and {uLow 0.25} once each. The receipt id was pinned before the trials were
    // deduplicated; a trial writes nothing to the receipt, so it must not move.
    let scenarios = [
        scenario("a", r#""uHigh": 3.0"#, 2.5, false),
        scenario("b", r#""uHigh": 3.0"#, 2.5, false),
        scenario("c", r#""uLow": 0.5"#, 0.75, false),
        scenario("d", "", 0.0, false),
        scenario("e", r#""uHigh": 3.0"#, 3.5, true),
    ];
    let scenarios: Vec<&str> = scenarios.iter().map(String::as_str).collect();
    let receipt = receipt(&set(r#""uLow": 0.25"#, &scenarios), &subject("hysteresis"));
    assert!(receipt.header.passed);
    assert_eq!(digest_without_version(&receipt), PINNED_DIGEST);
}

/// The digest of the receipt above, taken before the trials were deduplicated.
const PINNED_DIGEST: &str = "cfe2512073fac396f437ab73eaa9ed2c";

/// FNV-1a-128 of the canonical receipt with its `engine_version` and `receipt_id` lines cut, so
/// a workspace version bump leaves the pin alone while every other byte stays covered.
fn digest_without_version(receipt: &Receipt) -> String {
    let text = receipt.to_canonical_json();
    let kept: Vec<&str> = text
        .lines()
        .filter(|line| !line.contains("\"engine_version\"") && !line.contains("\"receipt_id\""))
        .collect();
    assert_eq!(kept.len() + 2, text.lines().count(), "both lines are cut");
    let mut hash = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d_u128;
    for byte in kept.join("\n").bytes() {
        hash ^= u128::from(byte);
        hash = hash.wrapping_mul(0x0000_0000_0100_0000_0000_0000_0000_013b);
    }
    format!("{hash:032x}")
}

#[test]
fn a_refused_list_is_named_at_the_first_scenario_that_holds_it() {
    // The subject's band is (1, 2): uHigh 0.5 is below uLow in every pass.
    let hysteresis = subject("hysteresis");
    for (scenarios, place) in [
        // The refused list first appears in the second scenario and repeats after it.
        (
            vec![
                scenario("a", r#""uHigh": 3.0"#, 0.0, false),
                scenario("b", r#""uHigh": 0.5"#, 0.0, false),
                scenario("c", r#""uHigh": 3.0"#, 0.0, false),
                scenario("d", r#""uHigh": 0.5"#, 0.0, false),
            ],
            "scenarios[1]",
        ),
        // An admitted list repeats before the refused one appears.
        (
            vec![
                scenario("a", r#""uHigh": 3.0"#, 0.0, false),
                scenario("b", r#""uHigh": 3.0"#, 0.0, false),
                scenario("c", r#""uHigh": 0.5"#, 0.0, false),
            ],
            "scenarios[2]",
        ),
    ] {
        let scenarios: Vec<&str> = scenarios.iter().map(String::as_str).collect();
        let error = refused(&set("", &scenarios), &hysteresis);
        assert_eq!(
            (error.code(), error.subject(), error.cause()),
            ("param-refused", place, Some("param-range"))
        );
    }
}

/// The parameters one catalog rule ties together at `Engine::set_param`, when it reads another
/// parameter of the same block beside the edited one; an array's elements count as one node. The
/// match is exhaustive, so a rule kind the facade adds does not compile here until it is placed.
fn relation(rule: &CatalogRule) -> Option<(String, String)> {
    match rule {
        CatalogRule::RealLessOrEqual { lower, upper } => Some(((*lower).into(), (*upper).into())),
        CatalogRule::IntegerProductLessOrEqualConstant { left, right, .. } => {
            Some(((*left).into(), (*right).into()))
        }
        CatalogRule::RealTimesIntegerInclusiveRange { real, integer, .. } => {
            Some(((*real).into(), (*integer).into()))
        }
        CatalogRule::IntegerArrayElementsInRange { base, max, .. } => {
            Some((format!("{base}[]"), (*max).into()))
        }
        // Each of these reads the edited value alone at `set_param`, or is not checked there.
        CatalogRule::Required { .. }
        | CatalogRule::Structural { .. }
        | CatalogRule::StructuralArrayElements { .. }
        | CatalogRule::Boolean { .. }
        | CatalogRule::Real { .. }
        | CatalogRule::RealFinite { .. }
        | CatalogRule::RealGreaterThan { .. }
        | CatalogRule::RealFiniteGreaterThan { .. }
        | CatalogRule::IntegerGreaterOrEqual { .. }
        | CatalogRule::IntegerLessOrEqualConstant { .. }
        | CatalogRule::IntegerArrayElements { .. }
        | CatalogRule::RealArrayElements { .. }
        | CatalogRule::RealMatrixElements { .. }
        | CatalogRule::TimeTableMatrix { .. }
        | CatalogRule::TimeTableOffset { .. }
        | CatalogRule::BooleanArrayElements { .. }
        | CatalogRule::BooleanArrayTrueCountEquals { .. }
        | CatalogRule::EnumMembers { .. }
        | CatalogRule::RealGreaterOrEqual { .. }
        | CatalogRule::RealLessOrEqualConstant { .. }
        | CatalogRule::RealLessOrEqualWarning { .. }
        | CatalogRule::RealGreaterOrEqualScaledWarning { .. }
        | CatalogRule::RealEqualWarning { .. } => None,
    }
}

/// The most parameters one simple path through `edges` visits.
fn longest_chain(edges: &[(String, String)]) -> usize {
    fn walk(node: &str, edges: &[(String, String)], seen: &mut Vec<String>) -> usize {
        let mut best = 0;
        for (a, b) in edges {
            let next = if a == node {
                b
            } else if b == node {
                a
            } else {
                continue;
            };
            if !seen.contains(next) {
                seen.push(next.clone());
                best = best.max(walk(next, edges, seen));
                seen.pop();
            }
        }
        best + 1
    }
    edges
        .iter()
        .flat_map(|(a, b)| [a, b])
        .map(|start| walk(start, edges, &mut vec![start.clone()]))
        .max()
        .unwrap_or(0)
}

#[test]
fn no_class_chains_more_than_two_parameters_through_its_rules() {
    // A later pass admits an edit only when a rule ties it to one admitted before it, so an edit
    // list takes at most as many passes as the longest such chain in one class. The work-budget
    // note counts on two; a catalog change that lengthens a chain fails here first.
    let mut longest = 0;
    let mut at_longest = Vec::new();
    for entry in oce_api::catalog() {
        let edges: Vec<(String, String)> = entry.param_rules.iter().filter_map(relation).collect();
        let chain = longest_chain(&edges);
        if chain > longest {
            longest = chain;
            at_longest.clear();
        }
        if chain == longest && chain > 0 {
            at_longest.push(entry.class_path);
        }
    }
    assert_eq!(longest, 2, "{at_longest:?}");
    assert!(
        at_longest.contains(&"CDL.Reals.Hysteresis"),
        "{at_longest:?}"
    );
    assert_eq!(at_longest.len(), 14, "{at_longest:?}");
}
