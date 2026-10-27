//! Public-facade refusal of non-finite Real parameters (issue #362) through real CXF bytes.
//!
//! Every CXF route by which a Real parameter value is authored — a typed XSD literal, an
//! expression grounded by `oce-expr`, and a bare JSON number — is driven through `load_cxf`, and
//! strict-finite parameter grounding must refuse `NaN`, `+inf` and `-inf` with the specific
//! `non-finite-parameter` diagnostic while still admitting every finite boundary value bit-exactly.
//! The tune-at-rest edit path (`set_param`) is checked against the same values.

use oce_api::{DiagnosticReceipt, DiagnosticStage, Engine, OcError, RunMode};
use oce_diag::DiagCode;
use oce_model::Value;
use serde_json::{Value as Json, json};

const MODEL: &str = include_str!("../../oce-cxf/tests/fixtures/minimal_loop.jsonld");
const GAIN_K_NODE: &str = "http://example.org#MinLoop.gain.k";
const GAIN_K_PATH: &str = "http://example.org#MinLoop.gain.k";
const GAIN_BLOCK: &str = "http://example.org#MinLoop.gain";
const DOUBLE: &str = "http://www.w3.org/2001/XMLSchema#double";

/// `minimal_loop.jsonld` with `MinLoop.gain.k` (a `MultiplyByParameter` gain, which carries no
/// class rule at all) re-bound to `value`.
fn with_gain_k(value: Json) -> Vec<u8> {
    let mut doc: Json = serde_json::from_str(MODEL).expect("fixture is JSON");
    let node = doc["@graph"]
        .as_array_mut()
        .expect("@graph array")
        .iter_mut()
        .find(|node| node["@id"] == GAIN_K_NODE)
        .expect("gain.k parameter node");
    node["S231:value"] = value;
    serde_json::to_vec(&doc).expect("serialize mutated fixture")
}

fn typed_double(lexical: &str) -> Json {
    json!({ "@value": lexical, "@type": DOUBLE })
}

/// Load `bytes`, require refusal, and require the complete diagnostic stream to be exactly one
/// `non-finite-parameter` error on the gain block naming `k`.
fn assert_refused_as_non_finite(bytes: &[u8], case: &str) {
    let mut engine = Engine::in_memory();
    let error = engine
        .load_cxf(bytes)
        .expect_err(&format!("{case}: a non-finite gain must refuse the load"));
    // The resolver refuses while grounding (issue #362), before flattening or validation.
    assert!(
        matches!(&error, OcError::Cxf(oce_cxf::CxfError::Validation(_))),
        "{case}: {error:?}"
    );
    let all: Vec<_> = error.all_diagnostics().collect();
    let refusals: Vec<_> = all.iter().filter(|diag| diag.is_error()).collect();
    assert_eq!(refusals.len(), 1, "{case}: {all:?}");
    assert_eq!(refusals[0].code, DiagCode::NonFiniteParameter, "{case}");
    assert_eq!(refusals[0].subject.as_deref(), Some(GAIN_BLOCK), "{case}");
    assert!(
        refusals[0]
            .message
            .starts_with("parameter `k` on `http://example.org#MinLoop.gain` must be finite; "),
        "{case}: {}",
        refusals[0].message
    );
    // The refused load left nothing behind: no parameter table, no model.
    assert!(engine.get_param(GAIN_K_PATH).is_err(), "{case}");
    assert!(engine.params().is_empty(), "{case}");
}

#[test]
fn typed_non_finite_literals_refuse_the_load() {
    // Every lexical spelling `f64::from_str` accepts for a non-finite value, both signs, both
    // cases. Parameter grounding refuses them in the resolver (the `oce-validate` gate is the
    // backstop on the flattened graph).
    for lexical in [
        "NaN",
        "nan",
        "NAN",
        "-NaN",
        "+nan",
        "INF",
        "inf",
        "+inf",
        "-INF",
        "-inf",
        "infinity",
        "Infinity",
        "-Infinity",
        "+INFINITY",
    ] {
        assert_refused_as_non_finite(&with_gain_k(typed_double(lexical)), lexical);
    }
}

#[test]
fn expressions_grounding_to_non_finite_values_refuse_the_load() {
    // Division by zero, 0/0, decimal overflow and arithmetic overflow of the finite `inf` alias
    // (`f64::MAX`) all produce a non-finite Real, which strict-finite grounding refuses.
    for expression in [
        "1/0",
        "-1/0",
        "0/0",
        "1e400",
        "-1e400",
        "1e308*10",
        "-(1e308*10)",
        "Modelica.Constants.inf*2",
        "(1/0)-(1/0)",
    ] {
        assert_refused_as_non_finite(&with_gain_k(json!(expression)), expression);
    }
}

