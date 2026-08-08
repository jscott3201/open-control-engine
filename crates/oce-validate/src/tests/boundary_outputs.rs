//! Structural and transactional checks for declared boundary-output aliases.

use super::common::*;

#[test]
fn valid_alias_matches_its_output_source() {
    let model = ModelGraph {
        blocks: vec![constant_block(0, &[0])],
        connectors: vec![real_unit(0, 0, Dir::Out, None)],
        boundary_outputs: vec![boundary_output(
            "urn:test:boundary",
            0,
            Attrs::Real(RealAttrs::default()),
        )],
        ..ModelGraph::new()
    };

    assert!(validate(&model).expect("valid boundary alias").is_empty());
}

#[test]
fn dangling_alias_source_is_malformed() {
    let model = ModelGraph {
        boundary_outputs: vec![boundary_output(
            "urn:test:dangling",
            7,
            Attrs::Real(RealAttrs::default()),
        )],
        ..ModelGraph::new()
    };

    let error = validate(&model).expect_err("dangling boundary source must fail");
    assert_eq!(codes(&error.diagnostics), vec![DiagCode::MalformedDocument]);
    assert_eq!(
        error.diagnostics[0].subject.as_deref(),
        Some("urn:test:dangling")
    );
}

#[test]
fn alias_source_must_be_an_output_with_matching_attrs() {
    let model = ModelGraph {
        blocks: vec![block(0, "unknown.Class", &[0], &[])],
        connectors: vec![conn(0, 0, Dir::In, ValueType::Real)],
        external_inputs: vec![ConnectorId(0)],
        boundary_outputs: vec![boundary_output(
            "urn:test:mismatched",
            0,
            Attrs::Integer(IntAttrs::default()),
        )],
        ..ModelGraph::new()
    };

    let error = validate(&model).expect_err("malformed boundary source must fail");
    assert_eq!(
        codes(&error.diagnostics),
        vec![DiagCode::DirectionMismatch, DiagCode::MalformedDocument]
    );
    assert!(
        error
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.subject.as_deref() == Some("urn:test:mismatched"))
    );
}

#[test]
fn later_structural_failure_rolls_back_alias_propagation() {
    let mut model = ModelGraph {
        connectors: vec![real_unit(0, 0, Dir::Out, None)],
        boundary_outputs: vec![boundary_output(
            "urn:test:boundary",
            0,
            Attrs::Real(RealAttrs {
                unit: Some(Arc::from("K")),
                ..RealAttrs::default()
            }),
        )],
        ..ModelGraph::new()
    };

    let error = unify_and_validate(&mut model).expect_err("missing owner block must fail");
    assert_eq!(codes(&error.diagnostics), vec![DiagCode::MalformedDocument]);
    assert!(model.connectors[0].attrs.as_real().unwrap().unit.is_none());
}
