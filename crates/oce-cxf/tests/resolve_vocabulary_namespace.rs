//! The importer reads the S231 vocabulary in one namespace, `http://data.ashrae.org/S231#`, and
//! refuses every other spelling of it with `vocabulary-namespace-mismatch` (GitHub #527).
//!
//! `fixtures/previous_layout/minimal_loop.export.cxf.json` is the export of `minimal_loop.jsonld`
//! exactly as release 0.4.0 wrote it (the blob at tag `v0.4.0`, path
//! `crates/oce-cxf/tests/fixtures/export/minimal_loop.export.cxf.json`; SHA-256
//! `5c6ced21099fddb2f50ae2b3dd28963caaade719f9370b3d2ae027273b4f556e`). It binds the namespace the
//! engine wrote through that release and is never re-blessed: a document of that layout is
//! refused, and nothing reads it. Every other document here is built from `minimal_loop.jsonld`
//! by editing its `@context` or one node, with the expected result stated by hand.

use oce_cxf::{
    Context, CxfError, ResolveOptions, export, import_cxf, parse_document, write_document,
};
use oce_diag::{DiagCode, Diagnostic};
use serde_json::{Map, Value as Json, json};

const MINIMAL_LOOP: &str = include_str!("fixtures/minimal_loop.jsonld");
const PREVIOUS_LAYOUT: &[u8] =
    include_bytes!("fixtures/previous_layout/minimal_loop.export.cxf.json");

/// The namespace the engine reads.
const PUBLISHED: &str = "http://data.ashrae.org/S231#";
/// The namespace the engine wrote through release 0.4.0.
const RETIRED: &str = "http://data.ashrae.org/S231P#";
const SOURCE_PORT: &str = "http://example.org#MinLoop.con.y";

fn fixture() -> Json {
    serde_json::from_str(MINIMAL_LOOP).expect("fixture is JSON")
}

fn bytes(doc: &Json) -> Vec<u8> {
    serde_json::to_vec(doc).expect("serialize doc")
}

fn node_mut<'a>(doc: &'a mut Json, id: &str) -> &'a mut Map<String, Json> {
    doc["@graph"]
        .as_array_mut()
        .expect("@graph array")
        .iter_mut()
        .find(|entry| entry["@id"] == id)
        .and_then(Json::as_object_mut)
        .expect("node present")
}

/// The complete import outcome, rendered: the model and every diagnostic on success, the full
/// error on refusal.
fn outcome(doc: &[u8]) -> String {
    match import_cxf(doc, &ResolveOptions::default()) {
        Ok((graph, report)) => format!("ok {graph:?}\n{:?}", report.diagnostics),
        Err(error) => format!("err {error:?}"),
    }
}

fn refusal(doc: &[u8]) -> Vec<Diagnostic> {
    match import_cxf(doc, &ResolveOptions::default()) {
        Err(CxfError::Validation(diags)) => diags,
        other => panic!("expected a resolver refusal, got {other:?}"),
    }
}

/// `fixture()` with its whole `@context` replaced.
fn with_context(context: Json) -> Vec<u8> {
    let mut doc = fixture();
    doc["@context"] = context;
    bytes(&doc)
}

/// The one refusal a context binding `term` to `found` gives.
fn binding_refusal(term: &str, found: &str) -> Diagnostic {
    Diagnostic::error(
        DiagCode::VocabularyNamespaceMismatch,
        format!(
            "@context term `{term}` binds `{found}`; the S231 vocabulary is read only in \
             `{PUBLISHED}`"
        ),
    )
    .with_subject(term.to_owned())
}

#[test]
fn a_release_0_4_0_export_is_refused_by_its_namespace_and_nothing_else() {
    let expected = vec![binding_refusal("S231", RETIRED)];
    assert_eq!(refusal(PREVIOUS_LAYOUT), expected);
    // Deterministic, and not a warning a host could wave through.
    assert_eq!(refusal(PREVIOUS_LAYOUT), expected);
    assert!(expected[0].is_error());
    assert_eq!(expected[0].code.as_str(), "vocabulary-namespace-mismatch");
    let message = &expected[0].message;
    assert!(message.contains(RETIRED) && message.contains(PUBLISHED));
}

#[test]
fn the_previous_layout_differs_from_the_current_export_only_in_the_namespace_and_the_label() {
    // The fixture is a well-formed document: only its binding stands between it and a load.
    let previous = parse_document(PREVIOUS_LAYOUT).expect("the 0.4.0 export parses");
    let Context::Map(context) = &previous.context else {
        panic!("the 0.4.0 export carries an inline context map");
    };
    assert_eq!(context.len(), 1);
    assert_eq!(context["S231"], json!(RETIRED));

    // Today's export of the same model, with the two label members removed and the binding
    // spelled the old way, is the 0.4.0 export byte for byte.
    let (graph, _) = import_cxf(MINIMAL_LOOP.as_bytes(), &ResolveOptions::default())
        .expect("the current fixture imports");
    let mut current = parse_document(&export(&graph).expect("exports")).expect("export parses");
    let root = &mut current.graph[0];
    assert_eq!(
        root.other.remove("S231:translationSoftware"),
        Some(json!("open-control-engine"))
    );
    assert_eq!(
        root.other.remove("S231:translationSoftwareVersion"),
        Some(json!(env!("CARGO_PKG_VERSION")))
    );
    let Context::Map(context) = &mut current.context else {
        panic!("the export carries an inline context map");
    };
    assert_eq!(
        context.insert("S231".into(), json!(RETIRED)),
        Some(json!(PUBLISHED))
    );
    assert_eq!(
        write_document(&current).expect("serialize"),
        PREVIOUS_LAYOUT
    );
}

