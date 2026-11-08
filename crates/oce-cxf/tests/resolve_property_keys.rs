//! Modeled `S231:` properties import identically in every JSON-LD key spelling (issue #383).
//!
//! The importer used to recognise a modeled property (`S231:min`, `S231:unit`, `S231:hasInput`, …)
//! only under its compact key. The same property keyed by its full IRI, by another prefix bound to
//! the S231 namespace, or by a declared `@context` term fell into the pass-through map: no
//! validation, no enforcement, no export. Every spelling now normalizes to the compact key on the
//! resolver's working clone, so the model, the diagnostics and the export bytes are identical; a
//! property written twice is refused, and unrelated full-IRI keys still pass through.
//! Expected values are stated by hand from the authored bytes.

use std::path::{Path, PathBuf};
use std::process::Command;

use oce_cxf::{CxfError, ResolveOptions, export, import_cxf, parse_document, write_document};
use oce_diag::{DiagCode, Diagnostic};
use oce_model::ModelGraph;
use serde_json::{Map, Value as Json, json};

const CONNECTOR_ATTRS: &str = include_str!("fixtures/connector_attrs.jsonld");
const S231P: &str = "http://data.ashrae.org/S231P#";
const SOURCE_PORT: &str = "http://example.org#A.con.y";
const BOUNDARY_OUTPUT: &str = "http://example.org#A.yOut";
const DOUBLE: &str = "http://www.w3.org/2001/XMLSchema#double";

/// Every modeled `Node` term. `the_test_term_list_is_the_modeled_set` pins it against the DTO.
const MODELED_TERMS: [&str; 25] = [
    "hasInput",
    "hasOutput",
    "hasParameter",
    "hasConstant",
    "containsBlock",
    "hasInstance",
    "isConnectedTo",
    "isOfDataType",
    "label",
    "accessSpecifier",
    "value",
    "min",
    "max",
    "nominal",
    "unit",
    "quantity",
    "displayUnit",
    "isFinal",
    "isArray",
    "numberDimensions",
    "sizeOfDimensions",
    "hasFmuPath",
    "isConditionalComponent",
    "conditionalExpression",
    "isReplaceable",
];

/// A key spelling for a modeled term.
type Spelling = dyn Fn(&str) -> String;

