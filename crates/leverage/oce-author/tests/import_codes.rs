//! Every [`oce_author::ImportErrorKind`] code is produced by at least one document (issue #496,
//! acceptance): one small case per code, CXF or v0, each asserted to give exactly that code. The
//! case table and [`oce_author::IMPORT_ERROR_CODES`] must cover each other, so a new kind needs a
//! case here before the suite passes.

mod support;

use std::collections::BTreeSet;

use oce_author::{
    IMPORT_ERROR_CODES, ImportOptions, MAX_WIRE_LIST_LEN, import_cxf, import_graph_v0,
};
use serde_json::json;

use support::edit::{Document, reference, typed};
use support::fixture;

const ROOT: &str = "urn:oce:author:econ_enable";

fn id(local: &str) -> String {
    format!("{ROOT}.{local}")
}

fn econ() -> Document {
    Document::read(&fixture("emit/econ_enable.cxf.json"))
}

fn v0(param_value: &str) -> String {
    format!(
        "{{\"name\": \"s\", \"inputs\": [{{\"name\": \"u\", \"type\": \"real\", \"unit\": \"1\", \"description\": \"\"}}], \"outputs\": [], \"blocks\": [{{\"id\": \"g\", \"class\": \"Reals.MultiplyByParameter\", \"params\": [{{\"name\": \"k\", \"value\": {param_value}}}]}}], \"connections\": [\"u -> g.u\"]}}"
    )
}

/// A source form and its bytes.
enum Case {
    Cxf(Vec<u8>),
    V0(Vec<u8>),
}

fn code_of(case: &Case) -> &'static str {
    let options = ImportOptions::new();
    let read = match case {
        Case::Cxf(bytes) => import_cxf(bytes, &options),
        Case::V0(bytes) => import_graph_v0(bytes, &options),
    };
    match read {
        Err(error) => error.code(),
        Ok(_) => "imported",
    }
}

fn cases() -> Vec<(&'static str, Case)> {
    let cxf = |d: &mut Document| Case::Cxf(d.bytes());
    let many_blocks = {
        let one = "{\"id\": \"b\", \"class\": \"Logical.Not\", \"params\": []}";
        v0("1").replacen(
            "\"blocks\": [",
            &format!("\"blocks\": [{}, ", vec![one; MAX_WIRE_LIST_LEN].join(",")),
            1,
        )
    };
    vec![
        (
            "too-large",
            Case::Cxf(vec![b' '; oce_api::MAX_CXF_BYTES + 1]),
        ),
        (
            "not-strict-json",
            Case::Cxf(b"{\"@graph\": [], \"@graph\": []}".to_vec()),
        ),
        ("limit-exceeded", Case::V0(many_blocks.into_bytes())),
        (
            "unknown-key",
            cxf(econ().put(ROOT, "S231:isConnectedTo", json!([]))),
        ),
        (
            "missing-key",
            cxf(econ().remove(&id("hys.uLow"), "S231:value")),
        ),
        ("wrong-shape", cxf(econ().put(ROOT, "S231:label", json!(7)))),
        (
            "no-root",
            Case::Cxf(b"{\"@graph\": [{\"@id\": \"urn:x:a\"}]}".to_vec()),
        ),
        (
            "duplicate-id",
            Case::Cxf(b"{\"@graph\": [{\"@id\": \"urn:x:a\"}, {\"@id\": \"urn:x:a\"}]}".to_vec()),
        ),
        (
            "unresolved-reference",
            cxf(econ().put(ROOT, "S231:hasOutput", json!([reference(&id("nope"))]))),
        ),
        (
            "stray-node",
            cxf(econ().prepend(json!({ "@id": id("stray") }))),
        ),
        (
            "invalid-name",
            cxf(econ().prepend(json!({ "@id": id("a-b") }))),
        ),
        (
            "nested-composite",
            cxf(econ().put(&id("sub"), "S231:containsBlock", json!([]))),
        ),
        (
            "instance-member",
            cxf(econ().put(&id("sub"), "S231:hasInstance", json!([]))),
        ),
        (
            "constant-member",
            cxf(econ().put(ROOT, "S231:hasConstant", json!([]))),
        ),
        (
            "conditional-component",
            cxf(econ().put(&id("sub"), "S231:conditionalExpression", json!("true"))),
        ),
        (
            "array-connector",
            cxf(econ().put(&id("TOut"), "S231:isArray", json!(true))),
        ),
        (
            "array-sequence-parameter",
            cxf(econ().put(&id("dTHys"), "S231:isArray", json!(true))),
        ),
        (
            "expression-value",
            cxf(econ().put(&id("hys.uHigh"), "S231:value", json!("dTHys / 2"))),
        ),
        (
            "port-attribute",
            cxf(econ().put(&id("hys.u"), "S231:unit", json!("K"))),
        ),
        (
            "connector-attribute",
            cxf(econ().put(&id("TOut"), "S231:nominal", json!(293.15))),
        ),
        (
            "driving-sink",
            cxf(econ().put(
                &id("ecoEna"),
                "S231:isConnectedTo",
                json!([reference(&id("hys.u"))]),
            )),
        ),
        (
            "class-outside-obc",
            cxf(econ().put(
                &id("sub"),
                "@type",
                json!("http://example.org#Modelica.Blocks.Math.Add"),
            )),
        ),
        (
            "datatype-mismatch",
            cxf(econ().put(&id("hys.uLow"), "S231:value", json!(true))),
        ),
        (
            "unknown-datatype",
            cxf(econ().put(&id("dTHys"), "S231:isOfDataType", reference("S231:Bogus"))),
        ),
        (
            "unknown-member-type",
            cxf(econ().put(
                &id("hys.uLow"),
                "S231:value",
                json!("Buildings.Controls.OBC.CDL.Types.NoSuch.member"),
            )),
        ),
        (
            "malformed-literal",
            cxf(econ().put(&id("hys.uLow"), "S231:value", typed("zero", "double"))),
        ),
        (
            "array-shape",
            cxf(econ().put(&id("hys.uLow"), "S231:value", json!([0.0, 1.0]))),
        ),
        ("string-value", Case::V0(v0("\"two\"").into_bytes())),
        (
            "malformed-connection",
            Case::V0(v0("1").replacen("u -> g.u", "u => g.u", 1).into_bytes()),
        ),
    ]
}

#[test]
fn every_import_error_code_is_produced_by_a_case() {
    let mut produced = BTreeSet::new();
    for (expected, case) in cases() {
        assert_eq!(code_of(&case), expected);
        produced.insert(expected);
    }
    let listed: BTreeSet<&str> = IMPORT_ERROR_CODES.iter().copied().collect();
    assert_eq!(produced, listed);
    assert_eq!(IMPORT_ERROR_CODES.len(), 29);
}

#[test]
fn the_unedited_documents_import() {
    assert_eq!(code_of(&Case::Cxf(econ().bytes())), "imported");
    assert_eq!(code_of(&Case::V0(v0("1").into_bytes())), "imported");
}
