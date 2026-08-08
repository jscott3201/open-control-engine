//! Role-and-containment orientation contracts for nested-composite boundaries.

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::{DiagCode, Diagnostic};
use oce_model::ModelGraph;
use serde_json::{Value, json};

const FIXTURE: &str = include_str!("fixtures/nested_composite.jsonld");
const BASE: &str = "http://example.org#g36.profile.nested_composite";

fn document() -> Value {
    serde_json::from_str(FIXTURE).expect("nested fixture JSON")
}

fn iri(suffix: &str) -> String {
    format!("{BASE}{suffix}")
}

fn node_mut<'a>(document: &'a mut Value, suffix: &str) -> &'a mut Value {
    document["@graph"]
        .as_array_mut()
        .expect("@graph")
        .iter_mut()
        .find(|node| {
            node["@id"]
                .as_str()
                .is_some_and(|candidate| candidate.ends_with(suffix))
        })
        .unwrap_or_else(|| panic!("missing node ending in {suffix}"))
}

fn set_targets(document: &mut Value, suffix: &str, targets: &[&str]) {
    let values = targets
        .iter()
        .map(|target| json!({ "@id": iri(target) }))
        .collect::<Vec<_>>();
    node_mut(document, suffix)["S231:isConnectedTo"] = Value::Array(values);
}

fn clear_targets(document: &mut Value, suffix: &str) {
    node_mut(document, suffix)
        .as_object_mut()
        .expect("node")
        .remove("S231:isConnectedTo");
}

fn import(document: &Value) -> Result<ModelGraph, Vec<Diagnostic>> {
    let bytes = serde_json::to_vec(document).expect("serialize");
    match import_cxf(&bytes, &ResolveOptions::default()) {
        Ok((graph, report)) => {
            assert!(report.is_empty(), "{report:?}");
            Ok(graph)
        }
        Err(CxfError::Validation(diagnostics)) => Err(diagnostics),
        Err(error) => panic!("unexpected import error: {error:?}"),
    }
}

fn rendered_edges(graph: &ModelGraph) -> Vec<(String, String)> {
    let connector = |id: oce_model::ConnectorId| {
        let connector = &graph.connectors[id.0 as usize];
        let block = &graph.blocks[connector.block.0 as usize];
        (
            block.instance_iri.as_deref().unwrap_or("<root>").to_owned(),
            connector.decl_order,
        )
    };
    graph
        .connections
        .iter()
        .map(|edge| {
            let (source, source_port) = connector(edge.from);
            let (target, target_port) = connector(edge.to);
            (
                format!("{source}:{source_port}"),
                format!("{target}:{target_port}"),
            )
        })
        .collect::<Vec<_>>()
}

fn assert_forward_identical(candidate: &Value) {
    let expected = rendered_edges(&import(&document()).expect("forward fixture imports"));
    let actual = rendered_edges(&import(candidate).expect("reoriented fixture imports"));
    assert_eq!(actual, expected);
}

/// Base: replacing `sub.u→sub.gain.u` with `sub.gain.u→sub.u` dropped the edge and reported an
/// undriven input. Head: role+containment re-anchors it and produces the forward graph.
#[test]
fn reversed_nested_input_edge_is_forward_identical() {
    let mut reversed = document();
    clear_targets(&mut reversed, ".sub.u");
    set_targets(&mut reversed, ".sub.gain.u", &[".sub.u"]);
    assert_forward_identical(&reversed);
}

/// Base: spelling `sub.y→[sub.gain.y,post.u]` dropped the inward edge and reported an undriven
/// input. Head: the inward arm swaps while the outward arm stays authored, yielding the forward
/// graph.
#[test]
fn reversed_nested_output_interior_edge_is_forward_identical() {
    let mut reversed = document();
    clear_targets(&mut reversed, ".sub.gain.y");
    set_targets(&mut reversed, ".sub.y", &[".sub.gain.y", ".post.u"]);
    assert_forward_identical(&reversed);
}

