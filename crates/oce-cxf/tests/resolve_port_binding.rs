//! End-to-end port identity: a document that names its ports is wired by name, not by position.
//!
//! The unit tests in `resolve::port_binding_tests` cover the matcher. These resolve whole documents,
//! because the property that matters is not "the permutation was computed" but "the connector
//! carrying the setpoint ends up at the input index `CDL.Reals.PID` reads the setpoint from".
//!
//! `CDL.Reals.PID` is the worked case throughout. Both its inputs are `Real`, so `oce-validate`'s
//! per-position kind check cannot see a swap, and swapping `u_s` with `u_m` exchanges setpoint and
//! measurement — the control action inverts and nothing in the workspace says a word. The reference
//! CDL toolchain, `modelica-json`, orders connectors alphabetically, which renders this class as
//! `u_m` then `u_s`: the dangerous order is what a conforming generator produces.

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::DiagCode;
use oce_model::ModelGraph;
use serde_json::{Value, json};

/// Two `Constant`s driving a `PID`, whose ports are named `names[0]` and `names[1]`.
///
/// The **wiring is fixed**: the 20.0 constant always drives `names[0]` and the 18.0 constant always
/// drives `names[1]`. Only `order` changes, and it changes just one thing — the sequence the two
/// ports appear in inside the `hasInput` array. That is what makes two calls here two *renderings
/// of one model* rather than two different models, which is the only comparison worth making.
fn document(names: [&str; 2], order: [usize; 2]) -> Value {
    let port = |n: &str| format!("http://example.org#M.ctl.{n}");
    json!({
      "@context": { "S231": "http://data.ashrae.org/S231P#", "base": "http://example.org#" },
      "@graph": [
        { "@id": "http://example.org#M", "@type": "S231:Block",
          "S231:containsBlock": [
            { "@id": "http://example.org#M.setpoint" },
            { "@id": "http://example.org#M.measurement" },
            { "@id": "http://example.org#M.ctl" } ] },

        { "@id": "http://example.org#M.setpoint",
          "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
          "S231:hasParameter": { "@id": "http://example.org#M.setpoint.k" },
          "S231:hasOutput": { "@id": "http://example.org#M.setpoint.y" } },
        { "@id": "http://example.org#M.setpoint.k",
          "S231:value": { "@value": "20.0", "@type": "http://www.w3.org/2001/XMLSchema#double" } },
        { "@id": "http://example.org#M.setpoint.y", "@type": "S231:RealOutput",
          "S231:isOfDataType": { "@id": "S231:Real" },
          "S231:isConnectedTo": { "@id": port(names[0]) } },

        { "@id": "http://example.org#M.measurement",
          "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
          "S231:hasParameter": { "@id": "http://example.org#M.measurement.k" },
          "S231:hasOutput": { "@id": "http://example.org#M.measurement.y" } },
        { "@id": "http://example.org#M.measurement.k",
          "S231:value": { "@value": "18.0", "@type": "http://www.w3.org/2001/XMLSchema#double" } },
        { "@id": "http://example.org#M.measurement.y", "@type": "S231:RealOutput",
          "S231:isOfDataType": { "@id": "S231:Real" },
          "S231:isConnectedTo": { "@id": port(names[1]) } },

        { "@id": "http://example.org#M.ctl",
          "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.PID",
          "S231:hasInput": [
            { "@id": port(names[order[0]]) },
            { "@id": port(names[order[1]]) } ],
          "S231:hasOutput": { "@id": "http://example.org#M.ctl.y" } },
        { "@id": port(names[0]), "@type": "S231:RealInput",
          "S231:isOfDataType": { "@id": "S231:Real" } },
        { "@id": port(names[1]), "@type": "S231:RealInput",
          "S231:isOfDataType": { "@id": "S231:Real" } },
        { "@id": "http://example.org#M.ctl.y", "@type": "S231:RealOutput",
          "S231:isOfDataType": { "@id": "S231:Real" } }
      ]
    })
}

/// The CDL names, wired the way the class means them: setpoint to `u_s`, measurement to `u_m`.
const CDL: [&str; 2] = ["u_s", "u_m"];
/// Declaration order, and the alphabetical order `modelica-json` actually emits.
const DECLARED: [usize; 2] = [0, 1];
const ALPHABETICAL: [usize; 2] = [1, 0];

fn import(doc: &Value) -> Result<ModelGraph, CxfError> {
    let bytes = serde_json::to_vec(doc).expect("serialize");
    import_cxf(&bytes, &ResolveOptions::default()).map(|(g, _)| g)
}

/// The parameter value of the `Constant` feeding PID input `port_idx`.
///
/// Following the wire back to its source constant is what makes this a wiring assertion rather
/// than an assertion about port arrays: `20.0` is only reachable at input 0 if the connector the
/// setpoint drives really landed there.
#[track_caller]
fn source_constant_of_pid_input(graph: &ModelGraph, port_idx: usize) -> f64 {
    let pid = graph
        .blocks
        .iter()
        .find(|b| b.class_iri.contains("PID"))
        .expect("PID instance");
    let target = pid.inputs[port_idx];
    let conn = graph
        .connections
        .iter()
        .find(|c| c.to == target)
        .expect("PID input is driven");
    let source_block = graph.connectors[conn.from.0 as usize].block;
    match graph.blocks[source_block.0 as usize].params.values.first() {
        Some((_, oce_model::Value::Real(v))) => *v,
        other => panic!("source constant has no Real parameter: {other:?}"),
    }
}

/// Declaration order resolves as it always has: setpoint at input 0.
#[test]
fn declaration_order_wires_the_setpoint_to_input_zero() {
    let graph = import(&document(CDL, DECLARED)).expect("resolves");
    assert_eq!(source_constant_of_pid_input(&graph, 0), 20.0);
    assert_eq!(source_constant_of_pid_input(&graph, 1), 18.0);
}

