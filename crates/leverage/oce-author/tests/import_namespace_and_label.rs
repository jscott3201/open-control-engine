//! The CXF importer and the emitter agree with the engine on the S231 namespace and on the
//! translation-software label (GitHub #527).
//!
//! `fixtures/previous_layout/reheat_overrides.cxf.json` is the corpus document
//! `reheat_overrides.jsonld` exactly as release 0.4.0 carried it (the blob at tag `v0.4.0`, path
//! `crates/oce-cxf/tests/fixtures/g36/reheat_overrides.jsonld`; SHA-256
//! `88c11731f22884fb2766d282fcd0dd0c3bf41e888a51db2cf928c2e693d95149`), renamed `.cxf.json` like
//! every CXF fixture of this crate. It binds the namespace the engine wrote through that release
//! and is never re-blessed: both importers refuse it. Release 0.4.0 had no emitter, so there is no
//! emitted document of that layout to keep; this is a document the importer reads in today's
//! layout.

mod support;

use oce_api::{DiagCode, Engine, OcError};
use oce_author::{
    EmitOptions, ImportError, ImportErrorKind, ImportOptions, Imported, SourceForm, emit,
    import_cxf,
};
use serde_json::{Value, json};

use support::cxf::{corpus_fixture, identity, load_clean};
use support::edit::Document;
use support::fixture;

const PUBLISHED: &str = "http://data.ashrae.org/S231#";
const RETIRED: &str = "http://data.ashrae.org/S231P#";
const SOFTWARE_KEY: &str = "S231:translationSoftware";
const VERSION_KEY: &str = "S231:translationSoftwareVersion";
const ROOT: &str = "urn:oce:author:econ_enable";

fn import(bytes: &[u8]) -> Result<Imported, ImportError> {
    import_cxf(bytes, &ImportOptions::new())
}

fn golden() -> String {
    std::fs::read_to_string(fixture("emit/econ_enable.cxf.json")).expect("golden")
}

fn econ() -> Document {
    Document::read(&fixture("emit/econ_enable.cxf.json"))
}

/// The golden with its `@context` members replaced by `members` (the text between the braces).
fn with_context(members: &str) -> Vec<u8> {
    let text = golden();
    let binding = format!("\"S231\": \"{PUBLISHED}\"");
    assert_eq!(text.matches(&binding).count(), 1);
    text.replacen(&binding, members, 1).into_bytes()
}

fn mismatch(term: &str, found: &str) -> ImportErrorKind {
    ImportErrorKind::NamespaceMismatch {
        term: term.to_owned(),
        found: found.to_owned(),
        expected: PUBLISHED,
    }
}

#[test]
fn a_release_0_4_0_document_is_refused_by_both_importers() {
    let previous =
        std::fs::read(fixture("previous_layout/reheat_overrides.cxf.json")).expect("fixture");

    let error = import(&previous).map(|_| ()).expect_err("refused");
    assert_eq!(error.source, SourceForm::Cxf);
    assert_eq!(error.subject, "document");
    assert_eq!(error.kind, mismatch("S231", RETIRED));
    assert_eq!(error.code(), "namespace-mismatch");
    assert_eq!(
        error.to_string(),
        format!(
            "CXF import refused `document` (namespace-mismatch): the `@context` term `S231` \
             binds `{RETIRED}`; the S231 vocabulary is read only in `{PUBLISHED}`"
        )
    );

    let mut engine = Engine::new();
    let refused = engine.load_cxf(&previous).expect_err("the engine refuses");
    assert!(matches!(refused, OcError::Cxf(_)), "{refused:?}");
    let diagnostics = refused.diagnostics();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, DiagCode::VocabularyNamespaceMismatch);
}

