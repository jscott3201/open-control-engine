//! Structural rules (1–3) + panic-free contract: minimal-valid, boundary-aware single assignment,
//! per-connection direction/type, connector↔signature port-kind, and malformed-graph safety.

use super::common::*;

// ---- T1–T2: trivial / minimal-valid ---------------------------------------------------------

#[test]
fn t1_empty_model_validates_clean() {
    let m = ModelGraph::new();
    assert!(validate(&m).expect("empty graph is valid").is_empty());
    let mut m2 = ModelGraph::new();
    assert!(unify_attributes(&mut m2).expect("empty unify").is_empty());
    assert!(
        unify_and_validate(&mut ModelGraph::new())
            .expect("empty unify+validate")
            .is_empty()
    );
}

#[test]
fn t2_minimal_valid_graph_passes_all_rules() {
    // Constant.y(C0) → Add.u1(C1); Add.u2(C2) is a declared external boundary input; Add.y(C3).
    let m = ModelGraph {
        blocks: vec![
            block(0, "CDL.Reals.Sources.Constant", &[], &[0]),
            block(1, "CDL.Reals.Add", &[1, 2], &[3]),
        ],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(1, 1, Dir::In, ValueType::Real),
            conn(2, 1, Dir::In, ValueType::Real),
            conn(3, 1, Dir::Out, ValueType::Real),
        ],
        connections: vec![conn_edge(0, 1)],
        external_inputs: vec![ConnectorId(2)],
    };
    let warnings = validate(&m).expect("minimal valid graph passes");
    assert!(warnings.is_empty(), "no warnings expected: {warnings:?}");
}

// ---- T3–T7: Rule 1 boundary-aware single assignment -----------------------------------------

#[test]
fn t3_undriven_input_not_external_is_single_assignment_error() {
    // One input connector, in-degree 0, NOT in external_inputs → error.
    let m = ModelGraph {
        blocks: vec![block(0, "unknown.Class", &[0], &[])],
        connectors: vec![conn(0, 0, Dir::In, ValueType::Real)],
        connections: vec![],
        external_inputs: vec![],
    };
    let err = validate(&m).expect_err("undriven non-external input must fail");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::SingleAssignment]);
}

#[test]
fn t4_undriven_input_that_is_external_is_ok() {
    let m = ModelGraph {
        blocks: vec![block(0, "unknown.Class", &[0], &[])],
        connectors: vec![conn(0, 0, Dir::In, ValueType::Real)],
        connections: vec![],
        external_inputs: vec![ConnectorId(0)],
    };
    assert!(
        validate(&m)
            .expect("external boundary input is valid")
            .is_empty()
    );
}

#[test]
fn t5_input_with_in_degree_two_is_single_assignment_error() {
    // Two outputs drive one input → in-degree 2.
    let m = ModelGraph {
        blocks: vec![block(0, "unknown.Class", &[2], &[0, 1])],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(1, 0, Dir::Out, ValueType::Real),
            conn(2, 0, Dir::In, ValueType::Real),
        ],
        connections: vec![conn_edge(0, 2), conn_edge(1, 2)],
        external_inputs: vec![],
    };
    let err = validate(&m).expect_err("doubly-driven input must fail");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::SingleAssignment]);
    assert!(err.diagnostics[0].message.contains("in-degree 2"));
}

#[test]
fn t6_external_input_driven_from_inside_is_still_an_error() {
    // Listed as external (in-degree 0 would be legal) but ALSO driven → in-degree 1 here is fine;
    // we want in-degree >= 2 to error even when external. Drive it twice.
    let m = ModelGraph {
        blocks: vec![block(0, "unknown.Class", &[2], &[0, 1])],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(1, 0, Dir::Out, ValueType::Real),
            conn(2, 0, Dir::In, ValueType::Real),
        ],
        connections: vec![conn_edge(0, 2), conn_edge(1, 2)],
        external_inputs: vec![ConnectorId(2)], // declared external, yet doubly driven
    };
    let err = validate(&m).expect_err("external + doubly-driven must still fail");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::SingleAssignment]);
}

