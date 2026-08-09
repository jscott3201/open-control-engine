//! Panic-free export checks for malformed host-built block port references.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use oce_cxf::{CxfError, export};
use oce_diag::{DiagCode, Diagnostic};
use oce_model::{
    BlockId, BlockInstance, Connector, ConnectorId, Dir, ModelGraph, ParamTable, Value, ValueType,
};

const STRUCTURE: &str = "export subset: block/connector wiring is structurally inconsistent";

fn block(inputs: Vec<ConnectorId>, outputs: Vec<ConnectorId>, class_path: &str) -> BlockInstance {
    BlockInstance {
        id: BlockId(0),
        class_iri: Arc::from(class_path),
        inputs,
        outputs,
        params: ParamTable {
            values: vec![(Arc::from("k"), Value::Real(1.0))],
        },
        decl_order: 0,
        instance_iri: Some(Arc::from("urn:test:malformed-port")),
    }
}

fn assert_structure_rejection(graph: &ModelGraph) {
    let result = catch_unwind(AssertUnwindSafe(|| export(graph))).expect("export must not unwind");
    let diagnostics = match result {
        Err(CxfError::Validation(diagnostics)) => diagnostics,
        other => panic!("expected structural rejection, got {other:?}"),
    };
    assert_eq!(
        diagnostics,
        vec![
            Diagnostic::error(DiagCode::ExportUnsupported, STRUCTURE)
                .with_subject("urn:test:malformed-port".to_owned()),
        ]
    );
}

#[test]
fn out_of_range_input_port_returns_a_structural_diagnostic() {
    let graph = ModelGraph {
        blocks: vec![block(
            vec![ConnectorId(9)],
            vec![ConnectorId(0)],
            "CDL.Reals.MultiplyByParameter",
        )],
        connectors: vec![Connector::new(
            ConnectorId(0),
            BlockId(0),
            Dir::Out,
            ValueType::Real,
            0,
        )],
        ..ModelGraph::new()
    };

    assert_structure_rejection(&graph);
}

#[test]
fn out_of_range_output_port_returns_a_structural_diagnostic() {
    let graph = ModelGraph {
        blocks: vec![block(
            vec![],
            vec![ConnectorId(9)],
            "CDL.Reals.Sources.Constant",
        )],
        ..ModelGraph::new()
    };

    assert_structure_rejection(&graph);
}