#[test]
fn the_previous_layout_fixture_is_todays_corpus_document_with_the_iri_spelled_the_old_way() {
    // The namespace is the only thing that refuses it: the same bytes under today's spelling are
    // the corpus document, which imports.
    let previous =
        std::fs::read(fixture("previous_layout/reheat_overrides.cxf.json")).expect("fixture");
    let current =
        std::fs::read_to_string(corpus_fixture("reheat_overrides")).expect("corpus fixture");
    assert_eq!(current.matches(PUBLISHED).count(), 1);
    assert_eq!(
        current.replacen(PUBLISHED, RETIRED, 1).into_bytes(),
        previous
    );
    import(current.as_bytes()).expect("today's document imports");
}

#[test]
fn s231_bound_to_any_other_iri_is_a_namespace_mismatch_naming_both_iris() {
    for found in [
        RETIRED,
        "https://data.ashrae.org/S231#",
        "https://data.ashrae.org/S231P#",
        "http://data.ashrae.org/S231",
        "http://data.ashrae.org/s231#",
        "http://data.ashrae.org/S231#Block",
        "urn:x:",
        "",
    ] {
        let error = import(&with_context(&format!("\"S231\": \"{found}\"")))
            .map(|_| ())
            .expect_err("refused");
        assert_eq!(error.kind, mismatch("S231", found), "{found:?}");
        assert_eq!(error.subject, "document");
        let text = error.to_string();
        assert!(text.contains(PUBLISHED), "{text}");
        assert!(found.is_empty() || text.contains(found), "{text}");
    }
}

#[test]
fn the_retired_namespace_is_refused_under_any_term_beside_the_right_binding() {
    let right = format!("\"S231\": \"{PUBLISHED}\"");
    // `base` is the one other term the dialect admits; it does not admit this value.
    for term in ["S231P", "base", "old"] {
        let members = format!("{right}, \"{term}\": \"{RETIRED}\"");
        let error = import(&with_context(&members))
            .map(|_| ())
            .expect_err("refused");
        assert_eq!(error.kind, mismatch(term, RETIRED), "{term}");
    }
    // The inert `base` term with any other value still reads.
    let members = format!("{right}, \"base\": \"http://example.org#\"");
    import(&with_context(&members)).expect("the corpus's inert base term is accepted");
}

#[test]
fn a_document_without_an_s231_binding_imports_the_same_sequence() {
    let baseline = import(golden().as_bytes()).expect("the golden imports");
    assert!(baseline.report.is_empty());
    // An empty context, the inert term alone, and no `@context` at all.
    let no_context = {
        let mut value: Value = serde_json::from_str(&golden()).expect("JSON");
        value.as_object_mut().expect("object").remove("@context");
        serde_json::to_vec(&value).expect("write")
    };
    for bytes in [
        with_context(""),
        with_context("\"base\": \"http://example.org#\""),
        no_context,
    ] {
        let imported = import(&bytes).expect("an unbound document imports");
        assert_eq!(imported, baseline);
    }
}

#[test]
fn every_emitted_document_carries_the_engines_label_on_its_root_and_nowhere_else() {
    for name in ["econ_enable", "every_element", "reheat_overrides"] {
        let bytes = std::fs::read(fixture(&format!("emit/{name}.cxf.json"))).expect("golden");
        let document: Value = serde_json::from_slice(&bytes).expect("JSON");
        assert_eq!(document["@context"], json!({ "S231": PUBLISHED }), "{name}");
        let graph = document["@graph"].as_array().expect("@graph");
        let root = graph[0].as_object().expect("root");
        assert!(root.contains_key("S231:containsBlock"), "{name}");
        assert_eq!(root[SOFTWARE_KEY], json!("open-control-engine"), "{name}");
        assert_eq!(
            root[VERSION_KEY],
            json!(env!("CARGO_PKG_VERSION")),
            "{name}"
        );
        // The label closes the root node, after its relations: the writer's fixed key order.
        let text = String::from_utf8(bytes.clone()).expect("UTF-8");
        let label = format!(
            "      ],\n      \"{SOFTWARE_KEY}\": \"open-control-engine\",\n      \
             \"{VERSION_KEY}\": \"{}\"\n    }},\n",
            env!("CARGO_PKG_VERSION")
        );
        assert_eq!(text.matches(&label).count(), 1, "{name}");
        let node_ids: Vec<usize> = text
            .match_indices("\n      \"@id\": ")
            .map(|(at, _)| at)
            .collect();
        let at = text.find(&label).expect("the label is present");
        assert!(node_ids[0] < at && at < node_ids[1], "{name}");
        for node in &graph[1..] {
            let node = node.as_object().expect("node");
            assert!(!node.contains_key(SOFTWARE_KEY) && !node.contains_key(VERSION_KEY));
        }

        // The engine's exporter writes the same two values on its own root.
        let engine = load_clean(&bytes);
        let exported: Value =
            serde_json::from_slice(&engine.export_cxf().expect("exports").bytes).expect("JSON");
        assert_eq!(
            exported["@graph"][0][SOFTWARE_KEY], root[SOFTWARE_KEY],
            "{name}"
        );
        assert_eq!(
            exported["@graph"][0][VERSION_KEY], root[VERSION_KEY],
            "{name}"
        );
    }
}

