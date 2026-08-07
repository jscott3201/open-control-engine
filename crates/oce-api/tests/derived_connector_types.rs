//! Load-refused type documents for `hasInstance`-derived connectors (`_spec/19` R19-6),
//! outside the contract corpus because an accepted-corpus fixture the product refuses would
//! pin a graph the engine rejects.
//!
//! The precedence direction is the pin: a member node's RESOLVABLE declared type overrides the
//! signature `PortKind`, so an authored enum — and an `Analog*` coercion landing on an
//! `Integer`-signature port — is accepted by `import_cxf` and then refused loudly by
//! `Engine::load_cxf` with `PortKindMismatch`, exactly as the authored dialect refuses it.
//! Signature-first typing would turn both into silent accepts. The control with the member
//! type absent takes the signature kind (`Integer`) and loads clean. The boundary input's
//! declared type tracks the member's in each document, so the load refusal is attributable to
//! the block-signature check alone — never to a connection-endpoint mismatch, which the
//! resolver would refuse at import.

use oce_api::{Engine, OcError};
use oce_cxf::{ResolveOptions, import_cxf};
use oce_diag::DiagCode;
use oce_model::ValueType;
use serde_json::{Value as JsonValue, json};

const EX: &str = "http://example.org#";

/// A root wiring two boundary inputs into a derivation-shaped `CDL.Integers.Equal`.
/// `u1_type` decorates the `u1` member node (an `isOfDataType` enum reference, an `Analog*`
/// `@type`, or nothing) and `boundary1` types the driving boundary node to match.
fn equal_doc(u1_type: &JsonValue, boundary1: &JsonValue) -> JsonValue {
    let mut u1 = json!({ "@id": format!("{EX}M.eq.u1") });
    for (k, v) in u1_type.as_object().expect("decoration object") {
        u1[k] = v.clone();
    }
    let mut b1 = json!({ "@id": format!("{EX}M.u1"),
                         "S231:isConnectedTo": { "@id": format!("{EX}M.eq.u1") } });
    for (k, v) in boundary1.as_object().expect("boundary object") {
        b1[k] = v.clone();
    }
    json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#", "base": "http://example.org#" },
        "@graph": [
            {
                "@id": format!("{EX}M"), "@type": "S231:Block",
                "S231:hasInput": [ { "@id": format!("{EX}M.u1") }, { "@id": format!("{EX}M.u2") } ],
                "S231:containsBlock": { "@id": format!("{EX}M.eq") }
            },
            b1,
            { "@id": format!("{EX}M.u2"), "@type": "S231:IntegerInput",
              "S231:isOfDataType": { "@id": "S231:Integer" },
              "S231:isConnectedTo": { "@id": format!("{EX}M.eq.u2") } },
            {
                "@id": format!("{EX}M.eq"),
                "@type": format!("{EX}Buildings.Controls.OBC.CDL.Integers.Equal"),
                "S231:hasInstance": [
                    { "@id": format!("{EX}M.eq.u1") },
                    { "@id": format!("{EX}M.eq.u2") },
                    { "@id": format!("{EX}M.eq.y") }
                ]
            },
            u1
        ]
    })
}

fn load(doc: &JsonValue) -> Result<oce_api::LoadReport, OcError> {
    let bytes = serde_json::to_vec(doc).expect("serialize");
    Engine::in_memory().load_cxf(&bytes)
}

/// The `(code, subject, message)` triples of a load refusal's errors.
fn refusal_signature(e: &OcError) -> Vec<(DiagCode, Option<String>, String)> {
    e.diagnostics()
        .iter()
        .filter(|d| d.is_error())
        .map(|d| {
            (
                d.code,
                d.subject.as_deref().map(str::to_owned),
                d.message.clone(),
            )
        })
        .collect()
}

/// The value type `import_cxf` derives for the `u1` member connector — read through the
/// block's signature-bound input vector, because an externally driven derived input's `iri` is
/// re-keyed onto the driving boundary port's `@id` (R19-5).
fn u1_value_type(doc: &JsonValue) -> ValueType {
    let bytes = serde_json::to_vec(doc).expect("serialize");
    let (graph, report) =
        import_cxf(&bytes, &ResolveOptions::default()).expect("import accepts the document");
    assert!(
        report.diagnostics.iter().all(|d| !d.is_error()),
        "import must carry no errors: {:?}",
        report.diagnostics
    );
    let eq = graph
        .blocks
        .iter()
        .find(|b| b.class_iri.as_ref() == "CDL.Integers.Equal")
        .expect("the Equal instance");
    graph.connectors[eq.inputs[0].0 as usize].value_type
}

#[test]
fn enum_typed_member_is_imported_and_then_refused_by_the_deep_gate() {
    let doc = equal_doc(
        &json!({
            "S231:isOfDataType": {
                "@id": format!("{EX}Buildings.Controls.OBC.CDL.Types.SimpleController")
            }
        }),
        &json!({
            "@type": "S231:IntegerInput",
            "S231:isOfDataType": {
                "@id": format!("{EX}Buildings.Controls.OBC.CDL.Types.SimpleController")
            }
        }),
    );
    assert_eq!(
        u1_value_type(&doc),
        ValueType::Enum(oce_model::EnumClassId::SIMPLE_CONTROLLER),
        "the resolvable authored enum overrides the Integer signature kind at import"
    );
    let err = load(&doc).expect_err("the deep gate refuses an enum-typed derived input");
    let signature = refusal_signature(&err);
    assert_eq!(signature.len(), 1, "exactly one refusal: {signature:?}");
    assert_eq!(signature[0].0, DiagCode::PortKindMismatch);
    assert!(
        signature[0].2.contains("CDL.Integers.Equal")
            && signature[0].2.contains("Integer")
            && signature[0].2.contains("Enum"),
        "the refusal names the class, the signature kind, and the authored enum: {:?}",
        signature[0].2
    );
}

#[test]
fn analog_member_on_an_integer_signature_port_coerces_then_refuses() {
    let doc = equal_doc(
        &json!({ "@type": "S231:AnalogInput" }),
        &json!({ "@type": "S231:RealInput", "S231:isOfDataType": { "@id": "S231:Real" } }),
    );
    assert_eq!(
        u1_value_type(&doc),
        ValueType::Real,
        "the Analog* bridge resolves to Real (with the advisory), overriding Integer"
    );
    let err = load(&doc).expect_err("Real against an Integer-signature port refuses");
    let signature = refusal_signature(&err);
    assert_eq!(signature.len(), 1, "exactly one refusal: {signature:?}");
    assert_eq!(signature[0].0, DiagCode::PortKindMismatch);
}

#[test]
fn untyped_member_control_takes_the_signature_kind_and_loads_clean() {
    let doc = equal_doc(
        &json!({}),
        &json!({ "@type": "S231:IntegerInput", "S231:isOfDataType": { "@id": "S231:Integer" } }),
    );
    assert_eq!(
        u1_value_type(&doc),
        ValueType::Integer,
        "absence of a declared type is not a diagnostic; the signature kind types the connector"
    );
    let report = load(&doc).expect("the byte-identical untyped control loads end-to-end");
    assert!(
        report.warnings.is_empty(),
        "the control loads warning-free: {:?}",
        report.warnings
    );
}
