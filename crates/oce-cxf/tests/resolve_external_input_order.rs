//! `external_inputs` order does not follow a boundary port's `isConnectedTo` array spelling.
//!
//! The resolver fills `external_inputs` while walking `@graph`, eliding each composite boundary
//! input onto the child input it drives. Left as insertion order, the vector inherited the order of
//! the targets inside that boundary port's `isConnectedTo` array — so two documents listing one
//! port's fan-out targets in different orders described the same model and resolved to transposed
//! vectors.
//!
//! That order is **observable**, which is what makes it a defect rather than untidiness. `export`
//! emits the composite's boundary nodes in `external_inputs` first-occurrence order *and* fills
//! each boundary node's own `isConnectedTo` array from the same vector. A transposition of two
//! same-kind ports is the defect class `oce-blocks`' `port_names` and the resolver's name binding
//! exist to prevent, and no arity or kind check can see it: both entries are `Real` inputs and the
//! count is unchanged.
//!
//! Step 9 re-keys the vector on `(position of the boundary port's own node, ConnectorId)`. **Scope
//! of the claim, stated precisely because the commit title cannot hold it all:** this removes the
//! dependence on ARRAY spelling. It does not make the order independent of `@graph` node order —
//! permuting nodes is also a semantically neutral respelling in JSON-LD, and it still moves the
//! result. That axis is load-bearing throughout the resolver and is already recorded as such by
//! `resolve_golden::resolve_is_byte_identical_across_imports`; this change narrows the dependence,
//! it does not eliminate it.

use oce_cxf::{ResolveOptions, import_cxf};
use oce_model::ModelGraph;
use serde_json::{Value, json};

const M: &str = "http://example.org#M";

fn iri(suffix: &str) -> String {
    format!("{M}.{suffix}")
}

/// One model: a boundary input `uExt` fanning out to BOTH inputs of an `Add`, whose output leaves
/// through the boundary output `yOut`.
///
/// Fan-out is the shape that makes this visible — a boundary input feeding a single child has
/// nothing to reorder. `targets_reversed` flips only the order of the two entries inside `uExt`'s
/// `isConnectedTo` array. The model is identical either way: the same port drives the same two
/// inputs.
fn document(targets_reversed: bool) -> Value {
    let mut targets = vec![
        json!({ "@id": iri("sum.u1") }),
        json!({ "@id": iri("sum.u2") }),
    ];
    if targets_reversed {
        targets.reverse();
    }
    json!({
      "@context": { "S231": "http://data.ashrae.org/S231P#", "base": "http://example.org#" },
      "@graph": [
        { "@id": M, "@type": "S231:Block",
          "S231:containsBlock": [ { "@id": iri("sum") } ],
          "S231:hasInput":  { "@id": iri("uExt") },
          "S231:hasOutput": { "@id": iri("yOut") } },

        { "@id": iri("sum"),
          "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Add",
          "S231:hasInput": [ { "@id": iri("sum.u1") }, { "@id": iri("sum.u2") } ],
          "S231:hasOutput": { "@id": iri("sum.y") } },
        { "@id": iri("sum.u1"), "@type": "S231:RealInput",
          "S231:isOfDataType": { "@id": "S231:Real" } },
        { "@id": iri("sum.u2"), "@type": "S231:RealInput",
          "S231:isOfDataType": { "@id": "S231:Real" } },
        { "@id": iri("sum.y"), "@type": "S231:RealOutput",
          "S231:isOfDataType": { "@id": "S231:Real" },
          "S231:isConnectedTo": { "@id": iri("yOut") } },

        { "@id": iri("uExt"), "@type": "S231:RealInput",
          "S231:isOfDataType": { "@id": "S231:Real" },
          "S231:isConnectedTo": targets },
        { "@id": iri("yOut"), "@type": "S231:RealOutput",
          "S231:isOfDataType": { "@id": "S231:Real" } }
      ]
    })
}

fn import(doc: &Value) -> ModelGraph {
    let bytes = serde_json::to_vec(doc).expect("serialize");
    import_cxf(&bytes, &ResolveOptions::default())
        .expect("the document resolves")
        .0
}

#[test]
fn reordering_a_boundary_fan_out_array_does_not_reorder_external_inputs() {
    let a = import(&document(false));
    let b = import(&document(true));

    let order = |g: &ModelGraph| g.external_inputs.iter().map(|c| c.0).collect::<Vec<_>>();
    assert_eq!(
        order(&a),
        order(&b),
        "the same model spelled with the fan-out array reversed must yield the same \
         external_inputs order"
    );

    // Guard the guard: an empty or single-entry vector would satisfy the equality vacuously, and
    // this property is only meaningful when there is something to transpose.
    assert_eq!(a.external_inputs.len(), 2, "both child inputs are external");
    assert_eq!(
        order(&a),
        vec![0, 1],
        "canonical order is by ConnectorId within one boundary port, which Step 5a assigns from \
         @graph order over instance port nodes"
    );
}

