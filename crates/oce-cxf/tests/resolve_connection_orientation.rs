//! End-to-end connection orientation: which endpoint carries `isConnectedTo` is not information.
//!
//! CXF §8.2 gives `connectedTo` the domain *(OutputConnector, InputConnector)* and the range
//! *(InputConnector, OutputConnector)*, so either end may be the subject, and CDL states that the
//! order of the arguments in a `connect` statement does not matter. A producer that preserves
//! Modelica source order therefore emits input-subject edges for any class written
//! `connect(dst, src)` — `modelica-json` renders 11 of the 16 edges in
//! `Economizers.Subsequences.Modulations.Reliefs` that way.
//!
//! Every test here compares two *renderings of one model*, never two models. The resolver may not
//! see a difference between them. What it must still reject is a pair invalid in **both**
//! orientations: output→output, input→input, and an input driven more than once.
//!
//! Two shapes are deliberately NOT rejected, and are recorded here so they are not "fixed" later.
//! An input that is both an `external_inputs` entry and the target of one wire resolves — that is
//! the false-reject guard pinned by `export_single_assignment.rs`, because it is the shape
//! export/re-import produces. And symmetric closure — the same edge asserted from *both* ends, as
//! an OWL reasoner materialising a symmetric property would emit — is out of scope: it yields two
//! identical connections and is reported as multiply driven, since a duplicated target genuinely
//! is in-degree 2 (`resolve_errors::doubly_driven_input_is_single_assignment` pins that reading).

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::DiagCode;
use oce_model::ModelGraph;
use serde_json::{Value, json};

const M: &str = "http://example.org#M";

fn iri(suffix: &str) -> String {
    format!("{M}.{suffix}")
}

/// A canonical rendering of everything the resolver decided.
///
/// Connections are sorted because their *order* legitimately follows which node carried the edge —
/// that is document shape, not model content. Every other field is positional and must match
/// exactly, so a reordered connector or a re-bound port would still show up as a difference.
fn render(g: &ModelGraph) -> String {
    let mut out: Vec<String> = Vec::new();
    for b in &g.blocks {
        out.push(format!(
            "B{} {} in={:?} out={:?}",
            b.id.0, b.class_iri, b.inputs, b.outputs
        ));
    }
    for c in &g.connectors {
        out.push(format!(
            "C{} blk={} dir={:?} ty={:?} decl={}",
            c.id.0, c.block.0, c.dir, c.value_type, c.decl_order
        ));
    }
    let mut edges: Vec<String> = g
        .connections
        .iter()
        .map(|c| format!("E {} -> {}", c.from.0, c.to.0))
        .collect();
    edges.sort();
    out.extend(edges);
    let mut ext: Vec<String> = g
        .external_inputs
        .iter()
        .map(|c| format!("X {}", c.0))
        .collect();
    ext.sort();
    out.extend(ext);
    out.join("\n")
}

fn import(doc: &Value) -> Result<ModelGraph, CxfError> {
    let bytes = serde_json::to_vec(doc).expect("serialize");
    import_cxf(&bytes, &ResolveOptions::default()).map(|(g, _)| g)
}

fn codes(e: &CxfError) -> Vec<DiagCode> {
    match e {
        CxfError::Validation(diags) => diags.iter().map(|d| d.code).collect(),
        other => panic!("expected a validation error, got {other:?}"),
    }
}

/// The `(code, subject)` pairs of a rejection.
///
/// Asserting the code alone is too weak to pin orientation: a mutant that re-anchors an already
/// invalid pair the wrong way still reports the same code, and only the connector it *blames*
/// moves. Reviewing this suite by mutation found four such survivors, including
/// `(Some(Dir::In), _) => (target, source)`, which is why every rejection below names its subject.
fn faults(e: &CxfError) -> Vec<(DiagCode, String)> {
    match e {
        CxfError::Validation(diags) => diags
            .iter()
            .map(|d| {
                let s = d.subject.as_deref().unwrap_or("<none>").to_owned();
                (d.code, s)
            })
            .collect(),
        other => panic!("expected a validation error, got {other:?}"),
    }
}

