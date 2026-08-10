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
            constant_block(0, &[0]),
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
        boundary_inputs: vec![],
        boundary_outputs: vec![],
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
        ..ModelGraph::new()
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
        boundary_inputs: vec![],
        boundary_outputs: vec![],
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
        ..ModelGraph::new()
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
        boundary_inputs: vec![],
        boundary_outputs: vec![],
    };
    let err = validate(&m).expect_err("external + doubly-driven must still fail");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::SingleAssignment]);
}

#[test]
fn t7_output_with_zero_in_degree_is_fine() {
    // Outputs are never subject to the in-degree rule.
    let m = ModelGraph {
        blocks: vec![constant_block(0, &[0])],
        connectors: vec![conn(0, 0, Dir::Out, ValueType::Real)],
        connections: vec![],
        ..ModelGraph::new()
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
        boundary_inputs: vec![],
        boundary_outputs: vec![],
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
        ..ModelGraph::new()
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
        ..ModelGraph::new()
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
        boundary_inputs: vec![],
        boundary_outputs: vec![],
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
        boundary_inputs: vec![],
        boundary_outputs: vec![],
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
        ..ModelGraph::new()
    };
    let err = validate(&m).expect_err("connection type mismatch still fires");
    // Only the Rule-2 connection type mismatch, never a Rule-3 signature mismatch (class unknown).
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::TypeMismatch]);
    assert!(
        err.diagnostics[0].message.contains("implicit coercion"),
        "should be the Rule-2 connection diagnostic, not Rule-3"
    );
}

// ---- T23 / T25 / T39–T43: panic-free on malformed hand-built graphs ---------------------------

#[test]
fn t23_out_of_range_connection_endpoint_is_malformed_not_a_panic() {
    // Connection references ids beyond the connector arena — must report, never index OOB.
    let m = ModelGraph {
        blocks: vec![constant_block(0, &[0])],
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
fn t39_connector_block_out_of_range_is_malformed() {
    // A connector whose owning BlockId is outside the block arena would panic in topo decl_key.
    let m = ModelGraph {
        blocks: vec![constant_block(0, &[0])],
        connectors: vec![conn(0, 7, Dir::Out, ValueType::Real)],
        connections: vec![],
        ..ModelGraph::new()
    };
    let err = validate(&m).expect_err("connector must reference an in-range block");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::MalformedDocument]);
    assert!(
        err.diagnostics[0]
            .message
            .contains("out-of-range block id 7")
    );
    assert_eq!(err.diagnostics[0].subject.as_deref(), Some("connector#0"));
}

#[test]
fn t40_connector_block_at_blocks_len_boundary_is_malformed() {
    // The exact off-by-one boundary (`block == blocks.len()`) is out of range.
    let m = ModelGraph {
        blocks: vec![constant_block(0, &[0])],
        connectors: vec![conn(0, 1, Dir::Out, ValueType::Real)],
        connections: vec![],
        ..ModelGraph::new()
    };
    let err = validate(&m).expect_err("connector block id equal to blocks.len() must fail");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::MalformedDocument]);
    assert!(
        err.diagnostics[0]
            .message
            .contains("out-of-range block id 1"),
        "unexpected diagnostic: {:?}",
        err.diagnostics
    );
    assert_eq!(err.diagnostics[0].subject.as_deref(), Some("connector#0"));
}

#[test]
fn t25_block_port_out_of_range_connector_is_malformed() {
    // A block whose port list references a non-existent connector id → arena rule reports, no panic.
    let m = ModelGraph {
        blocks: vec![block(0, "CDL.Reals.Add", &[0, 7], &[1])], // 7 is out of range
        connectors: vec![
            conn(0, 0, Dir::In, ValueType::Real),
            conn(1, 0, Dir::Out, ValueType::Real),
        ],
        connections: vec![],
        external_inputs: vec![ConnectorId(0)],
        boundary_inputs: vec![],
        boundary_outputs: vec![],
    };
    let err = validate(&m).expect_err("out-of-range port connector must fail");
    assert!(codes(&err.diagnostics).contains(&DiagCode::MalformedDocument));
}

