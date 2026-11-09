//! The different-id classes over the whole corpus (`docs/design/content-id.md`, "Permutation
//! classes" and the test plan's one-change matrix): for each of the 47 documents, every
//! parameter's one-bit (or ±1, flipped, other-member) edit, a block, boundary and sequence rename,
//! a rewire, a same-arity class swap, a boundary `quantity`, `unit` and `displayUnit` change, an
//! input bound tightened and a parameter binding removed each move the canonical content id, and
//! no two variants of one document share an id.

#[path = "support/content_id_corpus.rs"]
mod corpus;

use std::collections::{BTreeMap, BTreeSet};

use corpus::*;
use oce_api::{ContentId, Engine, Value};
use serde_json::Value as Json;

/// The other values one parameter edit tries, in order, until `set_param` accepts one.
fn edits(value: &Value) -> Vec<Value> {
    match value {
        Value::Real(x) => {
            let bits = x.to_bits();
            let mut out = vec![
                Value::Real(f64::from_bits(bits ^ 1)),
                Value::Real(f64::from_bits(bits ^ 2)),
            ];
            if *x == 0.0 {
                // `+0.0` against `-0.0`: equal under `==`, different bits, different ids.
                out.insert(0, Value::Real(-*x));
            }
            out
        }
        Value::Integer(n) => vec![Value::Integer(n + 1), Value::Integer(n - 1)],
        Value::Boolean(b) => vec![Value::Boolean(!b)],
        Value::Enum { class, ordinal } => {
            let members = oce_model::enum_descriptor(*class).unwrap().members.len() as u32;
            vec![Value::Enum {
                class: *class,
                ordinal: ordinal % members + 1,
            }]
        }
        Value::String(text) => vec![Value::String(format!("{text}x").into())],
    }
}

/// Apply one parameter value through the halt / set / resume lifecycle; `false` when refused.
fn try_set(engine: &mut Engine, path: &str, value: Value) -> bool {
    engine.halt().unwrap();
    let accepted = engine.set_param(path, value).is_ok();
    engine.resume().unwrap();
    accepted
}

#[test]
fn every_parameter_edit_moves_the_id_and_setting_it_back_restores_it() {
    let (mut edited, mut zero_signs, mut structural) = (0, 0, 0);
    for name in corpus() {
        let mut engine = load(&fixture_bytes(&name));
        let original = engine.content_id().unwrap();
        let mut seen: BTreeMap<ContentId, String> = BTreeMap::new();
        seen.insert(original.clone(), "original".into());
        for (path, value, _) in engine.params().to_vec() {
            let candidates = edits(&value);
            let signed_zero = matches!(value, Value::Real(x) if x == 0.0);
            let mut applied = false;
            for (index, candidate) in candidates.into_iter().enumerate() {
                if !try_set(&mut engine, &path, candidate) {
                    continue;
                }
                let id = engine.content_id().unwrap();
                if let Some(other) = seen.insert(id, path.clone()) {
                    panic!("{name}: editing `{path}` gives the id of `{other}`");
                }
                if signed_zero && index == 0 {
                    zero_signs += 1;
                }
                applied = true;
                assert!(
                    try_set(&mut engine, &path, value.clone()),
                    "{name}: `{path}` resets"
                );
                assert_eq!(
                    engine.content_id().unwrap(),
                    original,
                    "{name}: `{path}` reset"
                );
                break;
            }
            if applied {
                edited += 1;
            } else {
                // Only the structural widths are refused (a width edit needs a fresh load).
                assert!(
                    path.ends_with(".nin") || path.ends_with(".nout"),
                    "{name}: `{path}` refused every edit"
                );
                structural += 1;
            }
        }
    }
    // Every bound parameter of the corpus but the 37 structural widths (`nin`, `nout`, which
    // `set_param` refuses) is edited, 120 of them from `+0.0` to `-0.0`; the counts pin the reach.
    assert_eq!((edited, zero_signs, structural), (837, 120, 37));
}

/// The `@id`s of a reference or reference list.
fn refs(value: Option<&Json>) -> Vec<String> {
    match value {
        Some(Json::Array(items)) => items
            .iter()
            .filter_map(|item| item["@id"].as_str().map(str::to_owned))
            .collect(),
        Some(item @ Json::Object(_)) => item["@id"]
            .as_str()
            .map(str::to_owned)
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}

fn node<'a>(document: &'a Json, id: &str) -> Option<&'a Json> {
    document["@graph"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["@id"] == id)
}

fn node_mut<'a>(document: &'a mut Json, id: &str) -> &'a mut Json {
    document["@graph"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|node| node["@id"] == id)
        .unwrap()
}

