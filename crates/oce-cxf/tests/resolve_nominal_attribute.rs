//! `S231:nominal` import, refusal and export round trip (issue #379).
//!
//! The importer used to leave `S231:nominal` in the opaque pass-through map, so a nominal loaded
//! with no diagnostic and never reached the model or the deep gate. It now grounds like a bound
//! onto `RealAttrs::nominal` on Real connectors and boundary declarations, refuses it on any
//! non-Real connector, and the exporter emits a finite one as a bare number so it round-trips.
//! Expected values are stated by hand from the authored bytes.

use oce_cxf::{CxfError, ResolveOptions, export, import_cxf, parse_document, write_document};
use oce_diag::{DiagCode, Diagnostic};
use oce_model::{Attrs, ModelGraph};
use serde_json::{Value as Json, json};

const CONNECTOR_ATTRS: &str = include_str!("fixtures/connector_attrs.jsonld");
const SOURCE_PORT: &str = "http://example.org#A.con.y";
const BOUNDARY_OUTPUT: &str = "http://example.org#A.yOut";
const DOUBLE: &str = "http://www.w3.org/2001/XMLSchema#double";

fn with_nominal(node: &str, nominal: Json) -> Vec<u8> {
    let mut doc: Json = serde_json::from_str(CONNECTOR_ATTRS).expect("fixture is JSON");
    let entry = doc["@graph"]
        .as_array_mut()
        .expect("@graph array")
        .iter_mut()
        .find(|entry| entry["@id"] == node)
        .expect("node present");
    entry["S231:nominal"] = nominal;
    serde_json::to_vec(&doc).expect("serialize mutated fixture")
}

fn import(bytes: &[u8]) -> Result<ModelGraph, CxfError> {
    import_cxf(bytes, &ResolveOptions::default()).map(|(graph, _report)| graph)
}

fn source_nominal(graph: &ModelGraph) -> Option<f64> {
    graph
        .connectors
        .iter()
        .find(|c| c.iri.as_deref() == Some(SOURCE_PORT))
        .and_then(|c| c.attrs.as_real())
        .expect("Real source port")
        .nominal
}

fn boundary_nominal(graph: &ModelGraph) -> Option<f64> {
    let output = graph
        .boundary_outputs
        .iter()
        .find(|o| o.iri.as_ref() == BOUNDARY_OUTPUT)
        .expect("declared boundary output");
    match &output.attrs {
        Attrs::Real(real) => real.nominal,
        other => panic!("Real boundary output, got {other:?}"),
    }
}

fn resolve_errors(bytes: &[u8]) -> Vec<Diagnostic> {
    match import(bytes) {
        Err(CxfError::Validation(diags)) => diags.into_iter().filter(|d| d.is_error()).collect(),
        other => panic!("expected a resolver refusal, got {other:?}"),
    }
}

#[test]
fn every_nominal_spelling_grounds_bit_exactly_onto_the_connector_and_declaration() {
    for (nominal, expected) in [
        (json!(300.0), 300.0),
        (json!(300), 300.0),
        (json!({"@value": "2.5e2", "@type": DOUBLE}), 250.0),
        (json!("2 * 150.5"), 301.0),
        (json!("Modelica.Constants.inf"), f64::MAX),
        (json!({"@value": "-0.0", "@type": DOUBLE}), -0.0),
        (json!({"@value": "5e-324", "@type": DOUBLE}), 5e-324),
    ] {
        for node in [SOURCE_PORT, BOUNDARY_OUTPUT] {
            let graph = import(&with_nominal(node, nominal.clone()))
                .unwrap_or_else(|e| panic!("{node} nominal={nominal}: {e:?}"));
            let got = if node == SOURCE_PORT {
                source_nominal(&graph)
            } else {
                boundary_nominal(&graph)
            };
            assert_eq!(
                got.map(f64::to_bits),
                Some(expected.to_bits()),
                "{node} nominal={nominal}"
            );
        }
    }
    // Absent stays absent: the attribute-free baseline is unchanged.
    let graph = import(CONNECTOR_ATTRS.as_bytes()).unwrap();
    assert_eq!(source_nominal(&graph), None);
    assert_eq!(boundary_nominal(&graph), None);
}

#[test]
fn a_non_finite_nominal_reaches_the_model_for_the_deep_gate_to_refuse() {
    // The resolver grounds attributes through the ordinary evaluator, like min/max; the deep gate
    // (`non-finite-bound`, pinned at the facade) is what refuses the value.
    for (lexical, check) in [
        ("NaN", f64::is_nan as fn(f64) -> bool),
        ("INF", |v: f64| v == f64::INFINITY),
        ("-inf", |v: f64| v == f64::NEG_INFINITY),
    ] {
        let graph = import(&with_nominal(
            SOURCE_PORT,
            json!({"@value": lexical, "@type": DOUBLE}),
        ))
        .expect("the resolver keeps the value");
        assert!(source_nominal(&graph).is_some_and(check), "{lexical}");
    }
}

#[test]
fn an_ungroundable_or_non_numeric_nominal_is_a_typed_resolver_refusal() {
    let diags = resolve_errors(&with_nominal(SOURCE_PORT, json!("noSuchSymbol * 2")));
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, DiagCode::GroundingFailed);
    assert_eq!(diags[0].subject.as_deref(), Some(SOURCE_PORT));
    assert_eq!(
        diags[0].message,
        "S231:nominal connector attribute failed to ground: expression binding did not ground: \
         unknown identifier: noSuchSymbol"
    );

    let diags = resolve_errors(&with_nominal(SOURCE_PORT, json!(true)));
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, DiagCode::MalformedDocument);
    assert_eq!(diags[0].subject.as_deref(), Some(SOURCE_PORT));
    assert_eq!(
        diags[0].message,
        "S231:nominal on a Real connector did not ground to a number"
    );
}

