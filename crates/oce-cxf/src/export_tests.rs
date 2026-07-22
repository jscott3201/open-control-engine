//! Tests for the minimal exporter's acceptance and rejection surfaces: an imported
//! `minimal_loop` now exports cleanly, while everything outside the flat/ground/scalar/attr-free
//! subset — an empty graph, declared connector attrs, enum parameters, IRI-less blocks,
//! non-finite Reals — is a typed `ExportUnsupported` rejection (subject = the owning block's
//! `instance_iri`), never a panic, and identical across repeated calls.

use std::sync::Arc;

use oce_diag::{DiagCode, Diagnostic, Severity};
use oce_model::{
    Attrs, BlockId, BlockInstance, Connector, ConnectorId, Dir, ModelGraph, ParamTable, RealAttrs,
    Value, ValueType,
};

use super::{CxfError, ResolveOptions, export, import_cxf};

const MINIMAL_LOOP: &str = include_str!("../tests/fixtures/minimal_loop.jsonld");
const CONNECTOR_ATTRS: &str = include_str!("../tests/fixtures/connector_attrs.jsonld");
const G36_ENUM_PARAM: &str = include_str!("../tests/fixtures/g36/cooling_only_dampers.jsonld");

fn import(src: &str) -> ModelGraph {
    let (graph, _report) =
        import_cxf(src.as_bytes(), &ResolveOptions::default()).expect("fixture resolves");
    graph
}

/// Unwrap a rejection: `export` must return `Err(CxfError::Validation(_))` with a non-empty
/// diagnostic list where every entry is an `ExportUnsupported` error. Panics (failing the
/// calling test) on `Ok` or any other error shape.
fn rejection(model: &ModelGraph) -> Vec<Diagnostic> {
    match export(model) {
        Err(CxfError::Validation(diags)) => {
            assert!(!diags.is_empty(), "a rejection must carry diagnostics");
            for d in &diags {
                assert_eq!(
                    d.code,
                    DiagCode::ExportUnsupported,
                    "unexpected code: {d:?}"
                );
                assert_eq!(d.severity, Severity::Error, "unexpected severity: {d:?}");
            }
            diags
        }
        Ok(bytes) => panic!(
            "expected an export rejection, but got Ok with {} byte(s)",
            bytes.len()
        ),
        Err(other) => panic!("expected CxfError::Validation, got {other:?}"),
    }
}

/// A hand-built one-block graph: `Constant` (registered, arity 0/1) with the given params and a
/// single Real output connector — the smallest graph inside the export subset.
fn constant_graph(params: Vec<(Arc<str>, Value)>) -> ModelGraph {
    ModelGraph {
        blocks: vec![BlockInstance {
            id: BlockId(0),
            class_iri: Arc::from("CDL.Reals.Sources.Constant"),
            inputs: vec![],
            outputs: vec![ConnectorId(0)],
            params: ParamTable { values: params },
            decl_order: 0,
            instance_iri: Some(Arc::from("http://example.org#Hand.con")),
        }],
        connectors: vec![Connector::new(
            ConnectorId(0),
            BlockId(0),
            Dir::Out,
            ValueType::Real,
            0,
        )],
        connections: vec![],
        external_inputs: vec![],
    }
}

#[test]
fn resolved_import_now_exports_cleanly() {
    // R4's staged floor rejected this exact graph; the exporter accepts it.
    let bytes = export(&import(MINIMAL_LOOP)).expect("minimal_loop is inside the export subset");
    assert!(!bytes.is_empty());
}

#[test]
fn empty_model_graph_is_rejected_without_a_subject() {
    // Pinned decision: a zero-block ModelGraph rejects. A root with no containsBlock is not a
    // runtime composite — a root-only document re-imports as MalformedDocument (zero candidate
    // roots), so there is no warning-free document to emit.
    let diags = rejection(&ModelGraph::new());
    assert_eq!(diags.len(), 1);
    assert_eq!(
        diags[0].subject, None,
        "a whole-operation rejection must not blame any node"
    );
    assert_eq!(
        diags[0].message,
        "CXF export requires at least one block: an empty ModelGraph has no runtime composite \
         to emit"
    );
}

