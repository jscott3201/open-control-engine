//! A boundary input, its elided child input, or a boundary output declared with `min > max` is
//! refused at load with `crossed-bounds` (issue #378), instead of loading and then refusing every
//! frame with `InputDomain`. `min == max` stays legal and admits exactly that value.
//!
//! The CXF documents are the authored `legacy_frame_add` fixture with attributes added; the
//! expected refusals are stated here, not taken from the engine.

use oce_api::{DiagnosticSeverity, DiagnosticStage, Engine, OcError, Value};
use serde_json::json;

const MODEL: &[u8] = include_bytes!("fixtures/legacy_frame_add.jsonld");
const A: &str = "urn:legacy-frame:a";
const B: &str = "urn:legacy-frame:b";
const U1: &str = "urn:legacy-frame:add.u1";
const Y: &str = "urn:legacy-frame:y";
const XSD_DOUBLE: &str = "http://www.w3.org/2001/XMLSchema#double";

/// One `(node, attribute, lexical)` typed-double edit to `MODEL`.
type Edit<'a> = (&'a str, &'a str, &'a str);

/// `MODEL` with each edit applied.
fn with_attributes(edits: &[Edit<'_>]) -> Vec<u8> {
    let mut document: serde_json::Value = serde_json::from_slice(MODEL).unwrap();
    for (node, attribute, lexical) in edits {
        let target = document["@graph"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|entry| entry["@id"] == *node)
            .expect("node present");
        target[*attribute] = json!({"@value": lexical, "@type": XSD_DOUBLE});
    }
    serde_json::to_vec(&document).unwrap()
}

fn refusals(error: &OcError) -> Vec<(String, Option<String>, String)> {
    error
        .all_diagnostics()
        .filter(|d| d.is_error())
        .map(|d| {
            (
                d.code.as_str().to_owned(),
                d.subject.as_deref().map(str::to_owned),
                d.message.clone(),
            )
        })
        .collect()
}

fn own_range(min: &str, max: &str, owner: &str) -> String {
    format!(
        "S231:min ({min}) exceeds S231:max ({max}) on a Real {owner}; the declared range admits \
         no value"
    )
}

fn combined_range(min: &str, max: &str) -> String {
    format!(
        "the combined S231:min ({min}) exceeds the combined S231:max ({max}) across the Real \
         boundary input and the child inputs it drives; no input value satisfies them all"
    )
}

#[test]
fn crossed_declarations_and_targets_are_refused_at_load_with_their_cause() {
    let cases: [(&[Edit<'_>], &str, String); 6] = [
        (
            &[(A, "S231:min", "2"), (A, "S231:max", "1")],
            A,
            own_range("2", "1", "boundary input"),
        ),
        // The elided child input answers to the boundary path it carries.
        (
            &[(U1, "S231:min", "2"), (U1, "S231:max", "1")],
            A,
            own_range("2", "1", "connector"),
        ),
        (
            &[(A, "S231:min", "5e-324"), (A, "S231:max", "0.0")],
            A,
            own_range(&5e-324_f64.to_string(), "0", "boundary input"),
        ),
        // Neither node is crossed alone; together they leave no admissible value.
        (
            &[(A, "S231:min", "5"), (U1, "S231:max", "3")],
            A,
            combined_range("5", "3"),
        ),
        (
            &[(U1, "S231:min", "5"), (A, "S231:max", "3")],
            A,
            combined_range("5", "3"),
        ),
        (
            &[
                (Y, "S231:min", "1.7976931348623157e308"),
                (Y, "S231:max", "-1.7976931348623157e308"),
            ],
            Y,
            own_range(
                &f64::MAX.to_string(),
                &f64::MIN.to_string(),
                "boundary output",
            ),
        ),
    ];
    for (edits, subject, message) in cases {
        let mut engine = Engine::in_memory();
        engine.load_cxf(MODEL).unwrap();
        let before = engine.state_snapshot().unwrap();
        let error = engine
            .load_cxf(&with_attributes(edits))
            .expect_err("crossed bounds");
        assert_eq!(
            refusals(&error),
            [(
                "crossed-bounds".to_owned(),
                Some(subject.to_owned()),
                message
            )],
            "{edits:?}"
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

#[test]
fn equal_bounds_load_and_admit_exactly_that_value() {
    for (min, max, admitted, refused) in [
        (
            "1.5",
            "1.5",
            vec![1.5],
            vec![1.499_999_999_999_999_8, 1.500_000_000_000_000_2],
        ),
        // IEEE-equal signed zeros, in both orders, admit zero of either sign.
        ("0.0", "-0.0", vec![0.0, -0.0], vec![5e-324, -5e-324]),
        ("-0.0", "0.0", vec![0.0, -0.0], vec![5e-324, -5e-324]),
    ] {
        // Equal on the declaration itself, and split across the declaration and its target.
        for edits in [
            vec![(A, "S231:min", min), (A, "S231:max", max)],
            vec![(A, "S231:min", min), (U1, "S231:max", max)],
        ] {
            let mut engine = Engine::in_memory();
            engine
                .load_cxf(&with_attributes(&edits))
                .unwrap_or_else(|error| panic!("{edits:?}: {error}"));
            for value in admitted.iter().copied() {
                engine
                    .prepare_frame(0.0, &[(A, Value::Real(value)), (B, Value::Real(0.0))])
                    .unwrap_or_else(|error| panic!("{edits:?} {value}: {error}"));
            }
            for value in refused.iter().copied() {
                assert!(
                    matches!(
                        engine.prepare_frame(0.0, &[(A, Value::Real(value)), (B, Value::Real(0.0))]),
                        Err(OcError::InputDomain(p)) if p == A
                    ),
                    "{edits:?} {value}"
                );
            }
        }
    }
}

#[test]
fn the_crossed_bounds_receipt_names_the_unification_stage_and_is_deterministic() {
    let bytes = with_attributes(&[(A, "S231:min", "2"), (A, "S231:max", "1")]);
    let failure = Engine::in_memory()
        .load_cxf_with_receipt(&bytes)
        .expect_err("crossed bounds");
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
            "crossed-bounds".to_owned()
        )]
    );
    for _ in 0..3 {
        let again = Engine::in_memory()
            .load_cxf_with_receipt(&bytes)
            .expect_err("crossed bounds");
        assert_eq!(
            refusals(&again.into_error()),
            refusals(&Engine::in_memory().load_cxf(&bytes).unwrap_err())
        );
    }
}

#[test]
fn a_non_finite_crossed_pair_reports_only_the_non_finite_cause() {
    let bytes = with_attributes(&[(A, "S231:min", "INF"), (A, "S231:max", "1")]);
    let error = Engine::in_memory().load_cxf(&bytes).unwrap_err();
    let codes: Vec<_> = refusals(&error)
        .into_iter()
        .map(|(code, ..)| code)
        .collect();
    assert_eq!(codes, ["non-finite-bound"]);
}

#[test]
fn an_internal_cluster_that_propagation_would_cross_is_refused_at_load() {
    // The adder's output declares only min = 5 and the boundary output it drives only max = 3.
    // Neither is crossed, and the frame never checks internal bounds, so this used to load and
    // run; §7.10 propagation would give both the empty range [5, 3], so the load is refused once,
    // on the cluster's source output.
    let bytes = with_attributes(&[
        ("urn:legacy-frame:add.y", "S231:min", "5"),
        (Y, "S231:max", "3"),
    ]);
    let failure = Engine::in_memory()
        .load_cxf_with_receipt(&bytes)
        .expect_err("propagated crossing");
    assert_eq!(failure.stage(), DiagnosticStage::AttributeUnification);
    assert_eq!(
        refusals(&failure.into_error()),
        [(
            "crossed-bounds".to_owned(),
            Some("urn:legacy-frame:add.y".to_owned()),
            "§7.10 propagation across the connected Real cluster driven by this output \
             (1 connector and 1 boundary output) gives the effective range S231:min (5) above \
             S231:max (3), which admits no value; no single member declares both"
                .to_owned()
        )]
    );
}
