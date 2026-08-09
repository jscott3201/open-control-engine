//! Export rejection contracts for host-built reserved pass-through blocks.

use std::sync::Arc;

use oce_cxf::{CxfError, export};
use oce_diag::{DiagCode, Diagnostic};
use oce_model::{
    Attrs, BlockId, BlockInstance, BoundaryOutput, Connection, Connector, ConnectorId, Dir,
    EnumClassId, ModelGraph, ParamTable, Value, ValueType,
};

const STRUCTURE: &str = "export subset: block/connector wiring is structurally inconsistent";
const DUPLICATE_ID: &str = "export subset: emitted node @id collides with another emitted node @id";
const BOUNDARY_SOURCE_NOT_EMITTED: &str =
    "export subset: declared boundary output source is not an emitted child output connector";
const CONNECTION_ENDPOINT_NOT_EMITTED: &str = "export subset: connection endpoint is a reserved lowering connector with no emitted \
     child-port node";
const TOTAL_DEFERRAL: &str = "CXF export requires at least one emitted runtime block: all blocks \
     were deferred or reserved lowering-only, leaving no runtime composite to emit";

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
        params: ParamTable {
            values: vec![(Arc::from("k"), Value::Real(1.0))],
        },
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
        boundary_outputs: vec![],
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
fn missing_reserved_external_input_membership_rejects_loudly() {
    let valid = || ModelGraph {
        blocks: vec![pass_block(vec![ConnectorId(0)]), survivor()],
        connectors: vec![
            connector(0, 0, Dir::In, Some("http://example.org#PassExport.u")),
            connector(1, 0, Dir::Out, Some("http://example.org#PassExport.y")),
            connector(2, 1, Dir::Out, None),
        ],
        connections: vec![],
        external_inputs: vec![ConnectorId(0)],
        boundary_outputs: vec![],
    };
    let mut missing_external = valid();
    missing_external.external_inputs.clear();
    let diagnostics = rejection(&missing_external);
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.code == DiagCode::ExportUnsupported && diagnostic.message == STRUCTURE
        }),
        "{diagnostics:?}"
    );
}

