//! End-to-end totality pins for recursively shaped CXF input.

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::{DiagCode, Diagnostic};
use oce_model::ModelGraph;
use serde_json::{Value, json};

const ROOT: &str = "http://example.org#top";
const COMPOSITE_DEPTH_LIMIT: usize = 64;

fn import(value: &Value) -> Result<ModelGraph, CxfError> {
    let bytes = serde_json::to_vec(value).unwrap();
    import_cxf(&bytes, &ResolveOptions::default()).map(|(graph, _)| graph)
}

fn on_small_stack(value: Value) -> Result<ModelGraph, CxfError> {
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || import(&value))
        .unwrap()
        .join()
        .unwrap()
}

fn diagnostics(result: Result<ModelGraph, CxfError>) -> Vec<Diagnostic> {
    match result {
        Err(CxfError::Validation(diags)) => diags,
        other => panic!("expected validation diagnostics, got {other:?}"),
    }
}

fn inactive_chain(length: usize) -> Value {
    let mut graph = vec![
        json!({
            "@id": ROOT,
            "@type": "S231:Block",
            "S231:hasParameter": { "@id": format!("{ROOT}.have") },
            "S231:containsBlock": { "@id": format!("{ROOT}.n0") }
        }),
        json!({ "@id": format!("{ROOT}.have"), "S231:value": false }),
    ];
    for index in 0..length {
        let id = format!("{ROOT}.n{index}");
        let mut node = json!({ "@id": id, "@type": "S231:Block" });
        if index == 0 {
            node["S231:isConditionalComponent"] = json!(true);
            node["S231:conditionalExpression"] = json!("have");
        }
        if index + 1 < length {
            node["S231:containsBlock"] = json!({ "@id": format!("{ROOT}.n{}", index + 1) });
        }
        graph.push(node);
    }
    json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" },
        "@graph": graph
    })
}

fn active_composite_chain(length: usize) -> Value {
    let mut graph = Vec::with_capacity(length + 3);
    for index in 0..length {
        let id = if index == 0 {
            ROOT.to_owned()
        } else {
            format!("{ROOT}.n{index}")
        };
        let mut node = json!({ "@id": id, "@type": "S231:Block" });
        node["S231:containsBlock"] = if index + 1 < length {
            json!({ "@id": format!("{ROOT}.n{}", index + 1) })
        } else {
            json!({ "@id": format!("{ROOT}.con") })
        };
        graph.push(node);
    }
    graph.extend([
        json!({
            "@id": format!("{ROOT}.con"),
            "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
            "S231:hasParameter": { "@id": format!("{ROOT}.con.k") },
            "S231:hasOutput": { "@id": format!("{ROOT}.con.y") }
        }),
        json!({
            "@id": format!("{ROOT}.con.k"),
            "@type": "S231:Parameter",
            "S231:value": 1.0
        }),
        json!({
            "@id": format!("{ROOT}.con.y"),
            "@type": "S231:RealOutput",
            "S231:isOfDataType": { "@id": "S231:Real" }
        }),
    ]);
    json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" },
        "@graph": graph
    })
}

fn constant_document(value: String) -> Value {
    json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" },
        "@graph": [
            {
                "@id": ROOT,
                "@type": "S231:Block",
                "S231:containsBlock": { "@id": format!("{ROOT}.con") }
            },
            {
                "@id": format!("{ROOT}.con"),
                "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
                "S231:hasParameter": { "@id": format!("{ROOT}.con.k") },
                "S231:hasOutput": { "@id": format!("{ROOT}.con.y") }
            },
            {
                "@id": format!("{ROOT}.con.k"),
                "@type": "S231:Parameter",
                "S231:value": value
            },
            {
                "@id": format!("{ROOT}.con.y"),
                "@type": "S231:RealOutput",
                "S231:isOfDataType": { "@id": "S231:Real" }
            }
        ]
    })
}

#[test]
fn false_guarded_self_loop_returns_normally() {
    let bytes = include_bytes!("fixtures/ingest_totality/false_guard_self_loop.jsonld");
    let result = import_cxf(bytes, &ResolveOptions::default()).map(|(graph, _)| graph);
    assert!(
        result.is_ok(),
        "false-guarded self-loop should be pruned: {result:?}"
    );
}

#[test]
fn false_guarded_two_node_cycle_returns_normally() {
    let bytes = include_bytes!("fixtures/ingest_totality/false_guard_two_node_cycle.jsonld");
    assert!(import_cxf(bytes, &ResolveOptions::default()).is_ok());
}

