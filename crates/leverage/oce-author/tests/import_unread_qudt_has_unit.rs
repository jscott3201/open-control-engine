//! A `hasUnit` key the QUDT schema reading does not reach is refused by both CXF readers (issue
//! #675; owner ruling of 2026-11-20). This is the twin of the engine's
//! `resolve_unread_qudt_has_unit.rs`: each probe is put to the importer and, through the facade,
//! to the engine, so the two are held to one verdict.
//!
//! The importer's `@context` is closed and it reads compact keys only, so it refuses each of
//! these documents as `unknown-key`: naming the key on the node, or the `@context` term on the
//! document. The engine reads any prefix map and any key that expands to a modeled property, and
//! refuses a `hasUnit` key that would otherwise sit unread in its pass-through map:
//!
//! | Document | Engine | Importer |
//! | --- | --- | --- |
//! | `qudt:hasUnit`, no `qudt` binding | `malformed-document` | `unknown-key`, the key |
//! | `qudt` bound to a near miss of a QUDT schema namespace, or to the unit vocabulary | `vocabulary-namespace-mismatch` | `unknown-key`, the term |
//! | `hasUnit` by full IRI in such a namespace | `vocabulary-namespace-mismatch` | `unknown-key`, the key |
//! | any of those only once the key is trimmed | `malformed-document` | `unknown-key`, the key |
//!
//! The codes differ, each reader's own. What is held here is the verdict: neither reader takes a
//! document the other refuses for this reason. Where the engine loads, the importer is the
//! stricter: a `qudt` prefix bound to an unrelated namespace (`import_unit_spellings.rs`), and
//! one bound to a near miss of a QUDT namespace in a document that writes no `hasUnit` key
//! through it, pinned here.

use oce_api::{DiagCode, Engine};
use oce_author::{ImportErrorKind, ImportOptions, import_cxf};
use serde_json::{Value as Json, json};

use crate::support::fixture;

const S231: &str = "http://data.ashrae.org/S231#";
const QUDT_HASH: &str = "http://qudt.org/schema/qudt#";
const UNIT_HASH: &str = "http://qudt.org/vocab/unit#";

/// Every node kind of the every-element sequence: two boundaries, a sequence parameter and a
/// block parameter, a block port, the root composite and a block instance.
const NODES: [&str; 7] = [
    "urn:oce:author:every_element.TSupSet",
    "urn:oce:author:every_element.yGain1",
    "urn:oce:author:every_element.kGain",
    "urn:oce:author:every_element.conPID.yMin",
    "urn:oce:author:every_element.conPID.u_m",
    "urn:oce:author:every_element",
    "urn:oce:author:every_element.conPID",
];

/// A boundary, where both readers read a unit.
const BOUNDARY: &str = NODES[0];

/// The namespaces `hasUnit` is refused in: a near miss of a QUDT schema namespace of every class
/// (`https:`, scheme case, host case, a default or empty port, a missing, repeated or wrong
/// separator), and the QUDT unit vocabulary with near misses of its own.
const UNREAD_NAMESPACES: [&str; 16] = [
    "https://qudt.org/schema/qudt#",
    "https://qudt.org/schema/qudt/",
    "HTTP://qudt.org/schema/qudt#",
    "http://QUDT.org/schema/qudt#",
    "http://QUDT.ORG/schema/qudt/",
    "http://qudt.org:80/schema/qudt#",
    "http://qudt.org:/schema/qudt#",
    "https://qudt.org:443/schema/qudt/",
    "http://qudt.org/schema/qudt",
    "http://qudt.org/schema/qudt/#",
    "http://qudt.org/schema/qudt##",
    "http://qudt.org/vocab/unit#",
    "http://qudt.org/vocab/unit/",
    "https://qudt.org/vocab/unit#",
    "http://Qudt.org/vocab/unit/",
    "http://qudt.org/vocab/unit",
];

/// The every-element golden under `context`.
fn document(context: Json) -> Json {
    let text = std::fs::read_to_string(fixture("emit/every_element.cxf.json")).expect("golden");
    let mut doc: Json = serde_json::from_str(&text).expect("golden parses");
    doc["@context"] = context;
    doc
}

