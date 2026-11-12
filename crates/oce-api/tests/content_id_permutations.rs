//! Same-id permutation classes over the whole corpus (`docs/design/content-id.md`, "Permutation
//! classes"): every enumerated rewrite of each of the 47 documents (46 G36 catalog fixtures and
//! `member_list_interface`) that loads to the same ground values keeps the canonical content id,
//! and a complete export round-trips to byte-identical canonical bytes. No RNG.

// Shared helper, declared once for this binary in its root `facade.rs`.
use crate::content_id_corpus as corpus;

use std::collections::{BTreeMap, BTreeSet};

use corpus::*;
use serde_json::Value;

/// One named rewrite of a parsed document to serialized bytes.
type Rewrite = (&'static str, fn(&Value) -> Vec<u8>);

/// The rewrites that must keep the id for every corpus document.
const ALWAYS_SAME: &[Rewrite] = &[
    ("@graph reversed", |d| to_bytes(&reverse_graph(d))),
    ("@graph rotated by one", |d| to_bytes(&rotate_graph(d, 1))),
    ("@graph rotated by half", |d| {
        let half = d["@graph"].as_array().unwrap().len() / 2;
        to_bytes(&rotate_graph(d, half))
    }),
    ("containsBlock reversed", |d| {
        to_bytes(&reverse_relation(d, "S231:containsBlock", |_| true))
    }),
    ("isConnectedTo reversed", |d| {
        to_bytes(&reverse_relation(d, "S231:isConnectedTo", |_| true))
    }),
    ("root hasInput and hasOutput reversed", |d| {
        let d = reverse_relation(d, "S231:hasInput", is_composite);
        to_bytes(&reverse_relation(&d, "S231:hasOutput", is_composite))
    }),
    ("every hasParameter reversed", |d| {
        to_bytes(&reverse_relation(d, "S231:hasParameter", |_| true))
    }),
    ("object keys reversed, no whitespace", reverse_keys),
    ("pretty-printed", |d| serde_json::to_vec_pretty(d).unwrap()),
    ("compact instance IRIs", |d| to_bytes(&compact_ids(d))),
    ("namespace urn:oce:author:", |d| {
        to_bytes(&move_namespace(d, "urn:oce:author:"))
    }),
    ("namespace https://example.com/lib/", |d| {
        to_bytes(&move_namespace(d, "https://example.com/lib/"))
    }),
    ("everything at once", |d| {
        let d = move_namespace(&rotate_graph(&reverse_graph(d), 3), "urn:oce:author:");
        let d = reverse_relation(&d, "S231:containsBlock", |_| true);
        reverse_keys(&reverse_relation(&d, "S231:isConnectedTo", |_| true))
    }),
];

#[test]
fn every_order_spelling_and_namespace_rewrite_keeps_the_id_of_every_document() {
    let mut checked = 0;
    for name in corpus() {
        let document = fixture_json(&name);
        let original = id_of(&fixture_bytes(&name));
        for (rewrite, apply) in ALWAYS_SAME {
            let bytes = apply(&document);
            let mut engine = oce_api::Engine::new();
            if let Err(error) = engine.load_cxf(&bytes) {
                panic!("{name}: `{rewrite}` must still load: {error}");
            }
            assert_eq!(engine.content_id().unwrap(), original, "{name}: {rewrite}");
            checked += 1;
        }
    }
    assert_eq!(checked, 47 * ALWAYS_SAME.len());
}

/// A block's resolved binding as `Topology` reports it, keyed by instance path: class, input and
/// output connector paths in port-index order, and parameters sorted by name (bits for Reals).
type Binding = BTreeMap<String, (String, Vec<String>, Vec<String>, Vec<String>)>;

fn binding(engine: &oce_api::Engine) -> (Binding, BTreeSet<(String, String)>) {
    let topology = engine.topology();
    let blocks = topology
        .blocks
        .iter()
        .map(|block| {
            let mut params: Vec<String> = block
                .params
                .iter()
                .map(|(name, value)| match value {
                    oce_api::Value::Real(x) => format!("{name}={:016x}", x.to_bits()),
                    other => format!("{name}={other:?}"),
                })
                .collect();
            params.sort();
            (
                block.instance_path.clone(),
                (
                    block.class_iri.clone(),
                    block.inputs.clone(),
                    block.outputs.clone(),
                    params,
                ),
            )
        })
        .collect();
    let edges = topology
        .connections
        .iter()
        .map(|edge| (edge.from.clone(), edge.to.clone()))
        .collect();
    (blocks, edges)
}

#[test]
fn leaf_port_and_parameter_list_reversals_keep_the_id_exactly_when_the_binding_is_unchanged() {
    // Reversing a leaf's `hasInput`/`hasOutput` keeps the id when its ports bind by name, and is
    // a rewire (a same-kind port swap on a positional block), which moves it, when they bind by
    // position. `hasParameter` order never reaches a ground value in this corpus.
    let (mut kept, mut rewired) = (0, Vec::new());
    for name in corpus() {
        let document = fixture_json(&name);
        let original = load(&fixture_bytes(&name));
        let (original_blocks, original_edges) = binding(&original);
        let original_id = original.content_id().unwrap();
        for key in ["S231:hasInput", "S231:hasOutput", "S231:hasParameter"] {
            let bytes = to_bytes(&reverse_relation(&document, key, |node| {
                !is_composite(node)
            }));
            let variant = try_load(&bytes)
                .unwrap_or_else(|| panic!("{name}: leaf `{key}` reversed must still load"));
            let (blocks, edges) = binding(&variant);
            let same_binding = blocks == original_blocks && edges == original_edges;
            let same_id = variant.content_id().unwrap() == original_id;
            assert_eq!(same_id, same_binding, "{name}: leaf `{key}` reversed");
            if same_binding {
                kept += 1;
            } else {
                assert_ne!(key, "S231:hasParameter", "{name}");
                rewired.push(format!("{name} {key}"));
            }
        }
    }
    assert_eq!(kept + rewired.len(), 47 * 3);
    // Both arms are exercised: the positional fixtures rebind ports.
    assert_eq!(
        rewired,
        [
            "multizone_vav_outdoor_airflow_sumzone S231:hasInput",
            "multizone_vav_outdoor_airflow_sumzone S231:hasOutput",
            "multizone_vav_outdoor_airflow_title24_sumzone S231:hasInput",
            "multizone_vav_outdoor_airflow_title24_sumzone S231:hasOutput",
            "multizone_vav_relief_fan_group S231:hasInput",
            "multizone_vav_relief_fan_group S231:hasOutput",
        ]
    );
}

#[test]
fn a_complete_export_reloads_to_byte_identical_canonical_bytes() {
    let mut complete = 0;
    for name in corpus() {
        let engine = load(&fixture_bytes(&name));
        let export = engine.export_cxf().unwrap();
        if export.complete().is_err() {
            continue;
        }
        let reloaded = load(&export.bytes);
        assert_eq!(
            reloaded.canonical_content_bytes().unwrap(),
            engine.canonical_content_bytes().unwrap(),
            "{name}"
        );
        assert_eq!(reloaded.content_id().unwrap(), engine.content_id().unwrap());
        complete += 1;
    }
    // 33 G36 fixtures and member_list_interface export completely.
    assert_eq!(complete, 34);
}
