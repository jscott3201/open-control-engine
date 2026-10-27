//! A boundary input whose `S231:min`/`S231:max` grounds to NaN or an infinity is refused at load
//! with `non-finite-bound`, instead of loading and then refusing every frame.
//!
//! The CXF documents are the authored `legacy_frame_add` fixture with one attribute added; the
//! expected refusals are stated here, not taken from the engine.

use oce_api::{DiagnosticSeverity, DiagnosticStage, Engine, OcError, Value};
use serde_json::json;

const MODEL: &[u8] = include_bytes!("fixtures/legacy_frame_add.jsonld");
const A: &str = "urn:legacy-frame:a";
const B: &str = "urn:legacy-frame:b";
const XSD_DOUBLE: &str = "http://www.w3.org/2001/XMLSchema#double";

/// `MODEL` with `attribute` set to `literal` on the node `node`.
fn with_attribute(node: &str, attribute: &str, literal: serde_json::Value) -> Vec<u8> {
    let mut document: serde_json::Value = serde_json::from_slice(MODEL).unwrap();
    let target = document["@graph"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entry| entry["@id"] == node)
        .expect("node present");
    target[attribute] = literal;
    serde_json::to_vec(&document).unwrap()
}

fn typed(lexical: &str) -> serde_json::Value {
    json!({"@value": lexical, "@type": XSD_DOUBLE})
}

fn codes_and_subjects(error: &OcError) -> Vec<(String, Option<String>)> {
    error
        .all_diagnostics()
        .filter(|d| d.is_error())
        .map(|d| {
            (
                d.code.as_str().to_owned(),
                d.subject.as_deref().map(str::to_owned),
            )
        })
        .collect()
}

#[test]
fn non_finite_boundary_and_target_bounds_are_refused_at_load() {
    // The boundary declaration itself, and the child input it is elided onto.
    for node in [A, "urn:legacy-frame:add.u1"] {
        for attribute in ["S231:min", "S231:max"] {
            for lexical in ["NaN", "-NaN", "INF", "inf", "-inf", "infinity"] {
                let bytes = with_attribute(node, attribute, typed(lexical));
                let mut engine = Engine::in_memory();
                engine.load_cxf(MODEL).unwrap();
                let before = engine.state_snapshot().unwrap();
                let error = engine.load_cxf(&bytes).expect_err("non-finite bound");
                // Exactly one refusal. The elided child input answers to the boundary path, so
                // both placements name the boundary input the host would have supplied.
                assert_eq!(
                    codes_and_subjects(&error),
                    [("non-finite-bound".to_owned(), Some(A.to_owned()))],
                    "{node} {attribute}={lexical}"
                );
                // A failed load preserves the previous model, which still accepts a frame.
                assert_eq!(
                    engine.state_snapshot().unwrap().as_bytes(),
                    before.as_bytes()
                );
                engine
                    .prepare_frame(0.0, &[(A, Value::Real(1.0)), (B, Value::Real(2.0))])
                    .unwrap();
            }
        }
    }
}

#[test]
fn the_failure_receipt_names_the_stage_code_and_subject() {
    let bytes = with_attribute(A, "S231:min", typed("NaN"));
    let failure = Engine::in_memory()
        .load_cxf_with_receipt(&bytes)
        .expect_err("NaN min");
    assert_eq!(failure.stage(), DiagnosticStage::AttributeUnification);
    let rows: Vec<_> = failure
        .diagnostics()
        .records()
        .iter()
        .map(|record| {
            let key = record.key();
            (key.stage(), key.severity(), key.code().to_owned())
        })
        .collect();
    assert_eq!(
        rows,
        [(
            DiagnosticStage::AttributeUnification,
            DiagnosticSeverity::Error,
            "non-finite-bound".to_owned()
        )]
    );
    // Repeat loads produce the identical refusal.
    for _ in 0..3 {
        let again = Engine::in_memory()
            .load_cxf_with_receipt(&bytes)
            .expect_err("NaN min");
        assert_eq!(
            codes_and_subjects(&again.into_error()),
            codes_and_subjects(&Engine::in_memory().load_cxf(&bytes).unwrap_err())
        );
    }
}

#[test]
fn finite_extreme_bounds_still_load_and_bound_the_frame() {
    for (attribute, lexical, inside, outside) in [
        ("S231:min", "-1.7976931348623157e308", f64::MIN, None),
        ("S231:max", "1.7976931348623157e308", f64::MAX, None),
        ("S231:min", "4.9e-324", 5e-324, Some(0.0)),
        ("S231:max", "-0.0", -0.0, Some(5e-324)),
    ] {
        let mut engine = Engine::in_memory();
        engine
            .load_cxf(&with_attribute(A, attribute, typed(lexical)))
            .unwrap_or_else(|error| panic!("{attribute}={lexical}: {error}"));
        engine
            .prepare_frame(0.0, &[(A, Value::Real(inside)), (B, Value::Real(0.0))])
            .unwrap();
        if let Some(outside) = outside {
            assert!(matches!(
                engine.prepare_frame(0.0, &[(A, Value::Real(outside)), (B, Value::Real(0.0))]),
                Err(OcError::InputDomain(p)) if p == A
            ));
        }
    }
}

#[test]
fn a_non_finite_nominal_in_cxf_is_not_read_so_it_cannot_reach_the_engine() {
    // CXF import does not parse `S231:nominal` (it is excluded from §7.10 unification), so the
    // attribute never reaches the model and cannot be refused there: the document loads without
    // any diagnostic (the key is known, merely unread), and frames are bounded only by min/max.
    // A non-finite nominal that does reach a model graph is refused as `non-finite-bound`
    // (pinned in oce-validate's `finite_bounds` tests).
    for lexical in ["NaN", "inf", "-inf"] {
        for node in [A, "urn:legacy-frame:add.u1"] {
            let bytes = with_attribute(node, "S231:nominal", typed(lexical));
            let mut engine = Engine::in_memory();
            let report = engine
                .load_cxf(&bytes)
                .unwrap_or_else(|error| panic!("{node} nominal={lexical}: {error}"));
            assert!(report.warnings.is_empty(), "{node} nominal={lexical}");
            engine
                .prepare_frame(0.0, &[(A, Value::Real(1.0)), (B, Value::Real(2.0))])
                .unwrap();
        }
    }
}
