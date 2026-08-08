//! Reading why a load was refused, through `oce-api` alone.
//!
//! The resolver seam is the one that was unreachable. Its payload sits in a tuple variant of
//! `CxfError`, so destructuring it needs that type's path and therefore a dependency on `oce-cxf`;
//! the deep-gate seam's payload is a struct field and was always readable. These tests exercise the
//! accessor the way an external consumer must — through `OcError` only, never naming `CxfError` —
//! so a refactor that reopened the gap by moving the payload would red here.
//!
//! The discrimination test is the load-bearing one. Both refusals below print the identical
//! sentence, because `CxfError::Validation`'s `Display` renders a count and nothing else, so a
//! consumer reading the message cannot tell them apart at all.

use crate::{Engine, OcError};

/// A document whose `containsBlock` names a node that is not in the graph.
const UNRESOLVED_REFERENCE: &[u8] = br#"{
  "@context": {"S231": "http://data.ashrae.org/standard231/1.0/Model/"},
  "@graph": [
    {"@id": "urn:probe#model", "@type": "S231:Block",
     "S231:containsBlock": [{"@id": "urn:probe#absent"}]}
  ]
}"#;

/// A document declaring the same subject IRI twice.
const DUPLICATE_ID: &[u8] = br#"{
  "@context": {"S231": "http://data.ashrae.org/standard231/1.0/Model/"},
  "@graph": [
    {"@id": "urn:probe#twice", "@type": "S231:Block"},
    {"@id": "urn:probe#twice", "@type": "S231:Block"}
  ]
}"#;

/// Not JSON at all — refused before any diagnostic can be raised.
const MALFORMED_JSON: &[u8] = b"{not json";

/// Refused by the resolver with a WARNING sorted ahead of its errors — the case that makes reading
/// element 0 wrong.
const WARNING_BEFORE_ERRORS: &str = include_str!(
    "../../../oce-cxf/tests/fixtures/composite_contract/rejected/partial_port_declaration.jsonld"
);

/// Resolves, then refused by the deep gate — the other producing site.
const DEEP_GATE_REFUSAL: &str =
    include_str!("../../../oce-cxf/tests/fixtures/invalid/unit_mismatch.jsonld");

fn refuse(bytes: &[u8]) -> OcError {
    Engine::in_memory()
        .load_cxf(bytes)
        .expect_err("fixture must be refused")
}

/// The codes of a refusal's **error**-severity diagnostics, which is what a consumer classifying a
/// rejection actually wants. Indexing the slice instead would read a warning on the resolver seam.
fn error_codes(err: &OcError) -> Vec<&'static str> {
    err.diagnostics()
        .iter()
        .filter(|d| d.is_error())
        .map(|d| d.code.as_str())
        .collect()
}

#[test]
fn a_resolver_refusal_reports_its_diagnostics_through_the_facade() {
    let err = refuse(UNRESOLVED_REFERENCE);
    let diagnostics = err.diagnostics();
    assert!(
        !diagnostics.is_empty(),
        "a resolver refusal must carry its diagnostics through the facade, got none from {err}"
    );
    let offending = diagnostics
        .iter()
        .find(|d| d.is_error())
        .expect("a refusal must carry at least one error-severity diagnostic");
    assert_eq!(
        offending.subject.as_deref(),
        Some("urn:probe#absent"),
        "the diagnostic must name the offending node, not the document"
    );
}

#[test]
fn a_deep_gate_refusal_reports_its_diagnostics_through_the_facade() {
    // The other producing site. Without this the file's non-empty claim would be demonstrated for
    // one of the two variants it names, and dropping the `Validate` arm from the accessor would be
    // caught only by tests elsewhere in the crate.
    let err = refuse(DEEP_GATE_REFUSAL.as_bytes());
    assert!(
        matches!(err, OcError::Validate(_)),
        "this fixture must resolve and then fail the deep gate, got {err:?}"
    );
    assert!(
        !error_codes(&err).is_empty(),
        "a deep-gate refusal must carry its diagnostics through the facade, got none from {err}"
    );
}

#[test]
fn a_resolver_refusal_can_lead_with_a_warning() {
    // Why `error_codes` filters instead of indexing. The resolver seam returns the whole finalized
    // stream in its pinned order, not severity order, so the first element can be a warning while
    // the errors that caused the refusal sit behind it. The deep-gate seam cannot do this — it is
    // filtered to errors before the error is built — and that asymmetry is the trap.
    let err = refuse(WARNING_BEFORE_ERRORS.as_bytes());
    let diagnostics = err.diagnostics();
    assert!(
        !diagnostics[0].is_error(),
        "fixture no longer leads with a warning, so it cannot pin the indexing trap"
    );
    assert!(
        diagnostics.iter().any(|d| d.is_error()),
        "a refusal must still carry an error behind the leading warning"
    );
    assert!(
        !error_codes(&err).is_empty(),
        "filtering by severity must recover the errors the leading warning hides"
    );
}

#[test]
fn two_refusals_with_one_display_string_are_told_apart_by_code() {
    let unresolved = refuse(UNRESOLVED_REFERENCE);
    let duplicate = refuse(DUPLICATE_ID);

    // The whole reason the accessor exists: the rendered message is the same sentence for both.
    assert_eq!(
        unresolved.to_string(),
        duplicate.to_string(),
        "if these ever render differently, this test no longer pins what it was written to pin"
    );
    assert_ne!(
        error_codes(&unresolved),
        error_codes(&duplicate),
        "two documents refused for unrelated reasons must be distinguishable by code"
    );
}

#[test]
fn a_refusal_carrying_no_diagnostics_reports_an_empty_slice() {
    let err = refuse(MALFORMED_JSON);
    assert!(
        err.diagnostics().is_empty(),
        "a malformed-JSON refusal raises no diagnostic, so the slice must be empty, not fabricated"
    );
}

#[test]
fn a_diagnostic_bearing_refusal_never_reports_an_empty_slice() {
    // What makes the empty slice unambiguous: emptiness identifies the failures that carry no
    // diagnostics, so a consumer can fall back to `Display` on exactly those and no others. Both
    // producing sites construct only from a non-empty vector, and both are exercised here — the
    // resolver seam by the first three, the deep gate by the last.
    for bytes in [
        UNRESOLVED_REFERENCE,
        DUPLICATE_ID,
        WARNING_BEFORE_ERRORS.as_bytes(),
        DEEP_GATE_REFUSAL.as_bytes(),
    ] {
        let err = refuse(bytes);
        assert!(
            !err.diagnostics().is_empty(),
            "a refusal that raised diagnostics must not report an empty slice: {err}"
        );
    }
}