#[test]
fn s231_bound_to_any_other_iri_is_refused_before_anything_else_is_read() {
    for found in [
        RETIRED,
        "https://data.ashrae.org/S231#",
        "https://data.ashrae.org/S231P#",
        "http://data.ashrae.org/S231",
        "http://data.ashrae.org/S231/",
        "http://data.ashrae.org/s231#",
        "http://DATA.ashrae.org/S231#",
        "http://data.ashrae.org/S231#Block",
        "http://data.ashrae.org/standard231/1.0/Model/",
        "urn:x:",
    ] {
        let doc = with_context(json!({ "S231": found, "base": "http://example.org#" }));
        assert_eq!(refusal(&doc), [binding_refusal("S231", found)], "{found}");
    }
}

#[test]
fn the_retired_namespace_is_refused_under_any_prefix_beside_the_right_binding() {
    for term in ["S231P", "s231", "old", "base"] {
        let doc = with_context(json!({ "S231": PUBLISHED, term: RETIRED }));
        assert_eq!(refusal(&doc), [binding_refusal(term, RETIRED)], "{term}");
    }
    // Both wrong: every offending binding is reported, in term order.
    let doc = with_context(json!({ "S231": RETIRED, "old": RETIRED, "right": PUBLISHED }));
    assert_eq!(
        refusal(&doc),
        [
            binding_refusal("S231", RETIRED),
            binding_refusal("old", RETIRED)
        ]
    );
    // The right IRI under another name does not rescue a wrong `S231`.
    let doc = with_context(json!({ "S231": RETIRED, "cdl": PUBLISHED }));
    assert_eq!(refusal(&doc), [binding_refusal("S231", RETIRED)]);
}

#[test]
fn a_document_without_an_s231_binding_loads_the_same_model() {
    let baseline = outcome(MINIMAL_LOOP.as_bytes());
    assert!(baseline.starts_with("ok "), "{baseline}");
    for context in [
        json!({}),
        json!({ "base": "http://example.org#" }),
        json!([]),
        json!([{ "base": "http://example.org#" }]),
    ] {
        assert_eq!(
            outcome(&with_context(context.clone())),
            baseline,
            "{context}"
        );
    }
    // Its full-IRI keys are read in the published namespace, and only there.
    let mut doc = fixture();
    doc["@context"] = json!({});
    let node = node_mut(&mut doc, SOURCE_PORT);
    let value = node
        .remove("S231:isConnectedTo")
        .expect("the port is wired");
    node.insert(format!("{PUBLISHED}isConnectedTo"), value);
    assert_eq!(outcome(&bytes(&doc)), baseline);
}

#[test]
fn another_prefix_bound_to_the_published_namespace_is_accepted() {
    let baseline = outcome(MINIMAL_LOOP.as_bytes());
    // Beside `S231`, and alone: a key written through it is the same property.
    for context in [
        json!({ "S231": PUBLISHED, "cdl": PUBLISHED }),
        json!({ "cdl": PUBLISHED }),
    ] {
        let mut doc = fixture();
        doc["@context"] = context.clone();
        let node = node_mut(&mut doc, SOURCE_PORT);
        let value = node
            .remove("S231:isConnectedTo")
            .expect("the port is wired");
        node.insert("cdl:isConnectedTo".into(), value);
        assert_eq!(outcome(&bytes(&doc)), baseline, "{context}");
    }
}

#[test]
fn only_the_binding_in_force_is_judged_in_a_list_of_context_maps() {
    let baseline = outcome(MINIMAL_LOOP.as_bytes());
    // A later map replaces an earlier binding (JSON-LD context processing order).
    let replaced = with_context(json!([{ "S231": RETIRED }, { "S231": PUBLISHED }]));
    assert_eq!(outcome(&replaced), baseline);
    let reverted = with_context(json!([{ "S231": PUBLISHED }, { "S231": RETIRED }]));
    assert_eq!(refusal(&reverted), [binding_refusal("S231", RETIRED)]);
    let aside = with_context(json!([{ "S231": PUBLISHED }, { "legacy": RETIRED }]));
    assert_eq!(refusal(&aside), [binding_refusal("legacy", RETIRED)]);
}

