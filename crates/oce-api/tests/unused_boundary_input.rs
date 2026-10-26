//! Facade pins for a declared but unread root boundary input (`unused-boundary-input`).
//!
//! The unread input is absent from `input_definitions()` by contract — every listed input must be
//! supplied exactly once per frame, and the model never reads this one — so the load receipt's
//! warning is the host's only signal. These tests read it the way a host must: through
//! `load_cxf_with_receipt` on success, and through the failure receipt when a later stage
//! refuses the same document.

use std::fs;

use oce_api::{DiagnosticSeverity, DiagnosticStage, DiagnosticSubject, Engine, OcError, Value};
use serde_json::json;

const UNREAD: &str = "http://example.org#M.v";

fn warned_fixture() -> Vec<u8> {
    fs::read(format!(
        "{}/../oce-cxf/tests/fixtures/composite_contract/warned/unused_boundary_input.jsonld",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("read warned corpus fixture")
}

/// `(stage, severity, code, subject)` of every receipt record, in receipt order.
type Row = (DiagnosticStage, DiagnosticSeverity, String, Option<String>);

fn rows(receipt: &oce_api::DiagnosticReceipt) -> Vec<Row> {
    receipt
        .records()
        .iter()
        .map(|record| {
            let key = record.key();
            let subject = match key.subject() {
                DiagnosticSubject::Opaque(text) => Some(text.clone()),
                _ => None,
            };
            (key.stage(), key.severity(), key.code().to_owned(), subject)
        })
        .collect()
}

fn unused_row() -> Row {
    (
        DiagnosticStage::Import,
        DiagnosticSeverity::Warning,
        "unused-boundary-input".to_owned(),
        Some(UNREAD.to_owned()),
    )
}

#[test]
fn the_load_receipt_names_the_unread_input_and_the_frame_contract_omits_it() {
    let mut engine = Engine::in_memory();
    let receipt = engine
        .load_cxf_with_receipt(&warned_fixture())
        .expect("an unread input is advisory, not a refusal");
    assert_eq!(rows(receipt.diagnostics()), vec![unused_row()]);
    assert_eq!(receipt.report().warnings.len(), 1);
    let paths: Vec<String> = engine
        .input_definitions()
        .expect("loaded")
        .into_iter()
        .map(|definition| definition.path)
        .collect();
    assert_eq!(paths, vec!["http://example.org#M.u".to_owned()]);
}

#[test]
fn a_frame_naming_the_unread_input_is_refused_as_unknown() {
    // The issue's downstream symptom, kept as the observable contract: the host learns of the
    // unread input from the receipt, not from this refusal.
    let mut engine = Engine::in_memory();
    engine.load_cxf(&warned_fixture()).expect("loads");
    let entries = [
        ("http://example.org#M.u", Value::Real(1.0)),
        (UNREAD, Value::Real(2.0)),
    ];
    assert!(
        matches!(
            engine.prepare_frame(1.0, &entries),
            Err(OcError::FrameUnknownInput { ref prefix, .. }) if prefix == UNREAD
        ),
        "the unread declaration is not a frame input"
    );
    engine
        .prepare_frame(1.0, &entries[..1])
        .expect("the declared contract is the read input alone");
}

#[test]
fn repeated_loads_produce_identical_receipts() {
    let bytes = warned_fixture();
    let first = Engine::in_memory().load_cxf_with_receipt(&bytes).unwrap();
    let second = Engine::in_memory().load_cxf_with_receipt(&bytes).unwrap();
    assert_eq!(rows(first.diagnostics()), rows(second.diagnostics()));
}

/// The deep-gate refusal fixture (a `K`/`degC` unit clash) with an extra connection-free root
/// input `U.spare`: resolve warns, then attribute unification refuses.
fn unit_mismatch_with_unread_input() -> Vec<u8> {
    let source = fs::read_to_string(format!(
        "{}/../oce-cxf/tests/fixtures/invalid/unit_mismatch.jsonld",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("read unit-mismatch fixture");
    let mut document: serde_json::Value = serde_json::from_str(&source).expect("fixture JSON");
    let graph = document["@graph"].as_array_mut().expect("@graph");
    graph[0]["S231:hasInput"] = json!([
        { "@id": "http://example.org#U.uIn" },
        { "@id": "http://example.org#U.spare" }
    ]);
    graph.push(json!({
        "@id": "http://example.org#U.spare",
        "@type": "S231:RealInput",
        "S231:isOfDataType": { "@id": "S231:Real" }
    }));
    serde_json::to_vec(&document).expect("serialize fixture")
}

#[test]
fn the_warning_survives_a_later_attribute_unification_failure() {
    let failure = Engine::in_memory()
        .load_cxf_with_receipt(&unit_mismatch_with_unread_input())
        .expect_err("the unit clash still refuses the load");
    assert_eq!(failure.stage(), DiagnosticStage::AttributeUnification);
    let rows = rows(failure.diagnostics());
    assert!(
        rows.contains(&(
            DiagnosticStage::Import,
            DiagnosticSeverity::Warning,
            "unused-boundary-input".to_owned(),
            Some("http://example.org#U.spare".to_owned()),
        )),
        "{rows:#?}"
    );
    assert!(
        rows.iter().any(|(stage, severity, code, _)| *stage
            == DiagnosticStage::AttributeUnification
            && *severity == DiagnosticSeverity::Error
            && code == "unit-quantity-mismatch"),
        "{rows:#?}"
    );
    let legacy = failure.into_error();
    assert!(matches!(legacy, OcError::LoadContext(_)), "{legacy:?}");
    assert!(
        legacy
            .all_diagnostics()
            .any(|d| d.code.as_str() == "unused-boundary-input")
    );
}
