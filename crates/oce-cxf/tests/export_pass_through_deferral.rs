//! Reserved pass-through validation that must run before cascade deferral skips Phase 6.

use std::sync::Arc;

use oce_cxf::{CxfError, export, export_with_report};
use oce_diag::{DiagCode, Diagnostic, Severity};
use oce_model::{
    Attrs, BlockId, BlockInstance, Connection, Connector, ConnectorId, Dir, EnumClassId, IntAttrs,
    ModelGraph, ParamTable, RealAttrs, Value, ValueType,
};

const SOURCE_IRI: &str = "http://example.org#PassExport.deferredSource";
const STRUCTURE: &str = "export subset: block/connector wiring is structurally inconsistent";
const EXTERNAL_IRI: &str =
    "export subset: external input carries no boundary IRI to rebuild the root hasInput";
const RESERVED_SHAPE: &str = "export subset: reserved pass-through block does not match its resolver-produced lowering shape";

fn connector(id: u32, block: u32, dir: Dir, iri: Option<&'static str>) -> Connector {
    let mut connector = Connector::new(ConnectorId(id), BlockId(block), dir, ValueType::Real, id);
    connector.iri = iri.map(Arc::from);
    connector
}

fn cascade_deferred_pass_graph() -> ModelGraph {
    ModelGraph {
        blocks: vec![
            BlockInstance {
                id: BlockId(0),
                class_iri: Arc::from("CDL.Reals.Sources.Constant"),
                inputs: vec![],
                outputs: vec![ConnectorId(0)],
                params: ParamTable {
                    values: vec![(
                        Arc::from("controllerType"),
                        Value::Enum {
                            class: EnumClassId::SIMPLE_CONTROLLER,
                            ordinal: 1,
                        },
                    )],
                },
                decl_order: 0,
                instance_iri: Some(Arc::from(SOURCE_IRI)),
            },
            BlockInstance {
                id: BlockId(1),
                class_iri: Arc::from("urn:oce:lowering#PassThrough.Real"),
                inputs: vec![ConnectorId(1)],
                outputs: vec![ConnectorId(2)],
                params: ParamTable::default(),
                decl_order: 1,
                instance_iri: None,
            },
            BlockInstance {
                id: BlockId(2),
                class_iri: Arc::from("CDL.Reals.Sources.Constant"),
                inputs: vec![],
                outputs: vec![ConnectorId(3)],
                params: ParamTable {
                    values: vec![(Arc::from("k"), Value::Real(1.0))],
                },
                decl_order: 2,
                instance_iri: Some(Arc::from("http://example.org#PassExport.keep")),
            },
        ],
        connectors: vec![
            connector(0, 0, Dir::Out, None),
            connector(1, 1, Dir::In, Some("http://example.org#PassExport.u")),
            connector(2, 1, Dir::Out, Some("http://example.org#PassExport.y")),
            connector(3, 2, Dir::Out, None),
        ],
        connections: vec![Connection {
            from: ConnectorId(0),
            to: ConnectorId(1),
        }],
        external_inputs: vec![ConnectorId(1)],
        boundary_outputs: vec![],
    }
}

fn deferral_warnings() -> Vec<Diagnostic> {
    vec![
        Diagnostic::new(
            Severity::Warning,
            DiagCode::ExportDeferred,
            format!(
                "export subset: deferring block `{SOURCE_IRI}` — parameter `controllerType` is \
                 enumeration-valued (class `EnumClass#1`); the block and its downstream consumers \
                 are omitted from the emitted document so the enum-free remainder can export"
            ),
        )
        .with_subject(SOURCE_IRI.to_owned()),
        Diagnostic::new(
            Severity::Warning,
            DiagCode::ExportDeferred,
            "export subset: deferring block `block#1` — all drivers of input connector `in0` were \
             deferred (upstream enumeration); the block is omitted from the emitted document so \
             the enum-free remainder can export",
        )
        .with_subject("block#1".to_owned()),
    ]
}

fn rejection(graph: &ModelGraph) -> Vec<Diagnostic> {
    match export(graph) {
        Err(CxfError::Validation(diagnostics)) => diagnostics,
        other => panic!("expected validation rejection, got {other:?}"),
    }
}

