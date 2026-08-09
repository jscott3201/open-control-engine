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

use oce_cxf::{ResolveOptions, export, export_with_report, import_cxf};
use oce_diag::{DiagCode, Diagnostic, Severity, has_errors};
use oce_model::{Attrs, ConnectorId, ModelGraph, RealAttrs, Value, ValueType};
use render::render;

const FIXTURE: &str = include_str!("fixtures/minimal_loop.jsonld");
/// The §7.4.1 attr-rich fixture: all three `TermAttr` wire shapes (bare/typed/IRI) plus Real
/// `min`/`max` on one Real output connector. Used by the R6 attr-bearing RT-2 fixpoint test.
const ATTRS_FIXTURE: &str = include_str!("fixtures/connector_attrs.jsonld");
/// R7 arrays flattened-form fixtures: a 1-D flattened array, a 2-D flattened array, a fill()
/// expression array (R5 lowers it to scalar params), and the preserved-encoding partner of the
/// 1-D fixture (both encodings lower to the same flat graph and emit identical flattened bytes).
const ARRAY_FLAT: &str = include_str!("fixtures/array_flattened.jsonld");
const ARRAY_2D_FLAT: &str = include_str!("fixtures/array2d_flattened.jsonld");
const ARRAY_EXPR: &str = include_str!("fixtures/array_expression_preserved.jsonld");
/// The R7 enum-deferral miniature: a `containsBlock` child carrying an enum-valued parameter (a
/// CDL.Reals.PID with `controllerType`), its cascade-deferred downstream consumer, and a surviving
/// downstream pair (a Constant source and a MultiplyByParameter gain) driven by the survivor cone.
/// A boundary input drives the enum-bearing block's child port (exercises the Phase 6 skip).
const ENUM_MINIATURE: &str = include_str!("fixtures/enum_deferral_miniature.jsonld");

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
            "http://example.org#MinLoop.gt.u1",
            "http://example.org#MinLoop.gt.in1"
        ],
        "the owning block retains authored ports and mints only the boundary-driven child"
    );
    let boundary = node("http://example.org#MinLoop.uSet");
    assert_eq!(
        boundary["S231:isOfDataType"]["@id"], "S231:Real",
        "the exported boundary carries the driven child input's value type"
    );
    assert_eq!(
        boundary["S231:isConnectedTo"]["@id"], "http://example.org#MinLoop.gt.in1",
        "the boundary drives the minted child port"
    );
    assert_eq!(
        g2.connectors[g2.external_inputs[0].0 as usize].value_type,
        ValueType::Real,
        "export → re-import preserves the boundary-fed connector type"
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

// ─────────────────────────────────────────────────────────────────────────
// R7 — arrays flattened-form RT-2 goldens (no new emit logic; pins R5).
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn flattened_array_params_reach_the_rt2_fixpoint() {
    // R5 already emits flattened arrays correctly; this pins the RT-2 fixpoint for the 1-D
    // flattened shape. `G1 = import(ARRAY_FLAT)`; `bytes = export(G1)`; `G2 = import(bytes)`;
    // `render(G1) == render(G2)` bit-exact; `export(G2) == bytes` (second-order byte fixpoint).
    // The oracle cross-check pins the flattened local_name (`k_1`, `k_2`) and Real bit-equality.
    let g1 = import_ok(ARRAY_FLAT.as_bytes());
    let bytes = export_ok(&g1);
    let g2 = import_ok(&bytes);
    assert_eq!(render(&g1), render(&g2));
    assert_eq!(export_ok(&g2), bytes); // second-order byte fixpoint
    assert_eq!(g2.blocks[0].params.values.len(), 2);
    assert_eq!(g2.blocks[0].params.values[0].0.as_ref(), "k_1");
    assert!(g2.blocks[0].params.values[0].1.bit_eq(&Value::Real(2.0)));
    assert!(g2.blocks[0].params.values[1].1.bit_eq(&Value::Real(3.0)));
}

#[test]
fn flattened_2d_array_params_reach_the_rt2_fixpoint() {
    // The 2-D golden is the ONLY pin for multi-underscore `local_name` handling
    // (`k_1_1` .. `k_2_2`) through the scalar branch — the 1-D golden does NOT catch a regression
    // there, which is why the 2-D golden is required.
    let g1 = import_ok(ARRAY_2D_FLAT.as_bytes());
    let bytes = export_ok(&g1);
    let g2 = import_ok(&bytes);
    assert_eq!(render(&g1), render(&g2));
    assert_eq!(export_ok(&g2), bytes);
}

#[test]
fn expression_array_preserved_reaches_the_rt2_fixpoint() {
    // Pins the expression-through-flattening convergence: a `fill(...)` expression array lowers
    // to scalar params on import, and those scalar params round-trip through export identically.
    let g1 = import_ok(ARRAY_EXPR.as_bytes());
    let bytes = export_ok(&g1);
    let g2 = import_ok(&bytes);
    assert_eq!(render(&g1), render(&g2));
    assert_eq!(export_ok(&g2), bytes);
}

#[test]
fn preserved_and_flattened_arrays_converge_through_export() {
    // The convergence target: the preserved encoding and the flattened encoding lower to the
    // SAME graph (`render` equal) AND emit the SAME flattened bytes (`export` equal). This is the
    // heart of the arrays flattened-form contract — both encodings reach one canonical wire shape.
    let g_pres = import_ok(include_str!("fixtures/array_preserved.jsonld").as_bytes());
    let g_flat = import_ok(ARRAY_FLAT.as_bytes());
    assert_eq!(
        render(&g_pres),
        render(&g_flat),
        "both encodings lower identically"
    );
    assert_eq!(
        export_ok(&g_pres),
        export_ok(&g_flat),
        "both encodings emit identical flattened bytes (the convergence target)"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// R7 — enum tracked-deferral RT-2 for the survivor cone (Deliverable B).
// ─────────────────────────────────────────────────────────────────────────

/// Re-import the emitted bytes and assert the survivor-cone invariants: zero error diagnostics
/// (no `SingleAssignment`, no `UnresolvedReference`), zero `ValueType::Enum` connectors, and zero
/// `Value::Enum` parameters. Returns the re-imported graph for further assertions.
fn reimport_survivor(bytes: &[u8]) -> ModelGraph {
    let (g, report) =
        import_cxf(bytes, &ResolveOptions::default()).expect("survivor bytes must re-import");
    assert!(
        !has_errors(&report.diagnostics),
        "re-import must carry zero error diagnostics, got: {:?}",
        report.diagnostics
    );
    assert!(
        g.connectors
            .iter()
            .all(|c| !matches!(c.value_type, ValueType::Enum(_))),
        "re-imported graph must carry zero enum connectors"
    );
    assert!(
        g.blocks.iter().all(|b| b
            .params
            .values
            .iter()
            .all(|(_, v)| !matches!(v, Value::Enum { .. }))),
        "re-imported graph must carry zero enum parameters"
    );
    g
}

/// Collect every `@id` string in the emitted JSON-LD `@graph` (the set of all emitted nodes —
/// block nodes, port nodes, boundary nodes, the synthetic root). Used by T4 to cross-reference
/// against the deferred blocks' instance_iris and minted port IRIs: NO emitted node may belong
/// to a deferred block.
fn emitted_graph_ids(bytes: &[u8]) -> Vec<String> {
    let doc: serde_json::Value = serde_json::from_slice(bytes).expect("export emits JSON");
    let graph = doc["@graph"]
        .as_array()
        .expect("@graph is an array of emitted nodes");
    graph
        .iter()
        .filter_map(|n| n["@id"].as_str().map(String::from))
        .collect()
}

#[test]
fn enum_deferral_miniature_reaches_the_rt2_fixpoint_for_the_survivor_cone() {
    // The R7 deferral contract, pinned on the hand-built miniature:
    //   T1 — `export_with_report(&g)` is `Ok` (deferral, not rejection).
    //   T2 — the emitted bytes re-import to an enum-free `ModelGraph` with zero error diagnostics.
    //   T3 — the `ExportReport` carries ≥1 `DiagCode::ExportDeferred` warning naming the offending
    //        block's `instance_iri`.
    //   T4 — the emitted `@graph` contains NO node (block OR port) whose `@id` corresponds to ANY
    //        deferred block (pins the strengthened subset-escape fix; the failure mode is a silent
    //        `Real` type-mutation or a dangling port node, NOT an arity failure).
    let g1 = import_ok(ENUM_MINIATURE.as_bytes());
    let report = export_with_report(&g1).expect("T1: enum deferral must be Ok, not Err");

    // T3: ≥1 ExportDeferred warning, all warnings are ExportDeferred + Warning severity, and the
    // enum-bearing block (`con`) is named.
    assert!(
        !report.warnings.is_empty(),
        "T3: the report must carry ≥1 ExportDeferred warning"
    );
    assert!(
        report
            .warnings
            .iter()
            .all(|d| d.code == DiagCode::ExportDeferred && d.severity == Severity::Warning),
        "T3: every warning must be ExportDeferred at Warning severity, got: {:?}",
        report.warnings
    );
    let con_iri = "http://example.org#Mini.con";
    assert!(
        report
            .warnings
            .iter()
            .any(|d: &Diagnostic| d.subject.as_deref() == Some(con_iri)),
        "T3: the enum-bearing block `con` must be named by a deferral warning, got: {:?}",
        report
            .warnings
            .iter()
            .map(|d| d.subject.as_deref().unwrap_or("?"))
            .collect::<Vec<_>>()
    );
    // The cascade-deferred consumer (`cons`) is also named — pins the transitive cascade.
    assert!(
        report
            .warnings
            .iter()
            .any(|d| d.subject.as_deref() == Some("http://example.org#Mini.cons")),
        "T3: the cascade-deferred consumer `cons` must also be named, got: {:?}",
        report
            .warnings
            .iter()
            .map(|d| d.subject.as_deref().unwrap_or("?"))
            .collect::<Vec<_>>()
    );

    // T2: re-import is enum-free with zero error diagnostics.
    let g2 = reimport_survivor(&report.bytes);
    // RT-2 holds for the survivor cone. The survivor blocks' class identity, parameter values,
    // and port arities match between g1 and g2 (compared by `instance_iri`, not by raw `BlockId`
    // — the deferred blocks are dropped on export, so g2 re-numbers its survivor ids). Reals are
    // compared by `to_bits()` (bit-exact, the render contract). The second-order byte fixpoint
    // (`export_ok(&g2) == report.bytes`) then pins the survivor cone at the byte level.
    let survivor_iris = [
        "http://example.org#Mini.src",
        "http://example.org#Mini.gain",
    ];
    fn param_bits(v: &Value) -> String {
        match v {
            Value::Real(r) => format!("Real(0x{:016x})", r.to_bits()),
            Value::Integer(i) => format!("Integer({i})"),
            Value::Boolean(b) => format!("Boolean({b})"),
            Value::String(s) => format!("String({s:?})"),
            Value::Enum { class, ordinal } => {
                format!("Enum(class={},ordinal={})", class.0, ordinal)
            }
        }
    }
    // A compact, id-stable render of the survivor cone: for each survivor block (sorted by
    // instance_iri so g1/g2 agree despite re-numbered BlockIds), emit one line carrying the
    // class_iri, the bit-exact params, and the input/output arities.
    let survivor_profile = |g: &ModelGraph| -> String {
        let mut lines: Vec<String> = g
            .blocks
            .iter()
            .filter(|b| survivor_iris.contains(&b.instance_iri.as_deref().unwrap_or("")))
            .map(|b| {
                let params: Vec<String> = b
                    .params
                    .values
                    .iter()
                    .map(|(n, v)| format!("{}={}", n, param_bits(v)))
                    .collect();
                format!(
                    "{} class={} params=[{}] in={} out={}",
                    b.instance_iri.as_deref().unwrap_or(""),
                    b.class_iri,
                    params.join(", "),
                    b.inputs.len(),
                    b.outputs.len(),
                )
            })
            .collect();
        lines.sort();
        lines.join("\n")
    };
    assert_eq!(
        survivor_profile(&g1),
        survivor_profile(&g2),
        "RT-2: the survivor cone (class, params bit-exact, arities) must match between g1 and g2"
    );
    assert_eq!(export_ok(&g2), report.bytes);

    // T4: the emitted @graph contains NO node for ANY deferred block. The deferred blocks are
    // `con` (enum param) and `cons` (cascade). Cross-reference every emitted @id against their
    // instance_iris AND every minted port IRI shape (`{block_iri}.{in|out}{k}`) — a leaked port
    // node for a deferred block is the silent subset-escape this pin catches.
    let ids = emitted_graph_ids(&report.bytes);
    let deferred_iris = [
        "http://example.org#Mini.con",
        "http://example.org#Mini.cons",
    ];
    for diri in deferred_iris {
        assert!(
            !ids.iter().any(|id| id == diri),
            "T4: the deferred block node `{diri}` must NOT appear in the emitted @graph, got: {ids:?}"
        );
        // Minted port IRIs for a deferred block: `{diri}.in{k}` / `{diri}.out{k}`.
        let leaked_ports: Vec<&String> = ids
            .iter()
            .filter(|id| {
                id.starts_with(&format!("{diri}.in")) || id.starts_with(&format!("{diri}.out"))
            })
            .collect();
        assert!(
            leaked_ports.is_empty(),
            "T4: no port node for the deferred block `{diri}` may appear in the emitted @graph, \
             got: {leaked_ports:?}"
        );
    }
    // Sanity: the survivors (src, gain) ARE emitted (guards against the deferral over-reaching and
    // dropping the survivor cone, which would make the T4 negatives vacuously pass).
    assert!(
        ids.iter().any(|id| id == "http://example.org#Mini.src"),
        "the survivor `src` must be emitted"
    );
    assert!(
        ids.iter().any(|id| id == "http://example.org#Mini.gain"),
        "the survivor `gain` must be emitted"
    );
}