/// Base: `post.u→sub.y` was discarded at the boundary and left `post.u` undriven. Head: the edge
/// re-anchors to the nested output and produces the forward graph.
#[test]
fn reversed_nested_output_exterior_edge_is_forward_identical() {
    let mut reversed = document();
    clear_targets(&mut reversed, ".sub.y");
    set_targets(&mut reversed, ".post.u", &[".sub.y"]);
    assert_forward_identical(&reversed);
}

/// Base: with `post` removed, `TOP.y→sub.y` imported clean while silently leaving the output
/// undriven — and this shape also lowers to the same flat graph at base, because a leaf output
/// driving only a top output elides to no connection. The pin is therefore spelling-equivalence:
/// the reverse spelling lowers identically to the forward one.
#[test]
fn reversed_top_output_edge_lowers_identically_to_the_forward_spelling() {
    let mut forward = document();
    let root = node_mut(&mut forward, "nested_composite");
    root["S231:containsBlock"] = json!({ "@id": iri(".sub") });
    forward["@graph"]
        .as_array_mut()
        .expect("@graph")
        .retain(|node| !node["@id"].as_str().is_some_and(|id| id.contains(".post")));
    set_targets(&mut forward, ".sub.y", &[".y"]);
    let expected = rendered_edges(&import(&forward).expect("forward output model imports"));

    let mut reversed = forward.clone();
    clear_targets(&mut reversed, ".sub.y");
    set_targets(&mut reversed, ".y", &[".sub.y"]);
    assert_eq!(
        rendered_edges(&import(&reversed).expect("reversed output model imports")),
        expected
    );
}

/// Base: reversing the nested input, both output arms, and the outer arm together produced a
/// garbage `post.u→sub.gain.y` edge and dropped valid edges. Head: each authored pair is
/// canonicalized independently and the complete graph is forward-identical.
#[test]
fn mixed_reversed_boundary_edges_are_forward_identical() {
    let mut reversed = document();
    clear_targets(&mut reversed, ".sub.u");
    clear_targets(&mut reversed, ".sub.gain.y");
    clear_targets(&mut reversed, ".sub.y");
    set_targets(&mut reversed, ".sub.gain.u", &[".sub.u"]);
    set_targets(&mut reversed, ".sub.y", &[".sub.gain.y"]);
    set_targets(&mut reversed, ".post.u", &[".sub.y"]);
    assert_forward_identical(&reversed);
}

/// Base: the same-half spelling `sub.u→[TOP.u,sub.gain.u]` dropped the external arm. Head: the
/// root input arm swaps and the interior arm stays authored, producing the forward graph.
#[test]
fn reversed_top_input_edge_is_forward_identical() {
    let mut reversed = document();
    clear_targets(&mut reversed, ".u");
    set_targets(&mut reversed, ".sub.u", &[".u", ".sub.gain.u"]);
    assert_forward_identical(&reversed);
}

/// Base: symmetric closure of `sub.u↔sub.gain.u` lowered a leaf self-loop and rejected with
/// `DirectionMismatch`. Head: the forward/reverse pair collapses and the root input still drives
/// the gain input.
#[test]
fn both_spellings_of_nested_input_collapse_to_one_relation() {
    let mut both = document();
    set_targets(&mut both, ".sub.gain.u", &[".sub.u"]);
    assert_forward_identical(&both);
}

