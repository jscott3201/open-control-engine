//! Emission order of boundary-input assignment-cardinality refusals.

use oce_diag::DiagCode;
use oce_model::{BlockId, Connection, Connector, ConnectorId, Dir, ValueType};

use super::AssignmentTracker;
use crate::resolve::connection_endpoints::EndpointId;
use oce_diag::DiagnosticList;

#[test]
fn reassigned_child_inputs_are_refused_in_connector_order() {
    let mut tracker = AssignmentTracker::default();
    // Record in descending connector order, each target twice from the same source.
    for id in (0..16u32).rev() {
        let target = format!("urn:child:u{id:02}");
        assert!(tracker.record(ConnectorId(id), EndpointId(0), &target));
        assert!(!tracker.record(ConnectorId(id), EndpointId(0), &target));
    }
    let mut diags = DiagnosticList::new();
    tracker.emit_diagnostics(&[], &[], &mut diags);

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
    assert!(tracker.record(ConnectorId(9), EndpointId(1), "urn:child:p"));
    assert!(!tracker.record(ConnectorId(9), EndpointId(2), "urn:child:p"));
    assert!(tracker.record(ConnectorId(2), EndpointId(1), "urn:child:q"));
    let mut diags = DiagnosticList::new();
    tracker.emit_diagnostics(&[], &[], &mut diags);
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].subject.as_deref(), Some("urn:child:p"));
    assert_eq!(
        diags[0].message,
        "input is driven by distinct boundary inputs"
    );
}

/// Connectors 0 (output) and 1 (input) on block 0, then inputs 2 and 3 on block 1.
fn wiring_table() -> Vec<Connector> {
    vec![
        Connector::new(ConnectorId(0), BlockId(0), Dir::Out, ValueType::Real, 0),
        Connector::new(ConnectorId(1), BlockId(0), Dir::In, ValueType::Real, 1),
        Connector::new(ConnectorId(2), BlockId(1), Dir::In, ValueType::Real, 2),
        Connector::new(ConnectorId(3), BlockId(1), Dir::In, ValueType::Real, 3),
    ]
}

fn wire(from: u32, to: u32) -> Connection {
    Connection {
        from: ConnectorId(from),
        to: ConnectorId(to),
    }
}

#[test]
fn a_boundary_reached_input_with_a_wire_from_an_output_is_refused_as_mixed() {
    let mut tracker = AssignmentTracker::default();
    assert!(tracker.record(ConnectorId(2), EndpointId(3), "urn:child:p"));
    assert!(tracker.record(ConnectorId(3), EndpointId(3), "urn:child:q"));
    let mut diags = DiagnosticList::new();
    tracker.emit_diagnostics(&[wire(0, 2)], &wiring_table(), &mut diags);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, DiagCode::SingleAssignment);
    assert_eq!(diags[0].subject.as_deref(), Some("urn:child:p"));
    assert_eq!(
        diags[0].message,
        "input is driven by a block output and a boundary input"
    );
}

#[test]
fn the_mixed_refusal_takes_precedence_over_the_boundary_count() {
    // Two distinct boundary inputs AND a wire: one refusal per input, naming the mixed shape.
    let mut tracker = AssignmentTracker::default();
    assert!(tracker.record(ConnectorId(2), EndpointId(3), "urn:child:p"));
    assert!(!tracker.record(ConnectorId(2), EndpointId(4), "urn:child:p"));
    let mut diags = DiagnosticList::new();
    tracker.emit_diagnostics(&[wire(0, 2)], &wiring_table(), &mut diags);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(
        diags[0].message,
        "input is driven by a block output and a boundary input"
    );
}

#[test]
fn a_wire_from_an_input_is_not_counted_as_a_driver() {
    // An input→input edge is Step 10's direction refusal, not a second driver.
    let mut tracker = AssignmentTracker::default();
    assert!(tracker.record(ConnectorId(2), EndpointId(3), "urn:child:p"));
    let mut diags = DiagnosticList::new();
    tracker.emit_diagnostics(&[wire(1, 2)], &wiring_table(), &mut diags);
    assert!(diags.is_empty(), "{diags:?}");
}

#[test]
fn a_wire_into_an_input_no_boundary_reaches_is_not_this_pass() {
    let mut tracker = AssignmentTracker::default();
    assert!(tracker.record(ConnectorId(2), EndpointId(3), "urn:child:p"));
    let mut diags = DiagnosticList::new();
    tracker.emit_diagnostics(&[wire(0, 3)], &wiring_table(), &mut diags);
    assert!(diags.is_empty(), "{diags:?}");
}

#[test]
fn a_wire_whose_source_is_out_of_range_is_not_counted() {
    let mut tracker = AssignmentTracker::default();
    assert!(tracker.record(ConnectorId(2), EndpointId(3), "urn:child:p"));
    let mut diags = DiagnosticList::new();
    tracker.emit_diagnostics(&[wire(99, 2)], &wiring_table(), &mut diags);
    assert!(diags.is_empty(), "{diags:?}");
}

/// Two blocks under one instance IRI, `urn:a`, that differ: the first binds `k`, the second binds
/// `m` and has its two inputs driven by boundary inputs named `urn:a.m` and `urn:a.in0`. The
/// reader builds such a graph only for a document it refuses anyway (a child listed twice), where
/// both blocks agree; the check is held here to every block under the IRI, as the set of spelled
/// identities it replaced was (issue #807).
#[test]
fn every_block_under_one_instance_iri_mints_its_own_identities() {
    use std::collections::{BTreeSet, HashSet};
    use std::sync::Arc;

    use oce_model::{BlockInstance, ModelGraph, ParamTable, Value};

    let block = |id: u32, name: &str, inputs: Vec<ConnectorId>| BlockInstance {
        id: BlockId(id),
        class_iri: Arc::from("CDL.Reals.Add"),
        inputs,
        outputs: vec![],
        params: ParamTable {
            values: vec![(Arc::from(name), Value::Real(1.0))],
        },
        decl_order: id,
        instance_iri: Some(Arc::from("urn:a")),
    };
    let mut graph = ModelGraph::new();
    graph.blocks = vec![
        block(0, "k", vec![]),
        block(1, "m", vec![ConnectorId(0), ConnectorId(1)]),
    ];
    graph.connectors = vec![
        Connector::new(ConnectorId(0), BlockId(1), Dir::In, ValueType::Real, 0).with_iri("urn:a.m"),
        Connector::new(ConnectorId(1), BlockId(1), Dir::In, ValueType::Real, 1)
            .with_iri("urn:a.in0"),
    ];
    graph.external_inputs = vec![ConnectorId(0), ConnectorId(1)];
    let doc = crate::parse_document(
        br#"{"@context":{},"@graph":[{"@id":"urn:a.m"},{"@id":"urn:a.in0"}]}"#,
    )
    .expect("parses");
    let represented: HashSet<&str> = ["urn:a.m", "urn:a.in0"].into_iter().collect();
    let mut diags = DiagnosticList::new();
    super::refuse_minted_aliases(
        &doc,
        &represented,
        &graph,
        &HashSet::new(),
        &BTreeSet::new(),
        &mut diags,
    );
    let subjects: Vec<&str> = diags
        .iter()
        .map(|d| d.subject.as_deref().expect("a declaration"))
        .collect();
    assert_eq!(subjects, ["urn:a.m", "urn:a.in0"]);
    assert!(diags.iter().all(|d| d.code == DiagCode::MalformedDocument
        && d.message == "boundary input collides with a canonical export node identity"));
}
