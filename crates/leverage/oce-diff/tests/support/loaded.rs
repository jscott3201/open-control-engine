//! Helpers for the CXF-level tests: loading a document into a throwaway engine, emitting an
//! authoring model, the content-id corpus and the re-spellings of a document.

use std::path::PathBuf;

use oce_api::{ContentId, Engine, Topology};
use oce_author::{EmitOptions, Sequence, emit};
use oce_diff::{Diff, DiffError, DiffOptions, diff_topology};
use serde_json::Value;

/// A document loaded into its own engine, with its topology and content id.
pub struct Loaded {
    /// The engine the document is loaded into.
    pub engine: Engine,
    /// Its topology view.
    pub topology: Topology,
    /// Its canonical content id.
    pub id: ContentId,
}

/// Load CXF bytes, or say why not.
pub fn try_load(bytes: &[u8]) -> Result<Loaded, String> {
    let mut engine = Engine::new();
    engine
        .load_cxf(bytes)
        .map_err(|error| format!("{error:?}"))?;
    let id = engine.content_id().map_err(|error| format!("{error:?}"))?;
    let topology = engine.topology();
    Ok(Loaded {
        engine,
        topology,
        id,
    })
}

/// Load CXF bytes that must load.
pub fn load(bytes: &[u8]) -> Loaded {
    try_load(bytes).unwrap_or_else(|error| panic!("the document loads: {error}"))
}

/// The emitted CXF of an authoring model under `namespace`.
pub fn emitted_under(sequence: &Sequence, namespace: &str) -> Vec<u8> {
    let options = EmitOptions::default().with_namespace(namespace);
    emit(sequence, &options).unwrap_or_else(|error| panic!("the model emits: {error}"))
}

/// The emitted CXF of an authoring model under the default namespace.
pub fn emitted(sequence: &Sequence) -> Vec<u8> {
    emit(sequence, &EmitOptions::default())
        .unwrap_or_else(|error| panic!("the model emits: {error}"))
}

/// An authoring model emitted and loaded.
pub fn loaded(sequence: &Sequence) -> Loaded {
    load(&emitted(sequence))
}

/// The CXF diff with the default options, which must not refuse.
pub fn compare_loaded(base: &Loaded, head: &Loaded) -> Diff {
    compare_topologies(&base.topology, &head.topology)
}

/// The CXF diff of two topologies with the default options, which must not refuse.
pub fn compare_topologies(base: &Topology, head: &Topology) -> Diff {
    diff_topology(base, head, &DiffOptions::default())
        .unwrap_or_else(|error| panic!("the topologies diff: {error}"))
}

/// The CXF diff's refusal, which there must be.
pub fn refusal(base: &Topology, head: &Topology) -> DiffError {
    match diff_topology(base, head, &DiffOptions::default()) {
        Ok(diff) => panic!("refused, got\n{}", diff.to_json()),
        Err(error) => error,
    }
}

/// The repository root.
pub fn repository() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// The content-id corpus: the source path of every document the facade's content-id pin lists
/// (`crates/oce-api/tests/fixtures/content_id/pin.tsv`), with its pinned id, in pin order.
pub fn corpus() -> Vec<(String, String)> {
    let pin = repository().join("crates/oce-api/tests/fixtures/content_id/pin.tsv");
    let text = std::fs::read_to_string(&pin)
        .unwrap_or_else(|error| panic!("read {}: {error}", pin.display()));
    text.lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .map(|line| {
            let (id, path) = line.split_once('\t').expect("an id and a path");
            (path.to_owned(), id.to_owned())
        })
        .collect()
}

/// The bytes of a corpus document by its repository-relative path.
pub fn corpus_bytes(path: &str) -> Vec<u8> {
    let full = repository().join(path);
    std::fs::read(&full).unwrap_or_else(|error| panic!("read {}: {error}", full.display()))
}

/// The S231 namespace a compact document abbreviates as `S231:`.
const S231: &str = "http://data.ashrae.org/S231#";

/// The keys whose lists of node references the ordering contract
/// (`docs/cxf-composite-subset.md`, "Ordering and identity contract") lets move without moving
/// the loaded graph: the block order `containsBlock` assigns and the connection order
/// `isConnectedTo` gives are loader order, which the content id ignores. A leaf's `hasInput` and
/// `hasOutput` are not among them: unnamed ports bind by position, so reversing them rewires.
const UNORDERED: [&str; 2] = ["S231:containsBlock", "S231:isConnectedTo"];

/// A document with its `@graph`, its `containsBlock` lists and its `isConnectedTo` lists in
/// reverse order.
pub fn reordered(bytes: &[u8]) -> Vec<u8> {
    let mut document: Value = serde_json::from_slice(bytes).expect("JSON");
    if let Some(graph) = document.get_mut("@graph").and_then(Value::as_array_mut) {
        graph.reverse();
        for node in graph {
            for key in UNORDERED {
                if let Some(list) = node.get_mut(key).and_then(Value::as_array_mut) {
                    list.reverse();
                }
            }
        }
    }
    serde_json::to_vec_pretty(&document).expect("JSON")
}

/// A document with every `S231:` compact IRI (a key, an `@id` or an `@type`) written in full.
pub fn expanded(bytes: &[u8]) -> Vec<u8> {
    let document: Value = serde_json::from_slice(bytes).expect("JSON");
    serde_json::to_vec_pretty(&rewrite(document, &|text| {
        text.strip_prefix("S231:")
            .map(|rest| format!("{S231}{rest}"))
    }))
    .expect("JSON")
}

/// A document with every `@id` and `@type` under `namespace` written as the compact IRI `cx:`
/// and the rest, the prefix `cx` bound to `namespace` in its context.
pub fn compacted(bytes: &[u8], namespace: &str) -> Vec<u8> {
    let document: Value = serde_json::from_slice(bytes).expect("JSON");
    let mut document = rewrite(document, &|text| {
        text.strip_prefix(namespace)
            .map(|rest| format!("cx:{rest}"))
    });
    if let Some(context) = document.get_mut("@context").and_then(Value::as_object_mut) {
        context.insert("cx".to_owned(), Value::String(namespace.to_owned()));
    }
    serde_json::to_vec_pretty(&document).expect("JSON")
}

/// Rewrite every key and every `@id` or `@type` string by `spell`, which returns `None` to keep
/// a string as it is. The `@context` is kept as it is.
fn rewrite(value: Value, spell: &dyn Fn(&str) -> Option<String>) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| {
                    if key == "@context" {
                        return (key, value);
                    }
                    let value = match (&*key, value) {
                        ("@id" | "@type", Value::String(text)) => {
                            Value::String(spell(&text).unwrap_or(text))
                        }
                        (_, value) => rewrite(value, spell),
                    };
                    (spell(&key).unwrap_or(key), value)
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(
            values
                .into_iter()
                .map(|value| rewrite(value, spell))
                .collect(),
        ),
        other => other,
    }
}

/// The instance namespace of a loaded document: its root id up to and including the last `#`,
/// `/` or `:`.
pub fn namespace(topology: &Topology) -> String {
    let root = topology.model_id.as_str();
    let end = root.rfind(['#', '/', ':']).map_or(0, |at| at + 1);
    root[..end].to_owned()
}