fn fixture() -> Json {
    serde_json::from_str(CONNECTOR_ATTRS).expect("fixture is JSON")
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

fn bytes(doc: &Json) -> Vec<u8> {
    serde_json::to_vec(doc).expect("serialize doc")
}

/// The complete import outcome, rendered: the model and every diagnostic on success, the full
/// error on refusal. Two spellings agree iff these strings agree.
fn outcome(doc: &[u8]) -> String {
    match import_cxf(doc, &ResolveOptions::default()) {
        Ok((graph, report)) => format!("ok {graph:?}\n{:?}", report.diagnostics),
        Err(error) => format!("err {error:?}"),
    }
}

fn import(doc: &[u8]) -> ModelGraph {
    import_cxf(doc, &ResolveOptions::default())
        .map(|(graph, _)| graph)
        .unwrap_or_else(|e| panic!("expected a clean import, got {e:?}"))
}

fn refusal(doc: &[u8]) -> Vec<Diagnostic> {
    match import_cxf(doc, &ResolveOptions::default()) {
        Err(CxfError::Validation(diags)) => diags.into_iter().filter(|d| d.is_error()).collect(),
        other => panic!("expected a resolver refusal, got {other:?}"),
    }
}

/// Re-key `node`'s compact `S231:<term>` to `spelling(term)`, keeping its value.
fn respell(doc: &mut Json, id: &str, term: &str, spelling: &dyn Fn(&str) -> String) {
    let node = node_mut(doc, id);
    let value = node
        .remove(&format!("S231:{term}"))
        .unwrap_or_else(|| panic!("{id} carries S231:{term}"));
    node.insert(spelling(term), value);
}

/// The document's own `S231` binding, or the canonical namespace when it declares none.
fn s231_namespace(doc: &Json) -> String {
    let maps: Vec<&Json> = match &doc["@context"] {
        Json::Array(entries) => entries.iter().collect(),
        other => vec![other],
    };
    maps.iter()
        .rev()
        .find_map(|map| map.get("S231").and_then(Json::as_str))
        .unwrap_or(S231P)
        .to_owned()
}

/// Rewrite every modeled compact key on every node to its full IRI under the document's binding.
fn expand_every_modeled_key(doc: &mut Json) -> usize {
    let namespace = s231_namespace(doc);
    let mut rewritten = 0;
    let Some(graph) = doc.get_mut("@graph").and_then(Json::as_array_mut) else {
        return 0;
    };
    for node in graph.iter_mut().filter_map(Json::as_object_mut) {
        for term in MODELED_TERMS {
            if let Some(value) = node.remove(&format!("S231:{term}")) {
                node.insert(format!("{namespace}{term}"), value);
                rewritten += 1;
            }
        }
    }
    rewritten
}

#[test]
fn the_test_term_list_is_the_modeled_set() {
    // Each listed term, compact, is consumed by a typed DTO field (not the pass-through map).
    for term in MODELED_TERMS {
        let value = match term {
            "isFinal" | "isArray" | "isConditionalComponent" | "isReplaceable" => json!(true),
            "numberDimensions" => json!(1),
            "label"
            | "accessSpecifier"
            | "sizeOfDimensions"
            | "hasFmuPath"
            | "conditionalExpression"
            | "value"
            | "min"
            | "max"
            | "nominal"
            | "unit"
            | "quantity"
            | "displayUnit" => json!("x"),
            _ => json!({"@id": "urn:y"}),
        };
        let doc =
            json!({"@context": {}, "@graph": [{"@id": "urn:x", format!("S231:{term}"): value}]});
        let parsed = parse_document(&bytes(&doc)).expect("parses");
        assert!(
            parsed.graph[0].other.is_empty(),
            "S231:{term} is not modeled"
        );
    }
}

#[test]
fn each_connector_attribute_imports_and_exports_identically_in_every_spelling() {
    let compact = bytes(&fixture());
    let baseline = outcome(&compact);
    assert!(baseline.starts_with("ok "), "{baseline}");
    let baseline_export = export(&import(&compact)).expect("baseline exports");

    let spellings: [(&str, &Spelling); 3] = [
        ("full IRI", &|term| format!("{S231P}{term}")),
        ("alternate prefix", &|term| format!("s231p:{term}")),
        ("declared term", &|term| format!("attr_{term}")),
    ];
    for term in [
        "unit",
        "quantity",
        "displayUnit",
        "min",
        "max",
        "isConnectedTo",
    ] {
        for (label, spelling) in &spellings {
            let mut doc = fixture();
            let context = doc["@context"].as_object_mut().unwrap();
            context.insert("s231p".into(), json!(S231P));
            context.insert(format!("attr_{term}"), json!(format!("{S231P}{term}")));
            respell(&mut doc, SOURCE_PORT, term, *spelling);
            let doc = bytes(&doc);
            assert_eq!(outcome(&doc), baseline, "{term} as {label}");
            assert_eq!(
                export(&import(&doc)).expect("exports"),
                baseline_export,
                "{term} as {label}: export must stay canonical"
            );
        }
    }
}

#[test]
fn nominal_in_full_iri_form_grounds_and_round_trips_like_the_compact_key() {
    for value in [
        json!(300.5),
        json!({"@value": "2.5e2", "@type": DOUBLE}),
        json!("2 * 150.5"),
    ] {
        let mut compact = fixture();
        node_mut(&mut compact, SOURCE_PORT).insert("S231:nominal".into(), value.clone());
        node_mut(&mut compact, BOUNDARY_OUTPUT).insert("S231:nominal".into(), json!(1.5));
        let mut full = fixture();
        node_mut(&mut full, SOURCE_PORT).insert(format!("{S231P}nominal"), value.clone());
        node_mut(&mut full, BOUNDARY_OUTPUT).insert(format!("{S231P}nominal"), json!(1.5));
        let (compact, full) = (bytes(&compact), bytes(&full));
        assert_eq!(outcome(&full), outcome(&compact), "nominal={value}");
        let exported = export(&import(&full)).expect("exports");
        assert_eq!(
            exported,
            export(&import(&compact)).unwrap(),
            "nominal={value}"
        );
        let text = String::from_utf8(exported).unwrap();
        assert!(text.contains(r#""S231:nominal":1.5"#), "{text}");
        assert!(
            !text.contains("S231P#nominal"),
            "export writes the compact key: {text}"
        );
    }
}

#[test]
fn full_iri_bounds_are_validated_exactly_like_compact_bounds() {
    // A crossed pair reaches the model identically in both spellings; the deep gate
    // (`crossed-bounds`, pinned at the facade in `oce-api/tests/cxf_property_key_spellings.rs`)
    // refuses it from there.
    let mut compact = fixture();
    node_mut(&mut compact, SOURCE_PORT).insert("S231:min".into(), json!(400.0));
    let mut full = compact.clone();
    respell(&mut full, SOURCE_PORT, "min", &|t| format!("{S231P}{t}"));
    respell(&mut full, SOURCE_PORT, "max", &|t| format!("{S231P}{t}"));
    let (compact, full) = (bytes(&compact), bytes(&full));
    assert_eq!(outcome(&full), outcome(&compact));
    let real = |graph: &ModelGraph| {
        let attrs = graph
            .connectors
            .iter()
            .find(|c| c.iri.as_deref() == Some(SOURCE_PORT))
            .and_then(|c| c.attrs.as_real())
            .cloned()
            .expect("Real source port");
        (attrs.min.map(f64::to_bits), attrs.max.map(f64::to_bits))
    };
    assert_eq!(
        real(&import(&full)),
        (Some(400.0_f64.to_bits()), Some(350.0_f64.to_bits()))
    );

    // An ungroundable full-IRI bound is a typed grounding refusal, not a silent drop.
    let mut full = fixture();
    node_mut(&mut full, SOURCE_PORT).remove("S231:min");
    node_mut(&mut full, SOURCE_PORT).insert(format!("{S231P}min"), json!("noSuchSymbol"));
    let diags = refusal(&bytes(&full));
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, DiagCode::GroundingFailed);
    assert_eq!(diags[0].subject.as_deref(), Some(SOURCE_PORT));

    // A non-finite full-IRI min reaches the model, where the deep gate refuses it.
    let mut full = fixture();
    node_mut(&mut full, SOURCE_PORT).remove("S231:min");
    node_mut(&mut full, SOURCE_PORT).insert(
        format!("{S231P}min"),
        json!({"@value": "NaN", "@type": DOUBLE}),
    );
    let graph = import(&bytes(&full));
    let min = graph
        .connectors
        .iter()
        .find(|c| c.iri.as_deref() == Some(SOURCE_PORT))
        .and_then(|c| c.attrs.as_real())
        .and_then(|real| real.min);
    assert!(min.is_some_and(f64::is_nan), "{min:?}");
}

#[test]
fn every_modeled_key_of_every_tracked_fixture_spelled_as_a_full_iri_imports_identically() {
    let mut compared = 0;
    let mut rewritten_keys = 0;
    for (rel, path) in tracked_fixtures() {
        let original = std::fs::read(&path).expect("fixture readable");
        let Ok(mut doc) = serde_json::from_slice::<Json>(&original) else {
            continue;
        };
        let rewritten = expand_every_modeled_key(&mut doc);
        if rewritten == 0 {
            continue;
        }
        rewritten_keys += rewritten;
        compared += 1;
        let full = bytes(&doc);
        let compact = outcome(&original);
        if compact.starts_with("err Json") {
            // A compact value of the wrong shape is a serde error with byte positions; the same
            // value under a full-IRI key is refused by the resolver instead. Both refuse.
            assert!(outcome(&full).starts_with("err "), "{rel}");
            continue;
        }
        assert_eq!(outcome(&full), compact, "{rel}");
        if let Ok((graph, _)) = import_cxf(&original, &ResolveOptions::default()) {
            let (expanded, _) = import_cxf(&full, &ResolveOptions::default()).unwrap();
            assert_eq!(
                export(&expanded).ok(),
                export(&graph).ok(),
                "{rel}: export bytes"
            );
        }
    }
    // The corpus is large; a walk that silently stops rewriting fails here.
    assert!(compared >= 150, "only {compared} fixtures compared");
    assert!(
        rewritten_keys >= 10_000,
        "only {rewritten_keys} keys rewritten"
    );
}

#[test]
fn a_property_written_twice_is_refused_naming_both_spellings() {
    // Compact plus full IRI.
    let mut doc = fixture();
    node_mut(&mut doc, SOURCE_PORT).insert(format!("{S231P}max"), json!(10.0));
    let diags = refusal(&bytes(&doc));
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, DiagCode::MalformedDocument);
    assert_eq!(diags[0].subject.as_deref(), Some(SOURCE_PORT));
    assert_eq!(
        diags[0].message,
        format!(
            "node `{SOURCE_PORT}` writes `S231:max` twice, as `S231:max` and as \
             `{S231P}max`; a property must have exactly one spelling"
        )
    );

    // Two expanded spellings, no compact key — even with the same value.
    let mut doc = fixture();
    doc["@context"]["s231p"] = json!(S231P);
    let node = node_mut(&mut doc, SOURCE_PORT);
    node.remove("S231:unit");
    node.insert(format!("{S231P}unit"), json!("K"));
    node.insert("s231p:unit".into(), json!("K"));
    let diags = refusal(&bytes(&doc));
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, DiagCode::MalformedDocument);
    assert_eq!(
        diags[0].message,
        format!(
            "node `{SOURCE_PORT}` writes `S231:unit` twice, as `{S231P}unit` and as \
             `s231p:unit`; a property must have exactly one spelling"
        )
    );

    // A structural edge too: a second hasOutput spelling on the root is not merged.
    let mut doc = fixture();
    node_mut(&mut doc, "http://example.org#A")
        .insert(format!("{S231P}hasOutput"), json!({"@id": BOUNDARY_OUTPUT}));
    let diags = refusal(&bytes(&doc));
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, DiagCode::MalformedDocument);
    assert_eq!(diags[0].subject.as_deref(), Some("http://example.org#A"));
}

