//! Executable OCE ordering oracle: hand-derived positions, not an export/re-import fixpoint.
//! Repeated imports exercise fresh lookup maps; the expected sequences never iterate those maps.

#[path = "resolve_ordering/diagnostics.rs"]
mod diagnostics;
#[path = "resolve_ordering/documents.rs"]
mod documents;

use documents::{COMPACT, EXPANDED, node_mut, reverse_object_keys};
use oce_cxf::{ResolveOptions, import_cxf};
use oce_model::{ModelGraph, Value};
use serde_json::{Value as Json, json};

fn import(bytes: &[u8]) -> ModelGraph {
    let (graph, report) = import_cxf(bytes, &ResolveOptions::default()).unwrap();
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    graph
}

fn import_json(doc: &Json) -> ModelGraph {
    import(&serde_json::to_vec(doc).unwrap())
}

fn block_order(graph: &ModelGraph) -> Vec<(&str, u32, u32)> {
    graph
        .blocks
        .iter()
        .map(|block| {
            (
                block.instance_iri.as_deref().unwrap(),
                block.id.0,
                block.decl_order,
            )
        })
        .collect()
}

fn edges(graph: &ModelGraph) -> Vec<(u32, u32)> {
    graph
        .connections
        .iter()
        .map(|edge| (edge.from.0, edge.to.0))
        .collect()
}

fn assert_authored_order(graph: &ModelGraph) {
    assert_eq!(
        block_order(graph),
        [
            ("urn:order:M.branch.zSource", 0, 0),
            ("urn:order:M.branch.aSum", 1, 1),
            ("urn:order:M.tail", 2, 2),
        ]
    );
    // Graph node order deliberately disagrees with containment, signature and lexical order.
    let expected = [
        ("urn:order:M.branch.aSum.u2", 1),
        ("urn:order:M.tail.y", 2),
        ("urn:order:M.branch.zSource.y", 0),
        ("urn:order:M.tail.u1", 2),
        ("urn:order:M.branch.aSum.y", 1),
        ("urn:order:M.branch.aSum.u1", 1),
        ("urn:order:M.tail.u2", 2),
    ];
    assert_eq!(graph.connectors.len(), expected.len());
    for (index, (connector, (iri, owner))) in graph.connectors.iter().zip(expected).enumerate() {
        assert_eq!(connector.id.0, index as u32);
        assert_eq!(connector.decl_order, index as u32);
        assert_eq!(connector.block.0, owner);
        assert_eq!(connector.iri.as_deref(), Some(iri));
    }
    assert_eq!(
        graph.blocks[1]
            .inputs
            .iter()
            .map(|id| id.0)
            .collect::<Vec<_>>(),
        [5, 0]
    );
    assert_eq!(
        graph.blocks[2]
            .inputs
            .iter()
            .map(|id| id.0)
            .collect::<Vec<_>>(),
        [3, 6]
    );
    assert_eq!(graph.blocks[0].params.values.len(), 1);
    assert_eq!(graph.blocks[0].params.values[0].0.as_ref(), "k");
    assert!(graph.blocks[0].params.values[0].1.bit_eq(&Value::Real(2.0)));
    assert_eq!(edges(graph), [(2, 6), (2, 5), (2, 3), (2, 0)]);
    assert!(graph.external_inputs.is_empty());
    assert!(graph.boundary_inputs.is_empty());
    assert_eq!(
        graph
            .boundary_outputs
            .iter()
            .map(|o| (o.iri.as_ref(), o.source.0))
            .collect::<Vec<_>>(),
        [("urn:order:M.outTail", 1), ("urn:order:M.outSum", 4)]
    );
}

#[test]
fn independent_spellings_preserve_the_hand_derived_executable_order() {
    let baseline = import(COMPACT.as_bytes());
    let export = oce_cxf::export(&baseline).unwrap();
    for source in [COMPACT, EXPANDED] {
        let doc: Json = serde_json::from_str(source).unwrap();
        for bytes in [
            source.as_bytes().to_vec(),
            serde_json::to_vec(&doc).unwrap(),
            reverse_object_keys(&doc).into_bytes(),
        ] {
            for _ in 0..8 {
                let graph = import(&bytes);
                assert_authored_order(&graph);
                assert_eq!(oce_cxf::export(&graph).unwrap(), export);
            }
        }
    }
}

