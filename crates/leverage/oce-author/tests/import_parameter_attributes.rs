//! The CXF importer checks the shape of a parameter node's descriptive attributes before it drops
//! them (GitHub issue #559; owner ruling of 2026-11-11). `S231:min` and `S231:max` are a bound: a
//! JSON number, a typed literal or a string. `S231:unit`, `S231:quantity` and `S231:displayUnit`
//! are a term: a string, a typed literal or an IRI node. A typed literal is an object of exactly
//! `@value` and `@type`, both strings; an IRI node one of exactly `@id`, a string. None of those
//! strings may be empty or whitespace-only (owner ruling). Anything else, `null` and a list
//! included, is refused as `wrong-shape` naming the key. A legal one is still dropped unread and
//! reported as written (importer note, decision 9).
//!
//! Each case edits one node of a corpus fixture the importer reads: the sequence parameter
//! `dpBuiSet`, and the block parameter `zerDam.k`.

use crate::support;

use oce_author::{
    DroppedProperty, ImportError, ImportErrorKind, ImportOptions, Imported, SourceForm, import_cxf,
};
use serde_json::{Map, Value, json};

use support::cxf::corpus_fixture;

const SEQUENCE_PARAMETER: &str =
    "http://example.org#g36.source.multizone_vav_relief_damper.dpBuiSet";
const BLOCK_PARAMETER: &str = "http://example.org#g36.source.multizone_vav_relief_damper.zerDam.k";
const DOUBLE: &str = "http://www.w3.org/2001/XMLSchema#double";

/// Every attribute with its key and the dropped property it reports as.
const ATTRIBUTES: [(&str, DroppedProperty); 5] = [
    ("S231:min", DroppedProperty::Min),
    ("S231:max", DroppedProperty::Max),
    ("S231:unit", DroppedProperty::Unit),
    ("S231:quantity", DroppedProperty::Quantity),
    ("S231:displayUnit", DroppedProperty::DisplayUnit),
];

fn document() -> Value {
    let path = corpus_fixture("multizone_vav_relief_damper");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_slice(&bytes).expect("fixture is JSON")
}

fn node_mut<'a>(doc: &'a mut Value, id: &str) -> &'a mut Map<String, Value> {
    doc["@graph"]
        .as_array_mut()
        .expect("@graph array")
        .iter_mut()
        .find(|entry| entry["@id"] == id)
        .and_then(Value::as_object_mut)
        .expect("node present")
}

fn import(doc: &Value) -> Result<Imported, ImportError> {
    let bytes = serde_json::to_vec(doc).expect("serialize");
    import_cxf(&bytes, &ImportOptions::new())
}

/// The fixture with `key` set to `value` on node `id`.
fn with(id: &str, key: &str, value: Value) -> Value {
    let mut doc = document();
    node_mut(&mut doc, id).insert(key.to_owned(), value);
    doc
}

fn is_bound(key: &str) -> bool {
    matches!(key, "S231:min" | "S231:max")
}

/// The legal shapes of `key`.
fn legal(key: &str) -> Vec<Value> {
    let mut shapes = vec![
        json!("k"),
        json!(" k "),
        json!({ "@value": "30", "@type": DOUBLE }),
    ];
    if is_bound(key) {
        shapes.extend([json!(0), json!(-1), json!(30.5)]);
    } else {
        shapes.push(json!({ "@id": "unit:PA" }));
    }
    shapes
}

/// The malformed shapes of `key`.
fn malformed(key: &str) -> Vec<Value> {
    let mut shapes = vec![
        Value::Null,
        json!(true),
        json!([]),
        json!(["30", DOUBLE]),
        json!([{ "@value": "30", "@type": DOUBLE }]),
        json!({}),
        json!({ "@value": "30" }),
        json!({ "@value": 30, "@type": DOUBLE }),
        json!({ "@value": "30", "@type": DOUBLE, "@language": "en" }),
        json!({ "@id": "unit:PA", "@type": DOUBLE }),
        // Owner ruling: no string here is empty or whitespace-only.
        json!(""),
        json!("   "),
        json!("\t"),
        json!({ "@value": "", "@type": DOUBLE }),
        json!({ "@value": " ", "@type": DOUBLE }),
        json!({ "@id": "" }),
    ];
    if is_bound(key) {
        shapes.push(json!({ "@id": "unit:PA" }));
    } else {
        shapes.extend([json!(5), json!(1.5)]);
    }
    shapes
}

#[test]
fn every_legal_shape_is_dropped_unread_and_reported_as_written() {
    let original = import(&document()).expect("the fixture imports");
    for id in [SEQUENCE_PARAMETER, BLOCK_PARAMETER] {
        for (key, property) in ATTRIBUTES {
            for value in legal(key) {
                let imported = import(&with(id, key, value.clone()))
                    .unwrap_or_else(|e| panic!("{id} {key} = {value}: {e}"));
                assert_eq!(imported.sequence, original.sequence, "{id} {key} = {value}");
                let written = match &value {
                    Value::String(text) => text.clone(),
                    other => other.to_string(),
                };
                assert!(
                    imported
                        .report
                        .dropped
                        .iter()
                        .any(|d| d.subject == id && d.property == property && d.value == written),
                    "{id} {key} = {value}: {:?}",
                    imported.report.dropped
                );
            }
        }
    }
}

#[test]
fn every_malformed_shape_is_refused_as_wrong_shape_naming_the_key() {
    for id in [SEQUENCE_PARAMETER, BLOCK_PARAMETER] {
        for (key, _) in ATTRIBUTES {
            for value in malformed(key) {
                let expected = ImportError::new(
                    SourceForm::Cxf,
                    id,
                    ImportErrorKind::WrongShape {
                        key: key.to_owned(),
                    },
                );
                assert_eq!(
                    import(&with(id, key, value.clone())).err(),
                    Some(expected),
                    "{id} {key} = {value}"
                );
            }
        }
    }
}
