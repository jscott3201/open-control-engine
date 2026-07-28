//! Export rejection contracts for host-built reserved pass-through blocks.

use std::sync::Arc;

use oce_cxf::{CxfError, export};
use oce_diag::{DiagCode, Diagnostic};
use oce_model::{
    BlockId, BlockInstance, Connector, ConnectorId, Dir, EnumClassId, ModelGraph, ParamTable,
    Value, ValueType,
};

const STRUCTURE: &str = "export subset: block/connector wiring is structurally inconsistent";
const DUPLICATE_ID: &str = "export subset: emitted node @id collides with another emitted node @id";
const TOTAL_DEFERRAL: &str = "CXF export requires at least one surviving block: every block was \
     deferred (enumeration content plus its downstream cascade), leaving no runtime composite to emit";

fn connector(id: u32, block: u32, dir: Dir, iri: Option<&'static str>) -> Connector {
    let mut connector = Connector::new(ConnectorId(id), BlockId(block), dir, ValueType::Real, id);
    connector.iri = iri.map(Arc::from);
    connector
}

fn pass_block(inputs: Vec<ConnectorId>) -> BlockInstance {
    BlockInstance {
        id: BlockId(0),
        class_iri: Arc::from("urn:oce:lowering#PassThrough.Real"),
        inputs,
        outputs: vec![ConnectorId(1)],
        params: ParamTable::default(),
        decl_order: 0,
        instance_iri: None,
    }
}

fn survivor() -> BlockInstance {
    BlockInstance {
        id: BlockId(1),
        class_iri: Arc::from("CDL.Reals.Sources.Constant"),
        inputs: vec![],
        outputs: vec![ConnectorId(2)],
        params: ParamTable::default(),
        decl_order: 1,
        instance_iri: Some(Arc::from("http://example.org#PassExport.keep")),
    }
}

fn rejection(graph: &ModelGraph) -> Vec<Diagnostic> {
    match export(graph) {
        Err(CxfError::Validation(diagnostics)) => diagnostics,
        other => panic!("expected validation rejection, got {other:?}"),
    }
}

#[test]
fn malformed_reserved_block_rejects_loudly() {
    let graph = ModelGraph {
        blocks: vec![pass_block(vec![]), survivor()],
        connectors: vec![
            connector(0, 0, Dir::In, Some("http://example.org#PassExport.u")),
            connector(1, 0, Dir::Out, Some("http://example.org#PassExport.y")),
            connector(2, 1, Dir::Out, None),
        ],
        connections: vec![],
        external_inputs: vec![ConnectorId(0)],
    };
    let diagnostics = rejection(&graph);
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.code == DiagCode::ExportUnsupported && diagnostic.message == STRUCTURE
        }),
        "{diagnostics:?}"
    );
}

#[test]
fn boundary_output_iri_collision_rejects_at_plan_time() {
    let graph = ModelGraph {
        blocks: vec![pass_block(vec![ConnectorId(0)]), survivor()],
        connectors: vec![
            connector(0, 0, Dir::In, Some("http://example.org#PassExport.u")),
            connector(1, 0, Dir::Out, Some("http://example.org#PassExport.keep")),
            connector(2, 1, Dir::Out, None),
        ],
        connections: vec![],
        external_inputs: vec![ConnectorId(0)],
    };
    let diagnostics = rejection(&graph);
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.code == DiagCode::ExportUnsupported && diagnostic.message == DUPLICATE_ID
        }),
        "{diagnostics:?}"
    );
}

#[test]
fn pass_through_plus_only_deferred_blocks_rejects_total_deferral() {
    let deferred = BlockInstance {
        id: BlockId(1),
        class_iri: Arc::from("CDL.Reals.Sources.Constant"),
        inputs: vec![],
        outputs: vec![ConnectorId(2)],
        params: ParamTable {
            values: vec![(
                Arc::from("controllerType"),
                Value::Enum {
                    class: EnumClassId::SIMPLE_CONTROLLER,
                    ordinal: 1,
                },
            )],
        },
        decl_order: 1,
        instance_iri: Some(Arc::from("http://example.org#PassExport.deferred")),
    };
    let graph = ModelGraph {
        blocks: vec![pass_block(vec![ConnectorId(0)]), deferred],
        connectors: vec![
            connector(0, 0, Dir::In, Some("http://example.org#PassExport.u")),
            connector(1, 0, Dir::Out, Some("http://example.org#PassExport.y")),
            connector(2, 1, Dir::Out, None),
        ],
        connections: vec![],
        external_inputs: vec![ConnectorId(0)],
    };
    let diagnostics = rejection(&graph);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message == TOTAL_DEFERRAL),
        "{diagnostics:?}"
    );
}
