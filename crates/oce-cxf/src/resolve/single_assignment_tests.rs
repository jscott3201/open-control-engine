//! The wire-only single-assignment check and its hand-off of frame-driven inputs.

use oce_diag::DiagCode;
use oce_model::{BlockId, Connection, Connector, ConnectorId, Dir, ValueType};

use super::check;

/// Two outputs (0, 1) then two inputs (2, 3), all on one block.
fn connectors() -> Vec<Connector> {
    vec![
        Connector::new(ConnectorId(0), BlockId(0), Dir::Out, ValueType::Real, 0),
        Connector::new(ConnectorId(1), BlockId(0), Dir::Out, ValueType::Real, 1),
        Connector::new(ConnectorId(2), BlockId(0), Dir::In, ValueType::Real, 2),
        Connector::new(ConnectorId(3), BlockId(0), Dir::In, ValueType::Real, 3),
    ]
}

fn wire(from: u32, to: u32) -> Connection {
    Connection {
        from: ConnectorId(from),
        to: ConnectorId(to),
    }
}

fn messages(connections: &[Connection], external: &[ConnectorId]) -> Vec<(String, String)> {
    let mut diags = Vec::new();
    check(&connectors(), connections, external, &mut diags);
    diags
        .into_iter()
        .map(|diag| {
            assert_eq!(diag.code, DiagCode::SingleAssignment);
            (diag.subject.unwrap_or_default().to_string(), diag.message)
        })
        .collect()
}

#[test]
fn one_wire_per_input_passes() {
    assert!(messages(&[wire(0, 2), wire(1, 3)], &[]).is_empty());
}

#[test]
fn two_wires_into_one_input_are_refused_with_the_degree() {
    assert_eq!(
        messages(&[wire(0, 2), wire(1, 2), wire(0, 3)], &[]),
        vec![(
            "connector#2".to_owned(),
            "input is multiply driven (in-degree 2)".to_owned()
        )]
    );
}

#[test]
fn an_unwired_internal_input_is_refused_as_undriven() {
    assert_eq!(
        messages(&[wire(0, 2)], &[]),
        vec![(
            "connector#3".to_owned(),
            "input is undriven (in-degree 0)".to_owned()
        )]
    );
}

#[test]
fn frame_driven_inputs_are_left_to_the_boundary_pass_at_every_wire_degree() {
    // In-degree 0 is the legal shape; 1 and 2 are refused by `AssignmentTracker` with the
    // authored subject, so this check must not add a second, differently named refusal.
    let external = [ConnectorId(2), ConnectorId(3)];
    assert!(messages(&[], &external).is_empty());
    assert!(messages(&[wire(0, 2)], &external).is_empty());
    assert!(messages(&[wire(0, 2), wire(1, 2)], &external).is_empty());
}