#[test]
fn declared_connector_attrs_are_rejected_with_the_owning_block_subject() {
    // connector_attrs.jsonld's attr-bearing connector is an OUTPUT (`A.con.y`, iri=None), so the
    // subject must be the OWNING BLOCK's instance_iri — connectors carry no IRI of their own.
    let graph = import(CONNECTOR_ATTRS);
    let diags = rejection(&graph);
    assert_eq!(diags.len(), 1, "exactly one offending connector: {diags:?}");
    assert_eq!(
        diags[0].subject.as_deref(),
        Some("http://example.org#A.con")
    );
    assert_eq!(
        diags[0].message,
        "export subset: connector declares §7.4.1 attributes, which the minimal exporter cannot \
         emit"
    );
}

#[test]
fn rejection_is_identical_across_repeated_calls() {
    let graph = import(CONNECTOR_ATTRS);
    let first = rejection(&graph);
    for _ in 0..3 {
        assert_eq!(
            rejection(&graph),
            first,
            "the rejection must be stable across calls"
        );
    }
}

#[test]
fn enum_valued_parameter_is_rejected_with_the_owning_block_subject() {
    // cooling_only_dampers surfaces `controllerType = Enum` on the conPID BlockInstance (the
    // high-limits/EnergyStandard fixtures do NOT — their enums are specialization-consumed and
    // never reach a BlockInstance param, so those graphs would be accepted).
    let graph = import(G36_ENUM_PARAM);
    let diags = rejection(&graph);
    assert_eq!(
        diags[0].subject.as_deref(),
        Some("http://example.org#g36.source.cooling_only_dampers.conPID"),
        "the first offender in block order is the enum-carrying conPID"
    );
    assert_eq!(
        diags[0].message,
        "export subset: parameter `controllerType` is enumeration-valued, which has no CXF \
         literal form in the minimal export subset"
    );
    assert_eq!(rejection(&graph), diags, "stable across repeated calls");
}

#[test]
fn block_without_an_instance_iri_is_rejected() {
    let mut graph = constant_graph(vec![(Arc::from("k"), Value::Real(2.0))]);
    graph.blocks[0].instance_iri = None;
    let diags = rejection(&graph);
    assert_eq!(diags[0].subject.as_deref(), Some("block#0"));
    assert_eq!(
        diags[0].message,
        "export subset: block has no instance_iri to name its CXF node"
    );
}

#[test]
fn non_finite_real_parameters_are_rejected() {
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        // serde_json serializes a non-finite f64 as `null`, which no CxfValue arm re-parses —
        // emitting it would poison the whole document. Reject instead.
        let graph = constant_graph(vec![(Arc::from("k"), Value::Real(bad))]);
        let diags = rejection(&graph);
        assert_eq!(
            diags[0].subject.as_deref(),
            Some("http://example.org#Hand.con")
        );
        assert_eq!(
            diags[0].message,
            "export subset: parameter `k` is a non-finite Real, which cannot round-trip through \
             JSON"
        );
    }
}

#[test]
fn dotted_parameter_name_is_rejected() {
    // Re-import recovers the parameter name as the segment after the last `.`; a dotted name
    // would silently come back renamed.
    let graph = constant_graph(vec![(Arc::from("k.nested"), Value::Real(2.0))]);
    let diags = rejection(&graph);
    assert_eq!(
        diags[0].subject.as_deref(),
        Some("http://example.org#Hand.con")
    );
    assert_eq!(
        diags[0].message,
        "export subset: parameter name `k.nested` is not a bare member name (re-import recovers \
         the name after the last `.`)"
    );
}

#[test]
fn string_parameter_round_trips_through_export_and_reimport() {
    // Strings are in-subset: emitted as a quoted CDL string-literal expression (escaping `\` and
    // `"`), re-grounded by oce-expr to the bit-identical Value::String.
    let tricky = r#"deg "C" \ path"#;
    let graph = constant_graph(vec![
        (Arc::from("k"), Value::Real(2.0)),
        (Arc::from("note"), Value::String(Arc::from(tricky))),
    ]);
    let bytes = export(&graph).expect("String params are inside the subset");
    let (reimported, report) =
        import_cxf(&bytes, &ResolveOptions::default()).expect("exported doc re-imports");
    assert!(
        report.is_empty(),
        "clean reimport: {:?}",
        report.diagnostics
    );
    let note = reimported.blocks[0]
        .params
        .values
        .iter()
        .find(|(n, _)| n.as_ref() == "note")
        .expect("note parameter survives");
    assert!(
        note.1.bit_eq(&Value::String(Arc::from(tricky))),
        "got {:?}",
        note.1
    );
}

