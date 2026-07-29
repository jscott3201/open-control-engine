//! Rejection and specialization coverage for array-valued connector and block-instance nodes.

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::{DiagCode, Diagnostic};
use serde_json::{Value as JsonValue, json};

fn document() -> JsonValue {
    json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" },
        "@graph": [
            { "@id": "http://example.org#M", "@type": "S231:Block",
              "S231:containsBlock": [
                { "@id": "http://example.org#M.c1" },
                { "@id": "http://example.org#M.c2" }
              ] },
            { "@id": "http://example.org#M.c1",
              "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
              "S231:hasParameter": { "@id": "http://example.org#M.c1.k" },
              "S231:hasOutput": { "@id": "http://example.org#M.c1.y" } },
            { "@id": "http://example.org#M.c1.k",
              "S231:value": { "@value": "1.0",
                "@type": "http://www.w3.org/2001/XMLSchema#double" } },
            { "@id": "http://example.org#M.c1.y", "@type": "S231:RealOutput",
              "S231:isOfDataType": { "@id": "S231:Real" },
              "S231:isConnectedTo": { "@id": "http://example.org#M.c2.u" } },
            { "@id": "http://example.org#M.c2",
              "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.MultiplyByParameter",
              "S231:hasParameter": { "@id": "http://example.org#M.c2.k" },
              "S231:hasInput": { "@id": "http://example.org#M.c2.u" },
              "S231:hasOutput": { "@id": "http://example.org#M.c2.y" } },
            { "@id": "http://example.org#M.c2.k",
              "S231:value": { "@value": "2.0",
                "@type": "http://www.w3.org/2001/XMLSchema#double" } },
            { "@id": "http://example.org#M.c2.u", "@type": "S231:RealInput",
              "S231:isOfDataType": { "@id": "S231:Real" } },
            { "@id": "http://example.org#M.c2.y", "@type": "S231:RealOutput",
              "S231:isOfDataType": { "@id": "S231:Real" } }
        ]
    })
}

fn node_mut<'a>(doc: &'a mut JsonValue, suffix: &str) -> &'a mut JsonValue {
    doc["@graph"]
        .as_array_mut()
        .expect("@graph array")
        .iter_mut()
        .find(|node| node["@id"].as_str().is_some_and(|id| id.ends_with(suffix)))
        .unwrap_or_else(|| panic!("missing node ending in {suffix:?}"))
}

fn reject(doc: &JsonValue) -> Vec<Diagnostic> {
    let bytes = serde_json::to_vec(doc).expect("serialize document");
    match import_cxf(&bytes, &ResolveOptions::default()) {
        Err(CxfError::Validation(diags)) => diags,
        other => panic!("expected validation rejection, got {other:?}"),
    }
}

fn tagged(code: DiagCode, subject: &str, message: &str) -> Diagnostic {
    Diagnostic::error(code, message).with_subject(subject.to_owned())
}

fn import_clean(src: &[u8]) {
    let (_, report) =
        import_cxf(src, &ResolveOptions::default()).expect("positive control must import");
    assert!(
        report.is_empty(),
        "positive control must import warning-free: {:?}",
        report.diagnostics
    );
}

#[test]
fn active_array_connector_is_rejected_with_stable_rule_identity() {
    let mut doc = document();
    node_mut(&mut doc, "M.c2.u")["S231:isArray"] = json!(true);
    assert_eq!(
        reject(&doc),
        vec![tagged(
            DiagCode::NonSubsetConstruct,
            "http://example.org#M.c2.u",
            "composite/array-connector: array-valued connector nodes are not supported; use \
             per-element connectors named `name_1` through `name_n`",
        )]
    );
}

#[test]
fn active_array_instance_is_rejected_with_stable_rule_identity() {
    let mut doc = document();
    node_mut(&mut doc, "M.c2")["S231:sizeOfDimensions"] = json!("(2)");
    assert_eq!(
        reject(&doc),
        vec![tagged(
            DiagCode::NonSubsetConstruct,
            "http://example.org#M.c2",
            "composite/array-instance: array-valued block-instance nodes are not supported; use \
             per-element instances named `name_1` through `name_n`",
        )]
    );
}