/// Rename `from` and every IRI under it (`from.` prefix) to `to`.
fn rename(document: &Json, from: &str, to: &str) -> Json {
    rewrite_ids(document, &|id: &str| {
        if id == from {
            to.to_owned()
        } else if let Some(rest) = id.strip_prefix(&format!("{from}.")) {
            format!("{to}.{rest}")
        } else {
            id.to_owned()
        }
    })
}

/// The root composite: the node with `containsBlock` that no `containsBlock` names.
fn root(document: &Json) -> String {
    let graph = document["@graph"].as_array().unwrap();
    let contained: BTreeSet<String> = graph
        .iter()
        .flat_map(|node| refs(node.get("S231:containsBlock")))
        .collect();
    graph
        .iter()
        .filter(|node| node.get("S231:containsBlock").is_some())
        .map(|node| node["@id"].as_str().unwrap().to_owned())
        .find(|id| !contained.contains(id))
        .unwrap()
}

/// Swap the drivers of two same-typed inputs of one leaf block that two different sources drive.
fn rewire(document: &Json) -> Option<Json> {
    let graph = document["@graph"].as_array().unwrap();
    let mut driver: BTreeMap<String, String> = BTreeMap::new();
    for node in graph {
        for target in refs(node.get("S231:isConnectedTo")) {
            driver.insert(target, node["@id"].as_str().unwrap().to_owned());
        }
    }
    for block in graph.iter().filter(|node| !is_composite(node)) {
        let inputs = refs(block.get("S231:hasInput"));
        for (index, first) in inputs.iter().enumerate() {
            for second in &inputs[index + 1..] {
                let (Some(a), Some(b)) = (driver.get(first), driver.get(second)) else {
                    continue;
                };
                let kind = |id: &str| node(document, id).map(|n| n["@type"].clone());
                if a == b || kind(first).is_none() || kind(first) != kind(second) {
                    continue;
                }
                let mut changed = document.clone();
                for (source, from, to) in [(a, first, second), (b, second, first)] {
                    let source = node_mut(&mut changed, source);
                    let edges = source.get_mut("S231:isConnectedTo").unwrap();
                    let swap = |edge: &mut Json| {
                        if edge["@id"] == from.as_str() {
                            edge["@id"] = Json::String(to.clone());
                        }
                    };
                    match edges {
                        Json::Array(items) => items.iter_mut().for_each(swap),
                        other => swap(other),
                    }
                }
                return Some(changed);
            }
        }
    }
    None
}

/// Same-arity class swaps with compatible parameters, tried in order.
const SWAPS: &[(&str, &str)] = &[
    ("CDL.Reals.Subtract", "CDL.Reals.Add"),
    ("CDL.Reals.Add", "CDL.Reals.Subtract"),
    ("CDL.Logical.And", "CDL.Logical.Or"),
    ("CDL.Logical.Or", "CDL.Logical.And"),
    ("CDL.Reals.Max", "CDL.Reals.Min"),
    ("CDL.Reals.Min", "CDL.Reals.Max"),
    ("CDL.Reals.Multiply", "CDL.Reals.Add"),
    ("CDL.Reals.GreaterThreshold", "CDL.Reals.LessThreshold"),
    ("CDL.Reals.LessThreshold", "CDL.Reals.GreaterThreshold"),
    ("CDL.Reals.Greater", "CDL.Reals.Less"),
    ("CDL.Integers.Add", "CDL.Integers.Subtract"),
    ("CDL.Logical.Nor", "CDL.Logical.Or"),
    ("CDL.Logical.Xor", "CDL.Logical.Or"),
];

fn swap_class(document: &Json) -> Option<Json> {
    for (from, to) in SWAPS {
        let suffix = format!("#Buildings.Controls.OBC.{from}");
        let mut changed = document.clone();
        if let Some(block) = changed["@graph"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|node| node["@type"].as_str().is_some_and(|t| t.ends_with(&suffix)))
        {
            let class = block["@type"]
                .as_str()
                .unwrap()
                .replace(&suffix, &format!("#Buildings.Controls.OBC.{to}"));
            block["@type"] = Json::String(class);
            return Some(changed);
        }
    }
    None
}

/// The root's Real boundary input declarations, in `hasInput` order.
fn real_boundary_inputs(document: &Json) -> Vec<String> {
    let root = root(document);
    refs(node(document, &root).unwrap().get("S231:hasInput"))
        .into_iter()
        .filter(|id| {
            node(document, id).is_some_and(|n| n["S231:isOfDataType"]["@id"] == "S231:Real")
        })
        .collect()
}

/// The first variant from `attempts` that loads, as its id.
fn first_loading(attempts: impl IntoIterator<Item = Json>) -> Option<ContentId> {
    attempts
        .into_iter()
        .find_map(|document| try_load(&to_bytes(&document)))
        .map(|engine| engine.content_id().unwrap())
}

