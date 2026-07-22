//! RT-2 export fixpoint, import-parity oracle, and value-shape tests for the minimal CXF
//! exporter (#141).
//!
//! The ratified contract: `G1 = import(fixture); bytes = export(G1); G2 = import(bytes);
//! render(G1) == render(G2)` bit-exact — via the same hand-written [`render::render`] the
//! resolver goldens use (floats by `to_bits()`), because `ModelGraph` is deliberately not
//! `Serialize`/`PartialEq`. The oracle tests re-check the load-bearing fields (class identity,
//! parameter values, connector types, boundary reconstruction) explicitly, independent of the
//! render string.

mod render;

use oce_cxf::{ResolveOptions, export, import_cxf};
use oce_model::{ConnectorId, ModelGraph, Value};
use render::render;

const FIXTURE: &str = include_str!("fixtures/minimal_loop.jsonld");

fn import_ok(bytes: &[u8]) -> ModelGraph {
    let (g, report) =
        import_cxf(bytes, &ResolveOptions::default()).expect("document must resolve without error");
    assert!(
        report.is_empty(),
        "expected zero diagnostics, got: {:?}",
        report.diagnostics
    );
    g
}

fn export_ok(g: &ModelGraph) -> Vec<u8> {
    export(g).expect("minimal_loop is inside the minimal export subset")
}

#[test]
fn export_then_import_reaches_the_rt2_fixpoint() {
    let g1 = import_ok(FIXTURE.as_bytes());
    let bytes = export_ok(&g1);
    let g2 = import_ok(&bytes);
    assert_eq!(
        render(&g1),
        render(&g2),
        "render(G1) must equal render(G2) bit-exactly"
    );
    // Second order: G2 renders equal to G1 in every field export reads, so exporting G2 must
    // reproduce the same bytes — the fixpoint holds at the byte level too.
    assert_eq!(export_ok(&g2), bytes);
}

#[test]
fn reimport_reproduces_class_identity_params_and_types_field_by_field() {
    // Oracle cross-check independent of the render string: every emitted @type re-imports to the
    // identical class_iri, every parameter re-grounds bit-identically, and derive_value_type
    // reproduces every connector's value_type.
    let g1 = import_ok(FIXTURE.as_bytes());
    let g2 = import_ok(&export_ok(&g1));

    assert_eq!(g1.blocks.len(), g2.blocks.len());
    for (a, b) in g1.blocks.iter().zip(&g2.blocks) {
        assert_eq!(a.class_iri, b.class_iri, "class identity must survive");
        assert_eq!(a.instance_iri, b.instance_iri, "block @id must survive");
        assert_eq!(a.inputs, b.inputs);
        assert_eq!(a.outputs, b.outputs);
        assert_eq!(a.params.values.len(), b.params.values.len());
        for ((an, av), (bn, bv)) in a.params.values.iter().zip(&b.params.values) {
            assert_eq!(an, bn, "parameter name must survive");
            assert!(
                av.bit_eq(bv),
                "parameter {an} must re-ground bit-identically: {av:?} vs {bv:?}"
            );
        }
    }
    assert_eq!(g1.connectors.len(), g2.connectors.len());
    for (a, b) in g1.connectors.iter().zip(&g2.connectors) {
        assert_eq!(a.value_type, b.value_type, "derive_value_type must agree");
        assert_eq!(a.dir, b.dir);
        assert_eq!(a.block, b.block);
    }
    assert_eq!(g1.connections, g2.connections);
    assert_eq!(g1.external_inputs, g2.external_inputs);
}