#[test]
fn t41_block_ids_must_be_dense_unique_arena_indices() {
    // A block id that is in range but not equal to its arena index would panic in BUILD when the
    // feedthrough pass indexes instantiated blocks by `BlockId.0`.
    let m = ModelGraph {
        blocks: vec![constant_block(1, &[0])],
        connectors: vec![conn(0, 0, Dir::Out, ValueType::Real)],
        connections: vec![],
        ..ModelGraph::new()
    };
    let err = validate(&m).expect_err("block id must equal arena index");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::MalformedDocument]);
    assert!(err.diagnostics[0].message.contains("block id invariant"));
    assert_eq!(err.diagnostics[0].subject.as_deref(), Some("block#1"));

    let duplicate = ModelGraph {
        blocks: vec![
            block(0, "unknown.Class", &[], &[]),
            block(0, "unknown.Class", &[], &[]),
        ],
        ..ModelGraph::new()
    };
    let err = validate(&duplicate).expect_err("duplicate block id must fail density");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::MalformedDocument]);
    assert!(err.diagnostics[0].message.contains("arena index 1"));
}

#[test]
fn connector_ids_must_be_dense_unique_arena_indices() {
    let out_of_position = ModelGraph {
        blocks: vec![constant_block(0, &[0])],
        connectors: vec![conn(1, 0, Dir::Out, ValueType::Real)],
        connections: vec![],
        ..ModelGraph::new()
    };
    let err = validate(&out_of_position).expect_err("connector id must equal arena index");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::MalformedDocument]);
    assert!(
        err.diagnostics[0]
            .message
            .contains("connector id invariant")
    );
    assert_eq!(err.diagnostics[0].subject.as_deref(), Some("connector#1"));

    let duplicate = ModelGraph {
        blocks: vec![block(0, "unknown.Class", &[], &[0, 1])],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(0, 0, Dir::Out, ValueType::Real),
        ],
        connections: vec![],
        ..ModelGraph::new()
    };
    let err = validate(&duplicate).expect_err("duplicate connector id must fail density");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::MalformedDocument]);
    assert!(err.diagnostics[0].message.contains("arena index 1"));
}

#[test]
fn t42_block_port_ids_are_bounded_before_signature_lookup() {
    // The arena rule must be self-sufficient even when Rule 3 skips an unknown block class.
    let m = ModelGraph {
        blocks: vec![block(0, "unknown.Class", &[7], &[8])],
        connectors: vec![],
        connections: vec![],
        ..ModelGraph::new()
    };
    let err = validate(&m).expect_err("block port connector ids must be bounded unconditionally");
    assert_eq!(
        codes(&err.diagnostics),
        vec![DiagCode::MalformedDocument, DiagCode::MalformedDocument]
    );
    assert!(err.diagnostics[0].message.contains("input port 0"));
    assert!(err.diagnostics[1].message.contains("output port 0"));
    assert!(
        err.diagnostics
            .iter()
            .all(|d| d.subject.as_deref() == Some("block#0"))
    );
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
        boundary_inputs: vec![],
        boundary_outputs: vec![],
    };
    let err = validate(&m).expect_err("mistyped Greater output must fail");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::PortKindMismatch]);
    assert!(err.diagnostics[0].message.contains("output port"));
    assert_eq!(err.diagnostics[0].subject.as_deref(), Some("connector#2"));
}

#[test]
fn t44_resolved_signature_accepts_dynamic_vector_width() {
    let m = one_block_model(
        "CDL.Logical.MultiAnd",
        &[ValueType::Boolean, ValueType::Boolean, ValueType::Boolean],
        &[ValueType::Boolean],
        vec![(Arc::from("nin"), Value::Integer(3))],
    );
    assert!(
        validate(&m)
            .expect("MultiAnd nin=3 with three Boolean inputs is valid")
            .is_empty()
    );
}

#[test]
fn t45_resolved_signature_rejects_dynamic_vector_arity_mismatch() {
    let m = one_block_model(
        "CDL.Logical.MultiAnd",
        &[ValueType::Boolean, ValueType::Boolean],
        &[ValueType::Boolean],
        vec![(Arc::from("nin"), Value::Integer(3))],
    );
    let err = validate(&m).expect_err("MultiAnd nin=3 requires three inputs");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::MalformedDocument]);
    assert!(
        err.diagnostics[0].message.contains("class requires 3/1"),
        "unexpected diagnostic: {:?}",
        err.diagnostics
    );
}

#[test]
fn t46_resolved_signature_checks_dynamic_vector_port_kind() {
    let m = one_block_model(
        "CDL.Logical.MultiOr",
        &[ValueType::Boolean, ValueType::Real, ValueType::Boolean],
        &[ValueType::Boolean],
        vec![(Arc::from("nin"), Value::Integer(3))],
    );
    let err = validate(&m).expect_err("MultiOr input elements must all be Boolean");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::PortKindMismatch]);
    assert_eq!(err.diagnostics[0].subject.as_deref(), Some("connector#1"));
}

