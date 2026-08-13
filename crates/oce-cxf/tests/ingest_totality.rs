//! End-to-end totality pins for recursively shaped CXF input.

use oce_cxf::{CxfError, ResolveOptions, ValidationReport, import_cxf};
use oce_diag::{DiagCode, Diagnostic};
use oce_model::ModelGraph;
use serde_json::{Value, json};
use std::fmt::Write as _;

const ROOT: &str = "http://example.org#top";
const COMPOSITE_DEPTH_LIMIT: usize = 64;
const BOUNDARY_HOP_LIMIT: usize = 64;
const BOUNDARY_TARGET_LIMIT: usize = 65_536;
const BOUNDARY_TARGET_BYTE_LIMIT: usize = 8 * 1024 * 1024;
const BOUNDARY_DIAGNOSTIC_GOLDEN: &str =
    include_str!("fixtures/golden/boundary_traversal_limits.diagnostics.txt");
const BOUNDARY_ACCEPTED_GOLDEN: &str =
    include_str!("fixtures/golden/boundary_traversal_accepted.txt");
const CONDITIONAL_GUARD_TERMS: usize = 2000;
const _: () = assert!(2 * CONDITIONAL_GUARD_TERMS - 1 < oce_expr::MAX_EXPR_NODES);

type ImportResult = Result<(ModelGraph, ValidationReport), CxfError>;

fn import(value: &Value) -> ImportResult {
    let bytes = serde_json::to_vec(value).unwrap();
    import_cxf(&bytes, &ResolveOptions::default())
}

fn on_small_stack(value: Value) -> ImportResult {
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || import(&value))
        .unwrap()
        .join()
        .unwrap()
}

fn diagnostics(result: ImportResult) -> Vec<Diagnostic> {
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

fn boundary_iri(index: usize) -> String {
    let suffix = if index.is_multiple_of(2) { "u" } else { "y" };
    format!("{ROOT}.sub{}.{suffix}", index / 2)
}

fn boundary_chain(
    hops: usize,
    fanout: usize,
    entries: usize,
    terminal_padding: usize,
    close_cycle: bool,
) -> Value {
    assert!(hops > 0);
    assert!(fanout > 0);
    assert!(entries > 0);
    let composite_count = hops.div_ceil(2);
    let terminal = format!("{ROOT}.gain.u{}", "x".repeat(terminal_padding));
    let mut children = vec![json!({ "@id": format!("{ROOT}.gain") })];
    children
        .extend((0..composite_count).map(|index| json!({ "@id": format!("{ROOT}.sub{index}") })));
    children.extend((0..entries).map(|index| json!({ "@id": format!("{ROOT}.src{index}") })));
    let mut graph = vec![
        json!({
            "@id": ROOT,
            "@type": "S231:Block",
            "S231:containsBlock": children
        }),
        json!({
            "@id": format!("{ROOT}.gain"),
            "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.MultiplyByParameter",
            "S231:hasParameter": { "@id": format!("{ROOT}.gain.k") },
            "S231:hasInput": { "@id": terminal.clone() },
            "S231:hasOutput": { "@id": format!("{ROOT}.gain.y") }
        }),
        json!({ "@id": format!("{ROOT}.gain.k"), "S231:value": 1 }),
        json!({
            "@id": terminal,
            "@type": "S231:RealInput",
            "S231:isOfDataType": { "@id": "S231:Real" }
        }),
        json!({
            "@id": format!("{ROOT}.gain.y"),
            "@type": "S231:RealOutput",
            "S231:isOfDataType": { "@id": "S231:Real" }
        }),
    ];
    for index in 0..entries {
        graph.extend([
            json!({
                "@id": format!("{ROOT}.src{index}"),
                "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
                "S231:hasParameter": { "@id": format!("{ROOT}.src{index}.k") },
                "S231:hasOutput": { "@id": format!("{ROOT}.src{index}.y") }
            }),
            json!({ "@id": format!("{ROOT}.src{index}.k"), "S231:value": 1 }),
            json!({
                "@id": format!("{ROOT}.src{index}.y"),
                "@type": "S231:RealOutput",
                "S231:isOfDataType": { "@id": "S231:Real" },
                "S231:isConnectedTo": { "@id": boundary_iri(0) }
            }),
        ]);
    }
    for index in 0..composite_count {
        let composite = format!("{ROOT}.sub{index}");
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
            json!({ "@id": format!("{composite}.keep.k"), "S231:value": 1 }),
            json!({
                "@id": format!("{composite}.keep.y"),
                "@type": "S231:RealOutput",
                "S231:isOfDataType": { "@id": "S231:Real" }
            }),
        ]);
    }
    for index in 0..hops {
        let target = if index + 1 < hops {
            boundary_iri(index + 1)
        } else if close_cycle {
            boundary_iri(0)
        } else {
            terminal.clone()
        };
        let targets = std::iter::repeat_n(json!({ "@id": target }), fanout).collect::<Vec<_>>();
        let node = graph
            .iter_mut()
            .find(|node| node["@id"].as_str() == Some(boundary_iri(index).as_str()))
            .expect("chain boundary node");
        node["S231:isConnectedTo"] = if fanout == 1 {
            targets.into_iter().next().expect("one target")
        } else {
            Value::Array(targets)
        };
    }
    json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" },
        "@graph": graph
    })
}