/// Four-step gap probe: (i) add a `pre` Constant leaf, including `pre.k` and `pre.y`, to the root
/// `containsBlock`; (ii) remove the root input's `isConnectedTo`; (iii) leave
/// `sub.enumCarrier.y` with no outgoing edge; (iv) author
/// `sub.u isConnectedTo [sub.gain.u, pre.y]`. Base rejected with `SingleAssignment` on the
/// undriven `sub.gain.u` because the reverse arm vanished. Head imports clean and contains the
/// re-anchored `pre.y→sub.gain.u` connection.
#[test]
fn reverse_spelled_external_leaf_reanchors_through_nested_input() {
    let mut probe = document();
    node_mut(&mut probe, "nested_composite")["S231:containsBlock"]
        .as_array_mut()
        .expect("root children")
        .push(json!({ "@id": iri(".pre") }));
    clear_targets(&mut probe, ".u");
    set_targets(&mut probe, ".sub.u", &[".sub.gain.u", ".pre.y"]);
    probe["@graph"].as_array_mut().expect("@graph").extend([
        json!({
            "@id": iri(".pre"),
            "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
            "S231:hasParameter": { "@id": iri(".pre.k") },
            "S231:hasOutput": { "@id": iri(".pre.y") }
        }),
        json!({
            "@id": iri(".pre.k"),
            "@type": "S231:Parameter",
            "S231:isOfDataType": { "@id": "S231:Real" },
            "S231:value": 3
        }),
        json!({
            "@id": iri(".pre.y"),
            "@type": "S231:RealOutput",
            "S231:isOfDataType": { "@id": "S231:Real" }
        }),
    ]);
    let graph = import(&probe).expect("re-anchored probe imports");
    assert!(
        rendered_edges(&graph)
            .iter()
            .any(|(source, target)| { source.contains(".pre:") && target.contains(".sub.gain:") }),
        "{graph:?}"
    );
}

fn boundary_ring(entry: bool) -> Value {
    let model = "http://example.org#boundary_ring";
    let mut graph = vec![
        json!({
            "@id": model,
            "@type": "S231:Block",
            "S231:containsBlock": [
                { "@id": format!("{model}.sub") },
                { "@id": format!("{model}.sub2") }
            ],
            "S231:hasInput": { "@id": format!("{model}.u") },
            "S231:hasOutput": { "@id": format!("{model}.y") }
        }),
        json!({
            "@id": format!("{model}.u"),
            "@type": "S231:RealInput",
            "S231:isOfDataType": { "@id": "S231:Real" }
        }),
        json!({
            "@id": format!("{model}.y"),
            "@type": "S231:RealOutput",
            "S231:isOfDataType": { "@id": "S231:Real" }
        }),
    ];
    if entry {
        graph[1]["S231:isConnectedTo"] = json!({ "@id": format!("{model}.sub.u") });
    }
    for name in ["sub", "sub2"] {
        let composite = format!("{model}.{name}");
        graph.extend([
            json!({
                "@id": composite,
                "@type": "S231:Block",
                "S231:containsBlock": { "@id": format!("{composite}.keep") },
                "S231:hasInput": { "@id": format!("{composite}.u") },
                "S231:hasOutput": { "@id": format!("{composite}.y") }
            }),
            json!({
                "@id": format!("{composite}.u"),
                "@type": "S231:RealInput",
                "S231:isOfDataType": { "@id": "S231:Real" }
            }),
            json!({
                "@id": format!("{composite}.y"),
                "@type": "S231:RealOutput",
                "S231:isOfDataType": { "@id": "S231:Real" }
            }),
            json!({
                "@id": format!("{composite}.keep"),
                "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
                "S231:hasParameter": { "@id": format!("{composite}.keep.k") },
                "S231:hasOutput": { "@id": format!("{composite}.keep.y") }
            }),
            json!({
                "@id": format!("{composite}.keep.k"),
                "@type": "S231:Parameter",
                "S231:value": 1
            }),
            json!({
                "@id": format!("{composite}.keep.y"),
                "@type": "S231:RealOutput",
                "S231:isOfDataType": { "@id": "S231:Real" }
            }),
        ]);
    }
    let mut document = json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" },
        "@graph": graph
    });
    let ring_node = |document: &mut Value, suffix: &str, target: &str| {
        document["@graph"]
            .as_array_mut()
            .expect("@graph")
            .iter_mut()
            .find(|node| node["@id"].as_str().is_some_and(|id| id.ends_with(suffix)))
            .expect("ring node")["S231:isConnectedTo"] = json!({ "@id": target });
    };
    ring_node(&mut document, ".sub.u", &format!("{model}.sub.y"));
    ring_node(&mut document, ".sub.y", &format!("{model}.sub2.u"));
    ring_node(&mut document, ".sub2.u", &format!("{model}.sub2.y"));
    ring_node(&mut document, ".sub2.y", &format!("{model}.sub.u"));
    document
}