fn two_boundary_outputs(reversed: bool) -> Value {
    let mut doc = document(false);
    let graph = doc["@graph"].as_array_mut().expect("graph array");
    graph[0]["S231:hasOutput"] = json!([
        { "@id": iri("yOut") },
        { "@id": iri("yOut2") }
    ]);
    graph[4]["S231:isConnectedTo"] = json!([
        { "@id": iri("yOut") },
        { "@id": iri("yOut2") }
    ]);
    graph.push(json!({
        "@id": iri("yOut2"),
        "@type": "S231:RealOutput",
        "S231:isOfDataType": { "@id": "S231:Real" }
    }));
    if reversed {
        graph[0]["S231:hasOutput"].as_array_mut().unwrap().reverse();
        graph[4]["S231:isConnectedTo"]
            .as_array_mut()
            .unwrap()
            .reverse();
    }
    doc
}

#[test]
fn array_spelling_does_not_reorder_outputs_sharing_one_driver() {
    let a = import(&two_boundary_outputs(false));
    let b = import(&two_boundary_outputs(true));
    let identity = |g: &ModelGraph| {
        g.boundary_outputs
            .iter()
            .map(|output| (output.iri.to_string(), output.source))
            .collect::<Vec<_>>()
    };
    assert_eq!(identity(&a), identity(&b));
    assert_eq!(a.boundary_outputs.len(), 2);
    assert_eq!(a.boundary_outputs[0].source, a.boundary_outputs[1].source);
}

/// The order reaches the **wire**, so pin it there too.
///
/// `export` fills each boundary node's own `isConnectedTo` array from `external_inputs`, not only
/// the composite's `hasInput` list. Reviewing this change found that nothing in the suite could see
/// that: the checked-in export byte goldens cover none of the affected fixtures, and
/// `export_g36_roundtrip` holds no bytes at all — it is a pure fixpoint check, which by
/// construction passes when a change alters the emitted bytes identically on both sides of the
/// trip. Asserting the emitted array directly is the only thing here that would catch a
/// re-ordering that round-trips.
#[test]
fn the_exported_boundary_target_array_follows_the_canonical_order() {
    let emitted = |targets_reversed: bool| -> Vec<String> {
        let g = import(&document(targets_reversed));
        let bytes = oce_cxf::export(&g).expect("the flat ground model exports");
        let doc: Value = serde_json::from_slice(&bytes).expect("emitted bytes are JSON");
        doc["@graph"]
            .as_array()
            .expect("@graph")
            .iter()
            .filter_map(|n| n["S231:isConnectedTo"].as_array().map(|a| (n, a)))
            .filter(|(n, _)| n["@id"].as_str().is_some_and(|s| s.ends_with("uExt")))
            .flat_map(|(_, a)| {
                a.iter()
                    .map(|t| t["@id"].as_str().expect("@id").to_owned())
                    .collect::<Vec<_>>()
            })
            .collect()
    };
    let forward = emitted(false);
    assert_eq!(
        forward,
        emitted(true),
        "the emitted boundary fan-out array must not follow the input document's array order"
    );
    assert_eq!(forward.len(), 2, "both targets must reach the wire");
    // Export MINTS port IRIs rather than round-tripping the authored names (`export.rs`: "re-import
    // rebuilds wiring from `isConnectedTo`, so port names never need to round-trip"), and mints them
    // in connector order — so the canonical order shows up as `in0` before `in1`.
    assert!(
        forward[0].ends_with(".in0") && forward[1].ends_with(".in1"),
        "emitted in canonical connector order, got {forward:?}"
    );
}

/// The corpus's only fixture named for this shape had no checked-in expectation of any kind.
#[test]
fn the_boundary_fanout_fixture_has_a_pinned_external_input_order() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/boundary_fanout.jsonld"
    ))
    .expect("read fixture");
    let (g, _) = import_cxf(&bytes, &ResolveOptions::default()).expect("fixture resolves");
    assert_eq!(
        g.external_inputs.iter().map(|c| c.0).collect::<Vec<_>>(),
        vec![0, 2],
        "boundary_fanout's resolved external_inputs order was pinned by nothing before this test"
    );
}

#[test]
fn both_spellings_agree_on_the_whole_graph_not_just_the_order() {
    let a = import(&document(false));
    let b = import(&document(true));
    let render = |g: &ModelGraph| {
        let mut s = format!(
            "blocks={} connectors={}\n",
            g.blocks.len(),
            g.connectors.len()
        );
        for c in &g.connectors {
            s += &format!("C{} dir={:?} iri={:?}\n", c.id.0, c.dir, c.iri.as_deref());
        }
        for c in &g.connections {
            s += &format!("E {} -> {}\n", c.from.0, c.to.0);
        }
        for c in &g.external_inputs {
            s += &format!("X {}\n", c.0);
        }
        s
    };
    assert_eq!(render(&a), render(&b));
    assert!(
        a.connectors.iter().any(|c| c.iri.is_some()),
        "the boundary IRI must survive onto a child connector, or the comparison above is weak"
    );
}
