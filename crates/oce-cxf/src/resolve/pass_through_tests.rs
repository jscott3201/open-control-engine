//! Boundary-type derivation over shared and separate lowered target allocations.

use std::collections::HashSet;
use std::sync::Arc;

use serde_json::json;

use super::derive_boundary_types;
use crate::dto::CxfDocument;
use oce_diag::DiagnosticList;

/// A top input wired to a long String-typed top output, so the output's type is derived only
/// when the pass-through edge is seen.
fn document(output: &str) -> CxfDocument {
    serde_json::from_value(json!({
        "@context": { "S231": "http://data.ashrae.org/S231#" },
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
    let mut diags = DiagnosticList::new();
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

/// Two pairs onto one output: an Integer input and a Real input, the output of no declared type.
/// Each pair reads the output under its own type, so each minted connector carries the attribute
/// set of its own type, and the shared reading is never handed to a pair of the other type
/// (issue #750).
#[test]
fn an_output_read_under_two_types_gives_each_pair_the_attributes_of_its_own_type() {
    use std::collections::HashMap;

    use oce_model::{Attrs, ValueType};

    use super::materialize;
    use crate::dto::Node;

    let output: Node = serde_json::from_value(json!({
        "@id": "urn:top:y", "@type": "S231:RealOutput", "S231:min": 0, "S231:max": 4
    }))
    .expect("node");
    let by_id: HashMap<&str, &Node> = HashMap::from([("urn:top:y", &output)]);
    let types = HashMap::from([
        ("urn:top:i", Some(ValueType::Integer)),
        ("urn:top:r", Some(ValueType::Real)),
        ("urn:top:j", Some(ValueType::Integer)),
    ]);
    let pairs = ["urn:top:i", "urn:top:r", "urn:top:j"]
        .map(|input| (input.to_owned(), "urn:top:y".to_owned()))
        .to_vec();
    let (mut blocks, mut connectors, mut external) = (Vec::new(), Vec::new(), Vec::new());
    let mut diags = DiagnosticList::new();
    materialize(
        pairs,
        &types,
        &by_id,
        &mut blocks,
        &mut connectors,
        &mut external,
        &mut diags,
    );
    assert!(diags.is_empty(), "{diags:?}");
    let outputs: Vec<&Attrs> = connectors
        .iter()
        .skip(1)
        .step_by(2)
        .map(|c| &c.attrs)
        .collect();
    assert_eq!(outputs.len(), 3);
    assert!(matches!(outputs[0], Attrs::Integer(a) if a.min == Some(0) && a.max == Some(4)));
    assert!(matches!(outputs[1], Attrs::Real(a) if a.min == Some(0.0) && a.max == Some(4.0)));
    assert_eq!(outputs[0], outputs[2]);
    for connector in &connectors {
        assert!(connector.attrs.matches(connector.value_type));
    }
}

/// Two outputs with ids of `id_bytes` bytes, each declaring malformed attributes that every type
/// refuses in its own way, read by fourteen pairs of mixed types (43 findings in all).
/// `materialize` must list what a read per pair straight into the import's list lists, in the
/// same order, count what it counts, and give each pair the attributes that read gives (issue
/// #750). Returns how many findings were listed and how many raised.
fn reference_case(id_bytes: usize) -> (usize, u64) {
    use std::collections::HashMap;

    use oce_model::ValueType;

    use super::super::attrs::connector_attrs;
    use super::materialize;
    use crate::dto::Node;

    let ids = [
        format!("urn:o1:{}", "a".repeat(id_bytes.saturating_sub(7))),
        format!("urn:o2:{}", "b".repeat(id_bytes.saturating_sub(7))),
    ];
    let outputs: Vec<Node> = ids
        .iter()
        .map(|id| {
            serde_json::from_value(json!({
                "@id": id, "@type": "S231:RealOutput",
                "S231:quantity": " ", "S231:unit": 5, "S231:min": "no", "S231:nominal": "no"
            }))
            .expect("node")
        })
        .collect();
    let by_id: HashMap<&str, &Node> = outputs.iter().map(|n| (n.id.as_str(), n)).collect();
    let kinds = [ValueType::Real, ValueType::Integer, ValueType::Boolean];
    let inputs: Vec<String> = (0..14).map(|i| format!("urn:i{i:02}")).collect();
    let types: HashMap<&str, Option<ValueType>> = inputs
        .iter()
        .enumerate()
        .map(|(i, input)| (input.as_str(), Some(kinds[(i * 5 + i / 3) % 3])))
        .collect();
    let pairs: Vec<(String, String)> = inputs
        .iter()
        .enumerate()
        .map(|(i, input)| (input.clone(), ids[(i * 7 / 3) % 2].clone()))
        .collect();

    let (mut blocks, mut connectors, mut external) = (Vec::new(), Vec::new(), Vec::new());
    let mut diags = DiagnosticList::new();
    materialize(
        pairs.clone(),
        &types,
        &by_id,
        &mut blocks,
        &mut connectors,
        &mut external,
        &mut diags,
    );

    let mut reference = DiagnosticList::new();
    let attrs: Vec<_> = pairs
        .iter()
        .map(|(input, output)| {
            let value_type = types[input.as_str()].expect("typed");
            connector_attrs(by_id[output.as_str()], value_type, &mut reference)
        })
        .collect();
    assert_eq!(
        format!("{diags:?}"),
        format!("{reference:?}"),
        "{id_bytes}-byte ids"
    );
    assert_eq!(diags.raised(), reference.raised());
    assert_eq!(diags.has_ended(), reference.has_ended());
    let minted: Vec<_> = connectors
        .iter()
        .skip(1)
        .step_by(2)
        .map(|c| c.attrs.clone())
        .collect();
    assert_eq!(minted, attrs);
    let listed = diags.len() - usize::from(diags.has_ended());
    (listed, diags.raised())
}

#[test]
fn every_pair_raises_what_a_read_of_its_own_raises_wherever_the_list_ends() {
    // At ids of 8 bytes every finding is listed; at longer ids the list ends after fewer and
    // fewer, and the shared readings must still list, count and order them as reads per pair.
    // Of the 43 findings, 43, 27, 11, 5 and 2 are listed.
    let mut listed = Vec::new();
    for id_bytes in [8, 300_000, 700_000, 1_400_000, 3_000_000] {
        let (count, raised) = reference_case(id_bytes);
        assert_eq!(raised, 43);
        listed.push(count);
    }
    assert_eq!(listed, [43, 27, 11, 5, 2]);
}