/// One model — a constant and a composite input summed into a composite output — rendered with each
/// of its three edges authored from either end.
///
/// `reversed[i]` moves edge `i`'s `isConnectedTo` from the driving connector to the driven one. The
/// three edges deliberately cover the three shapes the resolver dispatches on separately: child to
/// child, composite boundary **input** to child, and child to composite boundary **output**.
fn document(reversed: [bool; 3]) -> Value {
    let pairs: [(String, String); 3] = [
        (iri("src.y"), iri("sum.u1")),
        (iri("uExt"), iri("sum.u2")),
        (iri("sum.y"), iri("yOut")),
    ];
    let mut subject_of: Vec<(String, String)> = Vec::new();
    for (i, (driver, driven)) in pairs.into_iter().enumerate() {
        if reversed[i] {
            subject_of.push((driven, driver));
        } else {
            subject_of.push((driver, driven));
        }
    }
    // `isConnectedTo` for a node, if it is the subject of any edge in this authoring.
    let conn = |id: &str| -> Option<Value> {
        let targets: Vec<Value> = subject_of
            .iter()
            .filter(|(s, _)| s == id)
            .map(|(_, t)| json!({ "@id": t }))
            .collect();
        (!targets.is_empty()).then_some(Value::Array(targets))
    };
    let port = |id: String, ty: &str| -> Value {
        let mut n = json!({ "@id": id, "@type": ty, "S231:isOfDataType": { "@id": "S231:Real" } });
        if let Some(c) = conn(n["@id"].as_str().expect("id")) {
            n["S231:isConnectedTo"] = c;
        }
        n
    };
    json!({
      "@context": { "S231": "http://data.ashrae.org/S231P#", "base": "http://example.org#" },
      "@graph": [
        { "@id": M, "@type": "S231:Block",
          "S231:containsBlock": [ { "@id": iri("src") }, { "@id": iri("sum") } ],
          "S231:hasInput":  { "@id": iri("uExt") },
          "S231:hasOutput": { "@id": iri("yOut") } },

        { "@id": iri("src"),
          "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
          "S231:hasParameter": { "@id": iri("src.k") },
          "S231:hasOutput": { "@id": iri("src.y") } },
        { "@id": iri("src.k"),
          "S231:value": { "@value": "3.5", "@type": "http://www.w3.org/2001/XMLSchema#double" } },
        port(iri("src.y"), "S231:RealOutput"),

        { "@id": iri("sum"),
          "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Add",
          "S231:hasInput": [ { "@id": iri("sum.u1") }, { "@id": iri("sum.u2") } ],
          "S231:hasOutput": { "@id": iri("sum.y") } },
        port(iri("sum.u1"), "S231:RealInput"),
        port(iri("sum.u2"), "S231:RealInput"),
        port(iri("sum.y"),  "S231:RealOutput"),

        port(iri("uExt"), "S231:RealInput"),
        port(iri("yOut"), "S231:RealOutput")
      ]
    })
}

#[test]
fn every_authoring_of_one_model_resolves_to_the_same_graph() {
    let canonical = import(&document([false, false, false])).expect("declaration order imports");
    let expected = render(&canonical);
    // Three independent edges → eight authorings, and the model is the same in all of them.
    for mask in 0u8..8 {
        let reversed = [mask & 1 != 0, mask & 2 != 0, mask & 4 != 0];
        let g = import(&document(reversed))
            .unwrap_or_else(|e| panic!("authoring {reversed:?} rejected: {e:?}"));
        assert_eq!(
            render(&g),
            expected,
            "authoring {reversed:?} produced a different graph"
        );
    }
    // Guard the guard: the renderings above would also be equal if the model were empty.
    assert_eq!(canonical.blocks.len(), 2);
    assert_eq!(
        canonical.connections.len(),
        1,
        "src.y → sum.u1 is the only non-elided edge"
    );
    assert_eq!(
        canonical.external_inputs.len(),
        1,
        "uExt is the composite's only boundary input"
    );
}