#[test]
fn t7_output_with_zero_in_degree_is_fine() {
    // Outputs are never subject to the in-degree rule.
    let m = ModelGraph {
        blocks: vec![block(0, "CDL.Reals.Sources.Constant", &[], &[0])],
        connectors: vec![conn(0, 0, Dir::Out, ValueType::Real)],
        connections: vec![],
        external_inputs: vec![],
    };
    assert!(validate(&m).expect("lone output is valid").is_empty());
}

// ---- T8–T10: Rule 2 direction + value type --------------------------------------------------

#[test]
fn t8_connection_from_input_is_direction_mismatch() {
    // from is an In connector (wrong), to is In.
    let m = ModelGraph {
        blocks: vec![block(0, "unknown.Class", &[0, 1], &[])],
        connectors: vec![
            conn(0, 0, Dir::In, ValueType::Real),
            conn(1, 0, Dir::In, ValueType::Real),
        ],
        connections: vec![conn_edge(0, 1)],
        external_inputs: vec![ConnectorId(0)], // silence the single-assignment rule on C0
    };
    let err = validate(&m).expect_err("from-an-input is a direction mismatch");
    assert!(codes(&err.diagnostics).contains(&DiagCode::DirectionMismatch));
}

#[test]
fn t9_connection_to_output_is_direction_mismatch() {
    let m = ModelGraph {
        blocks: vec![block(0, "unknown.Class", &[], &[0, 1])],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(1, 0, Dir::Out, ValueType::Real),
        ],
        connections: vec![conn_edge(0, 1)], // to an output → wrong
        external_inputs: vec![],
    };
    let err = validate(&m).expect_err("to-an-output is a direction mismatch");
    assert!(codes(&err.diagnostics).contains(&DiagCode::DirectionMismatch));
}

#[test]
fn t10_cross_type_connection_is_type_mismatch() {
    // Real output → Boolean input: no implicit coercion.
    let m = ModelGraph {
        blocks: vec![block(0, "unknown.Class", &[1], &[0])],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(1, 0, Dir::In, ValueType::Boolean),
        ],
        connections: vec![conn_edge(0, 1)],
        external_inputs: vec![],
    };
    let err = validate(&m).expect_err("Real→Boolean must fail");
    assert!(codes(&err.diagnostics).contains(&DiagCode::TypeMismatch));
}

// ---- T11–T13: Rule 3 connector value-type ↔ block-signature port-kind -----------------------

#[test]
fn t11_input_mistyped_against_signature_is_port_kind_mismatch() {
    // CDL.Reals.Add expects two Real inputs; type C1 as Boolean → mismatch on port 0.
    let m = ModelGraph {
        blocks: vec![block(0, "CDL.Reals.Add", &[0, 1], &[2])],
        connectors: vec![
            conn(0, 0, Dir::In, ValueType::Boolean), // wrong: Add.u1 must be Real
            conn(1, 0, Dir::In, ValueType::Real),
            conn(2, 0, Dir::Out, ValueType::Real),
        ],
        connections: vec![],
        external_inputs: vec![ConnectorId(0), ConnectorId(1)], // silence single-assignment
    };
    let err = validate(&m).expect_err("mistyped Add input must fail");
    // Exactly one Rule-3 diagnostic — the conforming Real input C1 and output C2 must not fire.
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::PortKindMismatch]);
    assert!(
        err.diagnostics[0]
            .message
            .contains("block-signature port kind")
    );
    // Subject is the specific mistyped connector C0.
    assert_eq!(err.diagnostics[0].subject.as_deref(), Some("connector#0"));
}

#[test]
fn t12_switch_boolean_control_port_typed_real_is_mismatch() {
    // CDL.Reals.Switch inputs = [Real, Boolean, Real]; type the control port (1) as Real → mismatch.
    let m = ModelGraph {
        blocks: vec![block(0, "CDL.Reals.Switch", &[0, 1, 2], &[3])],
        connectors: vec![
            conn(0, 0, Dir::In, ValueType::Real),
            conn(1, 0, Dir::In, ValueType::Real), // wrong: control port must be Boolean
            conn(2, 0, Dir::In, ValueType::Real),
            conn(3, 0, Dir::Out, ValueType::Real),
        ],
        connections: vec![],
        external_inputs: vec![ConnectorId(0), ConnectorId(1), ConnectorId(2)],
    };
    let err = validate(&m).expect_err("Switch control port mistype must fail");
    // Exactly one Rule-3 diagnostic on the control port C1; the conforming Real ports must not fire.
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::PortKindMismatch]);
    assert_eq!(err.diagnostics[0].subject.as_deref(), Some("connector#1"));
}