#[test]
fn bare_json_overflow_literal_is_refused_before_grounding() {
    // A bare JSON number beyond f64 range never reaches grounding: the JSON layer refuses it as a
    // typed CXF error, so it cannot smuggle an infinity past the gate either.
    let mut bytes = with_gain_k(json!(0.5));
    let text = String::from_utf8(bytes.clone()).unwrap();
    let mutated = text.replacen("\"S231:value\":0.5", "\"S231:value\":1e400", 1);
    assert_ne!(text, mutated, "fixture mutation must apply");
    bytes = mutated.into_bytes();
    let error = Engine::in_memory().load_cxf(&bytes).unwrap_err();
    assert!(
        matches!(&error, OcError::Cxf(oce_cxf::CxfError::Json(_))),
        "{error:?}"
    );
}

#[test]
fn finite_boundary_values_load_and_round_trip_bit_exactly() {
    let cases: [(Json, f64); 9] = [
        (typed_double("1.7976931348623157e308"), f64::MAX),
        (typed_double("-1.7976931348623157e308"), f64::MIN),
        (typed_double("2.2250738585072014e-308"), f64::MIN_POSITIVE),
        (typed_double("5e-324"), f64::from_bits(1)),
        (typed_double("-5e-324"), -f64::from_bits(1)),
        (typed_double("-0.0"), -0.0),
        (typed_double("0.0"), 0.0),
        // The retained `Modelica.Constants.inf` alias is `f64::MAX`, deliberately finite.
        (json!("Modelica.Constants.inf"), f64::MAX),
        (json!("-Modelica.Constants.inf"), f64::MIN),
    ];
    for (value, expected) in cases {
        let case = value.to_string();
        let mut engine = Engine::in_memory();
        let report = engine
            .load_cxf(&with_gain_k(value))
            .unwrap_or_else(|error| panic!("{case}: finite gain must load: {error:?}"));
        assert!(
            report
                .warnings
                .iter()
                .all(|diag| diag.code != DiagCode::NonFiniteParameter),
            "{case}"
        );
        let loaded = engine.get_param(GAIN_K_PATH).unwrap();
        assert!(loaded.bit_eq(&Value::Real(expected)), "{case}: {loaded:?}");
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
fn non_finite_load_failure_receipt_is_pinned() {
    let mut engine = Engine::in_memory();
    let failure = engine
        .load_cxf_with_receipt(&with_gain_k(typed_double("-inf")))
        .expect_err("non-finite gain must refuse");
    assert_eq!(failure.stage(), DiagnosticStage::Import);
    let rendered = render(failure.diagnostics());
    assert_eq!(
        rendered,
        [
            "Import|Error|non-finite-parameter|Opaque(\"http://example.org#MinLoop.gain\")|\
             parameter `k` on `http://example.org#MinLoop.gain` must be finite; got -inf"
        ]
    );
    // Repeat: the refusal and its receipt are deterministic.
    let again = Engine::in_memory()
        .load_cxf_with_receipt(&with_gain_k(typed_double("-inf")))
        .expect_err("non-finite gain must refuse");
    assert_eq!(render(again.diagnostics()), rendered);
}

#[test]
fn set_param_refuses_non_finite_gain_and_leaves_the_engine_usable() {
    let mut engine = Engine::in_memory();
    engine.load_cxf(MODEL.as_bytes()).unwrap();
    engine.halt().unwrap();
    for value in [f64::NAN, -f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let error = engine
            .set_param(GAIN_K_PATH, Value::Real(value))
            .expect_err("non-finite gain edit must refuse");
        assert!(
            matches!(&error, OcError::ParamRange { path } if path == GAIN_K_PATH),
            "{value}: {error:?}"
        );
        assert!(
            engine
                .get_param(GAIN_K_PATH)
                .unwrap()
                .bit_eq(&Value::Real(0.5))
        );
    }
    for value in [
        f64::MAX,
        f64::MIN,
        f64::MIN_POSITIVE,
        f64::from_bits(1),
        -0.0,
    ] {
        engine
            .set_param(GAIN_K_PATH, Value::Real(value))
            .unwrap_or_else(|error| panic!("finite gain {value:e} must be accepted: {error:?}"));
        assert!(
            engine
                .get_param(GAIN_K_PATH)
                .unwrap()
                .bit_eq(&Value::Real(value))
        );
    }
    engine.set_param(GAIN_K_PATH, Value::Real(0.5)).unwrap();
    engine.resume().unwrap();
    assert_eq!(engine.mode(), RunMode::Running);
    let plan = engine
        .prepare_frame(
            1.0,
            &[("http://example.org#MinLoop.uSet", Value::Real(0.0))],
        )
        .unwrap();
    engine.execute_frame(plan).unwrap();
}
