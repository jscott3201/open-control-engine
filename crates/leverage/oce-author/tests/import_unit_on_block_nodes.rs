//! A unit spelling on a node that carries no unit, the root composite or a block instance, is
//! `unknown-key` in the CXF importer, under every spelling the importer reads elsewhere (issue
//! #631; unit-spellings note `docs/design/unit-spellings.md`, decision 6).
//!
//! A unit is read on a boundary and checked on a parameter (`import_unit_spellings.rs`). On the
//! root or a block node no spelling is in the whitelist, the QUDT one included where the document
//! binds `qudt`: the importer must refuse the key by name, never accept it and drop the value.
//!
//! The engine's verdict is pinned beside each case. It is a standing difference, the engine the
//! more lenient: the engine moves the key into the node's modeled `unit` or `displayUnit`, reads
//! neither on a node that is no connector and no parameter, and loads the document to the content
//! id it has without the key.

use oce_api::Engine;
use oce_author::{ImportErrorKind, ImportOptions, import_cxf};
use serde_json::{Value as Json, json};

use crate::support::cxf::identity;
use crate::support::fixture;

const S231: &str = "http://data.ashrae.org/S231#";
const QUDT_HASH: &str = "http://qudt.org/schema/qudt#";
const QUDT_SLASH: &str = "http://qudt.org/schema/qudt/";
const UNIT_HASH: &str = "http://qudt.org/vocab/unit#";

/// The root composite of the every-element sequence, and one of its block instances.
const ROOT: &str = "urn:oce:author:every_element";
const BLOCK: &str = "urn:oce:author:every_element.conPID";

/// Every key that spells a unit or a display unit on a boundary or a parameter.
const KEYS: [&str; 5] = [
    "S231:unit",
    "S231:hasUnit",
    "qudt:hasUnit",
    "S231:displayUnit",
    "S231:hasDisplayUnit",
];

/// The every-element golden under the `@context` modelica-json writes, with `qudt` bound to
/// `schema`.
fn document(schema: &str) -> Json {
    let text = std::fs::read_to_string(fixture("emit/every_element.cxf.json")).expect("golden");
    let mut doc: Json = serde_json::from_str(&text).expect("golden parses");
    doc["@context"] = json!({ "S231": S231, "qudt": schema, "unit": UNIT_HASH });
    doc
}

/// `doc` with `key`, as written, set to `value` on the node `id`.
fn with(mut doc: Json, id: &str, key: &str, value: Json) -> Json {
    let node = doc["@graph"]
        .as_array_mut()
        .expect("@graph")
        .iter_mut()
        .find(|node| node["@id"] == id)
        .unwrap_or_else(|| panic!("no node `{id}`"));
    node[key] = value;
    doc
}

/// The importer's refusal of `doc`: its subject and kind.
fn refusal(doc: &Json) -> (String, ImportErrorKind) {
    let bytes = serde_json::to_vec(doc).expect("serialize");
    match import_cxf(&bytes, &ImportOptions::new()) {
        Err(error) => (error.subject, error.kind),
        Ok(_) => panic!("expected a refusal, but it imported"),
    }
}

/// The content id the engine loads `doc` to; it must load.
fn engine_identity(doc: &Json) -> String {
    let mut engine = Engine::new();
    engine
        .load_cxf(&serde_json::to_vec(doc).expect("serialize"))
        .unwrap_or_else(|error| panic!("the engine loads it: {error}"));
    identity(&engine)
}

#[test]
fn the_golden_under_the_upstream_context_imports() {
    for schema in [QUDT_HASH, QUDT_SLASH] {
        let bytes = serde_json::to_vec(&document(schema)).expect("serialize");
        import_cxf(&bytes, &ImportOptions::new()).expect("the golden imports");
    }
}

#[test]
fn every_unit_spelling_on_the_root_or_a_block_node_is_an_unknown_key_naming_it() {
    for schema in [QUDT_HASH, QUDT_SLASH] {
        let without = engine_identity(&document(schema));
        for node in [ROOT, BLOCK] {
            for key in KEYS {
                // A string, a mapped unit IRI and an unmapped one: the key is refused, whatever
                // its value.
                for value in [
                    json!("K"),
                    json!({ "@id": "unit:K" }),
                    json!({ "@id": "unit:FOO" }),
                ] {
                    let doc = with(document(schema), node, key, value.clone());
                    assert_eq!(
                        refusal(&doc),
                        (
                            node.to_owned(),
                            ImportErrorKind::UnknownKey {
                                key: key.to_owned()
                            }
                        ),
                        "{schema} {node} {key} {value}"
                    );
                    // The engine reads no unit on such a node: it loads, and to the content id
                    // of the document without the key.
                    assert_eq!(
                        engine_identity(&doc),
                        without,
                        "{schema} {node} {key} {value}"
                    );
                }
            }
        }
    }
}

#[test]
fn two_unit_spellings_on_a_block_node_are_refused_by_both_readers() {
    // The importer names the first key outside the whitelist, in key order. The engine refuses
    // the second spelling wherever it sits, read or not.
    for node in [ROOT, BLOCK] {
        let doc = with(
            with(document(QUDT_HASH), node, "S231:hasUnit", json!("K")),
            node,
            "qudt:hasUnit",
            json!({ "@id": "unit:K" }),
        );
        assert_eq!(
            refusal(&doc),
            (
                node.to_owned(),
                ImportErrorKind::UnknownKey {
                    key: "S231:hasUnit".to_owned()
                }
            ),
            "{node}"
        );
        let mut engine = Engine::new();
        let error = engine
            .load_cxf(&serde_json::to_vec(&doc).expect("serialize"))
            .expect_err("the engine refuses two spellings");
        let refused: Vec<_> = error
            .all_diagnostics()
            .filter(|d| d.is_error())
            .map(|d| (d.code.as_str(), d.subject.as_deref()))
            .collect();
        assert_eq!(refused, [("malformed-document", Some(node))], "{node}");
    }
}