/// The real thing: take a G36 fixture and re-author **every** edge from the opposite end.
///
/// This is the case the change exists for. `Modulations.Reliefs` is the upstream class with the
/// highest input-first density (11 of 13 `connect` statements), so its fixture is where a producer
/// that preserves Modelica source order diverges most from ours.
#[test]
fn a_g36_fixture_with_every_edge_reversed_resolves_to_the_same_graph() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/g36/multizone_vav_economizer_modulations_reliefs.jsonld"
    );
    let bytes = std::fs::read(path).expect("read fixture");
    let (canonical, _) =
        import_cxf(&bytes, &ResolveOptions::default()).expect("the fixture imports as authored");

    let doc: Value = serde_json::from_slice(&bytes).expect("fixture is JSON");
    let graph = doc["@graph"].as_array().expect("@graph").clone();

    // Collect every edge, then rebuild the document with each one anchored on its other end.
    let mut edges: Vec<(String, String)> = Vec::new();
    for node in &graph {
        let Some(id) = node["@id"].as_str() else {
            continue;
        };
        match &node["S231:isConnectedTo"] {
            Value::Array(ts) => {
                for t in ts {
                    edges.push((id.to_owned(), t["@id"].as_str().expect("@id").to_owned()));
                }
            }
            Value::Object(o) => {
                edges.push((id.to_owned(), o["@id"].as_str().expect("@id").to_owned()));
            }
            _ => {}
        }
    }
    assert!(
        edges.len() >= 13,
        "fixture should carry the class's edges, found {}",
        edges.len()
    );

    let mut rebuilt: Vec<Value> = graph
        .iter()
        .cloned()
        .map(|mut n| {
            if let Some(o) = n.as_object_mut() {
                o.remove("S231:isConnectedTo");
            }
            n
        })
        .collect();
    for (from, to) in &edges {
        let pos = rebuilt
            .iter()
            .position(|n| n["@id"].as_str() == Some(to.as_str()))
            .unwrap_or_else(|| panic!("no node for {to}"));
        let entry = rebuilt[pos]
            .as_object_mut()
            .expect("node is an object")
            .entry("S231:isConnectedTo")
            .or_insert_with(|| Value::Array(Vec::new()));
        entry
            .as_array_mut()
            .expect("array")
            .push(json!({ "@id": from }));
    }
    let mut flipped = doc.clone();
    flipped["@graph"] = Value::Array(rebuilt);

    let g = import(&flipped).expect("the fully reversed authoring imports");
    assert_eq!(
        render(&g),
        render(&canonical),
        "reversing every edge changed the resolved graph"
    );
    assert_eq!(g.connections.len(), canonical.connections.len());
    assert!(
        !g.connections.is_empty(),
        "a fixture with no connections would make this vacuous"
    );
}

/// A document may name either endpoint first, but it may not connect two drivers.
#[test]
fn an_output_wired_to_an_output_is_rejected() {
    let mut doc = document([false, false, false]);
    let graph = doc["@graph"].as_array_mut().expect("@graph");
    for n in graph.iter_mut() {
        if n["@id"].as_str() == Some(iri("src.y").as_str()) {
            n["S231:isConnectedTo"] = json!([{ "@id": iri("sum.y") }]);
        }
    }
    let err = import(&doc).expect_err("output → output must not resolve");
    assert!(
        codes(&err).contains(&DiagCode::DirectionMismatch),
        "got {:?}",
        codes(&err)
    );
}

/// The mirror case, which the orientation swap must not turn into a valid edge by relabelling.
#[test]
fn an_input_wired_to_an_input_is_rejected() {
    let mut doc = document([false, false, false]);
    let graph = doc["@graph"].as_array_mut().expect("@graph");
    for n in graph.iter_mut() {
        if n["@id"].as_str() == Some(iri("sum.u1").as_str()) {
            n["S231:isConnectedTo"] = json!([{ "@id": iri("sum.u2") }]);
        }
    }
    let err = import(&doc).expect_err("input → input must not resolve");
    assert!(
        codes(&err).contains(&DiagCode::DirectionMismatch),
        "got {:?}",
        codes(&err)
    );
}

