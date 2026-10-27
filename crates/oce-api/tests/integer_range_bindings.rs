//! Public-facade refusal of out-of-range `integer()` bindings (issue #374) through real CXF bytes.
//!
//! `integer(x)` of a Real whose floor lies outside `[-2^63, 2^63)` used to saturate silently, so
//! `integer(1e300)` loaded as `i64::MAX`. It now refuses the load as `grounding-failed` on the
//! binding's node, both for a parameter (strict-finite grounding) and for a connector bound
//! (ordinary grounding). The `Modelica.Constants.inf` alias is finite (`f64::MAX`), and its
//! documented finite arithmetic is pinned here at the facade. Expected values are stated by hand.

use oce_api::{DiagnosticReceipt, DiagnosticStage, Engine, OcError};
use oce_diag::DiagCode;
use oce_model::Value;
use serde_json::{Value as Json, json};

const MODEL: &str = include_str!("../../oce-cxf/tests/fixtures/minimal_loop.jsonld");
const GAIN_K: &str = "http://example.org#MinLoop.gain.k";
const BOUNDS_MODEL: &[u8] = include_bytes!("fixtures/legacy_frame_add.jsonld");
const A: &str = "urn:legacy-frame:a";
const REFUSAL: &str = "expression binding did not ground: domain error: integer() argument is \
                       outside the Integer range [-2^63, 2^63)";

/// `document` with `attribute` on node `node` set to `value`.
fn with(document: &[u8], node: &str, attribute: &str, value: Json) -> Vec<u8> {
    let mut doc: Json = serde_json::from_slice(document).expect("fixture is JSON");
    let entry = doc["@graph"]
        .as_array_mut()
        .expect("@graph array")
        .iter_mut()
        .find(|entry| entry["@id"] == node)
        .expect("node present");
    entry[attribute] = value;
    serde_json::to_vec(&doc).expect("serialize mutated fixture")
}

fn errors(error: &OcError) -> Vec<(DiagCode, Option<String>, String)> {
    error
        .all_diagnostics()
        .filter(|diag| diag.is_error())
        .map(|diag| {
            (
                diag.code,
                diag.subject.as_deref().map(str::to_owned),
                diag.message.clone(),
            )
        })
        .collect()
}

#[test]
fn out_of_range_integer_parameter_bindings_refuse_the_load() {
    for expression in [
        // 2^63 in every spelling, the first f64 below -2^63, and the reported repros.
        "integer(9.2233720368547758e18)",
        "integer(9223372036854775808.0)",
        "integer(9223372036854775807.0)",
        "integer(-9223372036854777856.0)",
        "integer(1e300)",
        "integer(-1e300)",
        "integer(9.3e18)",
        "integer(Modelica.Constants.inf)",
        "integer(-inf) * 1.0",
        "1.0 * integer(1e19)",
    ] {
        let mut engine = Engine::in_memory();
        let error = engine
            .load_cxf(&with(
                MODEL.as_bytes(),
                GAIN_K,
                "S231:value",
                json!(expression),
            ))
            .expect_err(expression);
        assert!(
            matches!(&error, OcError::Cxf(oce_cxf::CxfError::Validation(_))),
            "{expression}: {error:?}"
        );
        assert_eq!(
            errors(&error),
            [(
                DiagCode::GroundingFailed,
                Some(GAIN_K.to_owned()),
                REFUSAL.to_owned()
            )],
            "{expression}"
        );
        assert!(engine.params().is_empty(), "{expression}: nothing loaded");
    }
}

#[test]
fn in_range_integer_and_finite_inf_alias_arithmetic_load_bit_exactly() {
    for (expression, expected) in [
        // -2^63 is exactly i64::MIN; 2^63 - 1024 is the last representable in-range Real.
        (
            "integer(-9223372036854775808.0) * 1.0",
            -9_223_372_036_854_775_808.0,
        ),
        (
            "integer(9223372036854774784.0) * 1.0",
            9_223_372_036_854_774_784.0,
        ),
        ("integer(-2.5) * 1.0", -3.0),
        // The alias is f64::MAX: arithmetic on it is ordinary finite arithmetic.
        ("Modelica.Constants.inf / Modelica.Constants.inf", 1.0),
        ("Modelica.Constants.inf - Modelica.Constants.inf", 0.0),
        ("Modelica.Constants.inf + 1", f64::MAX),
        ("-Modelica.Constants.inf", f64::MIN),
    ] {
        let mut engine = Engine::in_memory();
        engine
            .load_cxf(&with(
                MODEL.as_bytes(),
                GAIN_K,
                "S231:value",
                json!(expression),
            ))
            .unwrap_or_else(|error| panic!("{expression}: {error:?}"));
        let loaded = engine.get_param(GAIN_K).unwrap();
        assert!(
            loaded.bit_eq(&Value::Real(expected)),
            "{expression}: {loaded:?}"
        );
    }
}

#[test]
fn out_of_range_integer_connector_bound_refuses_the_load() {
    // Connector bounds ground through the ordinary (non-strict) evaluator; the refusal holds
    // there too, so a saturated i64::MAX can no longer become a silent bound.
    for (attribute, expression) in [
        ("S231:min", "integer(1e300)"),
        ("S231:max", "integer(-1e19)"),
    ] {
        let error = Engine::in_memory()
            .load_cxf(&with(BOUNDS_MODEL, A, attribute, json!(expression)))
            .expect_err(expression);
        let message = format!("{attribute} connector bound failed to ground: {REFUSAL}");
        assert_eq!(
            errors(&error),
            [(DiagCode::GroundingFailed, Some(A.to_owned()), message)],
            "{attribute}={expression}"
        );
    }
}

fn render(receipt: &DiagnosticReceipt) -> Vec<String> {
    receipt
        .records()
        .iter()
        .map(|record| {
            format!(
                "{:?}|{:?}|{}|{:?}|{}",
                record.key().stage(),
                record.key().severity(),
                record.key().code(),
                record.key().subject(),
                record.message()
            )
        })
        .collect()
}

#[test]
fn integer_range_failure_receipt_is_pinned_and_deterministic() {
    let bytes = with(
        MODEL.as_bytes(),
        GAIN_K,
        "S231:value",
        json!("integer(1e300)"),
    );
    let failure = Engine::in_memory()
        .load_cxf_with_receipt(&bytes)
        .expect_err("out-of-range integer()");
    assert_eq!(failure.stage(), DiagnosticStage::Import);
    let rendered = render(failure.diagnostics());
    assert_eq!(
        rendered,
        [format!(
            "Import|Error|grounding-failed|Opaque(\"{GAIN_K}\")|{REFUSAL}"
        )]
    );
    for _ in 0..3 {
        let again = Engine::in_memory()
            .load_cxf_with_receipt(&bytes)
            .expect_err("out-of-range integer()");
        assert_eq!(render(again.diagnostics()), rendered);
    }
}
