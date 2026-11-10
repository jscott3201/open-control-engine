//! The S231 namespace and the translation-software label, seen through the facade (GitHub #527).
//!
//! Three properties a host relies on:
//!
//! - A document bound to the namespace the engine wrote through release 0.4.0 does not load: the
//!   facade returns the resolver's `vocabulary-namespace-mismatch` diagnostic and the engine stays
//!   empty. The fixture is `oce-cxf`'s unblessed copy of a release-0.4.0 export.
//! - The canonical content id, its canonical bytes and the compiled-schedule blob depend on
//!   neither the `@context` spelling nor the label: every accepted spelling of one model gives the
//!   same three values.
//! - Every export carries this engine's label, and loading that export changes none of them.

use oce_api::{DiagCode, Engine, OcError};
use serde_json::{Value as Json, json};

const MINIMAL_LOOP: &str = include_str!("../../oce-cxf/tests/fixtures/minimal_loop.jsonld");
const PREVIOUS_LAYOUT: &[u8] =
    include_bytes!("../../oce-cxf/tests/fixtures/previous_layout/minimal_loop.export.cxf.json");
const PUBLISHED: &str = "http://data.ashrae.org/S231#";
const RETIRED: &str = "http://data.ashrae.org/S231P#";
const ROOT: &str = "http://example.org#MinLoop";

/// The three identities of a loaded model: content id, canonical bytes, blob bytes.
fn identities(source: &[u8]) -> (String, Vec<u8>, Vec<u8>) {
    let mut engine = Engine::new();
    engine.load_cxf(source).expect("the document loads");
    let id = engine.content_id().expect("content id");
    let canonical = engine.canonical_content_bytes().expect("canonical bytes");
    let blob = engine.compile_blob().expect("blob");
    assert_eq!(blob.content_id.as_ref(), Some(&id));
    (id.as_str().to_owned(), canonical, blob.bytes)
}

fn fixture() -> Json {
    serde_json::from_str(MINIMAL_LOOP).expect("fixture is JSON")
}

fn bytes(doc: &Json) -> Vec<u8> {
    serde_json::to_vec(doc).expect("serialize doc")
}

fn root_mut(doc: &mut Json) -> &mut serde_json::Map<String, Json> {
    doc["@graph"]
        .as_array_mut()
        .expect("@graph array")
        .iter_mut()
        .find(|entry| entry["@id"] == ROOT)
        .and_then(Json::as_object_mut)
        .expect("root present")
}

#[test]
fn a_release_0_4_0_export_is_refused_with_the_namespace_diagnostic_and_loads_nothing() {
    let mut engine = Engine::new();
    let error = engine
        .load_cxf(PREVIOUS_LAYOUT)
        .expect_err("a document of the previous layout does not load");
    assert!(matches!(error, OcError::Cxf(_)), "{error:?}");
    let diagnostics = error.diagnostics();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, DiagCode::VocabularyNamespaceMismatch);
    assert!(diagnostics[0].is_error());
    assert_eq!(diagnostics[0].subject.as_deref(), Some("S231"));
    assert_eq!(
        diagnostics[0].message,
        format!(
            "@context term `S231` binds `{RETIRED}`; the S231 vocabulary is read only in \
             `{PUBLISHED}`"
        )
    );
    // Nothing was loaded: there is no model to name, export or encode.
    assert!(engine.content_id().is_err());
    assert!(engine.export_cxf().is_err());
    assert!(engine.compile_blob().is_err());
}

#[test]
fn a_refused_namespace_leaves_a_loaded_engine_as_it_was() {
    let mut engine = Engine::new();
    engine.load_cxf(MINIMAL_LOOP.as_bytes()).expect("loads");
    let before = engine.content_id().expect("content id");
    engine
        .load_cxf(PREVIOUS_LAYOUT)
        .expect_err("the previous layout is refused");
    assert_eq!(engine.content_id().expect("still loaded"), before);
}

#[test]
fn the_content_id_and_the_blob_do_not_depend_on_the_context_spelling_or_the_label() {
    let baseline = identities(MINIMAL_LOOP.as_bytes());
    let mut variants: Vec<(&str, Json)> = Vec::new();

    let mut unbound = fixture();
    unbound["@context"] = json!({});
    variants.push(("no S231 binding", unbound));

    let mut second_prefix = fixture();
    second_prefix["@context"]["cdl"] = json!(PUBLISHED);
    variants.push(("a second prefix for the namespace", second_prefix));

    let mut listed = fixture();
    listed["@context"] = json!([{ "S231": RETIRED }, { "S231": PUBLISHED }]);
    variants.push(("a replaced binding in a context list", listed));

    for (name, software, version) in [
        (
            "this engine's label",
            json!("open-control-engine"),
            json!(env!("CARGO_PKG_VERSION")),
        ),
        (
            "another tool's label",
            json!("modelica-json"),
            json!("2.1.0"),
        ),
        (
            "a malformed label",
            json!({ "@id": "urn:x" }),
            json!([1, null]),
        ),
    ] {
        let mut labelled = fixture();
        let root = root_mut(&mut labelled);
        root.insert("S231:translationSoftware".into(), software);
        root.insert("S231:translationSoftwareVersion".into(), version);
        variants.push((name, labelled));
    }

    for (name, doc) in &variants {
        assert_eq!(identities(&bytes(doc)), baseline, "{name}");
    }
}

#[test]
fn the_facade_export_carries_the_label_and_reloads_to_the_same_identities() {
    let baseline = identities(MINIMAL_LOOP.as_bytes());
    let mut engine = Engine::new();
    engine.load_cxf(MINIMAL_LOOP.as_bytes()).expect("loads");
    let exported = engine.export_cxf().expect("exports");
    exported.complete().expect("the export is complete");

    let document: Json = serde_json::from_slice(&exported.bytes).expect("export is JSON");
    assert_eq!(document["@context"], json!({ "S231": PUBLISHED }));
    let root = &document["@graph"][0];
    assert_eq!(
        root["S231:translationSoftware"],
        json!("open-control-engine")
    );
    assert_eq!(
        root["S231:translationSoftwareVersion"],
        json!(env!("CARGO_PKG_VERSION"))
    );

    // The instance namespace of every name is unchanged by export, so the id is too.
    assert_eq!(identities(&exported.bytes), baseline);
}
