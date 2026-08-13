//! Regression tests for the crate-private hand-built graph seam. These stay outside `tests.rs` so the
//! legacy validation harness remains below the CI file-size cap.

use std::sync::Arc;

use oce_diag::DiagCode;
use oce_model::{
    BlockId, BlockInstance, Connection, Connector, ConnectorId, Dir, ModelGraph, ParamTable, Value,
    ValueType,
};

use super::{Engine, OcError};

fn raw_block(id: u32, class: &str, inputs: &[u32], outputs: &[u32]) -> BlockInstance {
    BlockInstance {
        id: BlockId(id),
        class_iri: Arc::from(class),
        inputs: inputs.iter().map(|&i| ConnectorId(i)).collect(),
        outputs: outputs.iter().map(|&i| ConnectorId(i)).collect(),
        params: ParamTable::default(),
        decl_order: id,
        instance_iri: None,
    }
}

fn raw_block_with_params(
    id: u32,
    class: &str,
    inputs: &[u32],
    outputs: &[u32],
    params: Vec<(Arc<str>, Value)>,
) -> BlockInstance {
    BlockInstance {
        params: ParamTable { values: params },
        ..raw_block(id, class, inputs, outputs)
    }
}

fn rp(name: &str, value: f64) -> (Arc<str>, Value) {
    (Arc::from(name), Value::Real(value))
}

fn raw_conn(id: u32, block: u32, dir: Dir, vt: ValueType) -> Connector {
    Connector::new(ConnectorId(id), BlockId(block), dir, vt, id)
}

fn raw_edge(from: u32, to: u32) -> Connection {
    Connection {
        from: ConnectorId(from),
        to: ConnectorId(to),
    }
}

fn build_model_validate_error(model: ModelGraph) -> oce_validate::ValidationError {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut eng = Engine::in_memory();
        eng.build_model_in_memory(model, None)
    }));
    let err = outcome
        .expect("build_model_in_memory must return a typed error, not panic")
        .expect_err("malformed graph must not build");
    assert!(
        matches!(&err, OcError::Validate(_)),
        "expected OcError::Validate for malformed graph, got {err:?}"
    );
    if let OcError::Validate(err) = err {
        err
    } else {
        unreachable!("matches! asserted the error variant")
    }
}

fn assert_build_validate_codes(
    model: ModelGraph,
    expected: &[DiagCode],
) -> oce_validate::ValidationError {
    let err = build_model_validate_error(model);
    let codes: Vec<DiagCode> = err.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(codes, expected);
    err
}

#[test]
fn build_model_in_memory_rejects_out_of_range_connection_endpoint_without_panic() {
    let model = ModelGraph {
        blocks: vec![raw_block_with_params(
            0,
            "CDL.Reals.Sources.Constant",
            &[],
            &[0],
            vec![rp("k", 1.0)],
        )],
        connectors: vec![raw_conn(0, 0, Dir::Out, ValueType::Real)],
        connections: vec![raw_edge(0, 9)],
        ..ModelGraph::new()
    };
    let err = assert_build_validate_codes(model, &[DiagCode::MalformedDocument]);
    assert!(
        err.diagnostics[0]
            .message
            .contains("out-of-range connector id"),
        "unexpected diagnostic: {:?}",
        err.diagnostics
    );
}

#[test]
fn build_model_in_memory_rejects_sample_trigger_zero_period_without_panic() {
    let model = ModelGraph {
        blocks: vec![raw_block_with_params(
            0,
            "CDL.Logical.Sources.SampleTrigger",
            &[],
            &[0],
            vec![rp("period", 0.0)],
        )],
        connectors: vec![raw_conn(0, 0, Dir::Out, ValueType::Boolean)],
        ..ModelGraph::new()
    };
    let err = assert_build_validate_codes(model, &[DiagCode::ParameterOutOfRange]);
    assert_eq!(err.diagnostics[0].subject.as_deref(), Some("block#0"));
    assert!(
        err.diagnostics[0].message.contains(
            "`period` on block `CDL.Logical.Sources.SampleTrigger` must be finite and > 0"
        ),
        "unexpected diagnostic: {:?}",
        err.diagnostics
    );
}

#[test]
fn build_model_in_memory_rejects_duplicate_raw_parameter_names() {
    let model = ModelGraph {
        blocks: vec![raw_block_with_params(
            0,
            "CDL.Reals.Sources.Constant",
            &[],
            &[0],
            vec![rp("k", 1.0), rp("k", 2.0)],
        )],
        connectors: vec![raw_conn(0, 0, Dir::Out, ValueType::Real)],
        ..ModelGraph::new()
    };
    let err = assert_build_validate_codes(model, &[DiagCode::DuplicateParameter]);
    assert!(
        err.diagnostics[0]
            .message
            .contains("duplicate parameter name `k`")
    );
}