/// Five authored edges form a two-composite boundary ring. The root entry edge is load-bearing:
/// with it the walk revisits `sub.u`, which must remain a loud unresolved-reference rejection.
#[test]
fn entered_boundary_ring_names_the_revisited_input() {
    let diagnostics = import(&boundary_ring(true)).expect_err("entered ring must reject");
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.code == DiagCode::UnresolvedReference
                && diagnostic
                    .subject
                    .as_deref()
                    .is_some_and(|subject| subject.ends_with(".sub.u"))
        }),
        "{diagnostics:?}"
    );
}

/// Removing only the root entry edge leaves the same four-edge ring driverless. The lowering walk
/// must not invent an entry, so this control imports cleanly.
#[test]
fn entryless_boundary_ring_is_not_walked() {
    // Without the entry edge the root's declared output `#boundary_ring.y` has no internal
    // driver, so the import succeeds carrying exactly the R18-5 advisory — the ring itself is
    // still not walked, which is this test's subject.
    let bytes = serde_json::to_vec(&boundary_ring(false)).expect("serialize");
    let (_, report) =
        import_cxf(&bytes, &ResolveOptions::default()).expect("entryless ring imports");
    assert_eq!(
        report.diagnostics,
        vec![
            Diagnostic::warning(
                DiagCode::UndrivenBoundaryOutput,
                "declared boundary output has no internal driver",
            )
            .with_subject("http://example.org#boundary_ring.y".to_owned())
        ],
        "the undriven declared output is the only diagnostic"
    );
}

fn gain_composite(model: &str, name: &str) -> Vec<Value> {
    let composite = format!("{model}.{name}");
    vec![
        json!({
            "@id": composite,
            "@type": "S231:Block",
            "S231:containsBlock": { "@id": format!("{composite}.gain") },
            "S231:hasInput": { "@id": format!("{composite}.u") },
            "S231:hasOutput": { "@id": format!("{composite}.y") }
        }),
        json!({
            "@id": format!("{composite}.u"),
            "@type": "S231:RealInput",
            "S231:isOfDataType": { "@id": "S231:Real" },
            "S231:isConnectedTo": { "@id": format!("{composite}.gain.u") }
        }),
        json!({
            "@id": format!("{composite}.y"),
            "@type": "S231:RealOutput",
            "S231:isOfDataType": { "@id": "S231:Real" }
        }),
        json!({
            "@id": format!("{composite}.gain"),
            "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.MultiplyByParameter",
            "S231:hasParameter": { "@id": format!("{composite}.gain.k") },
            "S231:hasInput": { "@id": format!("{composite}.gain.u") },
            "S231:hasOutput": { "@id": format!("{composite}.gain.y") }
        }),
        json!({
            "@id": format!("{composite}.gain.k"),
            "@type": "S231:Parameter",
            "S231:value": 1
        }),
        json!({
            "@id": format!("{composite}.gain.u"),
            "@type": "S231:RealInput",
            "S231:isOfDataType": { "@id": "S231:Real" }
        }),
        json!({
            "@id": format!("{composite}.gain.y"),
            "@type": "S231:RealOutput",
            "S231:isOfDataType": { "@id": "S231:Real" },
            "S231:isConnectedTo": { "@id": format!("{composite}.y") }
        }),
    ]
}