/// A one-block document whose source port is `port_type` (`Integer` or `Boolean`), with a nominal.
fn non_real_document(class: &str, port_type: &str, k: Json) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "@context": {"S231": "http://data.ashrae.org/S231P#"},
        "@graph": [
            {"@id": "http://example.org#N", "@type": "S231:Block",
             "S231:containsBlock": {"@id": "http://example.org#N.con"}},
            {"@id": "http://example.org#N.con",
             "@type": format!("http://example.org#Buildings.Controls.OBC.CDL.{class}"),
             "S231:hasParameter": {"@id": "http://example.org#N.con.k"},
             "S231:hasOutput": {"@id": "http://example.org#N.con.y"}},
            {"@id": "http://example.org#N.con.k", "S231:value": k},
            {"@id": "http://example.org#N.con.y", "@type": format!("S231:{port_type}Output"),
             "S231:isOfDataType": {"@id": format!("S231:{port_type}")},
             "S231:nominal": 1.0}
        ]
    }))
    .unwrap()
}

#[test]
fn a_nominal_on_a_non_real_connector_is_malformed_not_dropped() {
    for (class, port_type, k, kind) in [
        (
            "Integers.Sources.Constant",
            "Integer",
            json!(3),
            "an Integer",
        ),
        (
            "Logical.Sources.Constant",
            "Boolean",
            json!(true),
            "a Boolean",
        ),
    ] {
        let diags = resolve_errors(&non_real_document(class, port_type, k));
        assert_eq!(diags.len(), 1, "{port_type}: {diags:?}");
        assert_eq!(diags[0].code, DiagCode::MalformedDocument, "{port_type}");
        assert_eq!(
            diags[0].subject.as_deref(),
            Some("http://example.org#N.con.y")
        );
        assert_eq!(
            diags[0].message,
            format!("§7.4.1 attribute nominal declared on {kind} connector; only Real has one")
        );
    }
}

#[test]
fn a_finite_nominal_round_trips_through_export_bit_exactly_and_deterministically() {
    let bytes = with_nominal(SOURCE_PORT, json!({"@value": "273.15", "@type": DOUBLE}));
    let bytes = {
        // Also on the declared boundary output, which exports its own authored attributes.
        let mut doc: Json = serde_json::from_slice(&bytes).unwrap();
        for entry in doc["@graph"].as_array_mut().unwrap() {
            if entry["@id"] == BOUNDARY_OUTPUT {
                entry["S231:nominal"] = json!(1.5);
            }
        }
        serde_json::to_vec(&doc).unwrap()
    };
    let graph = import(&bytes).unwrap();
    let exported = export(&graph).expect("a finite nominal is in the export subset");
    assert_eq!(export(&graph).unwrap(), exported, "byte-stable export");
    let text = std::str::from_utf8(&exported).unwrap();
    assert!(text.contains(r#""S231:nominal":273.15"#), "{text}");
    assert!(text.contains(r#""S231:nominal":1.5"#), "{text}");
    let again = import(&exported).expect("the export re-imports");
    assert_eq!(
        source_nominal(&again).map(f64::to_bits),
        Some(273.15_f64.to_bits())
    );
    assert_eq!(
        boundary_nominal(&again).map(f64::to_bits),
        Some(1.5_f64.to_bits())
    );
    // RT-2 fixpoint: exporting the re-import reproduces the same bytes.
    assert_eq!(export(&again).unwrap(), exported);
}

#[test]
fn the_document_layer_models_nominal_and_round_trips_it_losslessly() {
    let bytes = with_nominal(SOURCE_PORT, json!({"@value": "2.5e2", "@type": DOUBLE}));
    let document = parse_document(&bytes).unwrap();
    let node = document
        .graph
        .iter()
        .find(|node| node.id == SOURCE_PORT)
        .unwrap();
    assert!(node.nominal.is_some(), "nominal is a modeled field");
    assert!(
        !node.other.contains_key("S231:nominal"),
        "and no longer opaque"
    );
    let written: Json = serde_json::from_slice(&write_document(&document).unwrap()).unwrap();
    let original: Json = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(written, original, "RT-1: lossless modulo key order");
}

#[test]
fn every_finite_nominal_extreme_is_emitted_as_a_bare_number() {
    let graph = import(CONNECTOR_ATTRS.as_bytes()).unwrap();
    for (nominal, wire) in [
        (1.0, "1.0"),
        (-0.0, "-0.0"),
        (5e-324, "5e-324"),
        (f64::MAX, "1.7976931348623157e+308"),
        (f64::MIN, "-1.7976931348623157e+308"),
    ] {
        let mut graph = graph.clone();
        let port = graph
            .connectors
            .iter_mut()
            .find(|c| c.iri.as_deref() == Some(SOURCE_PORT))
            .unwrap();
        let Attrs::Real(real) = &mut port.attrs else {
            panic!("Real source port")
        };
        real.nominal = Some(nominal);
        let exported = export(&graph).expect("a finite nominal exports");
        let text = std::str::from_utf8(&exported).unwrap();
        assert!(
            text.contains(&format!(r#""S231:nominal":{wire}"#)),
            "{nominal}: {text}"
        );
        let again = import(&exported).expect("re-import");
        assert_eq!(
            source_nominal(&again).map(f64::to_bits),
            Some(nominal.to_bits()),
            "{nominal}"
        );
    }
}
