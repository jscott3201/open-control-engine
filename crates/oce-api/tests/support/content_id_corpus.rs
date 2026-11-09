//! The canonical content id corpus shared by the `content_id_*` test binaries: the 46 G36 catalog
//! fixtures and the resolver-contract document `member_list_interface.jsonld`, loaders, and the
//! enumerated JSON-LD rewrites the permutation classes apply (`docs/design/content-id.md`,
//! "Permutation classes"). Every rewrite is a pure function of the document; nothing is random.

#![allow(dead_code)]

use std::path::PathBuf;

use oce_api::{ContentId, Engine};
use serde_json::Value;

/// The G36 fixture directory, relative to this crate.
pub(crate) fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../oce-cxf/tests/fixtures/g36")
}

/// The 47 corpus documents by stem, sorted: 46 catalog fixtures and `member_list_interface`.
pub(crate) fn corpus() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(fixture_dir())
        .unwrap()
        .filter_map(|entry| {
            let name = entry.unwrap().file_name().into_string().unwrap();
            name.strip_suffix(".jsonld").map(str::to_owned)
        })
        .collect();
    names.sort_unstable();
    assert_eq!(
        names.len(),
        47,
        "46 catalog fixtures plus member_list_interface"
    );
    names
}

/// The bytes of corpus document `name`.
pub(crate) fn fixture_bytes(name: &str) -> Vec<u8> {
    std::fs::read(fixture_dir().join(format!("{name}.jsonld"))).unwrap()
}

/// The parsed JSON of corpus document `name`.
pub(crate) fn fixture_json(name: &str) -> Value {
    serde_json::from_slice(&fixture_bytes(name)).unwrap()
}

/// Load `bytes`, or `None` when the engine refuses them.
pub(crate) fn try_load(bytes: &[u8]) -> Option<Engine> {
    let mut engine = Engine::new();
    engine.load_cxf(bytes).ok().map(|_| engine)
}

/// Load `bytes`, which must load.
pub(crate) fn load(bytes: &[u8]) -> Engine {
    let mut engine = Engine::new();
    engine
        .load_cxf(bytes)
        .unwrap_or_else(|error| panic!("document refused: {error:?}"));
    engine
}

/// The canonical content id of `bytes`, which must load.
pub(crate) fn id_of(bytes: &[u8]) -> ContentId {
    load(bytes).content_id().unwrap()
}

/// Serialize `value` as compact JSON.
pub(crate) fn to_bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).unwrap()
}

fn graph_mut(document: &mut Value) -> &mut Vec<Value> {
    document["@graph"].as_array_mut().unwrap()
}

/// Reverse the top-level `@graph` array.
pub(crate) fn reverse_graph(document: &Value) -> Value {
    let mut document = document.clone();
    graph_mut(&mut document).reverse();
    document
}

/// Rotate the top-level `@graph` array left by `by` nodes (modulo its length).
pub(crate) fn rotate_graph(document: &Value, by: usize) -> Value {
    let mut document = document.clone();
    let graph = graph_mut(&mut document);
    let length = graph.len();
    graph.rotate_left(by % length);
    document
}

/// Reverse the array value of `key` on every node of `@graph` whose `keep` predicate holds.
pub(crate) fn reverse_relation(
    document: &Value,
    key: &str,
    keep: impl Fn(&Value) -> bool,
) -> Value {
    let mut document = document.clone();
    for node in graph_mut(&mut document) {
        if !keep(node) {
            continue;
        }
        if let Some(array) = node.get_mut(key).and_then(Value::as_array_mut) {
            array.reverse();
        }
    }
    document
}

/// Whether `node` is a composite: it lists `containsBlock` members (a G36 root is typed by its
/// sequence class, not `S231:Block`).
pub(crate) fn is_composite(node: &Value) -> bool {
    node.get("S231:containsBlock").is_some()
}

/// Every object key in reverse order, re-serialized compactly with no whitespace.
pub(crate) fn reverse_keys(value: &Value) -> Vec<u8> {
    fn walk(value: &Value, out: &mut String) {
        match value {
            Value::Object(map) => {
                out.push('{');
                for (index, (key, value)) in map.iter().rev().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    out.push_str(&serde_json::to_string(key).unwrap());
                    out.push(':');
                    walk(value, out);
                }
                out.push('}');
            }
            Value::Array(array) => {
                out.push('[');
                for (index, value) in array.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    walk(value, out);
                }
                out.push(']');
            }
            other => out.push_str(&serde_json::to_string(other).unwrap()),
        }
    }
    let mut out = String::new();
    walk(value, &mut out);
    out.into_bytes()
}

/// Rewrite every instance `@id` through `map`: node subjects, members and every reference to
/// them. Class and datatype references (`@type`, the `@id` of an `isOfDataType` enumeration) are
/// not instance IRIs and stay as authored, as does `@context`.
pub(crate) fn rewrite_ids(document: &Value, map: &dyn Fn(&str) -> String) -> Value {
    fn walk(value: &mut Value, map: &dyn Fn(&str) -> String) {
        match value {
            Value::Object(object) => {
                for (key, value) in object.iter_mut() {
                    match key.as_str() {
                        "@id" => {
                            if let Value::String(text) = value {
                                *text = map(text);
                            }
                        }
                        "@context" | "@type" | "S231:isOfDataType" => {}
                        _ => walk(value, map),
                    }
                }
            }
            Value::Array(array) => array.iter_mut().for_each(|value| walk(value, map)),
            _ => {}
        }
    }
    let mut document = document.clone();
    walk(&mut document, map);
    document
}

/// The authored instance namespace every corpus document uses.
pub(crate) const NAMESPACE: &str = "http://example.org#";

/// Move every instance `@id` from [`NAMESPACE`] to `namespace`.
pub(crate) fn move_namespace(document: &Value, namespace: &str) -> Value {
    rewrite_ids(document, &|id: &str| match id.strip_prefix(NAMESPACE) {
        Some(rest) => format!("{namespace}{rest}"),
        None => id.to_owned(),
    })
}

/// Spell every instance `@id` as a compact IRI against the document's `base` prefix.
pub(crate) fn compact_ids(document: &Value) -> Value {
    assert_eq!(document["@context"]["base"], NAMESPACE);
    rewrite_ids(document, &|id: &str| match id.strip_prefix(NAMESPACE) {
        Some(rest) => format!("base:{rest}"),
        None => id.to_owned(),
    })
}