#[test]
fn reserved_input_owned_by_another_block_rejects_loudly() {
    let keep = BlockInstance {
        id: BlockId(1),
        class_iri: Arc::from("CDL.Reals.MultiplyByParameter"),
        inputs: vec![ConnectorId(0)],
        outputs: vec![ConnectorId(2)],
        params: ParamTable {
            values: vec![(Arc::from("k"), Value::Real(2.0))],
        },
        decl_order: 1,
        instance_iri: Some(Arc::from("http://example.org#PassExport.keep")),
    };
    let graph = ModelGraph {
        blocks: vec![pass_block(vec![ConnectorId(0)]), keep],
        connectors: vec![
            connector(0, 1, Dir::In, Some("http://example.org#PassExport.u")),
            connector(1, 0, Dir::Out, Some("http://example.org#PassExport.y")),
            connector(2, 1, Dir::Out, None),
        ],
        connections: vec![],
        external_inputs: vec![ConnectorId(0)],
        boundary_outputs: vec![],
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
        boundary_outputs: vec![],
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
fn boundary_alias_on_reserved_output_rejects_instead_of_disappearing() {
    const ALIAS: &str = "http://example.org#PassExport.alias";
    let graph = ModelGraph {
        blocks: vec![pass_block(vec![ConnectorId(0)]), survivor()],
        connectors: vec![
            connector(0, 0, Dir::In, Some("http://example.org#PassExport.u")),
            connector(1, 0, Dir::Out, Some("http://example.org#PassExport.y")),
            connector(2, 1, Dir::Out, None),
        ],
        connections: vec![],
        external_inputs: vec![ConnectorId(0)],
        boundary_outputs: vec![BoundaryOutput {
            iri: Arc::from(ALIAS),
            source: ConnectorId(1),
            attrs: Attrs::default_for(ValueType::Real),
        }],
    };

    assert_eq!(
        rejection(&graph),
        vec![
            Diagnostic::error(DiagCode::ExportUnsupported, BOUNDARY_SOURCE_NOT_EMITTED)
                .with_subject(ALIAS.to_owned())
        ]
    );
}

#[test]
fn connection_from_reserved_output_rejects_instead_of_disappearing() {
    let gain = BlockInstance {
        id: BlockId(1),
        class_iri: Arc::from("CDL.Reals.MultiplyByParameter"),
        inputs: vec![ConnectorId(2)],
        outputs: vec![ConnectorId(3)],
        params: ParamTable {
            values: vec![(Arc::from("k"), Value::Real(2.0))],
        },
        decl_order: 1,
        instance_iri: Some(Arc::from("http://example.org#PassExport.keep")),
    };
    let graph = ModelGraph {
        blocks: vec![pass_block(vec![ConnectorId(0)]), gain],
        connectors: vec![
            connector(0, 0, Dir::In, Some("http://example.org#PassExport.u")),
            connector(1, 0, Dir::Out, Some("http://example.org#PassExport.y")),
            connector(2, 1, Dir::In, None),
            connector(3, 1, Dir::Out, None),
        ],
        connections: vec![Connection {
            from: ConnectorId(1),
            to: ConnectorId(2),
        }],
        external_inputs: vec![ConnectorId(0)],
        boundary_outputs: vec![],
    };

    assert_eq!(
        rejection(&graph),
        vec![
            Diagnostic::error(DiagCode::ExportUnsupported, CONNECTION_ENDPOINT_NOT_EMITTED)
                .with_subject("connector#1".to_owned())
        ]
    );
}

#[test]
fn connection_to_reserved_input_rejects_instead_of_disappearing() {
    let graph = ModelGraph {
        blocks: vec![pass_block(vec![ConnectorId(0)]), survivor()],
        connectors: vec![
            connector(0, 0, Dir::In, Some("http://example.org#PassExport.u")),
            connector(1, 0, Dir::Out, Some("http://example.org#PassExport.y")),
            connector(2, 1, Dir::Out, None),
        ],
        connections: vec![Connection {
            from: ConnectorId(2),
            to: ConnectorId(0),
        }],
        external_inputs: vec![ConnectorId(0)],
        boundary_outputs: vec![],
    };

    assert_eq!(
        rejection(&graph),
        vec![
            Diagnostic::error(DiagCode::ExportUnsupported, CONNECTION_ENDPOINT_NOT_EMITTED)
                .with_subject("connector#0".to_owned())
        ]
    );
}

#[test]
fn alias_on_unclaimed_output_reports_structure_before_loss() {
    const ALIAS: &str = "http://example.org#PassExport.orphanAlias";
    let graph = ModelGraph {
        blocks: vec![pass_block(vec![ConnectorId(0)]), survivor()],
        connectors: vec![
            connector(0, 0, Dir::In, Some("http://example.org#PassExport.u")),
            connector(1, 0, Dir::Out, Some("http://example.org#PassExport.y")),
            connector(2, 1, Dir::Out, None),
            connector(3, 1, Dir::Out, None),
        ],
        connections: vec![],
        external_inputs: vec![ConnectorId(0)],
        boundary_outputs: vec![BoundaryOutput {
            iri: Arc::from(ALIAS),
            source: ConnectorId(3),
            attrs: Attrs::default_for(ValueType::Real),
        }],
    };

    assert_eq!(
        rejection(&graph),
        vec![
            Diagnostic::error(DiagCode::ExportUnsupported, STRUCTURE)
                .with_subject("http://example.org#PassExport.keep".to_owned()),
            Diagnostic::error(DiagCode::ExportUnsupported, BOUNDARY_SOURCE_NOT_EMITTED)
                .with_subject(ALIAS.to_owned()),
        ]
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
        boundary_outputs: vec![],
    };
    let diagnostics = rejection(&graph);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message == TOTAL_DEFERRAL),
        "{diagnostics:?}"
    );
}

#[test]
fn only_reserved_pass_through_blocks_reject_with_truthful_message() {
    let graph = ModelGraph {
        blocks: vec![pass_block(vec![ConnectorId(0)])],
        connectors: vec![
            connector(0, 0, Dir::In, Some("http://example.org#PassExport.u")),
            connector(1, 0, Dir::Out, Some("http://example.org#PassExport.y")),
        ],
        connections: vec![],
        external_inputs: vec![ConnectorId(0)],
        boundary_outputs: vec![],
    };
    let diagnostics = rejection(&graph);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message == TOTAL_DEFERRAL),
        "{diagnostics:?}"
    );
}