#[test]
fn containment_permutation_changes_blocks_but_not_authored_connector_positions() {
    let mut doc: Json = serde_json::from_str(COMPACT).unwrap();
    node_mut(&mut doc, "m:M")["S231:containsBlock"]
        .as_array_mut()
        .unwrap()
        .reverse();
    node_mut(&mut doc, "m:M.branch")["S231:containsBlock"]
        .as_array_mut()
        .unwrap()
        .reverse();
    let graph = import_json(&doc);
    assert_eq!(
        block_order(&graph),
        [
            ("urn:order:M.tail", 0, 0),
            ("urn:order:M.branch.aSum", 1, 1),
            ("urn:order:M.branch.zSource", 2, 2),
        ]
    );
    assert_eq!(edges(&graph), [(2, 6), (2, 5), (2, 3), (2, 0)]);
    assert_eq!(graph.connectors[2].block.0, 2);
    assert_ne!(
        oce_cxf::export(&graph).unwrap(),
        oce_cxf::export(&import(COMPACT.as_bytes())).unwrap()
    );
}

#[test]
fn graph_and_direct_target_permutations_change_the_pinned_storage_order() {
    let mut doc: Json = serde_json::from_str(COMPACT).unwrap();
    node_mut(&mut doc, "m:M.branch.zSource.y")["S231:isConnectedTo"]
        .as_array_mut()
        .unwrap()
        .reverse();
    assert_eq!(edges(&import_json(&doc)), [(2, 0), (2, 3), (2, 5), (2, 6)]);
    // Swap authored connector nodes, not the port lists: IDs and references must move together.
    doc["@graph"].as_array_mut().unwrap().swap(5, 10);
    let graph = import_json(&doc);
    assert_eq!(
        graph.connectors[0].iri.as_deref(),
        Some("urn:order:M.branch.aSum.u1")
    );
    assert_eq!(
        graph.blocks[1]
            .inputs
            .iter()
            .map(|id| id.0)
            .collect::<Vec<_>>(),
        [0, 5]
    );
    assert_eq!(edges(&graph), [(2, 5), (2, 3), (2, 0), (2, 6)]);
}

#[test]
fn connection_sources_follow_graph_nodes_not_containment_order() {
    let mut doc: Json = serde_json::from_str(COMPACT).unwrap();
    node_mut(&mut doc, "m:M.tail.y")["S231:isConnectedTo"] =
        json!([{"@id":"m:M.outTail"},{"@id":"m:M.branch.aSum.u1"}]);
    node_mut(&mut doc, "m:M.branch.zSource.y")["S231:isConnectedTo"] =
        json!([{"@id":"m:M.tail.u2"},{"@id":"m:M.tail.u1"},{"@id":"m:M.branch.aSum.u2"}]);
    let before = import_json(&doc);
    assert_eq!(edges(&before), [(1, 5), (2, 6), (2, 3), (2, 0)]);
    // Tail is the last block but the first authored connection source. Reorder only its node.
    doc["@graph"].as_array_mut().unwrap().swap(6, 7);
    let after = import(reverse_object_keys(&doc).as_bytes());
    assert_eq!(block_order(&after), block_order(&before));
    assert_eq!(edges(&after), [(1, 6), (1, 3), (1, 0), (2, 5)]);
}

