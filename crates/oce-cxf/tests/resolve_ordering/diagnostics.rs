//! Final diagnostic order must override pass/emission order, not just repeat an already sorted pass.

use super::documents::{COMPACT, EXPANDED, node_mut, reverse_object_keys};
use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::{DiagCode, Diagnostic};
use serde_json::{Value, json};

fn reject(bytes: &[u8]) -> Vec<Diagnostic> {
    match import_cxf(bytes, &ResolveOptions::default()) {
        Err(CxfError::Validation(diags)) => diags,
        other => panic!("expected typed diagnostic refusal, got {other:?}"),
    }
}

#[test]
fn diagnostics_sort_by_dense_connector_then_code_and_message_across_spellings() {
    // Bounds diagnose before wiring; missing targets before single-assignment. Final sorting
    // must interleave those passes by dense ID, then put non-connectors in lexical subject order.
    let expected: Vec<_> = [
        (
            DiagCode::MalformedDocument,
            "urn:order:M.branch.aSum.u2",
            "S231:max on a Real connector did not ground to a number",
        ),
        (
            DiagCode::MalformedDocument,
            "urn:order:M.branch.aSum.u2",
            "S231:min on a Real connector did not ground to a number",
        ),
        (
            DiagCode::SingleAssignment,
            "urn:order:M.branch.aSum.u2",
            "input is undriven (in-degree 0)",
        ),
        (
            DiagCode::SingleAssignment,
            "urn:order:M.tail.u1",
            "input is undriven (in-degree 0)",
        ),
        (
            DiagCode::SingleAssignment,
            "urn:order:M.branch.aSum.u1",
            "input is undriven (in-degree 0)",
        ),
        (
            DiagCode::MalformedDocument,
            "urn:order:M.tail.u2",
            "S231:min on a Real connector did not ground to a number",
        ),
        (
            DiagCode::SingleAssignment,
            "urn:order:M.tail.u2",
            "input is undriven (in-degree 0)",
        ),
        (
            DiagCode::UnresolvedReference,
            "urn:order:missingA",
            "connection target not found",
        ),
        (
            DiagCode::UnresolvedReference,
            "urn:order:missingZ",
            "connection target not found",
        ),
    ]
    .into_iter()
    .map(|(code, subject, message)| Diagnostic::error(code, message).with_subject(subject))
    .collect();
    for (source, prefix) in [(COMPACT, "m:"), (EXPANDED, "urn:order:")] {
        let mut doc: Value = serde_json::from_str(source).unwrap();
        node_mut(&mut doc, &format!("{prefix}M.branch.zSource.y"))["S231:isConnectedTo"] =
            json!([{"@id":format!("{prefix}missingZ")},{"@id":format!("{prefix}missingA")}]);
        let input = node_mut(&mut doc, &format!("{prefix}M.branch.aSum.u2"));
        input["S231:min"] = json!(false);
        input["S231:max"] = json!(true);
        node_mut(&mut doc, &format!("{prefix}M.tail.u2"))["S231:min"] = json!(false);
        for bytes in [
            serde_json::to_vec(&doc).unwrap(),
            reverse_object_keys(&doc).into_bytes(),
        ] {
            for _ in 0..16 {
                assert_eq!(reject(&bytes), expected);
            }
        }
    }
}

#[test]
fn subjectless_diagnostics_precede_named_nonconnector_diagnostics() {
    let doc = json!({"@context":{}, "@graph":[
        {"@id":"urn:order:orphan", "S231:hasInput":{"@id":"urn:order:array"}},
        {"@id":"urn:order:array", "S231:isArray":true}
    ]});
    let expected = vec![
        Diagnostic::error(DiagCode::MalformedDocument,
            "composite/root-count: expected exactly one top composite root after nested classification, found zero candidate roots"),
        Diagnostic::error(DiagCode::NonSubsetConstruct,
            "composite/array-connector: array-valued connector nodes are not supported; flatten the array to one connector per element")
            .with_subject("urn:order:array"),
    ];
    assert_eq!(reject(reverse_object_keys(&doc).as_bytes()), expected);
}