fn render_diagnostics(diags: &[Diagnostic]) -> String {
    let mut rendered = String::new();
    for diag in diags {
        writeln!(
            rendered,
            "{}|{}|{}|{}",
            diag.severity.as_str(),
            diag.code.as_str(),
            diag.subject.as_deref().unwrap_or("<none>"),
            diag.message
        )
        .expect("write diagnostic");
    }
    rendered
}

fn render_boundary_graph(graph: &ModelGraph) -> String {
    let mut rendered = String::new();
    writeln!(rendered, "blocks={}", graph.blocks.len()).expect("write blocks");
    writeln!(rendered, "connectors={}", graph.connectors.len()).expect("write connectors");
    writeln!(rendered, "connections={}", graph.connections.len()).expect("write connections");
    for connection in &graph.connections {
        let source = graph.connectors[connection.from.0 as usize]
            .iri
            .as_deref()
            .unwrap_or("<none>");
        let target = graph.connectors[connection.to.0 as usize]
            .iri
            .as_deref()
            .unwrap_or("<none>");
        writeln!(rendered, "{source} -> {target}").expect("write connection");
    }
    rendered
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
    let result = import_cxf(bytes, &ResolveOptions::default());
    let (graph, report) = result.unwrap_or_else(|error| panic!("import failed: {error:?}"));
    assert!(report.is_empty(), "{report:?}");
    assert!(
        graph.blocks.iter().all(|block| {
            block
                .instance_iri
                .as_deref()
                .is_none_or(|iri| !iri.contains("#top.n"))
        }),
        "{graph:?}"
    );
}

#[test]
fn false_guarded_two_node_cycle_returns_normally() {
    let bytes = include_bytes!("fixtures/ingest_totality/false_guard_two_node_cycle.jsonld");
    let result = import_cxf(bytes, &ResolveOptions::default());
    let (graph, report) = result.unwrap_or_else(|error| panic!("import failed: {error:?}"));
    assert!(report.is_empty(), "{report:?}");
    assert!(
        graph.blocks.iter().all(|block| {
            block
                .instance_iri
                .as_deref()
                .is_none_or(|iri| !iri.contains("#top.n"))
        }),
        "{graph:?}"
    );
}

