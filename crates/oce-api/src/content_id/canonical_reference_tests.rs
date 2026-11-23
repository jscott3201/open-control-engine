//! The canonical bytes and the id are those the records built before issue #811 spell: for every
//! tracked fixture that loads, and for generated graphs that fan out, fan in, cross composite
//! boundaries, pass boundary inputs through, and name blocks whose relative names collide.

use std::path::{Path, PathBuf};

use super::canonical_reference::reference_bytes;
use super::hash::sha256;
use crate::Engine;

/// Every `.jsonld` fixture under `crates/*/tests/fixtures`, sorted.
fn fixtures() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "jsonld") {
                out.push(path);
            }
        }
    }
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut out = Vec::new();
    for krate in ["oce-api", "oce-cxf", "oce-conformance"] {
        walk(&crates.join(krate).join("tests").join("fixtures"), &mut out);
    }
    out.sort();
    out
}

/// Load `document`; if it loads, its canonical bytes and id must be the reference's. Returns
/// whether it loaded.
fn agrees(document: &[u8], what: &str) -> bool {
    let mut engine = Engine::new();
    if engine.load_cxf(document).is_err() {
        return false;
    }
    let bytes = engine.canonical_content_bytes().expect("bytes");
    let reference = reference_bytes(&engine.model, &engine.io).expect("reference");
    assert!(bytes == reference, "{what}: the canonical bytes differ");
    // The load-time budget counts the same length from the records as the model lists them.
    let counted = super::graph::unsorted_graph(&engine.model, &engine.io)
        .and_then(|graph| super::encode::length(&graph))
        .expect("counted");
    assert_eq!(counted, bytes.len(), "{what}: the counted length differs");
    let id = engine.content_id().expect("id");
    assert_eq!(id.digest(), &sha256(&reference), "{what}: the id differs");
    true
}

#[test]
fn every_fixture_that_loads_spells_the_reference_bytes() {
    let mut loaded = 0;
    for path in fixtures() {
        let document = std::fs::read(&path).expect("readable");
        loaded += usize::from(agrees(&document, &path.display().to_string()));
    }
    // The tracked fixtures that load today; a fixture added or removed moves this count.
    assert!(loaded >= 100, "only {loaded} fixtures loaded");
}

const T: &str = "http://example.org#Buildings.Controls.OBC.CDL.";

fn port(id: &str, kind: &str, wired: &[String]) -> String {
    let targets = if wired.is_empty() {
        String::new()
    } else {
        let items: Vec<String> = wired
            .iter()
            .map(|target| format!("{{ \"@id\": \"{target}\" }}"))
            .collect();
        format!(", \"S231:isConnectedTo\": [ {} ]", items.join(", "))
    };
    format!(
        "{{ \"@id\": \"{id}\", \"@type\": \"S231:Real{kind}\", \"S231:isOfDataType\": \
         {{ \"@id\": \"S231:Real\" }}{targets} }}"
    )
}