#[test]
fn whole_number_reals_emit_with_a_fractional_part_and_reground_to_real() {
    // The type-flip guard, Real direction: CxfValue is untagged Int-before-Float, so a
    // whole-number Real emitted bare (`2`) would silently re-ground as an Integer. The canonical
    // shape is a bare JSON literal WITH its fractional part.
    let g1 = import_ok(FIXTURE.as_bytes());
    let bytes = export_ok(&g1);
    let text = std::str::from_utf8(&bytes).expect("export emits UTF-8 JSON");
    assert!(
        text.contains(r#"{"@id":"http://example.org#MinLoop.con.k","S231:value":2.0}"#),
        "con.k = Real(2.0) must emit as the bare literal 2.0, got: {text}"
    );
    assert!(
        text.contains(r#"{"@id":"http://example.org#MinLoop.del.samplePeriod","S231:value":1.0}"#),
        "del.samplePeriod = Real(1.0) must emit as the bare literal 1.0"
    );

    let g2 = import_ok(&bytes);
    let con_k = &g2.blocks[0].params.values[0];
    assert_eq!(con_k.0.as_ref(), "k");
    assert!(con_k.1.bit_eq(&Value::Real(2.0)), "got {:?}", con_k.1);
    let sample_period = g2.blocks[2]
        .params
        .values
        .iter()
        .find(|(n, _)| n.as_ref() == "samplePeriod")
        .expect("UnitDelay samplePeriod");
    assert!(sample_period.1.bit_eq(&Value::Real(1.0)));
}

#[test]
fn integer_params_emit_bare_and_reground_to_integer_not_real() {
    // The reverse type-flip guard: del.y_start is Integer(0) (pinned by the resolver test
    // `unit_delay_bare_int_grounds_to_integer_not_real`). It must emit BARE (`0`, never `0.0`)
    // and come back as Integer.
    let g1 = import_ok(FIXTURE.as_bytes());
    let bytes = export_ok(&g1);
    let text = std::str::from_utf8(&bytes).expect("export emits UTF-8 JSON");
    assert!(
        text.contains(r#"{"@id":"http://example.org#MinLoop.del.y_start","S231:value":0}"#),
        "del.y_start = Integer(0) must emit as the bare literal 0"
    );
    assert!(
        !text.contains(r#""@id":"http://example.org#MinLoop.del.y_start","S231:value":0.0"#),
        "del.y_start must never grow a fractional part"
    );

    let g2 = import_ok(&bytes);
    let y_start = g2.blocks[2]
        .params
        .values
        .iter()
        .find(|(n, _)| n.as_ref() == "y_start")
        .expect("UnitDelay y_start");
    assert!(
        y_start.1.bit_eq(&Value::Integer(0)),
        "y_start must re-ground to Integer(0), got {:?}",
        y_start.1
    );
}

#[test]
fn boundary_input_reconstruction_reelides_to_the_original_iri() {
    // Shape A, pinned: the stored boundary IRI comes back as a root hasInput node wired to a
    // DISTINCT minted child port, so re-import re-elides it (AD-2) and restores both the
    // external_inputs entry and the C9 iri overwrite.
    let g1 = import_ok(FIXTURE.as_bytes());
    let bytes = export_ok(&g1);
    let g2 = import_ok(&bytes);

    assert_eq!(g2.external_inputs, vec![ConnectorId(9)]);
    assert_eq!(
        g2.connectors[9].iri.as_deref(),
        Some("http://example.org#MinLoop.uSet"),
        "the boundary IRI must travel back onto the driven child input"
    );

    // Structural pin on the emitted document: the boundary @id sits under the ROOT's hasInput
    // only; the owning block's hasInput lists minted child ports. Sharing one @id between the
    // two lists is the proven-rejected shape.
    let doc: serde_json::Value = serde_json::from_slice(&bytes).expect("export emits JSON");
    let graph = doc["@graph"].as_array().expect("@graph is an array");
    let node = |id: &str| {
        graph
            .iter()
            .find(|n| n["@id"] == id)
            .unwrap_or_else(|| panic!("node {id} must be emitted"))
    };
    let root = node("urn:open-control:cxf-export:root");
    assert_eq!(
        root["S231:hasInput"]["@id"], "http://example.org#MinLoop.uSet",
        "the root lists the boundary @id"
    );
    let gt = node("http://example.org#MinLoop.gt");
    let gt_inputs: Vec<&str> = gt["S231:hasInput"]
        .as_array()
        .expect("gt has two inputs")
        .iter()
        .filter_map(|r| r["@id"].as_str())
        .collect();
    assert_eq!(
        gt_inputs,
        vec![
            "http://example.org#MinLoop.gt.in0",
            "http://example.org#MinLoop.gt.in1"
        ],
        "the owning block lists only minted ports, never the boundary @id"
    );
    let boundary = node("http://example.org#MinLoop.uSet");
    assert_eq!(
        boundary["S231:isConnectedTo"]["@id"], "http://example.org#MinLoop.gt.in1",
        "the boundary drives the minted child port"
    );
}