/// `fixture()` with its `S231` binding replaced (`None` removes it).
fn bound_to(namespace: Option<&str>) -> Json {
    let mut doc = fixture();
    let context = doc["@context"].as_object_mut().unwrap();
    match namespace {
        Some(namespace) => context.insert("S231".into(), json!(namespace)),
        None => context.remove("S231"),
    };
    doc
}

/// The single foreign-namespace refusal for `min` spelled in `namespace`.
fn assert_foreign_refusal(doc: &Json, namespace: &str, binding: &str) {
    let mut doc = doc.clone();
    respell(&mut doc, SOURCE_PORT, "min", &|t| format!("{namespace}{t}"));
    let diags = refusal(&bytes(&doc));
    assert_eq!(diags.len(), 1, "{namespace}: {diags:?}");
    assert_eq!(diags[0].code, DiagCode::NonSubsetConstruct, "{namespace}");
    assert_eq!(diags[0].subject.as_deref(), Some(SOURCE_PORT));
    assert_eq!(
        diags[0].message,
        format!(
            "property `{namespace}min` on node `{SOURCE_PORT}` names the modeled term `min` in \
             `{namespace}`, but the document {binding}; write it as `S231:min`"
        )
    );
}

const S231_LEGACY: &str = "http://data.ashrae.org/S231#";
const HTTPS_S231P: &str = "https://data.ashrae.org/S231P#";
const HTTPS_S231_LEGACY: &str = "https://data.ashrae.org/S231#";