/// A document of `n` `Abs` blocks named under three namespaces whose last segments collide, a
/// `MultiSum` fed from every one of them, a boundary input fanned out to them, a second passed
/// straight through to `n` outputs, a `Constant` driving every output beside, and an inline
/// composite whose boundary input feeds a block inside it.
fn generated(n: usize) -> Vec<u8> {
    let mut nodes = Vec::new();
    let mut blocks = Vec::new();
    let mut outputs = Vec::new();
    // Two blocks per namespace pair end in the same relative name, so their names stay whole.
    let namespaces = ["urn:a:", "http://b.example#", "http://c.example/"];
    let abs: Vec<String> = (0..n)
        .map(|i| format!("{}seq.abs{}", namespaces[i % 3], i / 2))
        .collect();
    let sum_inputs: Vec<String> = (1..=n).map(|i| format!("urn:s:sum.u{i}")).collect();
    for (block, sum_input) in abs.iter().zip(&sum_inputs) {
        nodes.push(format!(
            "{{ \"@id\": \"{block}\", \"@type\": \"{T}Reals.Abs\", \"S231:hasInput\": \
             {{ \"@id\": \"{block}.u\" }}, \"S231:hasOutput\": {{ \"@id\": \"{block}.y\" }} }}"
        ));
        nodes.push(port(&format!("{block}.u"), "Input", &[]));
        nodes.push(port(
            &format!("{block}.y"),
            "Output",
            std::slice::from_ref(sum_input),
        ));
        blocks.push(block.clone());
    }
    let listed: Vec<String> = sum_inputs
        .iter()
        .map(|input| format!("{{ \"@id\": \"{input}\" }}"))
        .collect();
    nodes.push(format!(
        "{{ \"@id\": \"urn:s:sum\", \"@type\": \"{T}Reals.MultiSum\", \"S231:hasParameter\": \
         {{ \"@id\": \"urn:s:sum.nin\" }}, \"S231:hasInput\": [ {} ], \"S231:hasOutput\": \
         {{ \"@id\": \"urn:s:sum.y\" }} }}",
        listed.join(", ")
    ));
    nodes.push(format!(
        "{{ \"@id\": \"urn:s:sum.nin\", \"S231:value\": {n} }}"
    ));
    for input in &sum_inputs {
        nodes.push(port(input, "Input", &[]));
    }
    let driven: Vec<String> = (0..n).map(|i| format!("urn:top:out{i}")).collect();
    nodes.push(port("urn:s:sum.y", "Output", &driven));
    blocks.push("urn:s:sum".to_owned());
    let passed: Vec<String> = (0..n).map(|i| format!("urn:top:pass{i}")).collect();
    for output in driven.iter().chain(&passed) {
        nodes.push(port(output, "Output", &[]));
        outputs.push(output.clone());
    }
    let fed: Vec<String> = abs.iter().map(|block| format!("{block}.u")).collect();
    nodes.push(port("urn:top:u", "Input", &fed));
    nodes.push(port("urn:top:through", "Input", &passed));
    nodes.push(port(
        "urn:top:inner",
        "Input",
        &["urn:top:sub.u".to_owned()],
    ));
    nodes.push(
        "{ \"@id\": \"urn:top:sub\", \"@type\": \"urn:Sub\", \"S231:containsBlock\": \
         [ { \"@id\": \"urn:top:sub.gain\" } ], \"S231:hasInput\": \
         { \"@id\": \"urn:top:sub.u\" } }"
            .to_owned(),
    );
    nodes.push(port(
        "urn:top:sub.u",
        "Input",
        &["urn:top:sub.gain.u".to_owned()],
    ));
    nodes.push(format!(
        "{{ \"@id\": \"urn:top:sub.gain\", \"@type\": \"{T}Reals.Abs\", \"S231:hasInput\": \
         {{ \"@id\": \"urn:top:sub.gain.u\" }}, \"S231:hasOutput\": {{ \"@id\": \"urn:top:sub.gain.y\" }} }}"
    ));
    nodes.push(port("urn:top:sub.gain.u", "Input", &[]));
    nodes.push(port("urn:top:sub.gain.y", "Output", &[]));
    blocks.push("urn:top:sub".to_owned());
    let list = |ids: &[String]| {
        ids.iter()
            .map(|id| format!("{{ \"@id\": \"{id}\" }}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let inputs = ["urn:top:u", "urn:top:through", "urn:top:inner"].map(str::to_owned);
    format!(
        "{{ \"@context\": {{ \"S231\": \"http://data.ashrae.org/S231#\" }}, \"@graph\": [ \
         {{ \"@id\": \"urn:top\", \"@type\": \"S231:Block\", \"S231:containsBlock\": [ {} ], \
         \"S231:hasInput\": [ {} ], \"S231:hasOutput\": [ {} ] }},\n{} ] }}",
        list(&blocks),
        list(&inputs),
        list(&outputs),
        nodes.join(",\n")
    )
    .into_bytes()
}

#[test]
fn generated_graphs_spell_the_reference_bytes() {
    for n in [1, 2, 3, 7, 16, 33] {
        assert!(
            agrees(&generated(n), &format!("{n} blocks")),
            "{n} blocks load"
        );
    }
}

#[test]
fn a_port_listed_twice_or_by_another_block_is_found_where_its_owner_first_lists_it() {
    // A hand-built graph no load produces: the MultiSum lists input 1 twice and the Abs, which
    // does not own it, lists input 2 too; the Constant lists its output twice. Each port is its
    // owner's first listing of it, as searching the owner's list found it.
    use oce_model::{
        BlockId, BlockInstance, Connection, Connector, ConnectorId, Dir, ModelGraph, ParamTable,
        ValueType,
    };
    use std::sync::Arc;
    let block = |id: u32, class: &str, iri: &str, inputs: &[u32], outputs: &[u32]| BlockInstance {
        id: BlockId(id),
        class_iri: Arc::from(class),
        inputs: inputs.iter().copied().map(ConnectorId).collect(),
        outputs: outputs.iter().copied().map(ConnectorId).collect(),
        params: ParamTable::default(),
        decl_order: id,
        instance_iri: Some(Arc::from(iri)),
    };
    let connector = |id: u32, block: u32, dir: Dir| {
        Connector::new(ConnectorId(id), BlockId(block), dir, ValueType::Real, id)
    };
    let model = ModelGraph {
        blocks: vec![
            block(0, "CDL.Reals.Abs", "urn:t:x", &[2], &[]),
            block(1, "CDL.Reals.Sources.Constant", "urn:t:a", &[], &[0, 0]),
            block(2, "CDL.Reals.MultiSum", "urn:t:b", &[1, 2, 1], &[]),
        ],
        connectors: vec![
            connector(0, 1, Dir::Out),
            connector(1, 2, Dir::In),
            connector(2, 2, Dir::In),
        ],
        connections: vec![
            Connection {
                from: ConnectorId(0),
                to: ConnectorId(1),
            },
            Connection {
                from: ConnectorId(0),
                to: ConnectorId(2),
            },
        ],
        ..ModelGraph::new()
    };
    let io = crate::io::IoInventory::build_at_load(&model);
    let graph = super::graph::canonical_graph(&model, &io).expect("a graph");
    let ports: Vec<(&str, u32, &str, u32)> = graph
        .connections
        .iter()
        .map(|edge| {
            (
                edge.target.text,
                edge.input_port,
                edge.source.text,
                edge.output_port,
            )
        })
        .collect();
    assert_eq!(ports, [("b", 0, "a", 0), ("b", 1, "a", 0)]);
    assert_eq!(
        super::encode::encode(&graph).expect("bytes"),
        reference_bytes(&model, &io).expect("reference")
    );
}
