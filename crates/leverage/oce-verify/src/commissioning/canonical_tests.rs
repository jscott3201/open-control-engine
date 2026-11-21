//! The canonical writer of a spec, against documents written out by hand.

use super::super::limits::SpecLimits;
use super::*;

fn written(text: &str) -> String {
    CommissioningSpec::from_json_str(text, &SpecLimits::default())
        .expect("loads")
        .to_canonical_json()
}

#[test]
fn the_smallest_spec_is_its_references_and_its_tag() {
    assert_eq!(
        written(r#"{"references": []}"#),
        "{\n  \"references\": [],\n  \"schema\": \"oce/commissioning-spec/v1\"\n}\n"
    );
    assert_eq!(
        written(r#"{"references": [{"sequence": "s", "model": "M"}]}"#),
        "{\n  \"references\": [\n    {\n      \"model\": \"M\",\n      \"sequence\": \"s\"\n    }\n  ],\n  \"schema\": \"oce/commissioning-spec/v1\"\n}\n"
    );
}

#[test]
fn every_object_is_sorted_but_the_two_override_maps() {
    let text = written(
        r#"{"references": [{"sequence": "s", "model": "M",
            "parameters": {"z": 1, "a": 2},
            "outputs": {"s.z": {"rtoly": 1, "atoly": 2}, "s.a": {}},
            "indicators": {"s.z": ["s.q", "s.b"], "s.a": []},
            "inputs": [{"path": "p", "format": "csv", "timeColumn": "t",
                        "columns": {"z": "1", "a": "2"}, "hold": {"maxAge": 1}}]}]}"#,
    );
    let expected = r#"{
  "references": [
    {
      "indicators": {
        "s.z": [
          "s.q",
          "s.b"
        ],
        "s.a": []
      },
      "inputs": [
        {
          "columns": {
            "a": "2",
            "z": "1"
          },
          "format": "csv",
          "hold": {
            "maxAge": 1
          },
          "path": "p",
          "timeColumn": "t"
        }
      ],
      "model": "M",
      "outputs": {
        "s.z": {
          "atoly": 2,
          "rtoly": 1
        },
        "s.a": {}
      },
      "parameters": {
        "a": 2,
        "z": 1
      },
      "sequence": "s"
    }
  ],
  "schema": "oce/commissioning-spec/v1"
}
"#;
    assert_eq!(text, expected);
}

#[test]
fn a_key_is_written_only_when_the_spec_gave_it() {
    let bare = written(r#"{"references": [{"model": "M", "sequence": "s"}]}"#);
    for absent in [
        "time",
        "tolerances",
        "sampling",
        "controller",
        "modelJsonDirectory",
        "cxf",
        "window",
        "outputs",
        "indicators",
        "parameters",
        "inputs",
        "expected",
        "indicatorSources",
        "envelope",
        "funnelReference",
        "pointNameMapping",
        "controllerOutput",
        "generateJson",
        "runController",
        "table",
        "hold",
    ] {
        assert!(!bare.contains(&format!("\"{absent}\"")), "{absent}");
    }
    // Present and empty is written, and is not absent.
    let empty = written(
        r#"{"tolerances": {}, "references": [{"model": "M", "sequence": "s", "outputs": {},
            "indicators": {}, "parameters": {}, "inputs": [], "expected": [],
            "indicatorSources": [], "pointNameMapping": []}]}"#,
    );
    for present in [
        "\"tolerances\": {}",
        "\"outputs\": {}",
        "\"indicators\": {}",
        "\"parameters\": {}",
        "\"inputs\": []",
        "\"expected\": []",
        "\"indicatorSources\": []",
        "\"pointNameMapping\": []",
    ] {
        assert!(empty.contains(present), "{present}\n{empty}");
    }
}

#[test]
fn a_time_column_keeps_its_kind_and_a_number_its_spelling() {
    let text = written(
        r#"{"sampling": 1.20E2, "references": [{"model": "M", "sequence": "s", "sampling": 60,
            "window": {"start": -0.0, "stop": 1e-7},
            "inputs": [{"path": "a", "format": "csv", "timeColumn": "3", "columns": {}},
                       {"path": "b", "format": "combiTimeTable", "timeColumn": 3, "columns": {}}]}]}"#,
    );
    assert!(text.contains("\"timeColumn\": \"3\""), "{text}");
    assert!(text.contains("\"timeColumn\": 3\n"), "{text}");
    assert!(text.contains("\n  \"sampling\": 120.0,"), "{text}");
    assert!(text.contains("\n      \"sampling\": 60,"), "{text}");
    assert!(
        text.contains("\"start\": -0.0,") && text.contains("\"stop\": 1e-7\n"),
        "{text}"
    );
}

#[test]
fn the_tag_is_written_once_whatever_the_document_had() {
    for document in [
        r#"{"references": []}"#,
        r#"{"schema": "oce/commissioning-spec/v1", "references": []}"#,
        r#"{"references": [], "schema": "oce\/commissioning-spec\/v1"}"#,
    ] {
        let text = written(document);
        assert_eq!(text.matches("\"schema\"").count(), 1, "{document}");
        assert_eq!(
            text,
            "{\n  \"references\": [],\n  \"schema\": \"oce/commissioning-spec/v1\"\n}\n"
        );
    }
}