#[test]
fn only_the_documents_own_s231_namespace_normalizes_in_either_direction() {
    // (binding, own namespace, the binding as the refusal message states it)
    let unbound = format!("binds no `S231`, so its modeled keys are read in `{S231P}`");
    let cases = [
        (Some(S231P), S231P, format!("binds `S231` to `{S231P}`")),
        (
            Some(S231_LEGACY),
            S231_LEGACY,
            format!("binds `S231` to `{S231_LEGACY}`"),
        ),
        (None, S231P, unbound),
    ];
    for (binding, own, stated) in cases {
        let doc = bound_to(binding);
        let baseline = outcome(&bytes(&doc));
        assert!(baseline.starts_with("ok "), "{binding:?}: {baseline}");
        // The own namespace normalizes to exactly the compact outcome.
        let mut same = doc.clone();
        respell(&mut same, SOURCE_PORT, "min", &|t| format!("{own}{t}"));
        assert_eq!(outcome(&bytes(&same)), baseline, "{binding:?}");
        // Every other known namespace is refused, never dropped.
        for foreign in [S231P, S231_LEGACY, HTTPS_S231P, HTTPS_S231_LEGACY] {
            if foreign != own {
                assert_foreign_refusal(&doc, foreign, &stated);
            }
        }
    }
    // A document bound to an unrelated namespace refuses both known namespaces.
    let custom = "http://example.org/s231#";
    let doc = bound_to(Some(custom));
    for foreign in [S231P, S231_LEGACY] {
        assert_foreign_refusal(&doc, foreign, &format!("binds `S231` to `{custom}`"));
    }
}