#[test]
fn mismatched_port_wiring_is_rejected_not_exported_shifted() {
    // The block lists its Out connector under `inputs`: structurally inconsistent — exporting it
    // would emit wiring the importer rebuilds differently. Two diagnostics: the failed claim
    // (wrong direction) and the orphan scan (the connector ends up in no port list).
    let mut graph = constant_graph(vec![(Arc::from("k"), Value::Real(2.0))]);
    graph.blocks[0].inputs = vec![ConnectorId(0)];
    graph.blocks[0].outputs = vec![];
    let diags = rejection(&graph);
    assert_eq!(diags.len(), 2, "got: {diags:?}");
    for d in &diags {
        assert_eq!(d.subject.as_deref(), Some("http://example.org#Hand.con"));
        assert_eq!(
            d.message,
            "export subset: block/connector wiring is structurally inconsistent"
        );
    }
}

/// A hand-built one-block graph around `Abs` (one In, one Out connector) — the smallest shape
/// with an input, for the external-input and connection-direction rejection tests.
fn abs_graph() -> ModelGraph {
    ModelGraph {
        blocks: vec![BlockInstance {
            id: BlockId(0),
            class_iri: Arc::from("CDL.Reals.Abs"),
            inputs: vec![ConnectorId(0)],
            outputs: vec![ConnectorId(1)],
            params: ParamTable::default(),
            decl_order: 0,
            instance_iri: Some(Arc::from("http://example.org#Hand.abs")),
        }],
        connectors: vec![
            Connector::new(ConnectorId(0), BlockId(0), Dir::In, ValueType::Real, 0),
            Connector::new(ConnectorId(1), BlockId(0), Dir::Out, ValueType::Real, 1),
        ],
        connections: vec![],
        external_inputs: vec![],
    }
}

#[test]
fn external_input_without_a_boundary_iri_is_rejected() {
    // The resolver always stores the elided boundary IRI on the driven child input; a hand-built
    // graph without one cannot rebuild the root's hasInput.
    let mut graph = abs_graph();
    graph.external_inputs = vec![ConnectorId(0)];
    let diags = rejection(&graph);
    assert_eq!(diags.len(), 1, "got: {diags:?}");
    assert_eq!(
        diags[0].subject.as_deref(),
        Some("http://example.org#Hand.abs")
    );
    assert_eq!(
        diags[0].message,
        "export subset: external input carries no boundary IRI to rebuild the root hasInput"
    );
}

#[test]
fn hand_built_attr_bearing_connector_is_rejected() {
    // Same rejection as the imported connector_attrs fixture, proven on the builder path: any
    // Some field in the attrs set is out of subset because the exporter emits none.
    let mut graph = constant_graph(vec![(Arc::from("k"), Value::Real(2.0))]);
    graph.connectors[0] = Connector::new(ConnectorId(0), BlockId(0), Dir::Out, ValueType::Real, 0)
        .with_attrs(Attrs::Real(RealAttrs {
            unit: Some(Arc::from("K")),
            ..RealAttrs::default()
        }))
        .expect("Real attrs on a Real connector");
    let diags = rejection(&graph);
    assert_eq!(
        diags[0].subject.as_deref(),
        Some("http://example.org#Hand.con")
    );
}

#[test]
fn class_path_with_a_hash_is_rejected_by_the_bridge_oracle() {
    // A `#` in the class path shifts the fragment `class_path_of` extracts on re-import
    // ("Constant" here, not the full path) — a silent identity flip, caught by running the real
    // bridge over the @type before emission.
    let mut graph = constant_graph(vec![(Arc::from("k"), Value::Real(2.0))]);
    graph.blocks[0].class_iri = Arc::from("CDL.Reals.Sources#Constant");
    let diags = rejection(&graph);
    assert_eq!(diags.len(), 1, "got: {diags:?}");
    assert_eq!(
        diags[0].subject.as_deref(),
        Some("http://example.org#Hand.con")
    );
    assert_eq!(
        diags[0].message,
        "export subset: class path does not survive the class-IRI bridge round-trip"
    );
}

