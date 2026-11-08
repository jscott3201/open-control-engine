//! Emission order of boundary-input assignment-cardinality refusals.

use oce_diag::DiagCode;
use oce_model::ConnectorId;

use super::AssignmentTracker;

#[test]
fn reassigned_child_inputs_are_refused_in_connector_order() {
    let mut tracker = AssignmentTracker::default();
    // Record in descending connector order, each target twice from the same source.
    for id in (0..16u32).rev() {
        let target = format!("urn:child:u{id:02}");
        assert!(tracker.record(ConnectorId(id), "urn:top:x", &target));
        assert!(!tracker.record(ConnectorId(id), "urn:top:x", &target));
    }
    let mut diags = Vec::new();
    tracker.emit_diagnostics(&mut diags);

    let subjects: Vec<&str> = diags
        .iter()
        .map(|diag| diag.subject.as_deref().expect("refusal names its input"))
        .collect();
    let expected: Vec<String> = (0..16).map(|id| format!("urn:child:u{id:02}")).collect();
    assert_eq!(subjects, expected);
    assert!(
        diags
            .iter()
            .all(|diag| diag.code == DiagCode::SingleAssignment
                && diag.message == "input is multiply driven (in-degree 2)")
    );
}

#[test]
fn single_assignments_pass_and_distinct_sources_are_named() {
    let mut tracker = AssignmentTracker::default();
    assert!(tracker.record(ConnectorId(9), "urn:top:a", "urn:child:p"));
    assert!(!tracker.record(ConnectorId(9), "urn:top:b", "urn:child:p"));
    assert!(tracker.record(ConnectorId(2), "urn:top:a", "urn:child:q"));
    let mut diags = Vec::new();
    tracker.emit_diagnostics(&mut diags);
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].subject.as_deref(), Some("urn:child:p"));
    assert_eq!(
        diags[0].message,
        "input is driven by distinct boundary inputs"
    );
}