/// The `@context` modelica-json writes with `qudt` left out.
fn unbound_context() -> Json {
    json!({ "S231": S231, "unit": UNIT_HASH })
}

/// The `@context` modelica-json writes, with `qudt` bound to `binding`.
fn context_binding(binding: &str) -> Json {
    json!({ "S231": S231, "qudt": binding, "unit": UNIT_HASH })
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
fn importer_refusal(doc: &Json) -> (String, ImportErrorKind) {
    let bytes = serde_json::to_vec(doc).expect("serialize");
    match import_cxf(&bytes, &ImportOptions::new()) {
        Err(error) => (error.subject, error.kind),
        Ok(_) => panic!("expected a refusal, but it imported"),
    }
}

fn unknown_key(subject: &str, key: &str) -> (String, ImportErrorKind) {
    (
        subject.to_owned(),
        ImportErrorKind::UnknownKey {
            key: key.to_owned(),
        },
    )
}

/// Every error of the engine's refusal of `doc`, as `(code, subject)`; it must refuse.
fn engine_refusal(doc: &Json) -> Vec<(DiagCode, Option<String>)> {
    let mut engine = Engine::new();
    let error = engine
        .load_cxf(&serde_json::to_vec(doc).expect("serialize"))
        .expect_err("the engine refuses it");
    error
        .all_diagnostics()
        .filter(|d| d.is_error())
        .map(|d| (d.code, d.subject.as_deref().map(str::to_owned)))
        .collect()
}

/// The engine's whole refusal: one error of `code` on `node`.
fn engine_refuses(code: DiagCode, node: &str) -> Vec<(DiagCode, Option<String>)> {
    vec![(code, Some(node.to_owned()))]
}

/// A string, a mapped unit IRI and an unmapped one: the key is refused, whatever its value.
fn values() -> [Json; 3] {
    [
        json!("K"),
        json!({ "@id": "unit:K" }),
        json!({ "@id": "unit:FOO" }),
    ]
}

#[test]
fn the_golden_loads_and_imports_under_the_contexts_the_probes_start_from() {
    // The controls: without the probed key, both readers take the document under every context
    // the importer admits here, so each refusal below is the key's.
    for context in [unbound_context(), context_binding(QUDT_HASH)] {
        let bytes = serde_json::to_vec(&document(context)).expect("serialize");
        import_cxf(&bytes, &ImportOptions::new()).expect("the golden imports");
        Engine::new().load_cxf(&bytes).expect("the golden loads");
    }
}

#[test]
fn the_qudt_key_under_no_qudt_binding_is_refused_by_both_readers_on_every_node() {
    for node in NODES {
        for value in values() {
            let doc = with(document(unbound_context()), node, "qudt:hasUnit", value);
            assert_eq!(
                importer_refusal(&doc),
                unknown_key(node, "qudt:hasUnit"),
                "{node}"
            );
            assert_eq!(
                engine_refusal(&doc),
                engine_refuses(DiagCode::MalformedDocument, node),
                "{node}"
            );
        }
    }
}

#[test]
fn a_unit_contradicted_by_the_unbound_key_is_refused_by_both_readers() {
    // The case the ruling was made on: `K` under the engine's key and degrees Celsius under the
    // upstream one, with no `qudt` binding.
    for node in [NODES[0], NODES[1]] {
        let doc = with(
            with(document(unbound_context()), node, "S231:unit", json!("K")),
            node,
            "qudt:hasUnit",
            json!({ "@id": "unit:DEG_C" }),
        );
        assert_eq!(
            importer_refusal(&doc),
            unknown_key(node, "qudt:hasUnit"),
            "{node}"
        );
        assert_eq!(
            engine_refusal(&doc),
            engine_refuses(DiagCode::MalformedDocument, node),
            "{node}"
        );
    }
}

#[test]
fn has_unit_under_a_qudt_prefix_bound_to_a_near_miss_namespace_is_refused_by_both_readers() {
    for binding in UNREAD_NAMESPACES {
        for node in NODES {
            let doc = with(
                document(context_binding(binding)),
                node,
                "qudt:hasUnit",
                json!({ "@id": "unit:K" }),
            );
            // The importer refuses the binding itself, before any node is read.
            assert_eq!(
                importer_refusal(&doc),
                unknown_key("document", "qudt"),
                "{binding} {node}"
            );
            assert_eq!(
                engine_refusal(&doc),
                engine_refuses(DiagCode::VocabularyNamespaceMismatch, node),
                "{binding} {node}"
            );
        }
    }
}

#[test]
fn a_near_miss_binding_with_no_has_unit_key_through_it_loads_in_the_engine_alone() {
    // The engine does not judge the binding, only a `hasUnit` key through it: with no key, or
    // with `qudt:hasQuantityKind`, which is never read, the document loads to the content id it
    // has without them. The importer refuses the `@context` term whatever the nodes write.
    let identity = |doc: &Json| {
        let mut engine = Engine::new();
        engine
            .load_cxf(&serde_json::to_vec(doc).expect("serialize"))
            .unwrap_or_else(|error| panic!("the engine loads it: {error}"));
        crate::support::cxf::identity(&engine)
    };
    let without = identity(&document(unbound_context()));
    for binding in UNREAD_NAMESPACES {
        let bare = document(context_binding(binding));
        let kind = with(
            bare.clone(),
            BOUNDARY,
            "qudt:hasQuantityKind",
            json!({ "@id": "q:Pressure" }),
        );
        for doc in [bare, kind] {
            assert_eq!(
                importer_refusal(&doc),
                unknown_key("document", "qudt"),
                "{binding}"
            );
            assert_eq!(identity(&doc), without, "{binding}");
        }
    }
}

#[test]
fn has_unit_keyed_by_its_full_iri_in_a_near_miss_namespace_is_refused_by_both_readers() {
    for namespace in UNREAD_NAMESPACES {
        let key = format!("{namespace}hasUnit");
        for context in [unbound_context(), context_binding(QUDT_HASH)] {
            for node in NODES {
                let doc = with(document(context.clone()), node, &key, json!("K"));
                assert_eq!(
                    importer_refusal(&doc),
                    unknown_key(node, &key),
                    "{key} {node}"
                );
                assert_eq!(
                    engine_refusal(&doc),
                    engine_refuses(DiagCode::VocabularyNamespaceMismatch, node),
                    "{key} {node}"
                );
            }
        }
    }
}

#[test]
fn a_context_term_that_expands_to_an_unread_has_unit_is_refused_by_both_readers() {
    // The engine matches the expanded IRI, so a whole-token term is refused as the key it stands
    // for; the importer admits no such term at all.
    for (context, code) in [
        (
            json!({ "S231": S231, "hu": "https://qudt.org/schema/qudt#hasUnit" }),
            DiagCode::VocabularyNamespaceMismatch,
        ),
        (
            json!({ "S231": S231, "hu": "qudt:hasUnit" }),
            DiagCode::MalformedDocument,
        ),
    ] {
        let doc = with(document(context), BOUNDARY, "hu", json!("K"));
        assert_eq!(importer_refusal(&doc), unknown_key("document", "hu"));
        assert_eq!(engine_refusal(&doc), engine_refuses(code, BOUNDARY));
    }
}

#[test]
fn a_key_that_is_an_unread_has_unit_only_once_trimmed_is_refused_by_both_readers() {
    for key in ["qudt:hasUnit ", " qudt:hasUnit", "qudt:hasUnit\t"] {
        for node in NODES {
            let doc = with(document(unbound_context()), node, key, json!("K"));
            assert_eq!(
                importer_refusal(&doc),
                unknown_key(node, key),
                "{key:?} {node}"
            );
            assert_eq!(
                engine_refusal(&doc),
                engine_refuses(DiagCode::MalformedDocument, node),
                "{key:?} {node}"
            );
        }
    }
    for namespace in UNREAD_NAMESPACES {
        let key = format!(" {namespace}hasUnit");
        let doc = with(document(unbound_context()), BOUNDARY, &key, json!("K"));
        assert_eq!(
            importer_refusal(&doc),
            unknown_key(BOUNDARY, &key),
            "{key:?}"
        );
        assert_eq!(
            engine_refusal(&doc),
            engine_refuses(DiagCode::MalformedDocument, BOUNDARY),
            "{key:?}"
        );
    }
}
