//! Boundary-type derivation over shared and separate lowered target allocations.

use std::collections::HashSet;
use std::sync::Arc;

use serde_json::json;

use super::derive_boundary_types;
use crate::dto::CxfDocument;

/// A top input wired to a long String-typed top output, so the output's type is derived only
/// when the pass-through edge is seen.
fn document(output: &str) -> CxfDocument {
    serde_json::from_value(json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" },
        "@graph": [
            { "@id": "urn:top:u", "@type": "S231:StringInput",
              "S231:isOfDataType": { "@id": "S231:String" } },
            { "@id": output, "@type": "S231:StringOutput",
              "S231:isOfDataType": { "@id": "S231:String" } }
        ]
    }))
    .expect("document")
}

/// Derive the boundary types for `edges` and render them with their diagnostics.
fn derived(doc: &CxfDocument, output: &str, edges: &[(&str, Vec<Arc<str>>)]) -> String {
    let boundary_in = HashSet::from(["urn:top:u"]);
    let boundary_out = HashSet::from([output]);
    let mut diags = Vec::new();
    let types = derive_boundary_types(
        doc,
        edges
            .iter()
            .map(|(source, targets)| (*source, targets.as_slice())),
        &boundary_in,
        &boundary_out,
        &mut diags,
    );
    let mut types: Vec<_> = types
        .into_iter()
        .map(|(iri, value_type)| (iri.to_owned(), format!("{value_type:?}")))
        .collect();
    types.sort();
    format!("{types:?}\n{diags:?}")
}

/// One shared allocation per repeated target and a separate allocation per edge derive the same
/// types: skipping a repeated allocation never decides a result (GitHub issue #480).
#[test]
fn shared_and_separate_target_allocations_derive_identical_types() {
    let output = format!("urn:top:y{}", "x".repeat(4096));
    let doc = document(&output);
    let long: Arc<str> = Arc::from(output.as_str());
    let other: Arc<str> = Arc::from("urn:child:u");
    let shared = vec![(
        "urn:top:u",
        vec![
            Arc::clone(&other),
            Arc::clone(&long),
            Arc::clone(&long),
            Arc::clone(&other),
            Arc::clone(&long),
        ],
    )];
    let separate: Vec<(&str, Vec<Arc<str>>)> = shared
        .iter()
        .map(|(source, targets)| {
            (
                *source,
                targets
                    .iter()
                    .map(|target| Arc::from(target.to_string()))
                    .collect(),
            )
        })
        .collect();
    assert!(!Arc::ptr_eq(&separate[0].1[1], &separate[0].1[2]));
    let rendered = derived(&doc, &output, &shared);
    assert_eq!(rendered, derived(&doc, &output, &separate));
    assert!(rendered.contains("Some(String)"), "{rendered}");
    // Without the pass-through edge the String output is not derived as a String.
    assert!(!derived(&doc, &output, &[]).contains("Some(String)"));
}
