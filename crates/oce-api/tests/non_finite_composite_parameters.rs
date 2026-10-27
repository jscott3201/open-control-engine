//! Non-finite Real parameters at every composite level, and non-finite expression intermediates,
//! are refused at load (issue #362).
//!
//! Composite-level parameters are evaluated by the resolver and never reach the flattened
//! `ModelGraph`, so the `oce-validate` gate alone cannot see them, and a function such as
//! `integer`, `sign`, `min`/`max` or a comparison can absorb a non-finite intermediate into a
//! finite result. Array parameters (a list element on the G36 SumZone `MatrixGain.K`, and an
//! array expression on `array_expression_preserved.jsonld`) take the same path. Driven through
//! `load_cxf` on `nested_composite.jsonld` (a root composite with
//! `topK`, a nested `sub` composite passing it through as `innerK`, and leaf gains `sub.gain.k` and
//! `post.k`), every such value must refuse with exactly one `non-finite-parameter` error whose
//! subject is the declaring instance and whose message names the parameter (dependents that read
//! the refused declaration add only the existing unknown-identifier cascade).

use oce_api::{Engine, OcError};
use oce_diag::DiagCode;
use oce_model::Value;
use serde_json::{Value as Json, json};

const MODEL: &str = include_str!("../../oce-cxf/tests/fixtures/nested_composite.jsonld");
const ROOT: &str = "http://example.org#g36.profile.nested_composite";
const SUB: &str = "http://example.org#g36.profile.nested_composite.sub";
const SUB_GAIN: &str = "http://example.org#g36.profile.nested_composite.sub.gain";
const POST: &str = "http://example.org#g36.profile.nested_composite.post";
const DOUBLE: &str = "http://www.w3.org/2001/XMLSchema#double";

fn typed_double(lexical: &str) -> Json {
    json!({ "@value": lexical, "@type": DOUBLE })
}

/// The fixture with each `(parameter node suffix, value)` binding replaced.
fn document(bindings: &[(&str, Json)]) -> Json {
    let mut doc: Json = serde_json::from_str(MODEL).expect("fixture is JSON");
    for (suffix, value) in bindings {
        let id = format!("{ROOT}{suffix}");
        let node = doc["@graph"]
            .as_array_mut()
            .expect("@graph array")
            .iter_mut()
            .find(|node| node["@id"] == id.as_str())
            .unwrap_or_else(|| panic!("fixture node {id}"));
        node["S231:value"] = value.clone();
    }
    doc
}

fn bytes(doc: &Json) -> Vec<u8> {
    serde_json::to_vec(doc).expect("serialize")
}

/// Require refusal with exactly one `non-finite-parameter` error on `subject` naming `name`.
fn assert_refused(doc: &Json, subject: &str, name: &str, case: &str) {
    let mut engine = Engine::in_memory();
    let error = engine
        .load_cxf(&bytes(doc))
        .expect_err(&format!("{case}: must refuse"));
    assert!(
        matches!(&error, OcError::Cxf(oce_cxf::CxfError::Validation(_))),
        "{case}: {error:?}"
    );
    let all: Vec<_> = error.all_diagnostics().filter(|d| d.is_error()).collect();
    // A refused declaration binds nothing, so every dependent that reads it reports the existing
    // cascade (`grounding-failed`, unknown identifier) — never a second non-finite cause.
    let (errors, cascade): (Vec<&&oce_diag::Diagnostic>, Vec<_>) = all
        .iter()
        .partition(|d| d.code == DiagCode::NonFiniteParameter);
    assert_eq!(errors.len(), 1, "{case}: {all:?}");
    assert!(
        cascade.iter().all(
            |d| d.code == DiagCode::GroundingFailed && d.message.contains("unknown identifier")
        ),
        "{case}: {all:?}"
    );
    assert_eq!(errors[0].subject.as_deref(), Some(subject), "{case}");
    assert!(
        errors[0].message.starts_with(&format!(
            "parameter `{name}` on `{subject}` must be finite; "
        )),
        "{case}: {}",
        errors[0].message
    );
    assert!(engine.params().is_empty(), "{case}: nothing may load");
}

#[test]
fn unmodified_fixture_loads_as_the_positive_control() {
    let mut engine = Engine::in_memory();
    engine.load_cxf(MODEL.as_bytes()).expect("fixture loads");
    assert!(
        engine
            .get_param(&format!("{POST}.k"))
            .unwrap()
            .bit_eq(&Value::Real(2.0))
    );
    assert!(
        engine
            .get_param(&format!("{SUB_GAIN}.k"))
            .unwrap()
            .bit_eq(&Value::Real(0.5))
    );
}

#[test]
fn reviewer_repros_with_a_nan_root_parameter_are_refused_at_the_declaration() {
    // A NaN root `topK` used to vanish: `integer(topK)` grounded to Integer(0), `max(topK, 2.0)`
    // to 2.0. The declaration itself now refuses, whatever consumes it.
    for consumer in [
        "integer(topK)",
        "max(topK, 2.0)",
        "min(topK, 2.0)",
        "sign(topK)",
        "abs(topK)",
        "topK",
    ] {
        for lexical in ["NaN", "INF", "-INF"] {
            let doc = document(&[
                (".topK", typed_double(lexical)),
                (".post.k", json!(consumer)),
            ]);
            assert_refused(
                &doc,
                ROOT,
                "topK",
                &format!("topK={lexical}, post.k={consumer}"),
            );
        }
    }
}