#[test]
fn active_self_loop_preserves_the_existing_cycle_diagnostic() {
    for bytes in [
        include_bytes!("fixtures/ingest_totality/active_self_loop.jsonld").as_slice(),
        include_bytes!("fixtures/ingest_totality/unconditional_self_loop.jsonld").as_slice(),
    ] {
        let result = import_cxf(bytes, &ResolveOptions::default()).map(|(graph, _)| graph);
        let diags = diagnostics(result);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagCode::MalformedDocument);
        assert_eq!(
            diags[0].subject.as_deref(),
            Some(format!("{ROOT}.n0").as_str())
        );
        assert_eq!(
            diags[0].message,
            format!(
                "composite/contains-cycle: cycle in nested composite containsBlock graph: \
                 {ROOT}.n0 -> {ROOT}.n0"
            )
        );
    }
}

#[test]
fn false_guard_prunes_deep_descendants_and_their_ports() {
    let bytes = include_bytes!("fixtures/ingest_totality/false_guard_deep_pruning.jsonld");
    let graph = import_cxf(bytes, &ResolveOptions::default()).unwrap().0;
    for suffix in ["n0", "n1", "n2"] {
        assert!(graph.blocks.iter().all(
            |block| block.instance_iri.as_deref() != Some(format!("{ROOT}.{suffix}").as_str())
        ));
    }
    assert!(
        graph
            .blocks
            .iter()
            .all(|block| block.instance_iri.as_deref() != Some(format!("{ROOT}.con").as_str()))
    );
    assert!(graph.connectors.is_empty());
}

#[test]
fn deep_false_guarded_chain_returns_normally_on_a_small_stack() {
    assert!(on_small_stack(inactive_chain(5000)).is_ok());
}

#[test]
fn deep_active_chain_returns_one_typed_nesting_diagnostic() {
    let diags = diagnostics(on_small_stack(active_composite_chain(2000)));
    let nesting: Vec<_> = diags
        .iter()
        .filter(|diag| diag.message.starts_with("composite/nesting-too-deep:"))
        .collect();
    assert_eq!(nesting.len(), 1, "{diags:?}");
    assert_eq!(nesting[0].code, DiagCode::MalformedDocument);
}

#[test]
fn composite_nesting_accepts_the_limit_and_rejects_one_past() {
    let boundary = import(&active_composite_chain(COMPOSITE_DEPTH_LIMIT));
    assert!(boundary.is_ok(), "{boundary:?}");
    let diags = diagnostics(import(&active_composite_chain(COMPOSITE_DEPTH_LIMIT + 1)));
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, DiagCode::MalformedDocument);
    assert_eq!(
        diags[0].subject.as_deref(),
        Some(format!("{ROOT}.n64").as_str())
    );
    assert_eq!(
        diags[0].message,
        "composite/nesting-too-deep: containsBlock nesting exceeds the supported depth (64)"
    );
}

#[test]
fn deeply_parenthesized_binding_returns_grounding_failed() {
    let value = format!("{}1{}", "(".repeat(100), ")".repeat(100));
    let diags = diagnostics(on_small_stack(constant_document(value)));
    assert!(diags.iter().any(|diag| {
        diag.code == DiagCode::GroundingFailed
            && diag.message
                == "expression binding did not ground: expression nesting exceeds the supported depth (64)"
    }));
}

#[test]
fn oversized_left_leaning_binding_returns_grounding_failed() {
    let value = std::iter::repeat_n("1", 5000).collect::<Vec<_>>().join("+");
    let diags = diagnostics(on_small_stack(constant_document(value)));
    assert!(diags.iter().any(|diag| {
        diag.code == DiagCode::GroundingFailed
            && diag.message
                == "expression binding did not ground: expression exceeds the supported node count (4096)"
    }));
}

#[test]
fn deep_conditional_guard_is_rejected_during_parse() {
    let mut document = inactive_chain(1);
    document["@graph"][2]["S231:conditionalExpression"] = json!(
        std::iter::repeat_n("p", 2000)
            .collect::<Vec<_>>()
            .join(" and ")
    );
    let diags = diagnostics(on_small_stack(document));
    assert!(diags.iter().any(|diag| {
        diag.code == DiagCode::ConditionalGuardUnsupported
            && diag.message.starts_with("conditional guard did not parse:")
    }));
}