#[test]
fn reals_multi_reducer_resolved_signature_accepts_vector_width() {
    let m = one_block_model(
        "CDL.Reals.MultiSum",
        &[ValueType::Real, ValueType::Real, ValueType::Real],
        &[ValueType::Real],
        vec![(Arc::from("nin"), Value::Integer(3))],
    );
    assert!(
        validate(&m)
            .expect("MultiSum nin=3 with three Real inputs is valid")
            .is_empty()
    );
}

#[test]
fn reals_multi_reducer_resolved_signature_rejects_arity_mismatch() {
    let m = one_block_model(
        "CDL.Reals.MultiMin",
        &[ValueType::Real, ValueType::Real],
        &[ValueType::Real],
        vec![(Arc::from("nin"), Value::Integer(3))],
    );
    let err = validate(&m).expect_err("MultiMin nin=3 requires three inputs");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::MalformedDocument]);
    assert!(
        err.diagnostics[0].message.contains("class requires 3/1"),
        "unexpected diagnostic: {:?}",
        err.diagnostics
    );
}

#[test]
fn reals_multi_reducer_resolved_signature_checks_port_kind() {
    let m = one_block_model(
        "CDL.Reals.MultiMax",
        &[ValueType::Real, ValueType::Boolean, ValueType::Real],
        &[ValueType::Real],
        vec![(Arc::from("nin"), Value::Integer(3))],
    );
    let err = validate(&m).expect_err("MultiMax input elements must all be Real");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::PortKindMismatch]);
    assert_eq!(err.diagnostics[0].subject.as_deref(), Some("connector#1"));
}

#[test]
fn routing_real_resolved_signatures_accept_dynamic_vector_and_matrix_widths() {
    let extract = one_block_model(
        "CDL.Routing.RealExtractSignal",
        &[ValueType::Real, ValueType::Real, ValueType::Real],
        &[ValueType::Real, ValueType::Real],
        vec![
            (Arc::from("nin"), Value::Integer(3)),
            (Arc::from("nout"), Value::Integer(2)),
            (Arc::from("extract_1"), Value::Integer(3)),
            (Arc::from("extract_2"), Value::Integer(1)),
        ],
    );
    assert!(
        validate(&extract)
            .expect("RealExtractSignal nin=3,nout=2 has 3 inputs and 2 outputs")
            .is_empty()
    );

    let vector_replicator = one_block_model(
        "CDL.Routing.RealVectorReplicator",
        &[ValueType::Real, ValueType::Real],
        &[
            ValueType::Real,
            ValueType::Real,
            ValueType::Real,
            ValueType::Real,
            ValueType::Real,
            ValueType::Real,
        ],
        vec![
            (Arc::from("nin"), Value::Integer(2)),
            (Arc::from("nout"), Value::Integer(3)),
        ],
    );
    assert!(
        validate(&vector_replicator)
            .expect("RealVectorReplicator y[3,2] lowers to 6 Real outputs")
            .is_empty()
    );

    let boolean_extract = one_block_model(
        "CDL.Routing.BooleanExtractSignal",
        &[ValueType::Boolean, ValueType::Boolean, ValueType::Boolean],
        &[ValueType::Boolean, ValueType::Boolean],
        vec![
            (Arc::from("nin"), Value::Integer(3)),
            (Arc::from("nout"), Value::Integer(2)),
            (Arc::from("extract_1"), Value::Integer(3)),
            (Arc::from("extract_2"), Value::Integer(1)),
        ],
    );
    assert!(
        validate(&boolean_extract)
            .expect("BooleanExtractSignal nin=3,nout=2 has 3 inputs and 2 outputs")
            .is_empty()
    );

    let integer_vector_replicator = one_block_model(
        "CDL.Routing.IntegerVectorReplicator",
        &[ValueType::Integer, ValueType::Integer],
        &[
            ValueType::Integer,
            ValueType::Integer,
            ValueType::Integer,
            ValueType::Integer,
            ValueType::Integer,
            ValueType::Integer,
        ],
        vec![
            (Arc::from("nin"), Value::Integer(2)),
            (Arc::from("nout"), Value::Integer(3)),
        ],
    );
    assert!(
        validate(&integer_vector_replicator)
            .expect("IntegerVectorReplicator y[3,2] lowers to 6 Integer outputs")
            .is_empty()
    );
}