#[test]
fn absorbing_functions_cannot_hide_a_non_finite_intermediate() {
    // `topK` stays finite; the leaf expression itself produces the non-finite intermediate.
    for expression in [
        "integer(1/0)",
        "sign(0/0)",
        "min(1/0, 3.0)",
        "max(0/0, 3.0)",
        "abs(-1/0)",
        "floor(1e400)",
        "topK * 0 + (1/0) * 0",
        "Modelica.Constants.inf * 2",
        "sum({1e308, 1e308})",
    ] {
        let doc = document(&[(".post.k", json!(expression))]);
        assert_refused(&doc, POST, "k", expression);
    }
}

#[test]
fn a_boolean_derived_from_a_nan_comparison_is_refused() {
    // A root Boolean parameter whose comparison reads a NaN intermediate: CDL comparisons with NaN
    // are false, which would silently select a branch.
    let mut doc = document(&[]);
    let graph = doc["@graph"].as_array_mut().unwrap();
    graph[0]["S231:hasParameter"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "@id": format!("{ROOT}.flag") }));
    graph.push(json!({
        "@id": format!("{ROOT}.flag"),
        "@type": "S231:Parameter",
        "S231:isOfDataType": { "@id": "S231:Boolean" },
        "S231:value": "(0/0) > 0"
    }));
    assert_refused(&doc, ROOT, "flag", "flag = (0/0) > 0");
}

#[test]
fn pass_through_and_nested_composite_parameters_are_refused_at_their_own_level() {
    // The nested composite's own declaration produces the non-finite value.
    let doc = document(&[(".sub.innerK", json!("topK / 0"))]);
    assert_refused(&doc, SUB, "innerK", "sub.innerK = topK / 0");
    // A finite pass-through of an overflowing intermediate still refuses at the nested level.
    let doc = document(&[(".sub.innerK", json!("min(topK * 1e308 * 10, 1.0)"))]);
    assert_refused(&doc, SUB, "innerK", "sub.innerK = min(overflow, 1)");
    // The leaf inside the nested composite, reading the passed-through value.
    let doc = document(&[(".sub.gain.k", json!("innerK / 0"))]);
    assert_refused(&doc, SUB_GAIN, "k", "sub.gain.k = innerK / 0");
    // A non-finite root value reached only through the pass-through is refused at the root.
    let doc = document(&[(".topK", typed_double("NaN"))]);
    assert_refused(&doc, ROOT, "topK", "topK = NaN through sub.innerK");
}

#[test]
fn the_finite_inf_alias_and_finite_extremes_pass_through_every_level() {
    for (lexical_or_expr, expected) in [
        (json!("Modelica.Constants.inf"), f64::MAX),
        (json!("-Modelica.Constants.inf"), f64::MIN),
        (typed_double("1.7976931348623157e308"), f64::MAX),
        (typed_double("5e-324"), f64::from_bits(1)),
        (typed_double("-0.0"), -0.0),
    ] {
        let case = lexical_or_expr.to_string();
        let doc = document(&[
            (".topK", lexical_or_expr),
            (".post.k", json!("max(topK, -1.0)")),
        ]);
        let mut engine = Engine::in_memory();
        engine
            .load_cxf(&bytes(&doc))
            .unwrap_or_else(|error| panic!("{case}: finite topK must load: {error:?}"));
        let through_sub = engine.get_param(&format!("{SUB_GAIN}.k")).unwrap();
        assert!(
            through_sub.bit_eq(&Value::Real(expected)),
            "{case}: {through_sub:?}"
        );
        let post = engine.get_param(&format!("{POST}.k")).unwrap();
        assert!(
            post.bit_eq(&Value::Real(if expected > -1.0 { expected } else { -1.0 })),
            "{case}: {post:?}"
        );
    }
}

const SUMZONE: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/multizone_vav_outdoor_airflow_sumzone.jsonld");
const GRO_FLO: &str = "http://example.org#g36.source.multizone_vav_outdoor_airflow_sumzone.groFlo";
const ARRAY_EXPRESSION: &str =
    include_str!("../../oce-cxf/tests/fixtures/array_expression_preserved.jsonld");

#[test]
fn a_non_finite_array_list_element_is_refused_on_the_declaring_instance() {
    // Positive control: the G36 SumZone fixture (a `MatrixGain` with a `K[2,3]` list) loads.
    Engine::in_memory()
        .load_cxf(SUMZONE.as_bytes())
        .expect("fixture loads");
    for lexical in ["NaN", "INF", "-INF"] {
        let mut doc: Json = serde_json::from_str(SUMZONE).unwrap();
        let node = doc["@graph"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|node| node["@id"] == format!("{GRO_FLO}.K").as_str())
            .expect("groFlo.K");
        node["S231:value"][4] = typed_double(lexical);
        assert_refused(&doc, GRO_FLO, "K", &format!("groFlo.K[2,2] = {lexical}"));
    }
}

#[test]
fn a_non_finite_array_expression_is_refused_on_the_declaring_instance() {
    // `A.con.k[nin]` is an array expression; its intermediates are evaluated strict-finite.
    for expression in [
        "fill(1/0, nin)",
        "fill(integer(0/0), nin)",
        "{1.0, 1e400}",
        "{min(1/0, 3.0), 1.0}",
    ] {
        let mut doc: Json = serde_json::from_str(ARRAY_EXPRESSION).unwrap();
        let node = doc["@graph"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|node| node["@id"] == "http://example.org#A.con.k")
            .expect("A.con.k");
        node["S231:value"] = json!(expression);
        assert_refused(&doc, "http://example.org#A.con", "k", expression);
    }
}
