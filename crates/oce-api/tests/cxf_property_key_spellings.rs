//! A connector attribute keyed by its full IRI is validated and enforced exactly like the compact
//! `S231:` key (issue #383).
//!
//! Before the fix, `"http://data.ashrae.org/S231P#max"` fell into the CXF pass-through map: the
//! load accepted a crossed or non-finite bound without a diagnostic, and a frame outside the
//! authored range was admitted. The documents are the authored `legacy_frame_add` fixture with
//! attributes added; expected refusals are stated here, not taken from the engine.

use oce_api::{Engine, OcError, Value};
use serde_json::json;

const MODEL: &[u8] = include_bytes!("fixtures/legacy_frame_add.jsonld");
const S231P: &str = "http://data.ashrae.org/S231P#";
const A: &str = "urn:legacy-frame:a";
const B: &str = "urn:legacy-frame:b";
const XSD_DOUBLE: &str = "http://www.w3.org/2001/XMLSchema#double";

/// `MODEL` with typed-double attributes on boundary input `A`, keyed by `key(term)`.
fn with_attributes(attributes: &[(&str, &str)], key: &dyn Fn(&str) -> String) -> Vec<u8> {
    let mut document: serde_json::Value = serde_json::from_slice(MODEL).unwrap();
    let target = document["@graph"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entry| entry["@id"] == A)
        .expect("node present");
    for (term, lexical) in attributes {
        target[key(term)] = json!({"@value": lexical, "@type": XSD_DOUBLE});
    }
    serde_json::to_vec(&document).unwrap()
}

fn compact(term: &str) -> String {
    format!("S231:{term}")
}

fn full_iri(term: &str) -> String {
    format!("{S231P}{term}")
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

#[test]
fn crossed_and_non_finite_full_iri_bounds_are_refused_with_the_compact_diagnostics() {
    for (attributes, code) in [
        (&[("min", "2"), ("max", "1")][..], "crossed-bounds"),
        (&[("min", "NaN")][..], "non-finite-bound"),
        (&[("max", "INF")][..], "non-finite-bound"),
        (&[("nominal", "-INF")][..], "non-finite-bound"),
    ] {
        let expected = refusals(
            &Engine::in_memory()
                .load_cxf(&with_attributes(attributes, &compact))
                .expect_err("compact spelling refuses"),
        );
        assert_eq!(expected.len(), 1, "{attributes:?}: {expected:?}");
        assert_eq!(expected[0].0, code, "{attributes:?}");
        let actual = refusals(
            &Engine::in_memory()
                .load_cxf(&with_attributes(attributes, &full_iri))
                .expect_err("full-IRI spelling refuses too"),
        );
        assert_eq!(actual, expected, "{attributes:?}");
    }
}

#[test]
fn a_full_iri_range_is_enforced_on_every_frame() {
    for key in [&compact as &dyn Fn(&str) -> String, &full_iri] {
        let mut engine = Engine::in_memory();
        engine
            .load_cxf(&with_attributes(&[("min", "0"), ("max", "10")], key))
            .expect("the range loads");
        for admitted in [0.0, 5.0, 10.0] {
            engine
                .prepare_frame(0.0, &[(A, Value::Real(admitted)), (B, Value::Real(0.0))])
                .unwrap_or_else(|error| panic!("{admitted}: {error}"));
        }
        for refused in [-5e-324, 10.000_000_000_000_002, 1e300] {
            assert!(
                matches!(
                    engine.prepare_frame(0.0, &[(A, Value::Real(refused)), (B, Value::Real(0.0))]),
                    Err(OcError::InputDomain(path)) if path == A
                ),
                "{refused}"
            );
        }
    }
}

#[test]
fn compact_and_full_iri_documents_load_to_byte_identical_state() {
    let attributes = [("min", "-1.5"), ("max", "99.25"), ("nominal", "20")];
    let mut snapshots = Vec::new();
    for key in [&compact as &dyn Fn(&str) -> String, &full_iri] {
        let mut engine = Engine::in_memory();
        engine
            .load_cxf(&with_attributes(&attributes, key))
            .expect("loads");
        engine
            .prepare_frame(0.0, &[(A, Value::Real(3.0)), (B, Value::Real(4.0))])
            .expect("frame");
        snapshots.push(engine.state_snapshot().unwrap().as_bytes().to_vec());
    }
    assert_eq!(snapshots[0], snapshots[1]);
}