/// Two sibling outputs on one input, with the second edge authored from either end.
///
/// Deliberately uses two block outputs rather than a boundary input plus an output: a composite
/// boundary input is *elided* into `external_inputs` rather than becoming a `Connection`, so that
/// shape would not exercise the in-degree count this test is about.
fn two_driver_document(reversed: bool) -> Value {
    let second = if reversed {
        json!({ "@id": iri("sum.u1"), "@type": "S231:RealInput",
                "S231:isOfDataType": { "@id": "S231:Real" },
                "S231:isConnectedTo": [ { "@id": iri("other.y") } ] })
    } else {
        json!({ "@id": iri("sum.u1"), "@type": "S231:RealInput",
                "S231:isOfDataType": { "@id": "S231:Real" } })
    };
    let other_y = if reversed {
        json!({ "@id": iri("other.y"), "@type": "S231:RealOutput",
                "S231:isOfDataType": { "@id": "S231:Real" } })
    } else {
        json!({ "@id": iri("other.y"), "@type": "S231:RealOutput",
                "S231:isOfDataType": { "@id": "S231:Real" },
                "S231:isConnectedTo": [ { "@id": iri("sum.u1") } ] })
    };
    json!({
      "@context": { "S231": "http://data.ashrae.org/S231P#", "base": "http://example.org#" },
      "@graph": [
        { "@id": M, "@type": "S231:Block",
          "S231:containsBlock": [ { "@id": iri("src") }, { "@id": iri("other") }, { "@id": iri("sum") } ],
          "S231:hasOutput": { "@id": iri("yOut") } },
        { "@id": iri("src"),
          "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
          "S231:hasParameter": { "@id": iri("src.k") },
          "S231:hasOutput": { "@id": iri("src.y") } },
        { "@id": iri("src.k"),
          "S231:value": { "@value": "3.5", "@type": "http://www.w3.org/2001/XMLSchema#double" } },
        { "@id": iri("src.y"), "@type": "S231:RealOutput",
          "S231:isOfDataType": { "@id": "S231:Real" },
          "S231:isConnectedTo": [ { "@id": iri("sum.u1") } ] },
        { "@id": iri("other"),
          "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
          "S231:hasParameter": { "@id": iri("other.k") },
          "S231:hasOutput": { "@id": iri("other.y") } },
        { "@id": iri("other.k"),
          "S231:value": { "@value": "1.5", "@type": "http://www.w3.org/2001/XMLSchema#double" } },
        other_y,
        { "@id": iri("sum"),
          "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Add",
          "S231:hasInput": [ { "@id": iri("sum.u1") }, { "@id": iri("sum.u2") } ],
          "S231:hasOutput": { "@id": iri("sum.y") } },
        second,
        { "@id": iri("sum.u2"), "@type": "S231:RealInput",
          "S231:isOfDataType": { "@id": "S231:Real" } },
        { "@id": iri("sum.y"), "@type": "S231:RealOutput",
          "S231:isOfDataType": { "@id": "S231:Real" },
          "S231:isConnectedTo": [ { "@id": iri("yOut") } ] },
        { "@id": iri("yOut"), "@type": "S231:RealOutput",
          "S231:isOfDataType": { "@id": "S231:Real" } }
      ]
    })
}

/// Single assignment is a property of the model, so it must hold whichever end authored the edge.
#[test]
fn two_drivers_on_one_input_are_rejected_in_either_authoring() {
    for reversed in [false, true] {
        let err = import(&two_driver_document(reversed))
            .expect_err("an input driven by two outputs must not resolve");
        assert!(
            codes(&err).contains(&DiagCode::SingleAssignment),
            "reversed={reversed} got {:?}",
            codes(&err)
        );
    }
}

/// An endpoint naming nothing is still an unresolved reference — the swap must not silently drop it
/// by treating the unknown end as the driven one.
///
/// The subject matters as much as the code: the unresolvable end is `src.y`'s target, so that is
/// what must be blamed. A mutant that swaps here reports the same code against the *other* endpoint.
#[test]
fn an_endpoint_naming_no_connector_is_still_reported() {
    let mut doc = document([false, false, false]);
    let graph = doc["@graph"].as_array_mut().expect("@graph");
    for n in graph.iter_mut() {
        if n["@id"].as_str() == Some(iri("src.y").as_str()) {
            n["S231:isConnectedTo"] = json!([{ "@id": iri("nowhere.u") }]);
        }
    }
    let err = import(&doc).expect_err("a dangling endpoint must not resolve");
    assert!(
        faults(&err).contains(&(DiagCode::UnresolvedReference, iri("nowhere.u"))),
        "the dangling target must be the subject, got {:?}",
        faults(&err)
    );
}