#[test]
fn loaded_sample_clocks_require_finite_periods_and_shift() {
    let cases = [
        (
            "CDL.Logical.Sources.SampleTrigger",
            vec![rp("period", f64::INFINITY)],
        ),
        (
            "CDL.Logical.Sources.SampleTrigger",
            vec![rp("period", 1.0), rp("shift", f64::NAN)],
        ),
    ];
    for (class, params) in cases {
        let model = ModelGraph {
            blocks: vec![raw_block_with_params(0, class, &[], &[0], params)],
            connectors: vec![raw_conn(0, 0, Dir::Out, ValueType::Boolean)],
            ..ModelGraph::new()
        };
        assert_build_validate_codes(model, &[DiagCode::ParameterOutOfRange]);
    }

    let sampled = ModelGraph {
        blocks: vec![raw_block_with_params(
            0,
            "CDL.Discrete.Sampler",
            &[0],
            &[1],
            vec![rp("samplePeriod", f64::INFINITY)],
        )],
        connectors: vec![
            raw_conn(0, 0, Dir::In, ValueType::Real),
            raw_conn(1, 0, Dir::Out, ValueType::Real),
        ],
        external_inputs: vec![ConnectorId(0)],
        ..ModelGraph::new()
    };
    assert_build_validate_codes(sampled, &[DiagCode::ParameterOutOfRange]);
}

#[test]
fn build_model_in_memory_rejects_out_of_range_connector_block_without_panic() {
    let model = ModelGraph {
        blocks: vec![raw_block_with_params(
            0,
            "CDL.Reals.Sources.Constant",
            &[],
            &[0],
            vec![rp("k", 1.0)],
        )],
        connectors: vec![raw_conn(0, 9, Dir::Out, ValueType::Real)],
        connections: vec![],
        ..ModelGraph::new()
    };
    let err = assert_build_validate_codes(model, &[DiagCode::MalformedDocument]);
    assert!(
        err.diagnostics[0]
            .message
            .contains("out-of-range block id 9"),
        "unexpected diagnostic: {:?}",
        err.diagnostics
    );
}

#[test]
fn build_model_in_memory_rejects_connector_block_at_blocks_len_boundary_without_panic() {
    let model = ModelGraph {
        blocks: vec![raw_block_with_params(
            0,
            "CDL.Reals.Sources.Constant",
            &[],
            &[0],
            vec![rp("k", 1.0)],
        )],
        connectors: vec![raw_conn(0, 1, Dir::Out, ValueType::Real)],
        connections: vec![],
        ..ModelGraph::new()
    };
    let err = assert_build_validate_codes(model, &[DiagCode::MalformedDocument]);
    assert!(
        err.diagnostics[0]
            .message
            .contains("out-of-range block id 1"),
        "unexpected diagnostic: {:?}",
        err.diagnostics
    );
}

#[test]
fn build_model_in_memory_rejects_non_dense_block_id_without_panic() {
    let model = ModelGraph {
        blocks: vec![raw_block_with_params(
            1,
            "CDL.Reals.Sources.Constant",
            &[],
            &[0],
            vec![rp("k", 1.0)],
        )],
        connectors: vec![raw_conn(0, 0, Dir::Out, ValueType::Real)],
        connections: vec![],
        ..ModelGraph::new()
    };
    let err = assert_build_validate_codes(model, &[DiagCode::MalformedDocument]);
    assert!(
        err.diagnostics[0].message.contains("block id invariant"),
        "unexpected diagnostic: {:?}",
        err.diagnostics
    );
}

#[test]
fn build_model_in_memory_rejects_duplicate_block_id_without_panic() {
    let model = ModelGraph {
        blocks: vec![
            raw_block(0, "unknown.Class", &[], &[]),
            raw_block(0, "unknown.Class", &[], &[]),
        ],
        ..ModelGraph::new()
    };
    let err = assert_build_validate_codes(model, &[DiagCode::MalformedDocument]);
    assert!(
        err.diagnostics[0].message.contains("arena index 1"),
        "unexpected diagnostic: {:?}",
        err.diagnostics
    );
}

#[test]
fn build_model_in_memory_rejects_out_of_range_port_connector_without_panic() {
    let model = ModelGraph {
        blocks: vec![raw_block(0, "CDL.Reals.Add", &[0, 7], &[1])],
        connectors: vec![
            raw_conn(0, 0, Dir::In, ValueType::Real),
            raw_conn(1, 0, Dir::Out, ValueType::Real),
        ],
        connections: vec![],
        external_inputs: vec![ConnectorId(0)],
        boundary_inputs: vec![],
        boundary_outputs: vec![],
    };
    let err = assert_build_validate_codes(model, &[DiagCode::MalformedDocument]);
    assert!(
        err.diagnostics[0]
            .message
            .contains("out-of-range connector id 7"),
        "unexpected diagnostic: {:?}",
        err.diagnostics
    );
}