#[test]
fn every_name_wiring_class_and_boundary_change_moves_the_id() {
    let mut covered: BTreeMap<&str, usize> = BTreeMap::new();
    for name in corpus() {
        let document = fixture_json(&name);
        let engine = load(&fixture_bytes(&name));
        let mut ids: BTreeMap<ContentId, &str> = BTreeMap::new();
        ids.insert(engine.content_id().unwrap(), "original");
        let mut record = |change: &'static str, id: Option<ContentId>| {
            let Some(id) = id else { return };
            if let Some(other) = ids.insert(id, change) {
                panic!("{name}: `{change}` gives the id of `{other}`");
            }
            *covered.entry(change).or_default() += 1;
        };

        let block = engine
            .topology()
            .blocks
            .iter()
            .map(|b| b.instance_path.clone())
            .min()
            .unwrap();
        record(
            "block renamed",
            first_loading([rename(&document, &block, &format!("{block}Renamed"))]),
        );
        let definitions = engine.input_definitions().unwrap();
        let input = definitions.iter().map(|d| d.path.clone()).min();
        if let Some(input) = input {
            record(
                "boundary renamed",
                first_loading([rename(&document, &input, &format!("{input}Renamed"))]),
            );
        }
        let sequence = root(&document);
        record(
            "sequence renamed",
            first_loading([rename(&document, &sequence, &format!("{sequence}Renamed"))]),
        );
        record(
            "connection rewired",
            rewire(&document).and_then(|d| first_loading([d])),
        );
        record(
            "class swapped",
            swap_class(&document).and_then(|d| first_loading([d])),
        );
        let boundaries = real_boundary_inputs(&document);
        for (change, key, value) in [
            (
                "boundary quantity changed",
                "S231:quantity",
                "ChangedQuantity",
            ),
            ("boundary unit changed", "S231:unit", "changedUnit"),
            (
                "boundary displayUnit changed",
                "S231:displayUnit",
                "changedDisplay",
            ),
        ] {
            record(
                change,
                first_loading(boundaries.iter().map(|id| {
                    let mut changed = document.clone();
                    node_mut(&mut changed, id)[key] = Json::String(value.to_owned());
                    changed
                })),
            );
        }
        record(
            "input bound tightened",
            first_loading(boundaries.iter().filter_map(|id| {
                let definition = definitions.iter().find(|d| d.path == *id)?;
                let min = definition.min.as_ref().and_then(|v| v.as_real().ok());
                let max = definition.max.as_ref().and_then(|v| v.as_real().ok());
                let tightened = min.map_or(-1.0e30, f64::next_up);
                if max.is_some_and(|max| tightened > max) {
                    return None;
                }
                let mut changed = document.clone();
                node_mut(&mut changed, id)["S231:min"] = serde_json::json!({
                    "@value": format!("{tightened:?}"),
                    "@type": "http://www.w3.org/2001/XMLSchema#double"
                });
                Some(changed)
            })),
        );
        let surviving: BTreeSet<String> = engine
            .topology()
            .blocks
            .iter()
            .map(|block| block.instance_path.clone())
            .collect();
        let leaves: Vec<(String, Vec<String>)> = document["@graph"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|n| !is_composite(n))
            .map(|n| {
                (
                    n["@id"].as_str().unwrap().to_owned(),
                    refs(n.get("S231:hasParameter")),
                )
            })
            // A conditionally removed block is not in the loaded graph; only surviving leaves.
            .filter(|(id, params)| !params.is_empty() && surviving.contains(id))
            .collect();
        record(
            "binding removed",
            first_loading(leaves.iter().flat_map(|(id, params)| {
                (0..params.len()).map(|skip| {
                    let mut changed = document.clone();
                    let kept: Vec<Json> = params
                        .iter()
                        .enumerate()
                        .filter(|(index, _)| *index != skip)
                        .map(|(_, param)| serde_json::json!({ "@id": param }))
                        .collect();
                    node_mut(&mut changed, id)["S231:hasParameter"] = Json::Array(kept);
                    changed
                })
            })),
        );
    }
    // Each change applies wherever the document has what it changes: 7 fixed-limit fixtures have
    // no boundary input, 2 more have no Real one, and a rewire, a listed class swap or a
    // removable (not required) binding needs a block that offers one. The reach is pinned.
    let expected: BTreeMap<&str, usize> = [
        ("binding removed", 32),
        ("block renamed", 47),
        ("boundary displayUnit changed", 38),
        ("boundary quantity changed", 38),
        ("boundary renamed", 40),
        ("boundary unit changed", 38),
        ("class swapped", 32),
        ("connection rewired", 34),
        ("input bound tightened", 38),
        ("sequence renamed", 47),
    ]
    .into_iter()
    .collect();
    assert_eq!(covered, expected);
}