/// Anchoring an edge on a **composite boundary output** — the reversed spelling of
/// `child output → composite output`.
///
/// This is the authoring the swap exists to support, and it is the one the first version of this
/// change got wrong. Re-anchoring parks the boundary output in the target slot, which routes the
/// edge into Step 9's boundary-output elision. That arm `continue`s past Step 10, so if it does not
/// validate the driving end itself, *every* edge subjected on a boundary output vanishes silently —
/// whatever the other end names.
#[test]
fn a_boundary_output_subject_resolves_to_the_same_graph_as_the_forward_spelling() {
    let forward = import(&document([false, false, false])).expect("forward spelling imports");
    let reversed =
        import(&document([false, false, true])).expect("boundary-output subject imports");
    assert_eq!(
        render(&reversed),
        render(&forward),
        "the two spellings of `sum.y → yOut` must denote one model"
    );
}

/// The same arm, fed pairs that are wrong — each must be reported, not elided into silence.
#[test]
fn a_boundary_output_subject_does_not_swallow_an_invalid_counterpart() {
    // A composite output "driven by" a child INPUT: two driven ends, invalid in both orientations.
    let mut doc = document([false, false, false]);
    for n in doc["@graph"].as_array_mut().expect("@graph").iter_mut() {
        if n["@id"].as_str() == Some(iri("yOut").as_str()) {
            n["S231:isConnectedTo"] = json!([{ "@id": iri("sum.u1") }]);
        }
    }
    let err = import(&doc).expect_err("boundary output ← child input must not resolve");
    assert!(
        faults(&err).contains(&(DiagCode::DirectionMismatch, iri("sum.u1"))),
        "got {:?}",
        faults(&err)
    );

    // A composite output wired to an `@id` that appears in no node at all.
    let mut doc = document([false, false, false]);
    for n in doc["@graph"].as_array_mut().expect("@graph").iter_mut() {
        if n["@id"].as_str() == Some(iri("yOut").as_str()) {
            n["S231:isConnectedTo"] = json!([{ "@id": iri("nowhere.u") }]);
        }
    }
    let err = import(&doc).expect_err("boundary output ← dangling id must not resolve");
    assert!(
        faults(&err).contains(&(DiagCode::UnresolvedReference, iri("nowhere.u"))),
        "got {:?}",
        faults(&err)
    );

    // And a node that exists but is not a connector at all — a parameter.
    let mut doc = document([false, false, false]);
    for n in doc["@graph"].as_array_mut().expect("@graph").iter_mut() {
        if n["@id"].as_str() == Some(iri("yOut").as_str()) {
            n["S231:isConnectedTo"] = json!([{ "@id": iri("src.k") }]);
        }
    }
    let err = import(&doc).expect_err("boundary output ← parameter node must not resolve");
    assert!(
        faults(&err).contains(&(DiagCode::UnresolvedReference, iri("src.k"))),
        "got {:?}",
        faults(&err)
    );
}

/// A rejected pair must blame the **driving** end, and that pins the swap itself.
///
/// `sum.u1 → sum.u2` is input→input: `orient_edge` must NOT swap it, so `sum.u1` stays the driver
/// and is what Step 10 blames. This is the mutation-killer for widening the swap arm to
/// `(Some(Dir::In), _) => (target, source)` — under that mutant the pair is swapped anyway, the
/// code stays `DirectionMismatch`, and only the named connector moves. Asserting the code alone
/// leaves that mutant alive; asserting the subject kills it.
#[test]
fn a_rejected_pair_blames_the_driving_end() {
    let mut doc = document([false, false, false]);
    for n in doc["@graph"].as_array_mut().expect("@graph").iter_mut() {
        if n["@id"].as_str() == Some(iri("sum.u1").as_str()) {
            n["S231:isConnectedTo"] = json!([{ "@id": iri("sum.u2") }]);
        }
    }
    let err = import(&doc).expect_err("input → input must not resolve");
    assert_eq!(
        faults(&err),
        vec![(DiagCode::DirectionMismatch, "connector#1".to_owned())],
        "sum.u1 is connector#1 and is the authored driver; blaming anything else means the pair \
         was swapped when it should not have been"
    );
}