#[test]
fn build_model_in_memory_rejects_port_kind_mismatch_without_wrong_value() {
    let model = ModelGraph {
        blocks: vec![raw_block(0, "CDL.Reals.Add", &[0, 1], &[2])],
        connectors: vec![
            raw_conn(0, 0, Dir::In, ValueType::Boolean),
            raw_conn(1, 0, Dir::In, ValueType::Real),
            raw_conn(2, 0, Dir::Out, ValueType::Real),
        ],
        connections: vec![],
        external_inputs: vec![ConnectorId(0), ConnectorId(1)],
        boundary_inputs: vec![],
        boundary_outputs: vec![],
    };
    let err = assert_build_validate_codes(model, &[DiagCode::PortKindMismatch]);
    assert!(
        err.diagnostics[0]
            .message
            .contains("block-signature port kind"),
        "unexpected diagnostic: {:?}",
        err.diagnostics
    );
}

#[test]
fn build_model_in_memory_rejects_missing_input_arity_without_panic() {
    let model = ModelGraph {
        blocks: vec![raw_block(0, "CDL.Reals.Add", &[0], &[1])],
        connectors: vec![
            raw_conn(0, 0, Dir::In, ValueType::Real),
            raw_conn(1, 0, Dir::Out, ValueType::Real),
        ],
        connections: vec![],
        external_inputs: vec![ConnectorId(0)],
        boundary_inputs: vec![],
        boundary_outputs: vec![],
    };
    let err = assert_build_validate_codes(model, &[DiagCode::MalformedDocument]);
    assert!(
        err.diagnostics[0].message.contains("interface mismatch"),
        "unexpected diagnostic: {:?}",
        err.diagnostics
    );
}

#[test]
fn build_model_in_memory_rejects_missing_output_arity_without_release_panic() {
    let model = ModelGraph {
        blocks: vec![raw_block(0, "CDL.Reals.Add", &[0, 1], &[])],
        connectors: vec![
            raw_conn(0, 0, Dir::In, ValueType::Real),
            raw_conn(1, 0, Dir::In, ValueType::Real),
        ],
        connections: vec![],
        external_inputs: vec![ConnectorId(0), ConnectorId(1)],
        boundary_inputs: vec![],
        boundary_outputs: vec![],
    };
    let err = assert_build_validate_codes(model, &[DiagCode::MalformedDocument]);
    assert!(
        err.diagnostics[0].message.contains("interface mismatch"),
        "unexpected diagnostic: {:?}",
        err.diagnostics
    );
}

#[test]
fn build_model_in_memory_rejects_extra_input_arity_without_panic() {
    let model = ModelGraph {
        blocks: vec![raw_block(0, "CDL.Reals.Add", &[0, 1, 2], &[3])],
        connectors: vec![
            raw_conn(0, 0, Dir::In, ValueType::Real),
            raw_conn(1, 0, Dir::In, ValueType::Real),
            raw_conn(2, 0, Dir::In, ValueType::Real),
            raw_conn(3, 0, Dir::Out, ValueType::Real),
        ],
        connections: vec![],
        external_inputs: vec![ConnectorId(0), ConnectorId(1), ConnectorId(2)],
        boundary_inputs: vec![],
        boundary_outputs: vec![],
    };
    let err = assert_build_validate_codes(model, &[DiagCode::MalformedDocument]);
    assert!(
        err.diagnostics[0].message.contains("interface mismatch"),
        "unexpected diagnostic: {:?}",
        err.diagnostics
    );
}

#[test]
fn build_model_in_memory_rejects_extra_output_arity_without_panic() {
    let model = ModelGraph {
        blocks: vec![raw_block(0, "CDL.Reals.Add", &[0, 1], &[2, 3])],
        connectors: vec![
            raw_conn(0, 0, Dir::In, ValueType::Real),
            raw_conn(1, 0, Dir::In, ValueType::Real),
            raw_conn(2, 0, Dir::Out, ValueType::Real),
            raw_conn(3, 0, Dir::Out, ValueType::Real),
        ],
        connections: vec![],
        external_inputs: vec![ConnectorId(0), ConnectorId(1)],
        boundary_inputs: vec![],
        boundary_outputs: vec![],
    };
    let err = assert_build_validate_codes(model, &[DiagCode::MalformedDocument]);
    assert!(
        err.diagnostics[0].message.contains("interface mismatch"),
        "unexpected diagnostic: {:?}",
        err.diagnostics
    );
}