/// The finding, fixed. Alphabetical order lists `u_m` first; read positionally that puts the
/// measurement at input 0 and inverts the loop. Binding by name puts the setpoint back.
#[test]
fn alphabetical_order_still_wires_the_setpoint_to_input_zero() {
    let graph = import(&document(CDL, ALPHABETICAL)).expect("resolves");
    assert_eq!(
        source_constant_of_pid_input(&graph, 0),
        20.0,
        "input 0 is u_s, the setpoint — a document listing u_m first must not invert the loop"
    );
    assert_eq!(source_constant_of_pid_input(&graph, 1), 18.0);
}

/// Both orderings must produce the *same* model, not merely two acceptable ones. Port order is a
/// renderer's choice, so it must not survive into the resolved graph at all.
#[test]
fn port_array_order_does_not_survive_into_the_resolved_model() {
    let declared = import(&document(CDL, DECLARED)).expect("resolves");
    let alphabetical = import(&document(CDL, ALPHABETICAL)).expect("resolves");
    for idx in 0..2 {
        assert_eq!(
            source_constant_of_pid_input(&declared, idx),
            source_constant_of_pid_input(&alphabetical, idx),
            "input {idx} differs between the two renderings of the same model"
        );
    }
}

/// Position-named ports — what this engine's own exporter mints — keep binding positionally and
/// raise nothing. The RT-2 round-trip re-imports exactly these bytes.
#[test]
fn position_named_ports_resolve_without_a_diagnostic() {
    let doc = document(["in0", "in1"], DECLARED);
    let bytes = serde_json::to_vec(&doc).expect("serialize");
    let (graph, report) = import_cxf(&bytes, &ResolveOptions::default()).expect("resolves");
    assert!(
        !report.has_errors(),
        "position-named ports must not be diagnosed: {:?}",
        report.diagnostics
    );
    // Positional binding: the document's first-listed port stays at input 0.
    assert_eq!(source_constant_of_pid_input(&graph, 0), 20.0);
}

/// A document naming one port after a declared port and the other after nothing is following no
/// convention, and is reported rather than silently read either way.
#[test]
fn a_partially_named_port_list_is_rejected() {
    let doc = document(["u_s", "in1"], DECLARED);
    let bytes = serde_json::to_vec(&doc).expect("serialize");
    match import_cxf(&bytes, &ResolveOptions::default()) {
        Err(CxfError::Validation(diags)) => assert!(
            diags
                .iter()
                .any(|d| d.code == DiagCode::PortNameMismatch && d.is_error()),
            "expected PortNameMismatch, got {diags:#?}"
        ),
        other => panic!("expected a validation error, got {:?}", other.map(|_| ())),
    }
}

/// Resolution is deterministic under name binding: re-importing the same bytes yields an identical
/// graph. The permutation must not depend on map iteration order.
#[test]
fn name_binding_is_deterministic_across_imports() {
    let doc = document(CDL, ALPHABETICAL);
    let a = import(&doc).expect("resolves");
    let b = import(&doc).expect("resolves");
    let render = |g: &ModelGraph| {
        g.blocks
            .iter()
            .map(|blk| format!("{}:{:?}/{:?}", blk.class_iri, blk.inputs, blk.outputs))
            .collect::<Vec<String>>()
    };
    assert_eq!(render(&a), render(&b));
}

/// A real G36 fixture, with one block's port array permuted: the resolved model must be identical.
///
/// The synthetic documents above prove the mechanism. This proves the property on production input.
/// `ahu_economizer.jsonld` instantiates `CDL.Logical.Latch` as `enableLatch`; both its inputs are
/// `Boolean`, and exchanging `u` with `clr` inverts the latch — set becomes clear. Reordering the
/// `hasInput` array leaves `@graph` order untouched, so every `ConnectorId` is unchanged and the
/// only thing that could differ is which connector sits at which port index. Comparing the whole
/// graph, rather than that one block, is what makes this an assertion about the model instead of
/// about a vector.
///
/// The corpus cannot cover this on its own: all 46 fixtures already list ports in declaration
/// order, so the permutation is the identity on all 1206 of their block instances. Permuting one
/// here is the only way the reorder path meets a real document.
#[test]
fn permuting_a_real_fixtures_port_array_resolves_to_the_same_model() {
    const FIXTURE: &str = include_str!("fixtures/g36/ahu_economizer.jsonld");
    let original: Value = serde_json::from_str(FIXTURE).expect("fixture is valid JSON");

    let mut permuted = original.clone();
    let node = permuted["@graph"]
        .as_array_mut()
        .expect("@graph")
        .iter_mut()
        .find(|n| {
            n["@id"]
                .as_str()
                .is_some_and(|s| s.ends_with("enableLatch"))
        })
        .expect("ahu_economizer declares enableLatch");
    let ports = node["S231:hasInput"].as_array_mut().expect("two inputs");
    assert_eq!(ports.len(), 2, "Latch has exactly two inputs");
    ports.swap(0, 1);

    let render = |doc: &Value| {
        let g = import(doc).expect("resolves");
        let mut out = String::new();
        for b in &g.blocks {
            out.push_str(&format!("{} {:?} {:?}\n", b.class_iri, b.inputs, b.outputs));
        }
        for c in &g.connections {
            out.push_str(&format!("{:?}->{:?}\n", c.from, c.to));
        }
        out
    };
    assert_eq!(
        render(&original),
        render(&permuted),
        "a fixture rendered with its Latch inputs in the other order must resolve to the same model"
    );
}
