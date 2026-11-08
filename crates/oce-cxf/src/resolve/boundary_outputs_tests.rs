//! Emission order of boundary-output single-assignment refusals.

use std::collections::{HashMap, HashSet};

use oce_diag::DiagCode;

use super::refuse_multiply_driven;

fn drivers(outputs: &[(&str, &[&str])]) -> HashMap<String, HashSet<String>> {
    outputs
        .iter()
        .map(|(output, sources)| {
            let sources = sources.iter().map(|source| (*source).to_owned()).collect();
            ((*output).to_owned(), sources)
        })
        .collect()
}

#[test]
fn multiply_driven_outputs_are_refused_in_iri_order_not_hash_order() {
    // Sixteen multiply driven outputs: a hash-order emission would be sorted with probability
    // 1/16!, so a passing run is evidence of the sort, not luck.
    let names: Vec<String> = (0..16).rev().map(|i| format!("urn:top:y{i:02}")).collect();
    let mut table: Vec<(&str, &[&str])> = names
        .iter()
        .map(|name| (name.as_str(), &["urn:a", "urn:b"][..]))
        .collect();
    table.push(("urn:top:single", &["urn:a"][..]));
    let mut diags = Vec::new();
    refuse_multiply_driven(&drivers(&table), &mut diags);

    let subjects: Vec<&str> = diags
        .iter()
        .map(|diag| diag.subject.as_deref().expect("refusal names its output"))
        .collect();
    let mut expected: Vec<&str> = names.iter().map(String::as_str).collect();
    expected.sort_unstable();
    assert_eq!(subjects, expected);
    assert!(
        diags
            .iter()
            .all(|diag| diag.code == DiagCode::SingleAssignment
                && diag.message == "boundary output is multiply driven (distinct drivers 2)")
    );
}

#[test]
fn distinct_driver_count_is_reported_and_single_drivers_are_not_refused() {
    let mut diags = Vec::new();
    refuse_multiply_driven(
        &drivers(&[
            ("urn:top:b", &["urn:x", "urn:y", "urn:z"]),
            ("urn:top:a", &["urn:x"]),
            ("urn:top:c", &[]),
        ]),
        &mut diags,
    );
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].subject.as_deref(), Some("urn:top:b"));
    assert_eq!(
        diags[0].message,
        "boundary output is multiply driven (distinct drivers 3)"
    );
}