#[test]
fn a_key_that_names_a_modeled_property_only_after_trimming_is_refused() {
    let baseline = outcome(&bytes(&fixture()));
    for key in [
        "S231:max ".to_owned(),
        " S231:max".to_owned(),
        format!("{S231P}max\t"),
        format!(" {S231_LEGACY}max"),
    ] {
        let mut doc = fixture();
        node_mut(&mut doc, SOURCE_PORT).insert(key.clone(), json!(1.0));
        let diags = refusal(&bytes(&doc));
        assert_eq!(diags.len(), 1, "{key:?}: {diags:?}");
        assert_eq!(diags[0].code, DiagCode::MalformedDocument);
        assert_eq!(diags[0].subject.as_deref(), Some(SOURCE_PORT));
        assert_eq!(
            diags[0].message,
            format!(
                "property key {key:?} on node `{SOURCE_PORT}` carries surrounding whitespace; \
                 without it, it names the modeled term `max`; write it as `S231:max`"
            )
        );
    }
    // Whitespace around an unmodeled key is still just a pass-through key.
    let mut doc = fixture();
    node_mut(&mut doc, SOURCE_PORT).insert(" cdlLineNumStart ".into(), json!(1));
    node_mut(&mut doc, SOURCE_PORT).insert("S231:documentation ".into(), json!("x"));
    assert_eq!(outcome(&bytes(&doc)), baseline);
}

#[test]
fn an_explicit_null_or_empty_compact_key_still_counts_as_a_spelling() {
    let mut doc = fixture();
    let port = node_mut(&mut doc, SOURCE_PORT);
    port.insert("S231:min".into(), Json::Null);
    port.insert(format!("{S231P}min"), json!(3.0));
    let diags = refusal(&bytes(&doc));
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, DiagCode::MalformedDocument);
    assert_eq!(
        diags[0].message,
        format!(
            "node `{SOURCE_PORT}` writes `S231:min` twice, as `S231:min` and as `{S231P}min`; \
             a property must have exactly one spelling"
        )
    );
    // An empty structural edge is the same: `[]` plus an expanded edge is two spellings.
    let mut doc = fixture();
    let root = node_mut(&mut doc, "http://example.org#A");
    root.insert("S231:hasOutput".into(), json!([]));
    root.insert(format!("{S231P}hasOutput"), json!({"@id": BOUNDARY_OUTPUT}));
    let diags = refusal(&bytes(&doc));
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, DiagCode::MalformedDocument);
    assert_eq!(diags[0].subject.as_deref(), Some("http://example.org#A"));
    // Without a second spelling, an explicit null stays the old "absent".
    let mut doc = fixture();
    node_mut(&mut doc, SOURCE_PORT).insert("S231:nominal".into(), Json::Null);
    assert_eq!(outcome(&bytes(&doc)), outcome(&bytes(&fixture())));
}

#[test]
fn a_document_with_a_refused_context_reports_only_the_context_refusal() {
    // `@base` refuses the context; the node also writes `max` twice, which is not reported.
    let mut doc = fixture();
    doc["@context"]["@base"] = json!("http://example.org/");
    let only_context = refusal(&bytes(&doc));
    node_mut(&mut doc, SOURCE_PORT).insert(format!("{S231P}max"), json!(1.0));
    node_mut(&mut doc, SOURCE_PORT).insert(format!("{S231_LEGACY}min"), json!(1.0));
    assert_eq!(refusal(&bytes(&doc)), only_context);
    assert_eq!(only_context.len(), 1, "{only_context:?}");
    assert_eq!(only_context[0].code, DiagCode::NonSubsetConstruct);
}