fn expected_rejection(message: &str, subject: &str) -> Vec<Diagnostic> {
    let mut diagnostics = deferral_warnings();
    diagnostics.push(
        Diagnostic::error(DiagCode::ExportUnsupported, message).with_subject(subject.to_owned()),
    );
    diagnostics
}

#[derive(Clone, Copy, Debug)]
enum EndpointMutation {
    OutputOutOfRange,
    OutputWrongOwner,
    OutputWrongDirection,
    InputWrongDirection,
    InputWithoutIri,
    OutputWithoutIri,
}

#[test]
fn cascade_deferral_does_not_hide_reserved_endpoint_errors() {
    let cases = [
        (EndpointMutation::OutputOutOfRange, STRUCTURE),
        (EndpointMutation::OutputWrongOwner, STRUCTURE),
        (EndpointMutation::OutputWrongDirection, STRUCTURE),
        (EndpointMutation::InputWrongDirection, STRUCTURE),
        (EndpointMutation::InputWithoutIri, EXTERNAL_IRI),
        (EndpointMutation::OutputWithoutIri, EXTERNAL_IRI),
    ];

    for (mutation, message) in cases {
        let mut graph = cascade_deferred_pass_graph();
        match mutation {
            EndpointMutation::OutputOutOfRange => {
                graph.blocks[1].outputs[0] = ConnectorId(99);
            }
            EndpointMutation::OutputWrongOwner => graph.connectors[2].block = BlockId(0),
            EndpointMutation::OutputWrongDirection => graph.connectors[2].dir = Dir::In,
            EndpointMutation::InputWrongDirection => graph.connectors[1].dir = Dir::Out,
            EndpointMutation::InputWithoutIri => graph.connectors[1].iri = None,
            EndpointMutation::OutputWithoutIri => graph.connectors[2].iri = None,
        }

        let mut expected = expected_rejection(message, "connector#1");
        match mutation {
            EndpointMutation::OutputOutOfRange => expected.push(
                Diagnostic::error(DiagCode::ExportUnsupported, STRUCTURE)
                    .with_subject("connector#2".to_owned()),
            ),
            EndpointMutation::InputWrongDirection => expected.push(
                Diagnostic::error(DiagCode::ExportUnsupported, STRUCTURE)
                    .with_subject(SOURCE_IRI.to_owned()),
            ),
            _ => {}
        }
        assert_eq!(
            rejection(&graph),
            expected,
            "{mutation:?} escaped reserved validation"
        );
    }
}

#[test]
fn cascade_deferral_does_not_hide_reserved_connector_attribute_errors() {
    let cases = [
        Attrs::Real(RealAttrs {
            nominal: Some(1.0),
            ..RealAttrs::default()
        }),
        Attrs::Real(RealAttrs {
            unbounded: Some(true),
            ..RealAttrs::default()
        }),
        Attrs::Real(RealAttrs {
            min: Some(f64::NAN),
            ..RealAttrs::default()
        }),
        Attrs::Real(RealAttrs {
            max: Some(f64::INFINITY),
            ..RealAttrs::default()
        }),
        Attrs::Integer(IntAttrs::default()),
    ];

    for connector_position in [1, 2] {
        for attrs in &cases {
            let mut graph = cascade_deferred_pass_graph();
            graph.connectors[connector_position].attrs = attrs.clone();
            assert_eq!(
                rejection(&graph),
                expected_rejection(RESERVED_SHAPE, "block#1"),
                "connector {connector_position} attributes escaped reserved validation"
            );
        }
    }
}

#[test]
fn cascade_deferral_does_not_hide_an_undeclared_owned_connector() {
    let mut graph = cascade_deferred_pass_graph();
    graph.connectors.push(connector(
        4,
        1,
        Dir::Out,
        Some("http://example.org#PassExport.stray"),
    ));

    assert_eq!(
        rejection(&graph),
        expected_rejection(STRUCTURE, "connector#4")
    );
}

#[test]
fn canonical_cascade_deferred_reserved_shape_remains_warning_only() {
    let report = export_with_report(&cascade_deferred_pass_graph())
        .expect("canonical deferred pass-through shape must not reject");
    assert_eq!(report.warnings, deferral_warnings());
    assert!(!report.bytes.is_empty());
}