fn constant_composite(model: &str, name: &str, connect_output: bool) -> Vec<Value> {
    let composite = format!("{model}.{name}");
    let mut leaf_output = json!({
        "@id": format!("{composite}.src.y"),
        "@type": "S231:RealOutput",
        "S231:isOfDataType": { "@id": "S231:Real" }
    });
    if connect_output {
        leaf_output["S231:isConnectedTo"] = json!({ "@id": format!("{composite}.y") });
    }
    vec![
        json!({
            "@id": composite,
            "@type": "S231:Block",
            "S231:containsBlock": { "@id": format!("{composite}.src") },
            "S231:hasInput": { "@id": format!("{composite}.u") },
            "S231:hasOutput": { "@id": format!("{composite}.y") }
        }),
        json!({
            "@id": format!("{composite}.u"),
            "@type": "S231:RealInput",
            "S231:isOfDataType": { "@id": "S231:Real" }
        }),
        json!({
            "@id": format!("{composite}.y"),
            "@type": "S231:RealOutput",
            "S231:isOfDataType": { "@id": "S231:Real" }
        }),
        json!({
            "@id": format!("{composite}.src"),
            "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
            "S231:hasParameter": { "@id": format!("{composite}.src.k") },
            "S231:hasOutput": { "@id": format!("{composite}.src.y") }
        }),
        json!({
            "@id": format!("{composite}.src.k"),
            "@type": "S231:Parameter",
            "S231:value": 1
        }),
        leaf_output,
    ]
}

fn sibling_document() -> Value {
    let model = "http://example.org#siblings";
    let mut graph = vec![
        json!({
            "@id": model,
            "@type": "S231:Block",
            "S231:containsBlock": [
                { "@id": format!("{model}.subA") },
                { "@id": format!("{model}.subB") }
            ],
            "S231:hasInput": { "@id": format!("{model}.u") },
            "S231:hasOutput": { "@id": format!("{model}.y") }
        }),
        json!({
            "@id": format!("{model}.u"),
            "@type": "S231:RealInput",
            "S231:isOfDataType": { "@id": "S231:Real" },
            "S231:isConnectedTo": { "@id": format!("{model}.subA.u") }
        }),
        json!({
            "@id": format!("{model}.y"),
            "@type": "S231:RealOutput",
            "S231:isOfDataType": { "@id": "S231:Real" }
        }),
    ];
    graph.extend(gain_composite(model, "subA"));
    graph.extend(gain_composite(model, "subB"));
    json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" },
        "@graph": graph
    })
}

/// A sibling boundary input-to-input pair has contradictory polarity. It remains authored and
/// rejects loudly; the current generic `SingleAssignment` cause is intentionally accepted.
#[test]
fn contradictory_sibling_inputs_still_reject() {
    let mut document = sibling_document();
    let model = "http://example.org#siblings";
    set_absolute_targets(
        &mut document,
        ".subB.u",
        &[&format!("{model}.subA.u"), &format!("{model}.subB.gain.u")],
    );
    let diagnostics = import(&document).expect_err("input-to-input siblings reject");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagCode::SingleAssignment),
        "{diagnostics:?}"
    );
}

/// A sibling boundary output-to-output pair has contradictory polarity. It remains authored and
/// rejects loudly; the current generic `SingleAssignment` cause is intentionally accepted.
#[test]
fn contradictory_sibling_outputs_still_reject() {
    let mut document = sibling_document();
    let model = "http://example.org#siblings";
    set_absolute_targets(&mut document, ".subA.y", &[&format!("{model}.subB.y")]);
    set_absolute_targets(&mut document, ".subB.y", &[&format!("{model}.y")]);
    let diagnostics = import(&document).expect_err("output-to-output siblings reject");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagCode::SingleAssignment),
        "{diagnostics:?}"
    );
}

fn set_absolute_targets(document: &mut Value, suffix: &str, targets: &[&str]) {
    node_mut(document, suffix)["S231:isConnectedTo"] = Value::Array(
        targets
            .iter()
            .map(|target| json!({ "@id": target }))
            .collect(),
    );
}