#[test]
fn inactive_array_instance_and_its_connector_are_ignored_after_specialization() {
    let mut doc = document();
    node_mut(&mut doc, "#M")["S231:hasParameter"] = json!({ "@id": "http://example.org#M.have" });
    node_mut(&mut doc, "M.c1.y")
        .as_object_mut()
        .expect("connector object")
        .remove("S231:isConnectedTo");
    let instance = node_mut(&mut doc, "M.c2");
    instance["S231:isConditionalComponent"] = json!(true);
    instance["S231:conditionalExpression"] = json!("have");
    instance["S231:isArray"] = json!(true);
    node_mut(&mut doc, "M.c2.u")["S231:sizeOfDimensions"] = json!("(2)");
    doc["@graph"]
        .as_array_mut()
        .expect("@graph array")
        .push(json!({
            "@id": "http://example.org#M.have",
            "S231:value": {
                "@value": "false",
                "@type": "http://www.w3.org/2001/XMLSchema#boolean"
            }
        }));

    import_clean(&serde_json::to_vec(&doc).expect("serialize conditional fixture"));
}

#[test]
fn new_guard_and_boundary_pass_through_defense_both_report() {
    let mut doc: JsonValue =
        serde_json::from_str(include_str!("fixtures/pass_through_miniature.jsonld"))
            .expect("pass-through fixture JSON");
    node_mut(&mut doc, "realOut")["S231:isArray"] = json!(true);
    node_mut(&mut doc, "realOut")["S231:sizeOfDimensions"] = json!("(2)");
    let diagnostics = reject(&doc);
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message.starts_with("composite/array-connector: ")),
        "new document-wide guard must remain live: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message == "array boundary pass-through endpoints are unsupported"),
        "pass-through defense-in-depth arm must remain live: {diagnostics:?}"
    );
}

#[test]
fn preserved_array_parameter_fixtures_remain_warning_free() {
    for (name, src) in [
        (
            "g36/multizone_vav_outdoor_airflow_sumzone.jsonld",
            include_bytes!("fixtures/g36/multizone_vav_outdoor_airflow_sumzone.jsonld").as_slice(),
        ),
        (
            "g36/multizone_vav_outdoor_airflow_title24_sumzone.jsonld",
            include_bytes!("fixtures/g36/multizone_vav_outdoor_airflow_title24_sumzone.jsonld")
                .as_slice(),
        ),
        (
            "array_preserved.jsonld",
            include_bytes!("fixtures/array_preserved.jsonld").as_slice(),
        ),
        (
            "array2d_preserved.jsonld",
            include_bytes!("fixtures/array2d_preserved.jsonld").as_slice(),
        ),
        (
            "array_expression_preserved.jsonld",
            include_bytes!("fixtures/array_expression_preserved.jsonld").as_slice(),
        ),
    ] {
        let result = std::panic::catch_unwind(|| import_clean(src));
        assert!(
            result.is_ok(),
            "{name} must remain a warning-free positive control"
        );
    }
}

#[test]
fn existing_array_failure_fixtures_do_not_gain_new_rule_diagnostics() {
    for (name, src) in [
        (
            "invalid/array_flatten_collision.jsonld",
            include_bytes!("fixtures/invalid/array_flatten_collision.jsonld").as_slice(),
        ),
        (
            "composite_contract/rejected/array_parameter.jsonld",
            include_bytes!("fixtures/composite_contract/rejected/array_parameter.jsonld")
                .as_slice(),
        ),
    ] {
        let diagnostics = match import_cxf(src, &ResolveOptions::default()) {
            Err(CxfError::Validation(diags)) => diags,
            other => panic!("{name} must retain its existing failure, got {other:?}"),
        };
        assert!(
            diagnostics.iter().all(|diag| {
                !diag.message.starts_with("composite/array-connector: ")
                    && !diag.message.starts_with("composite/array-instance: ")
            }),
            "{name} gained an array-node diagnostic: {diagnostics:?}"
        );
    }
}