#[test]
fn moving_an_inactive_child_and_removing_its_nodes_preserves_the_active_order() {
    let mut doc: Json = serde_json::from_str(COMPACT).unwrap();
    node_mut(&mut doc, "m:M")["S231:containsBlock"]
        .as_array_mut()
        .unwrap()
        .swap(0, 1);
    assert_authored_order(&import_json(&doc));
    node_mut(&mut doc, "m:M")["S231:containsBlock"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    doc["@graph"]
        .as_array_mut()
        .unwrap()
        .retain(|node| !node["@id"].as_str().unwrap().starts_with("m:M.off"));
    assert_authored_order(&import_json(&doc));
}

#[test]
fn named_ports_and_composite_declarations_normalize_without_moving_ids() {
    for source in [COMPACT, EXPANDED] {
        let mut doc: Json = serde_json::from_str(source).unwrap();
        for node in doc["@graph"].as_array_mut().unwrap() {
            for field in [
                "S231:hasInput",
                "S231:hasOutput",
                "S231:hasParameter",
                "S231:hasConstant",
            ] {
                if let Some(array) = node.get_mut(field).and_then(Json::as_array_mut) {
                    array.reverse();
                }
            }
        }
        assert_authored_order(&import_json(&doc));
    }
}

#[test]
fn unnamed_ports_remain_positional_even_when_all_kinds_match() {
    // Renaming all Add inputs removes name binding, without changing either type or topology.
    let source = COMPACT.replace(".u1", ".inLeft").replace(".u2", ".inRight");
    let mut doc: Json = serde_json::from_str(&source).unwrap();
    let before = import_json(&doc);
    assert_eq!(
        before.blocks[1]
            .inputs
            .iter()
            .map(|id| id.0)
            .collect::<Vec<_>>(),
        [0, 5]
    );
    node_mut(&mut doc, "m:M.branch.aSum")["S231:hasInput"]
        .as_array_mut()
        .unwrap()
        .reverse();
    let after = import_json(&doc);
    assert_eq!(
        after.blocks[1]
            .inputs
            .iter()
            .map(|id| id.0)
            .collect::<Vec<_>>(),
        [5, 0]
    );
    assert_eq!(edges(&before), edges(&after));
}

#[test]
fn member_permutations_keep_signature_parameters_and_synthesized_owner_order() {
    let source =
        include_str!("fixtures/composite_contract/accepted/synthesized_order_across_owners.jsonld");
    let expected = import(source.as_bytes());
    for rotate in 0..4 {
        let mut doc: Json = serde_json::from_str(source).unwrap();
        for node in doc["@graph"].as_array_mut().unwrap() {
            if let Some(members) = node
                .get_mut("S231:hasInstance")
                .and_then(Json::as_array_mut)
            {
                let len = members.len();
                members.rotate_left(rotate % len);
                members.reverse();
            }
        }
        let graph = import(reverse_object_keys(&doc).as_bytes());
        assert_eq!(
            graph.blocks[2]
                .params
                .values
                .iter()
                .map(|(name, _)| name.as_ref())
                .collect::<Vec<_>>(),
            ["samplePeriod", "y_start"]
        );
        assert!(
            graph.connectors[3]
                .iri
                .as_deref()
                .unwrap()
                .ends_with(".gain.y")
        );
        assert!(
            graph.connectors[4]
                .iri
                .as_deref()
                .unwrap()
                .ends_with(".del.u")
        );
        assert_eq!(
            oce_cxf::export(&graph).unwrap(),
            oce_cxf::export(&expected).unwrap()
        );
    }
}

#[test]
fn export_survivors_keep_nonlexical_vector_order_across_spellings() {
    let mut previous = None;
    for source in [COMPACT, EXPANDED] {
        for _ in 0..8 {
            let mut graph = import(source.as_bytes());
            // An enum parameter defers ONLY the middle block; independent branches survive.
            graph.blocks[1].params.values.push((
                "deferred".into(),
                Value::Enum {
                    class: oce_model::EnumClassId(7),
                    ordinal: 1,
                },
            ));
            let report = oce_cxf::export_with_report(&graph).unwrap();
            assert_eq!(report.warnings.len(), 1);
            assert_eq!(report.warnings[0].code, oce_diag::DiagCode::ExportDeferred);
            assert_eq!(
                report.warnings[0].subject.as_deref(),
                Some("urn:order:M.branch.aSum")
            );
            let doc: Json = serde_json::from_slice(&report.bytes).unwrap();
            let ids = doc["@graph"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n["@id"].as_str().unwrap())
                .collect::<Vec<_>>();
            // Hand-listed survivor order: root, blocks/parameters, surviving connectors, boundary.
            assert_eq!(
                ids,
                [
                    "urn:open-control:cxf-export:root",
                    "urn:order:M.branch.zSource",
                    "urn:order:M.branch.zSource.k",
                    "urn:order:M.tail",
                    "urn:order:M.tail.y",
                    "urn:order:M.branch.zSource.y",
                    "urn:order:M.tail.u1",
                    "urn:order:M.tail.u2",
                    "urn:order:M.outTail"
                ]
            );
            assert_eq!(
                doc["@graph"][0]["S231:containsBlock"],
                json!([{"@id":"urn:order:M.branch.zSource"},{"@id":"urn:order:M.tail"}])
            );
            if let Some(bytes) = &previous {
                assert_eq!(&report.bytes, bytes);
            }
            previous = Some(report.bytes);
        }
    }
}

#[test]
fn crossed_boundary_fanout_uses_boundary_node_then_dense_connector_order() {
    let mut doc: Json = serde_json::from_str(COMPACT).unwrap();
    node_mut(&mut doc, "m:M")["S231:hasInput"] =
        json!([{"@id":"m:M.inputSum"},{"@id":"m:M.inputTail"}]);
    node_mut(&mut doc, "m:M.branch")["S231:hasInput"] = json!({"@id":"m:M.branch.u"});
    node_mut(&mut doc, "m:M.branch.zSource.y")["S231:isConnectedTo"] = json!([]);
    doc["@graph"].as_array_mut().unwrap().extend([
        json!({"@id":"m:M.inputTail", "@type":"S231:RealInput", "S231:isConnectedTo":[{"@id":"m:M.tail.u2"},{"@id":"m:M.tail.u1"}]}),
        json!({"@id":"m:M.inputSum", "@type":"S231:RealInput", "S231:isConnectedTo":{"@id":"m:M.branch.u"}}),
        json!({"@id":"m:M.branch.u", "@type":"S231:RealInput", "S231:isConnectedTo":[{"@id":"m:M.branch.aSum.u1"},{"@id":"m:M.branch.aSum.u2"}]}),
    ]);
    let baseline = import_json(&doc);
    let external = |graph: &ModelGraph| {
        graph
            .external_inputs
            .iter()
            .map(|id| id.0)
            .collect::<Vec<_>>()
    };
    assert_eq!(external(&baseline), [3, 6, 0, 5]);
    let exported = oce_cxf::export(&baseline).unwrap();
    for id in ["m:M.inputTail", "m:M.branch.u"] {
        node_mut(&mut doc, id)["S231:isConnectedTo"]
            .as_array_mut()
            .unwrap()
            .reverse();
    }
    for _ in 0..8 {
        let graph = import(reverse_object_keys(&doc).as_bytes());
        assert_eq!(external(&graph), [3, 6, 0, 5]);
        assert_eq!(oce_cxf::export(&graph).unwrap(), exported);
    }
    let emitted: Json = serde_json::from_slice(&exported).unwrap();
    for (id, targets) in [
        (
            "urn:order:M.inputTail",
            ["urn:order:M.tail.in0", "urn:order:M.tail.in1"],
        ),
        (
            "urn:order:M.inputSum",
            ["urn:order:M.branch.aSum.in1", "urn:order:M.branch.aSum.in0"],
        ),
    ] {
        let node = emitted["@graph"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["@id"] == id)
            .unwrap();
        assert_eq!(
            node["S231:isConnectedTo"],
            json!([{"@id":targets[0]},{"@id":targets[1]}])
        );
    }
    // Moving boundary nodes, unlike their fanout arrays, IS order-bearing.
    doc["@graph"].as_array_mut().unwrap().swap(21, 22);
    assert_eq!(external(&import_json(&doc)), [0, 5, 3, 6]);
}