#[test]
fn t13_unknown_class_skips_rule3_silently() {
    // An unknown class is the loader's OcError::Load, not a validate diagnostic — Rule 3 must skip.
    let m = ModelGraph {
        blocks: vec![block(0, "Totally.Unknown.Block", &[0], &[1])],
        connectors: vec![
            conn(0, 0, Dir::In, ValueType::Boolean),
            conn(1, 0, Dir::Out, ValueType::Real),
        ],
        connections: vec![conn_edge(1, 0)], // type mismatch on the *connection* (Rule 2), not Rule 3
        external_inputs: vec![],
    };
    let err = validate(&m).expect_err("connection type mismatch still fires");
    // Only the Rule-2 connection type mismatch, never a Rule-3 signature mismatch (class unknown).
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::TypeMismatch]);
    assert!(
        err.diagnostics[0].message.contains("implicit coercion"),
        "should be the Rule-2 connection diagnostic, not Rule-3"
    );
}

// ---- T23 / T25: panic-free on malformed hand-built graphs (structural ids) ------------------

#[test]
fn t23_out_of_range_connection_endpoint_is_malformed_not_a_panic() {
    // Connection references ids beyond the connector arena — must report, never index OOB.
    let m = ModelGraph {
        connectors: vec![conn(0, 0, Dir::Out, ValueType::Real)],
        connections: vec![conn_edge(0, 9)], // 9 is out of range
        ..ModelGraph::new()
    };
    let err = validate(&m).expect_err("out-of-range endpoint must fail");
    assert!(codes(&err.diagnostics).contains(&DiagCode::MalformedDocument));

    // unify must also be panic-free; it simply skips the out-of-range connection.
    let mut m2 = ModelGraph {
        connectors: vec![conn(0, 0, Dir::Out, ValueType::Real)],
        connections: vec![conn_edge(0, 9)],
        ..ModelGraph::new()
    };
    assert!(
        unify_attributes(&mut m2)
            .expect("unify skips out-of-range connection")
            .is_empty()
    );
}

#[test]
fn t25_block_port_out_of_range_connector_is_malformed() {
    // A block whose port list references a non-existent connector id → Rule 3 reports, no panic.
    let m = ModelGraph {
        blocks: vec![block(0, "CDL.Reals.Add", &[0, 7], &[1])], // 7 is out of range
        connectors: vec![
            conn(0, 0, Dir::In, ValueType::Real),
            conn(1, 0, Dir::Out, ValueType::Real),
        ],
        connections: vec![],
        external_inputs: vec![ConnectorId(0)],
    };
    let err = validate(&m).expect_err("out-of-range port connector must fail");
    assert!(codes(&err.diagnostics).contains(&DiagCode::MalformedDocument));
}

// ---- T30: Rule 3 OUTPUT-port branch ----------------------------------------------------------

#[test]
fn t30_output_port_mistyped_against_signature_is_port_kind_mismatch() {
    // CDL.Reals.Greater output is Boolean; type its output connector Real → exercises the OUTPUT
    // branch of check_port_types (T11/T12 only cover INPUT ports).
    let m = ModelGraph {
        blocks: vec![block(0, "CDL.Reals.Greater", &[0, 1], &[2])],
        connectors: vec![
            conn(0, 0, Dir::In, ValueType::Real),
            conn(1, 0, Dir::In, ValueType::Real),
            conn(2, 0, Dir::Out, ValueType::Real), // wrong: Greater.y must be Boolean
        ],
        connections: vec![],
        external_inputs: vec![ConnectorId(0), ConnectorId(1)],
    };
    let err = validate(&m).expect_err("mistyped Greater output must fail");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::PortKindMismatch]);
    assert!(err.diagnostics[0].message.contains("output port"));
    assert_eq!(err.diagnostics[0].subject.as_deref(), Some("connector#2"));
}