#[test]
fn routing_real_resolved_signatures_reject_arity_and_kind_mismatch() {
    let arity = one_block_model(
        "CDL.Routing.RealScalarReplicator",
        &[ValueType::Real],
        &[ValueType::Real],
        vec![(Arc::from("nout"), Value::Integer(2))],
    );
    let err = validate(&arity).expect_err("RealScalarReplicator nout=2 requires two outputs");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::MalformedDocument]);
    assert!(
        err.diagnostics[0].message.contains("class requires 1/2"),
        "unexpected diagnostic: {:?}",
        err.diagnostics
    );

    let kind = one_block_model(
        "CDL.Routing.RealExtractor",
        &[ValueType::Real, ValueType::Real, ValueType::Real],
        &[ValueType::Real],
        vec![(Arc::from("nin"), Value::Integer(2))],
    );
    let err = validate(&kind).expect_err("RealExtractor index input must be Integer");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::PortKindMismatch]);
    assert_eq!(err.diagnostics[0].subject.as_deref(), Some("connector#0"));

    let boolean_kind = one_block_model(
        "CDL.Routing.BooleanScalarReplicator",
        &[ValueType::Boolean],
        &[ValueType::Boolean, ValueType::Real],
        vec![(Arc::from("nout"), Value::Integer(2))],
    );
    let err = validate(&boolean_kind).expect_err("Boolean outputs must all be Boolean");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::PortKindMismatch]);
    assert_eq!(err.diagnostics[0].subject.as_deref(), Some("connector#2"));

    let integer_kind = one_block_model(
        "CDL.Routing.IntegerExtractor",
        &[ValueType::Boolean, ValueType::Integer, ValueType::Integer],
        &[ValueType::Integer],
        vec![(Arc::from("nin"), Value::Integer(2))],
    );
    let err = validate(&integer_kind).expect_err("IntegerExtractor index input must be Integer");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::PortKindMismatch]);
    assert_eq!(err.diagnostics[0].subject.as_deref(), Some("connector#0"));
}

#[test]
fn t43_block_interface_arity_mismatch_covers_missing_and_extra_ports() {
    // CXF resolve already rejects these shapes for documents. The pure validate seam must also reject
    // hand-built ModelGraph callers before oce-graph reaches emit/gather by port index.
    let cases = [
        (
            "missing input",
            block(0, "CDL.Reals.Add", &[0], &[1]),
            vec![
                conn(0, 0, Dir::In, ValueType::Real),
                conn(1, 0, Dir::Out, ValueType::Real),
            ],
            vec![ConnectorId(0)],
        ),
        (
            "missing output",
            block(0, "CDL.Reals.Add", &[0, 1], &[]),
            vec![
                conn(0, 0, Dir::In, ValueType::Real),
                conn(1, 0, Dir::In, ValueType::Real),
            ],
            vec![ConnectorId(0), ConnectorId(1)],
        ),
        (
            "extra input",
            block(0, "CDL.Reals.Add", &[0, 1, 2], &[3]),
            vec![
                conn(0, 0, Dir::In, ValueType::Real),
                conn(1, 0, Dir::In, ValueType::Real),
                conn(2, 0, Dir::In, ValueType::Real),
                conn(3, 0, Dir::Out, ValueType::Real),
            ],
            vec![ConnectorId(0), ConnectorId(1), ConnectorId(2)],
        ),
        (
            "extra output",
            block(0, "CDL.Reals.Add", &[0, 1], &[2, 3]),
            vec![
                conn(0, 0, Dir::In, ValueType::Real),
                conn(1, 0, Dir::In, ValueType::Real),
                conn(2, 0, Dir::Out, ValueType::Real),
                conn(3, 0, Dir::Out, ValueType::Real),
            ],
            vec![ConnectorId(0), ConnectorId(1)],
        ),
    ];

    for (label, block, connectors, external_inputs) in cases {
        let m = ModelGraph {
            blocks: vec![block],
            connectors,
            connections: vec![],
            external_inputs,
            boundary_inputs: vec![],
            boundary_outputs: vec![],
        };
        let err = match validate(&m) {
            Err(err) => err,
            Ok(_) => panic!("{label} Add arity must fail before BUILD"),
        };
        assert_eq!(codes(&err.diagnostics), vec![DiagCode::MalformedDocument]);
        assert!(
            err.diagnostics[0].message.contains("interface mismatch"),
            "{label}: unexpected diagnostic: {:?}",
            err.diagnostics
        );
        assert_eq!(err.diagnostics[0].subject.as_deref(), Some("block#0"));
    }
}
