//! Node-scoped JSON-LD contexts fail closed before identity expansion and indexing.

use std::fmt::Write;

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::{DiagCode, Diagnostic};
use serde_json::{Value, json};

const NODE_SCOPED_CONTEXTS: &[u8] = include_bytes!("fixtures/node_scoped_contexts.jsonld");
const NODE_SCOPED_CONTEXTS_GOLDEN: &str =
    include_str!("fixtures/golden/node_scoped_contexts.diagnostics.txt");

fn import_bytes(bytes: &[u8]) -> Result<(), Vec<Diagnostic>> {
    match import_cxf(bytes, &ResolveOptions::default()) {
        Ok(_) => Ok(()),
        Err(CxfError::Validation(diags)) => Err(diags),
        Err(other) => panic!("expected validation diagnostics, got {other:?}"),
    }
}

fn import_value(value: &Value) -> Result<(), Vec<Diagnostic>> {
    let bytes = serde_json::to_vec(value).expect("serialize test document");
    import_bytes(&bytes)
}

fn render_diagnostics(diags: &[Diagnostic]) -> String {
    let mut rendered = String::new();
    for diag in diags {
        writeln!(
            rendered,
            "{}|{}|{}|{}",
            diag.severity.as_str(),
            diag.code.as_str(),
            diag.subject.as_deref().unwrap_or("<none>"),
            diag.message
        )
        .expect("write diagnostic");
    }
    rendered
}

#[test]
fn node_scoped_context_shapes_refuse_before_identity_expansion() {
    let first = import_bytes(NODE_SCOPED_CONTEXTS).expect_err("scoped contexts must refuse");
    let second = import_bytes(NODE_SCOPED_CONTEXTS).expect_err("repeat must refuse");
    let rendered = render_diagnostics(&first);

    assert_eq!(rendered, NODE_SCOPED_CONTEXTS_GOLDEN);
    assert_eq!(rendered, render_diagnostics(&second));
    assert_eq!(first.len(), 6);
    assert!(
        first
            .iter()
            .all(|diag| diag.code == DiagCode::NonSubsetConstruct),
        "slot expansion must not add another refusal: {first:?}"
    );
}

#[test]
fn node_scoped_context_on_missing_id_precedes_missing_id_validation() {
    let document = json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" },
        "@graph": [ { "@context": {} } ]
    });
    let diags = import_value(&document).expect_err("scoped context must refuse first");

    assert_eq!(
        diags.len(),
        1,
        "context refusal must return alone: {diags:?}"
    );
    assert_eq!(diags[0].code, DiagCode::NonSubsetConstruct);
    assert_eq!(diags[0].subject.as_deref(), Some("@graph[0]"));
    assert_eq!(
        diags[0].message,
        "node at `@graph[0]` declares a scoped `@context`; node-scoped contexts are not \
         supported; declare required bindings in the document-level `@context`"
    );
}

#[test]
fn every_reference_slot_refuses_a_scoped_context_before_expansion() {
    for slot in [
        "S231:hasInput",
        "S231:hasOutput",
        "S231:hasParameter",
        "S231:hasConstant",
        "S231:containsBlock",
        "S231:hasInstance",
        "S231:isConnectedTo",
        "S231:isOfDataType",
    ] {
        let mut owner = json!({ "@id": "ex:M" });
        owner[slot] = json!({
            "@id": "local:target",
            "@context": { "local": "http://scoped.example#" }
        });
        let document = json!({
            "@context": { "ex": "http://document.example#" },
            "@graph": [owner]
        });
        let diags = import_value(&document).expect_err("reference context must refuse");

        assert_eq!(diags.len(), 1, "{slot}: {diags:?}");
        assert_eq!(diags[0].code, DiagCode::NonSubsetConstruct, "{slot}");
        assert_eq!(diags[0].subject.as_deref(), Some("ex:M"), "{slot}");
        assert_eq!(
            diags[0].message,
            format!(
                "{slot} reference `local:target` on node `ex:M` declares a scoped `@context`; \
                 node-scoped contexts are not supported; declare required bindings in the \
                 document-level `@context`"
            ),
            "{slot}"
        );
    }
}

#[test]
fn every_scoped_context_in_a_reference_list_is_reported_and_round_trips() {
    let document = json!({
        "@context": { "ex": "http://document.example#" },
        "@graph": [ {
            "@id": "ex:M",
            "S231:hasInput": [
                { "@id": "local:a", "@context": { "local": "http://first.example#" } },
                { "@id": "local:b", "@context": "http://second.example/context.jsonld" }
            ]
        } ]
    });
    let bytes = serde_json::to_vec(&document).expect("serialize test document");
    let parsed = oce_cxf::parse_document(&bytes).expect("DTO parses");
    assert_eq!(
        serde_json::to_value(parsed).expect("DTO serializes"),
        document
    );

    let diags = import_bytes(&bytes).expect_err("each reference context must refuse");
    assert_eq!(diags.len(), 2, "{diags:?}");
    assert_eq!(diags[0].subject.as_deref(), Some("ex:M"));
    assert_eq!(diags[1].subject.as_deref(), Some("ex:M"));
    assert!(
        diags
            .iter()
            .all(|diag| diag.code == DiagCode::NonSubsetConstruct),
        "{diags:?}"
    );
    assert!(diags[0].message.contains("reference `local:a`"));
    assert!(diags[1].message.contains("reference `local:b`"));
}

#[test]
fn document_and_node_context_refusals_aggregate_in_deterministic_order() {
    let document = json!({
        "@context": { "@base": "http://example.org/" },
        "@graph": [ { "@id": "ex:A", "@context": {} } ]
    });
    let first = import_value(&document).expect_err("both contexts must refuse");
    let second = import_value(&document).expect_err("repeat must refuse identically");

    assert_eq!(render_diagnostics(&first), render_diagnostics(&second));
    assert_eq!(first.len(), 2, "{first:?}");
    assert_eq!(first[0].subject.as_deref(), Some("@base"));
    assert_eq!(first[1].subject.as_deref(), Some("ex:A"));
    assert!(
        first
            .iter()
            .all(|diag| diag.code == DiagCode::NonSubsetConstruct),
        "{first:?}"
    );
}

#[test]
fn node_scoped_context_precedes_duplicate_id_indexing() {
    let document = json!({
        "@context": { "ex": "http://example.org#" },
        "@graph": [
            { "@id": "ex:A", "@context": {} },
            { "@id": "ex:A" }
        ]
    });
    let diags = import_value(&document).expect_err("scoped context must refuse before indexing");

    assert_eq!(
        diags.len(),
        1,
        "context refusal must return alone: {diags:?}"
    );
    assert_eq!(diags[0].code, DiagCode::NonSubsetConstruct);
    assert_eq!(diags[0].subject.as_deref(), Some("ex:A"));
}

#[test]
fn node_scoped_context_values_remain_lossless_in_the_dto() {
    let source: Value = serde_json::from_slice(NODE_SCOPED_CONTEXTS).expect("fixture parses");
    let parsed = oce_cxf::parse_document(NODE_SCOPED_CONTEXTS).expect("DTO parses");
    let serialized = serde_json::to_value(parsed).expect("DTO serializes");

    assert_eq!(serialized, source);
}