#[test]
fn unrelated_full_iri_properties_still_pass_through_losslessly() {
    let baseline = outcome(&bytes(&fixture()));
    let mut doc = fixture();
    doc["@context"]["qudt"] = json!("http://qudt.org/schema/qudt/");
    let node = node_mut(&mut doc, SOURCE_PORT);
    let unrelated = [
        ("http://example.org#min", json!(-1000.0)),
        ("http://example.org/vocab/unit", json!("degF")),
        (
            concat!("http://data.ashrae.org/S231P#", "documentation"),
            json!("doc"),
        ),
        (
            concat!("http://data.ashrae.org/S231P#", "minimum"),
            json!(1),
        ),
        ("qudt:min", json!(5)),
        ("cdlLineNumStart", json!(12)),
    ];
    for (key, value) in &unrelated {
        node.insert((*key).to_owned(), value.clone());
    }
    let doc = bytes(&doc);
    assert_eq!(outcome(&doc), baseline, "unrelated keys change nothing");

    // Layer A keeps every unrelated key — and a modeled full-IRI key — verbatim (RT-1).
    let mut with_modeled: Json = serde_json::from_slice(&doc).unwrap();
    respell(&mut with_modeled, SOURCE_PORT, "max", &|t| {
        format!("{S231P}{t}")
    });
    let parsed = parse_document(&bytes(&with_modeled)).unwrap();
    let port = parsed.graph.iter().find(|n| n.id == SOURCE_PORT).unwrap();
    for (key, _) in &unrelated {
        assert!(port.other.contains_key(*key), "{key} kept");
    }
    assert!(port.other.contains_key(&format!("{S231P}max")));
    assert!(port.max.is_none(), "Layer A never rewrites keys");
    let written: Json = serde_json::from_slice(&write_document(&parsed).unwrap()).unwrap();
    assert_eq!(written, with_modeled, "Layer A round trip is lossless");
}

#[test]
fn a_full_iri_value_of_the_wrong_shape_is_refused_like_the_compact_parse_error() {
    let mut compact = fixture();
    node_mut(&mut compact, SOURCE_PORT).insert("S231:isArray".into(), json!("yes"));
    assert!(matches!(
        import_cxf(&bytes(&compact), &ResolveOptions::default()),
        Err(CxfError::Json(_))
    ));
    let mut full = fixture();
    node_mut(&mut full, SOURCE_PORT).insert(format!("{S231P}isArray"), json!("yes"));
    let diags = refusal(&bytes(&full));
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, DiagCode::MalformedDocument);
    assert_eq!(diags[0].subject.as_deref(), Some(SOURCE_PORT));
    assert!(
        diags[0].message.starts_with(&format!(
            "node `{SOURCE_PORT}` carries a modeled property written as `{S231P}isArray` whose \
             value does not fit the modeled field: "
        )),
        "{}",
        diags[0].message
    );
}

#[test]
fn a_scoped_context_inside_a_full_iri_value_is_still_refused() {
    let mut doc = fixture();
    node_mut(&mut doc, SOURCE_PORT).remove("S231:min");
    node_mut(&mut doc, SOURCE_PORT).insert(
        format!("{S231P}min"),
        json!({"@value": "1", "@type": DOUBLE, "@context": {"x": "urn:x"}}),
    );
    let diags = refusal(&bytes(&doc));
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, DiagCode::NonSubsetConstruct);
    assert_eq!(diags[0].subject.as_deref(), Some(SOURCE_PORT));
}

#[test]
fn full_iri_import_and_export_are_deterministic() {
    let mut doc = fixture();
    assert_eq!(expand_every_modeled_key(&mut doc), 15);
    let doc = bytes(&doc);
    let first = (outcome(&doc), export(&import(&doc)).unwrap());
    for _ in 0..8 {
        assert_eq!((outcome(&doc), export(&import(&doc)).unwrap()), first);
    }
    assert_eq!(
        first.1,
        export(&import(CONNECTOR_ATTRS.as_bytes())).unwrap()
    );
}