/// Base fallback fabrication document: `subA.src.y→subA.y→subB.u`; `subB.u` is not wired inside
/// subB; undriven `subC.y` authors `[subB.u,subD.u]`; and subD passes through to the top output.
/// Base imported clean by inventing `subA.src.y→subD.gain.u`. Head deletes that route and rejects
/// because subD's leaf input is undriven.
#[test]
fn unwired_sibling_boundary_cannot_fabricate_a_leaf_connection() {
    let model = "http://example.org#fabrication";
    let mut graph = vec![
        json!({
            "@id": model,
            "@type": "S231:Block",
            "S231:containsBlock": [
                { "@id": format!("{model}.subA") },
                { "@id": format!("{model}.subB") },
                { "@id": format!("{model}.subC") },
                { "@id": format!("{model}.subD") }
            ],
            "S231:hasOutput": { "@id": format!("{model}.y") }
        }),
        json!({
            "@id": format!("{model}.y"),
            "@type": "S231:RealOutput",
            "S231:isOfDataType": { "@id": "S231:Real" }
        }),
        json!({
            "@id": format!("{model}.subA"),
            "@type": "S231:Block",
            "S231:containsBlock": { "@id": format!("{model}.subA.src") },
            "S231:hasOutput": { "@id": format!("{model}.subA.y") }
        }),
        json!({
            "@id": format!("{model}.subA.y"),
            "@type": "S231:RealOutput",
            "S231:isOfDataType": { "@id": "S231:Real" },
            "S231:isConnectedTo": { "@id": format!("{model}.subB.u") }
        }),
        json!({
            "@id": format!("{model}.subA.src"),
            "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
            "S231:hasParameter": { "@id": format!("{model}.subA.src.k") },
            "S231:hasOutput": { "@id": format!("{model}.subA.src.y") }
        }),
        json!({
            "@id": format!("{model}.subA.src.k"),
            "@type": "S231:Parameter",
            "S231:value": 1
        }),
        json!({
            "@id": format!("{model}.subA.src.y"),
            "@type": "S231:RealOutput",
            "S231:isOfDataType": { "@id": "S231:Real" },
            "S231:isConnectedTo": { "@id": format!("{model}.subA.y") }
        }),
    ];
    graph.extend(constant_composite(model, "subB", false));
    graph.extend(constant_composite(model, "subC", false));
    graph.extend(gain_composite(model, "subD"));
    let mut document = json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" },
        "@graph": graph
    });
    set_absolute_targets(
        &mut document,
        ".subC.y",
        &[&format!("{model}.subB.u"), &format!("{model}.subD.u")],
    );
    set_absolute_targets(&mut document, ".subD.y", &[&format!("{model}.y")]);
    let diagnostics = import(&document).expect_err("fabrication route must reject");
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.code == DiagCode::SingleAssignment
                && diagnostic.subject.as_deref()
                    == Some("http://example.org#fabrication.subD.gain.u")
        }),
        "undriven subject must be subD's authored leaf input: {diagnostics:?}"
    );
}