#[test]
fn obc_prefixed_class_path_is_rejected_before_emission() {
    // An already-OBC-prefixed class path PASSES the bridge round-trip (the bridge strips exactly
    // the one prefix the exporter adds), but registry keys never carry the prefix, so the bytes
    // would always fail re-import with ClassNotFound. Rejected explicitly instead.
    let mut graph = constant_graph(vec![(Arc::from("k"), Value::Real(2.0))]);
    graph.blocks[0].class_iri = Arc::from("Buildings.Controls.OBC.CDL.Reals.Sources.Constant");
    let diags = rejection(&graph);
    assert_eq!(diags.len(), 1, "got: {diags:?}");
    assert_eq!(
        diags[0].subject.as_deref(),
        Some("http://example.org#Hand.con")
    );
    assert_eq!(
        diags[0].message,
        "export subset: class path does not survive the class-IRI bridge round-trip"
    );
}

#[test]
fn string_typed_connector_is_rejected_with_the_owning_block_subject() {
    // Silent acceptance would emit the `S231:Real` placeholder as the connector's datatype — an
    // exported document that lies about the signal type.
    let mut graph = constant_graph(vec![(Arc::from("k"), Value::Real(2.0))]);
    graph.connectors[0] =
        Connector::new(ConnectorId(0), BlockId(0), Dir::Out, ValueType::String, 0);
    let diags = rejection(&graph);
    assert_eq!(diags.len(), 1, "got: {diags:?}");
    assert_eq!(
        diags[0].subject.as_deref(),
        Some("http://example.org#Hand.con")
    );
    assert_eq!(
        diags[0].message,
        "export subset: String connectors are not permitted in CXF (§7.8)"
    );
}

#[test]
fn enum_typed_connector_is_rejected_with_the_owning_block_subject() {
    // Hand-built for isolation, but this arm is import-reachable too: the resolver's
    // `value_type_of_datatype` maps enum datatype IRIs to `ValueType::Enum`, so an imported
    // graph can carry one. Same placeholder-datatype hazard as the String arm.
    let mut graph = constant_graph(vec![(Arc::from("k"), Value::Real(2.0))]);
    graph.connectors[0] = Connector::new(
        ConnectorId(0),
        BlockId(0),
        Dir::Out,
        ValueType::Enum(oce_model::EnumClassId::SIMPLE_CONTROLLER),
        0,
    );
    let diags = rejection(&graph);
    assert_eq!(diags.len(), 1, "got: {diags:?}");
    assert_eq!(
        diags[0].subject.as_deref(),
        Some("http://example.org#Hand.con")
    );
    assert_eq!(
        diags[0].message,
        "export subset: enumeration-typed connectors are outside the minimal export subset"
    );
}

#[test]
fn non_dense_ids_are_rejected_wholesale_without_a_subject() {
    // Every position-based index in plan() (and re-import itself) assumes BlockId.0 /
    // ConnectorId.0 equal vector position; a non-dense graph is rejected whole rather than
    // exported with shifted identities. Whole-graph defect, so no subject.
    let expect_wholesale = |graph: &ModelGraph| {
        let diags = rejection(graph);
        assert_eq!(diags.len(), 1, "got: {diags:?}");
        assert_eq!(diags[0].subject, None);
        assert_eq!(
            diags[0].message,
            "export subset: block/connector wiring is structurally inconsistent"
        );
    };
    let mut graph = constant_graph(vec![(Arc::from("k"), Value::Real(2.0))]);
    graph.blocks[0].id = BlockId(5);
    expect_wholesale(&graph);

    let mut graph = constant_graph(vec![(Arc::from("k"), Value::Real(2.0))]);
    graph.connectors[0].id = ConnectorId(7);
    expect_wholesale(&graph);
}

#[test]
fn backwards_connection_is_rejected_with_the_source_owner_subject() {
    // An in-range connection that is not output→input: emitting it would put isConnectedTo on
    // an input port and the importer would rebuild different wiring. The subject is the source
    // endpoint's owning block.
    let mut graph = abs_graph();
    graph.connections = vec![oce_model::Connection {
        from: ConnectorId(0), // In — backwards
        to: ConnectorId(1),   // Out
    }];
    let diags = rejection(&graph);
    assert_eq!(diags.len(), 1, "got: {diags:?}");
    assert_eq!(
        diags[0].subject.as_deref(),
        Some("http://example.org#Hand.abs")
    );
    assert_eq!(
        diags[0].message,
        "export subset: block/connector wiring is structurally inconsistent"
    );
}