#[test]
fn a_malformed_binding_keeps_its_own_refusal_and_reports_alone() {
    // A context whose shape is refused is not judged for its namespace as well.
    for (context, code) in [
        (json!({ "S231": 7 }), DiagCode::MalformedDocument),
        (json!({ "S231": null }), DiagCode::MalformedDocument),
        (
            json!({ "S231": { "@id": RETIRED } }),
            DiagCode::MalformedDocument,
        ),
        (json!({ "S231": [RETIRED] }), DiagCode::MalformedDocument),
        (json!({ "S231": "S231P#" }), DiagCode::NonSubsetConstruct),
        (json!({ "S231": "" }), DiagCode::NonSubsetConstruct),
        (json!(RETIRED), DiagCode::NonSubsetConstruct),
        (
            json!({ "S231": RETIRED, "@vocab": RETIRED }),
            DiagCode::NonSubsetConstruct,
        ),
    ] {
        let diags = refusal(&with_context(context.clone()));
        assert_eq!(diags.len(), 1, "{context}: {diags:?}");
        assert_eq!(diags[0].code, code, "{context}");
    }
}

#[test]
fn a_typing_token_in_the_retired_namespace_is_refused_under_the_right_binding() {
    // `@type` on a connector, spelled as a full IRI.
    let mut doc = fixture();
    node_mut(&mut doc, SOURCE_PORT).insert("@type".into(), json!(format!("{RETIRED}RealOutput")));
    let diags = refusal(&bytes(&doc));
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, DiagCode::VocabularyNamespaceMismatch);
    assert_eq!(diags[0].subject.as_deref(), Some(SOURCE_PORT));
    assert_eq!(
        diags[0].message,
        format!(
            "@type `{RETIRED}RealOutput` on node `{SOURCE_PORT}` is in `{RETIRED}`; the S231 \
             vocabulary is read only in `{PUBLISHED}`"
        )
    );
    // `isOfDataType`, likewise.
    let mut doc = fixture();
    node_mut(&mut doc, SOURCE_PORT).insert(
        "S231:isOfDataType".into(),
        json!({ "@id": format!("{RETIRED}Real") }),
    );
    let diags = refusal(&bytes(&doc));
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, DiagCode::VocabularyNamespaceMismatch);
    assert_eq!(
        diags[0].message,
        format!(
            "S231:isOfDataType `{RETIRED}Real` on node `{SOURCE_PORT}` is in `{RETIRED}`; the \
             S231 vocabulary is read only in `{PUBLISHED}`"
        )
    );
    // The same types as full IRIs in the published namespace are the compact spelling.
    let mut doc = fixture();
    let node = node_mut(&mut doc, SOURCE_PORT);
    node.insert("@type".into(), json!(format!("{PUBLISHED}RealOutput")));
    node.insert(
        "S231:isOfDataType".into(),
        json!({ "@id": format!("{PUBLISHED}Real") }),
    );
    assert_eq!(outcome(&bytes(&doc)), outcome(MINIMAL_LOOP.as_bytes()));
}

#[test]
fn a_modeled_key_in_the_retired_namespace_is_refused_and_an_unmodeled_one_passes_through() {
    let baseline = outcome(MINIMAL_LOOP.as_bytes());
    let mut doc = fixture();
    node_mut(&mut doc, SOURCE_PORT).insert(format!("{RETIRED}min"), json!(0.0));
    let diags = refusal(&bytes(&doc));
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, DiagCode::VocabularyNamespaceMismatch);
    assert_eq!(diags[0].subject.as_deref(), Some(SOURCE_PORT));
    assert_eq!(
        diags[0].message,
        format!(
            "property `{RETIRED}min` on node `{SOURCE_PORT}` names the modeled term `min` in \
             `{RETIRED}`; the S231 vocabulary is read only in `{PUBLISHED}`; write it as \
             `S231:min`"
        )
    );
    // A key the importer models nothing for is opaque in any namespace.
    let mut doc = fixture();
    node_mut(&mut doc, SOURCE_PORT).insert(format!("{RETIRED}documentation"), json!("text"));
    assert_eq!(outcome(&bytes(&doc)), baseline);
}

#[test]
fn a_long_retired_namespace_type_is_quoted_as_an_excerpt() {
    // GitHub #641: a megabyte of term after the retired namespace used to be quoted whole.
    let mut doc = fixture();
    let token = format!("{RETIRED}{}", "B".repeat(1 << 20));
    node_mut(&mut doc, SOURCE_PORT).insert("@type".into(), json!(token));
    let diags = refusal(&bytes(&doc));
    assert_eq!(diags.len(), 1, "{}", diags.len());
    let quoted = format!("{RETIRED}{}…", "B".repeat(64 - RETIRED.len()));
    assert_eq!(
        diags[0].message,
        format!(
            "@type `{quoted}` on node `{SOURCE_PORT}` is in `{RETIRED}`; the S231 vocabulary is \
             read only in `{PUBLISHED}`"
        )
    );
}