#[test]
fn the_importer_reads_no_label_whatever_a_block_node_says() {
    let baseline = import(golden().as_bytes()).expect("the golden imports");
    let leaf = econ().first("S231:containsBlock");
    let huge = "x".repeat(1 << 16);
    for (software, version) in [
        (json!("modelica-json"), json!("2.1.0")),
        (json!(""), json!("")),
        (json!(huge), json!("1")),
        (json!(7), json!(1.5)),
        (Value::Null, json!(false)),
        (json!(["a", { "b": null }]), json!({ "@id": "urn:absent" })),
    ] {
        for id in [ROOT, leaf.as_str()] {
            let bytes = econ()
                .put(id, SOFTWARE_KEY, software.clone())
                .put(id, VERSION_KEY, version.clone())
                .bytes();
            let imported = import(&bytes).expect("a labelled block node imports");
            assert_eq!(imported, baseline, "{software} on {id}");
        }
    }
    // Without the label at all (a document another tool wrote), and with one member alone.
    let bare = econ()
        .remove(ROOT, SOFTWARE_KEY)
        .remove(ROOT, VERSION_KEY)
        .bytes();
    assert_eq!(
        import(&bare).expect("an unlabelled document imports"),
        baseline
    );
    let half = econ().remove(ROOT, VERSION_KEY).bytes();
    assert_eq!(import(&half).expect("half a label imports"), baseline);
}

#[test]
fn the_label_is_a_block_property_and_an_unknown_key_anywhere_else() {
    let document = econ();
    let (_, block_parameter) = document.first_block_parameter();
    for id in [
        document.first("S231:hasParameter"),
        document.first("S231:hasInput"),
        document.first("S231:hasOutput"),
        block_parameter,
        document.first_port(),
    ] {
        for key in [SOFTWARE_KEY, VERSION_KEY] {
            let error = import(&econ().put(&id, key, json!("tool")).bytes())
                .map(|_| ())
                .expect_err("refused");
            assert_eq!(
                error.kind,
                ImportErrorKind::UnknownKey {
                    key: key.to_owned()
                },
                "{key} on {id}"
            );
            assert_eq!(error.subject, id);
        }
    }
}

#[test]
fn an_imported_label_is_replaced_by_the_emitters_and_never_reaches_the_content_id() {
    let foreign = econ()
        .put(ROOT, SOFTWARE_KEY, json!("modelica-json"))
        .put(ROOT, VERSION_KEY, json!("2.1.0"))
        .bytes();
    let imported = import(&foreign).expect("imports");
    let emitted = emit(&imported.sequence, &EmitOptions::new()).expect("emits");
    assert_eq!(emitted, golden().into_bytes());
    // The engine names the same model with or without the label, under either tool's.
    let bare = econ()
        .remove(ROOT, SOFTWARE_KEY)
        .remove(ROOT, VERSION_KEY)
        .bytes();
    let id = identity(&load_clean(golden().as_bytes()));
    assert_eq!(identity(&load_clean(&foreign)), id);
    assert_eq!(identity(&load_clean(&bare)), id);
}
