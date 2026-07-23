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
use oce_model::{Attrs, ConnectorId, ModelGraph, RealAttrs, Value};
use render::render;

const FIXTURE: &str = include_str!("fixtures/minimal_loop.jsonld");
/// The §7.4.1 attr-rich fixture: all three `TermAttr` wire shapes (bare/typed/IRI) plus Real
/// `min`/`max` on one Real output connector. Used by the R6 attr-bearing RT-2 fixpoint test.
const ATTRS_FIXTURE: &str = include_str!("fixtures/connector_attrs.jsonld");

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
    export(g).expect("graph is inside the minimal export subset")
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

#[test]
fn attr_bearing_connector_reaches_the_rt2_render_fixpoint() {
    // R6 attr-bearing RT-2 fixpoint: `G1 = import(connector_attrs.jsonld)`; `bytes = export(G1)`;
    // `G2 = import(bytes)`; `render(G1) == render(G2)` bit-exact. The fixture uses all three
    // `TermAttr` wire shapes (bare "K", typed quantity, IRI displayUnit) plus Real `min`/`max`.
    // Under Bare-Scalar Canonical, export emits bare shapes where the original had typed/IRI;
    // `import(export(G1))` then re-parses those bare shapes back to the identical `Arc<str>` terms
    // and finite `f64` bounds, so `render(G1) == render(G2)` holds (render compares `as_deref()`
    // for terms and `to_bits()` for bounds — the BSC shape survives the `as_term()` collapse and
    // reproduces itself).
    let g1 = import_ok(ATTRS_FIXTURE.as_bytes());
    let bytes = export_ok(&g1);
    let g2 = import_ok(&bytes);
    assert_eq!(
        render(&g1),
        render(&g2),
        "render(G1) must equal render(G2) bit-exactly for the attr-bearing fixture"
    );
    // Second order: exporting G2 reproduces the same bytes — the fixpoint holds at the byte level
    // too (the bare shapes are a fixpoint of `import ∘ export`).
    assert_eq!(export_ok(&g2), bytes);
}

#[test]
fn attr_bearing_connector_reproduces_all_five_bsc_real_attrs_field_by_field() {
    // Oracle cross-check independent of the render string: the five in-subset §7.4.1 `RealAttrs`
    // fields (unit, quantity, display_unit, min, max) survive `export ∘ import` bit-identically.
    // The fixture carries all three term wire shapes and both numeric bounds, so each field is
    // exercised. `nominal`/`unbounded` stay `None` on both sides (the importer hardcodes them).
    let g1 = import_ok(ATTRS_FIXTURE.as_bytes());
    let g2 = import_ok(&export_ok(&g1));
    let attrs_of = |g: &ModelGraph| match &g
        .connectors
        .iter()
        .find(|c| matches!(&c.attrs, Attrs::Real(a) if a.unit.is_some()))
        .expect("the con.y Real connector carries attrs")
        .attrs
    {
        Attrs::Real(a) => a.clone(),
        _ => unreachable!("matched a Real connector"),
    };
    let a1 = attrs_of(&g1);
    let a2 = attrs_of(&g2);
    assert_eq!(a1.unit, a2.unit, "bare-string unit survives BSC");
    assert_eq!(
        a1.quantity, a2.quantity,
        "typed-literal quantity survives BSC"
    );
    assert_eq!(
        a1.display_unit, a2.display_unit,
        "IRI-node displayUnit survives BSC"
    );
    assert_eq!(
        a1.min.map(f64::to_bits),
        a2.min.map(f64::to_bits),
        "Real min survives BSC bit-exactly"
    );
    assert_eq!(
        a1.max.map(f64::to_bits),
        a2.max.map(f64::to_bits),
        "Real max survives BSC bit-exactly"
    );
    assert_eq!(a1.nominal, a2.nominal, "nominal stays None on both sides");
    assert_eq!(
        a1.unbounded, a2.unbounded,
        "unbounded stays None on both sides"
    );
    // Sanity: the fixture's parsed attrs are the expected rich set (guards against a future
    // fixture edit silently weakening this oracle).
    let expected = RealAttrs {
        unit: Some(std::sync::Arc::from("K")),
        quantity: Some(std::sync::Arc::from("ThermodynamicTemperature")),
        display_unit: Some(std::sync::Arc::from("degC")),
        min: Some(0.0),
        max: Some(350.0),
        nominal: None,
        unbounded: None,
    };
    assert_eq!(
        a1, expected,
        "the fixture's parsed attrs are the expected rich set"
    );
}

#[test]
fn attr_bearing_port_emits_the_bare_scalar_canonical_keys() {
    // Structural pin on the emitted document: the attr-bearing port node carries the five BSC
    // keys as bare scalars/strings — `S231:unit`/`quantity`/`displayUnit` as bare strings, and
    // `S231:min`/`max` as bare JSON numbers (fractional when whole, per the type-flip guard).
    // The bare shape is what survives `as_term()` and reproduces itself; a typed/IRI shape would
    // not reproduce the original `RealAttrs` on re-import.
    let g1 = import_ok(ATTRS_FIXTURE.as_bytes());
    let bytes = export_ok(&g1);
    let text = std::str::from_utf8(&bytes).expect("export emits UTF-8 JSON");
    assert!(
        text.contains(r#""S231:unit":"K""#),
        "unit emits as a bare string, got: {text}"
    );
    assert!(
        text.contains(r#""S231:quantity":"ThermodynamicTemperature""#),
        "quantity emits as a bare string (the typed literal collapses to bare), got: {text}"
    );
    assert!(
        text.contains(r#""S231:displayUnit":"degC""#),
        "displayUnit emits as a bare string (the IRI node collapses to bare), got: {text}"
    );
    assert!(
        text.contains(r#""S231:min":0.0"#),
        "min emits as a bare number with its fractional part, got: {text}"
    );
    assert!(
        text.contains(r#""S231:max":350.0"#),
        "max emits as a bare number with its fractional part, got: {text}"
    );
}