#[test]
fn duplicate_block_instance_iris_are_rejected() {
    // Two blocks sharing one instance_iri would emit two @graph nodes with one @id (plus
    // colliding minted port ids) — re-import would fail DuplicateId. Rejected at plan time:
    // one diagnostic for the block-node collision, one for the colliding minted port.
    let mut graph = constant_graph(vec![(Arc::from("k"), Value::Real(2.0))]);
    graph.blocks.push(BlockInstance {
        id: BlockId(1),
        class_iri: Arc::from("CDL.Reals.Sources.Constant"),
        inputs: vec![],
        outputs: vec![ConnectorId(1)],
        params: ParamTable::default(),
        decl_order: 1,
        instance_iri: Some(Arc::from("http://example.org#Hand.con")), // same @id as block 0
    });
    graph.connectors.push(Connector::new(
        ConnectorId(1),
        BlockId(1),
        Dir::Out,
        ValueType::Real,
        1,
    ));
    let diags = rejection(&graph);
    assert_eq!(diags.len(), 2, "got: {diags:?}");
    for d in &diags {
        assert_eq!(d.subject.as_deref(), Some("http://example.org#Hand.con"));
        assert_eq!(
            d.message,
            "export subset: emitted node @id collides with another emitted node @id"
        );
    }
}

#[test]
fn parameter_named_like_a_minted_port_id_is_rejected() {
    // A parameter literally named `out0` mints the same @id as the block's first minted output
    // port — the emitted document would carry two nodes with one @id. Rejected at plan time.
    let graph = constant_graph(vec![(Arc::from("out0"), Value::Real(1.0))]);
    let diags = rejection(&graph);
    assert_eq!(diags.len(), 1, "got: {diags:?}");
    assert_eq!(
        diags[0].subject.as_deref(),
        Some("http://example.org#Hand.con")
    );
    assert_eq!(
        diags[0].message,
        "export subset: emitted node @id collides with another emitted node @id"
    );
}

#[test]
fn every_rejection_path_returns_instead_of_panicking() {
    // The never-panics property from the R4 floor survives on every path: exercise each
    // rejection shape and the acceptance shape through the same call.
    let mut graphs: Vec<ModelGraph> = vec![
        ModelGraph::new(),
        import(MINIMAL_LOOP),
        import(CONNECTOR_ATTRS),
        import(G36_ENUM_PARAM),
        constant_graph(vec![(Arc::from("k"), Value::Real(f64::NAN))]),
        constant_graph(vec![(
            Arc::from("mode"),
            Value::Enum {
                class: oce_model::EnumClassId::SIMPLE_CONTROLLER,
                ordinal: 1,
            },
        )]),
        constant_graph(vec![(Arc::from("out0"), Value::Real(1.0))]),
    ];
    let with_class = |class: &str| {
        let mut g = constant_graph(vec![(Arc::from("k"), Value::Real(2.0))]);
        g.blocks[0].class_iri = Arc::from(class);
        g
    };
    graphs.push(with_class("CDL.Reals.Sources#Constant"));
    graphs.push(with_class(
        "Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
    ));
    for value_type in [
        ValueType::String,
        ValueType::Enum(oce_model::EnumClassId::SIMPLE_CONTROLLER),
    ] {
        let mut g = constant_graph(vec![(Arc::from("k"), Value::Real(2.0))]);
        g.connectors[0] = Connector::new(ConnectorId(0), BlockId(0), Dir::Out, value_type, 0);
        graphs.push(g);
    }
    let mut non_dense = constant_graph(vec![(Arc::from("k"), Value::Real(2.0))]);
    non_dense.blocks[0].id = BlockId(5);
    graphs.push(non_dense);
    let mut backwards = abs_graph();
    backwards.connections = vec![oce_model::Connection {
        from: ConnectorId(0),
        to: ConnectorId(1),
    }];
    graphs.push(backwards);
    let mut no_boundary_iri = abs_graph();
    no_boundary_iri.external_inputs = vec![ConnectorId(0)];
    graphs.push(no_boundary_iri);
    let mut out_of_range_external = abs_graph();
    out_of_range_external.external_inputs = vec![ConnectorId(9)];
    graphs.push(out_of_range_external);
    for g in &graphs {
        // Ok or a typed Validation error — anything else (or a panic) fails the test.
        match export(g) {
            Ok(_) | Err(CxfError::Validation(_)) => {}
            Err(other) => panic!("unexpected error shape: {other:?}"),
        }
    }
}