#[test]
fn active_self_loop_preserves_the_existing_cycle_diagnostic() {
    for bytes in [
        include_bytes!("fixtures/ingest_totality/active_self_loop.jsonld").as_slice(),
        include_bytes!("fixtures/ingest_totality/unconditional_self_loop.jsonld").as_slice(),
    ] {
        let diags = diagnostics(import_cxf(bytes, &ResolveOptions::default()));
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
    let (graph, report) = import_cxf(bytes, &ResolveOptions::default()).unwrap();
    assert!(report.is_empty(), "{report:?}");
    let block_iris: Vec<_> = graph
        .blocks
        .iter()
        .map(|block| block.instance_iri.as_deref())
        .collect();
    assert_eq!(block_iris, [Some("http://example.org#top.survivor")]);
    // The survivor has one output. Exact connector ownership/count proves n2's boundary input and
    // the false-guarded descendant constant contributed no connectors.
    assert_eq!(graph.connectors.len(), 1);
    assert_eq!(graph.connectors[0].block, graph.blocks[0].id);
}

#[test]
fn deep_false_guarded_chain_returns_normally_on_a_small_stack() {
    let result = on_small_stack(inactive_chain(5000));
    let (graph, report) =
        result.unwrap_or_else(|error| panic!("deep inactive import failed: {error:?}"));
    assert!(report.is_empty(), "{report:?}");
    assert!(
        graph.blocks.iter().all(|block| {
            block
                .instance_iri
                .as_deref()
                .is_none_or(|iri| !iri.contains("#top.n"))
        }),
        "{graph:?}"
    );
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
fn boundary_hops_accept_the_limit_and_reject_the_attempted_next_hop() {
    let accepted = on_small_stack(boundary_chain(BOUNDARY_HOP_LIMIT, 1, 1, 0, false));
    let (graph, report) = accepted.expect("the exact hop limit must import");
    assert!(report.is_empty(), "{report:?}");
    assert_eq!(render_boundary_graph(&graph), BOUNDARY_ACCEPTED_GOLDEN);

    let diags = diagnostics(on_small_stack(boundary_chain(
        BOUNDARY_HOP_LIMIT + 1,
        1,
        1,
        0,
        false,
    )));
    assert_eq!(
        diags,
        vec![
            Diagnostic::error(
                DiagCode::MalformedDocument,
                "composite boundary resolution exceeds the supported isConnectedTo hop count (64)",
            )
            .with_subject(boundary_iri(BOUNDARY_HOP_LIMIT))
        ]
    );
}

#[test]
fn shallow_boundary_dag_rejects_the_attempted_target_past_the_work_limit() {
    // One 15-hop source walk examines 65,535 duplicate-preserving targets. The document-wide
    // budget therefore rejects boundary 1 in the second source walk, before graph construction or
    // complete path materialization.
    let document = boundary_chain(15, 2, 2, 0, false);
    let expected = vec![
        Diagnostic::error(
            DiagCode::MalformedDocument,
            "composite boundary resolution exceeds the supported target examination count (65536)",
        )
        .with_subject(boundary_iri(1)),
    ];
    let first = diagnostics(on_small_stack(document.clone()));
    let second = diagnostics(on_small_stack(document));
    assert_eq!(first, expected);
    assert_eq!(second, expected);
    assert_eq!(BOUNDARY_TARGET_LIMIT, 65_536);
}

#[test]
fn expanded_target_bytes_reject_before_the_count_limit() {
    let padding = 256;
    let document = boundary_chain(15, 2, 1, padding, false);
    let terminal = format!("{ROOT}.gain.u{}", "x".repeat(padding));
    let diags = diagnostics(on_small_stack(document));
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, DiagCode::MalformedDocument);
    assert_eq!(diags[0].subject.as_deref(), Some(terminal.as_str()));
    assert_eq!(
        diags[0].message,
        "composite boundary resolution exceeds the supported aggregate target IRI byte count \
         (8388608)"
    );
    assert_eq!(BOUNDARY_TARGET_BYTE_LIMIT, 8_388_608);
}

#[test]
fn boundary_cycle_revisit_precedes_the_hop_limit() {
    let diags = diagnostics(on_small_stack(boundary_chain(
        BOUNDARY_HOP_LIMIT,
        1,
        1,
        0,
        true,
    )));
    assert!(diags.iter().any(|diag| {
        diag.code == DiagCode::UnresolvedReference
            && diag.subject.as_deref() == Some(boundary_iri(0).as_str())
    }));
    assert!(
        diags
            .iter()
            .all(|diag| { !diag.message.contains("supported isConnectedTo hop count") })
    );
}

#[test]
fn missing_boundary_at_the_hop_limit_keeps_unresolved_reference_precedence() {
    let mut document = boundary_chain(BOUNDARY_HOP_LIMIT + 1, 1, 1, 0, false);
    let missing = boundary_iri(BOUNDARY_HOP_LIMIT);
    document["@graph"]
        .as_array_mut()
        .expect("@graph")
        .retain(|node| node["@id"].as_str() != Some(missing.as_str()));
    let diags = diagnostics(on_small_stack(document));
    assert!(diags.iter().any(|diag| {
        diag.code == DiagCode::UnresolvedReference
            && diag.subject.as_deref() == Some(missing.as_str())
    }));
    assert!(
        diags
            .iter()
            .all(|diag| { !diag.message.contains("supported isConnectedTo hop count") })
    );
}

#[test]
fn resource_rejections_match_the_checked_in_diagnostic_golden() {
    let hop = diagnostics(on_small_stack(boundary_chain(
        BOUNDARY_HOP_LIMIT + 1,
        1,
        1,
        0,
        false,
    )));
    let work = diagnostics(on_small_stack(boundary_chain(15, 2, 2, 0, false)));
    assert_eq!(
        render_diagnostics(&[hop[0].clone(), work[0].clone()]),
        BOUNDARY_DIAGNOSTIC_GOLDEN
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
        std::iter::repeat_n("p", CONDITIONAL_GUARD_TERMS)
            .collect::<Vec<_>>()
            .join(" and ")
    );
    let diags = diagnostics(on_small_stack(document));
    assert!(diags.iter().any(|diag| {
        diag.code == DiagCode::ConditionalGuardUnsupported
            && diag.message.starts_with("conditional guard did not parse:")
            && diag
                .message
                .contains("nesting exceeds the supported depth (64)")
    }));
}