fn tracked_fixtures() -> Vec<(String, PathBuf)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root");
    let output = Command::new("git")
        .args(["ls-files", "-z", "--", "crates", "third_party"])
        .current_dir(&root)
        .output()
        .expect("enumerate tracked repository files");
    assert!(output.status.success(), "git ls-files failed");
    let mut corpus: Vec<_> = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| String::from_utf8(path.to_vec()).expect("tracked path is UTF-8"))
        .filter(|rel| rel.ends_with(".jsonld"))
        .map(|rel| {
            let path = root.join(&rel);
            (rel, path)
        })
        .collect();
    corpus.sort();
    corpus
}

/// `fixture()` serialized with `members` (raw `"key":value` text, repeats allowed) spliced into
/// the source port node — `serde_json::Value` cannot hold a repeated key.
fn with_raw_members(members: &str) -> Vec<u8> {
    let mut doc = fixture();
    node_mut(&mut doc, SOURCE_PORT).insert("__splice__".into(), json!(0));
    let text = String::from_utf8(bytes(&doc)).unwrap();
    assert_eq!(text.matches(r#""__splice__":0"#).count(), 1);
    text.replace(r#""__splice__":0"#, members).into_bytes()
}

fn repeated_key_message(key: &str, count: usize) -> String {
    format!(
        "node `{SOURCE_PORT}` writes the key `{key}` {count} times; a key may appear once per \
         node, and a repeat would silently keep only one of the values"
    )
}

#[test]
fn a_repeated_expanded_key_is_refused_rather_than_keeping_one_value() {
    let key = format!("{S231P}max");
    for (members, count) in [
        (format!(r#""{key}":9,"{key}":350.0"#), 2),
        (format!(r#""{key}":350.0,"{key}":9"#), 2),
        (format!(r#""{key}":1,"{key}":2,"{key}":3"#), 3),
    ] {
        // The compact `S231:max` is removed so only the repeated expanded key remains.
        let mut doc = fixture();
        node_mut(&mut doc, SOURCE_PORT).remove("S231:max");
        node_mut(&mut doc, SOURCE_PORT).insert("__splice__".into(), json!(0));
        let text = String::from_utf8(bytes(&doc)).unwrap();
        let doc = text.replace(r#""__splice__":0"#, &members);
        let diags = refusal(doc.as_bytes());
        assert_eq!(diags.len(), 1, "{members}: {diags:?}");
        assert_eq!(diags[0].code, DiagCode::MalformedDocument);
        assert_eq!(diags[0].subject.as_deref(), Some(SOURCE_PORT));
        assert_eq!(diags[0].message, repeated_key_message(&key, count));
    }
}

#[test]
fn a_repeated_pass_through_key_is_refused_too() {
    for key in ["cdlLineNumStart", "http://example.org#note", "qudt:min"] {
        let diags = refusal(&with_raw_members(&format!(r#""{key}":1,"{key}":2"#)));
        assert_eq!(diags.len(), 1, "{key}: {diags:?}");
        assert_eq!(diags[0].code, DiagCode::MalformedDocument);
        assert_eq!(diags[0].message, repeated_key_message(key, 2));
    }
    // Distinct keys are fine, and a repeat inside a value object is not a node key.
    let distinct = with_raw_members(r#""cdlLineNumStart":1,"cdlLineNumEnd":2"#);
    assert!(outcome(&distinct).starts_with("ok "));
    let nested = with_raw_members(r#""documentation":{"info":1,"info":2}"#);
    assert!(outcome(&nested).starts_with("ok "));
}

#[test]
fn compact_spellings_repeated_or_mixed_with_an_expanded_one_are_refused() {
    // Two compact keys: the DTO refuses them at parse (`duplicate field`).
    let doc = with_raw_members(r#""S231:nominal":1,"S231:nominal":2"#);
    match import_cxf(&doc, &ResolveOptions::default()) {
        Err(CxfError::Json(error)) => assert!(error.to_string().contains("duplicate field")),
        other => panic!("expected a parse refusal, got {other:?}"),
    }
    // Compact plus expanded: one property in two spellings.
    let doc = with_raw_members(&format!(r#""S231:nominal":1,"{S231P}nominal":1"#));
    let diags = refusal(&doc);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, DiagCode::MalformedDocument);
    assert_eq!(
        diags[0].message,
        format!(
            "node `{SOURCE_PORT}` writes `S231:nominal` twice, as `S231:nominal` and as \
             `{S231P}nominal`; a property must have exactly one spelling"
        )
    );
}
