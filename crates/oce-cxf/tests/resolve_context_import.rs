//! Fail-closed handling of JSON-LD `@import` context entries.

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::{DiagCode, Diagnostic};
use serde_json::{Value, json};

const IMPORT_MESSAGE: &str = "@context declares `@import`, which changes identity semantics this \
                              engine does not implement; write prefix terms or absolute IRIs \
                              instead";

fn reject_context(context: Value) -> Vec<Diagnostic> {
    let document = json!({
        "@context": context,
        "@graph": [{ "@id": "relative-root" }]
    });
    let bytes = serde_json::to_vec(&document).expect("serializable test document");
    match import_cxf(&bytes, &ResolveOptions::default()) {
        Err(CxfError::Validation(diags)) => diags,
        Err(other) => panic!("unexpected import failure: {other:?}"),
        Ok(_) => panic!("@import must refuse before identity expansion"),
    }
}

fn assert_import_refusal(diag: &Diagnostic) {
    assert_eq!(diag.code, DiagCode::NonSubsetConstruct);
    assert_eq!(diag.subject.as_deref(), Some("@import"));
    assert_eq!(diag.message, IMPORT_MESSAGE);
}

#[test]
fn every_import_payload_shape_has_the_same_non_subset_diagnostic() {
    for value in [
        json!("https://example.org/context.jsonld"),
        Value::Null,
        json!(true),
        json!(1),
        json!(["https://example.org/context.jsonld"]),
        json!({ "@id": "https://example.org/context.jsonld" }),
    ] {
        let diags = reject_context(json!({ "@import": value }));
        assert_eq!(diags.len(), 1, "{diags:#?}");
        assert_import_refusal(&diags[0]);
    }
}

#[test]
fn import_in_a_context_list_refuses_before_identity_expansion() {
    let diags = reject_context(json!([
        { "ex": "https://example.org/first#" },
        { "@import": "https://example.org/context.jsonld" },
        { "ex": "https://example.org/later#" }
    ]));
    assert_eq!(
        diags.len(),
        1,
        "slot diagnostics must not be appended: {diags:#?}"
    );
    assert_import_refusal(&diags[0]);
}

#[test]
fn repeated_imports_and_other_identity_keywords_aggregate_deterministically() {
    let diags = reject_context(json!([
        { "@import": "https://example.org/first.jsonld" },
        { "@import": null },
        {
            "@base": "https://example.org/",
            "@vocab": "https://example.org/vocab#"
        }
    ]));
    assert_eq!(diags.len(), 4, "{diags:#?}");
    let subjects: Vec<&str> = diags
        .iter()
        .map(|diag| diag.subject.as_deref().expect("context diagnostic subject"))
        .collect();
    assert_eq!(subjects, ["@base", "@import", "@import", "@vocab"]);
    assert_import_refusal(&diags[1]);
    assert_import_refusal(&diags[2]);
    assert!(
        diags
            .iter()
            .all(|diag| diag.code == DiagCode::NonSubsetConstruct),
        "context validation must return before the relative root is inspected"
    );
}