/// A duplicated top-level leaf-to-leaf edge is a genuine double-drive and must keep rejecting
/// even though the document also contains a nested composite: acceptance must never depend on
/// unrelated document content.
#[test]
fn duplicated_top_level_edge_still_rejects_when_a_nested_composite_is_present() {
    let model = "http://example.org#duplicate";
    let document = json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" },
        "@graph": [
            { "@id": model, "@type": "S231:Block",
              "S231:containsBlock": [
                { "@id": format!("{model}.c1") },
                { "@id": format!("{model}.c2") },
                { "@id": format!("{model}.subZ") }
              ] },
            { "@id": format!("{model}.c1"),
              "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
              "S231:hasParameter": { "@id": format!("{model}.c1.k") },
              "S231:hasOutput": { "@id": format!("{model}.c1.y") } },
            { "@id": format!("{model}.c1.k"), "S231:value": 1 },
            { "@id": format!("{model}.c1.y"), "@type": "S231:RealOutput",
              "S231:isOfDataType": { "@id": "S231:Real" },
              "S231:isConnectedTo": [
                { "@id": format!("{model}.c2.u") },
                { "@id": format!("{model}.c2.u") }
              ] },
            { "@id": format!("{model}.c2"),
              "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.MultiplyByParameter",
              "S231:hasParameter": { "@id": format!("{model}.c2.k") },
              "S231:hasInput": { "@id": format!("{model}.c2.u") },
              "S231:hasOutput": { "@id": format!("{model}.c2.y") } },
            { "@id": format!("{model}.c2.k"), "S231:value": 2 },
            { "@id": format!("{model}.c2.u"), "@type": "S231:RealInput",
              "S231:isOfDataType": { "@id": "S231:Real" } },
            { "@id": format!("{model}.c2.y"), "@type": "S231:RealOutput",
              "S231:isOfDataType": { "@id": "S231:Real" } },
            { "@id": format!("{model}.subZ"), "@type": "S231:Block",
              "S231:containsBlock": { "@id": format!("{model}.subZ.src") },
              "S231:hasOutput": { "@id": format!("{model}.subZ.y") } },
            { "@id": format!("{model}.subZ.src"),
              "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
              "S231:hasParameter": { "@id": format!("{model}.subZ.src.k") },
              "S231:hasOutput": { "@id": format!("{model}.subZ.src.y") } },
            { "@id": format!("{model}.subZ.src.k"), "S231:value": 3 },
            { "@id": format!("{model}.subZ.src.y"), "@type": "S231:RealOutput",
              "S231:isOfDataType": { "@id": "S231:Real" },
              "S231:isConnectedTo": { "@id": format!("{model}.subZ.y") } },
            { "@id": format!("{model}.subZ.y"), "@type": "S231:RealOutput",
              "S231:isOfDataType": { "@id": "S231:Real" } }
        ]
    });
    let diagnostics = import(&document).expect_err("duplicated edge must stay multiply driven");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagCode::SingleAssignment),
        "{diagnostics:?}"
    );
}

/// A duplicated boundary-crossing continuation (`sub.y → [post.u, post.u]`) double-drives the
/// exterior leaf input and rejects at base with in-degree 2; the canonical-map spelling
/// suppression collapses forward+reverse restatements of ONE relation, never two authored copies
/// of the same edge, so the rejection must survive canonicalization.
#[test]
fn duplicated_boundary_crossing_edge_rejects_as_multiply_driven() {
    let mut duplicated = document();
    set_targets(&mut duplicated, ".sub.y", &[".post.u", ".post.u"]);
    let diagnostics = import(&duplicated).expect_err("duplicated edge must stay multiply driven");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagCode::SingleAssignment),
        "{diagnostics:?}"
    );
}

/// A reverse-spelled edge whose canonical driver has no `@graph` node is swap-blocked: it stays
/// authored so the ordinary dangling-reference diagnostic survives and names the missing port.
#[test]
fn swap_blocked_edge_without_a_canonical_source_node_keeps_the_dangling_diagnostic() {
    let mut blocked = document();
    clear_targets(&mut blocked, "nested_composite.u");
    set_targets(&mut blocked, ".sub.gain.u", &[".sub.u"]);
    let missing = iri(".sub.u");
    blocked["@graph"]
        .as_array_mut()
        .expect("@graph")
        .retain(|node| node["@id"].as_str() != Some(missing.as_str()));
    let diagnostics = import(&blocked).expect_err("swap-blocked edge must stay loud");
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.code == DiagCode::UnresolvedReference
                && diagnostic.subject.as_deref() == Some(missing.as_str())
        }),
        "dangling reference must name the missing canonical driver: {diagnostics:?}"
    );
}

/// A port claimed by two owners with opposite directions has no derivable role; the edge stays
/// authored (no silent reorientation) and the document keeps its loud undriven rejection.
#[test]
fn conflicting_ownership_claims_leave_the_edge_authored_and_loud() {
    let mut conflicted = document();
    clear_targets(&mut conflicted, ".sub.u");
    set_targets(&mut conflicted, ".sub.gain.u", &[".sub.u"]);
    node_mut(&mut conflicted, ".post")["S231:hasOutput"] = json!([
        { "@id": iri(".post.y") },
        { "@id": iri(".sub.gain.u") }
    ]);
    let diagnostics = import(&conflicted).expect_err("conflicted ownership must stay loud");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagCode::SingleAssignment),
        "{diagnostics:?}"
    );
}
