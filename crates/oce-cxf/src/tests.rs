//! Layer-A DTO tests: the RT-1 lossless round-trip, the `OneOrMany` serialization symmetry (a
//! single value is NOT a 1-element array), the `CxfValue` untagged variant ordering, and that
//! the primary `minimal_loop.jsonld` fixture parses into the expected shape.

use super::{CxfError, CxfValue, Node, parse_document, write_document};

const FIXTURE: &str = include_str!("../tests/fixtures/minimal_loop.jsonld");

#[test]
fn rt1_minimal_loop_is_lossless_modulo_key_order() {
    // parse → serialize → re-parse must equal the original as a normalized JSON value tree
    // (serde_json::Value compares objects key-order-insensitively, which is the RT-1 contract).
    let doc = parse_document(FIXTURE.as_bytes()).expect("fixture must parse");
    let bytes = write_document(&doc).expect("doc must serialize");
    let reparsed: serde_json::Value = serde_json::from_slice(&bytes).expect("re-parse");
    let original: serde_json::Value = serde_json::from_str(FIXTURE).expect("original parse");
    assert_eq!(
        original, reparsed,
        "Layer-A round-trip must be lossless modulo key ordering / whitespace"
    );
}

#[test]
fn one_or_many_serializes_one_as_scalar_not_array() {
    // A single value round-trips as a bare object, NOT a 1-element array (producer byte style).
    let node: Node = serde_json::from_str(r#"{"@id":"x","S231:hasInput":{"@id":"a"}}"#).unwrap();
    assert_eq!(node.has_input.len(), 1);
    let v = serde_json::to_value(&node).unwrap();
    assert!(
        v["S231:hasInput"].is_object(),
        "One must serialize as an object, got {:?}",
        v["S231:hasInput"]
    );

    let node2: Node =
        serde_json::from_str(r#"{"@id":"x","S231:hasInput":[{"@id":"a"},{"@id":"b"}]}"#).unwrap();
    assert_eq!(node2.has_input.len(), 2);
    let v2 = serde_json::to_value(&node2).unwrap();
    assert!(
        v2["S231:hasInput"].is_array(),
        "Many must serialize as an array"
    );
}

#[test]
fn one_or_many_empty_is_absent_on_serialize() {
    // A node with no edges omits the keys entirely (is_empty skip), not `null`/`[]`.
    let node: Node = serde_json::from_str(r#"{"@id":"x"}"#).unwrap();
    assert!(node.has_input.is_empty());
    let v = serde_json::to_value(&node).unwrap();
    assert!(v.get("S231:hasInput").is_none());
}

#[test]
fn cxf_value_untagged_variant_ordering() {
    // false→Bool, 0→Int, 1.5→Float, {@value,@type}→Typed, "expr"→Expr (R-5 order).
    assert!(matches!(
        serde_json::from_str::<CxfValue>("false").unwrap(),
        CxfValue::Bool(false)
    ));
    assert!(matches!(
        serde_json::from_str::<CxfValue>("0").unwrap(),
        CxfValue::Int(0)
    ));
    assert!(matches!(
        serde_json::from_str::<CxfValue>("1.5").unwrap(),
        CxfValue::Float(_)
    ));
    let typed: CxfValue = serde_json::from_str(
        r#"{"@value":"1e-37","@type":"http://www.w3.org/2001/XMLSchema#double"}"#,
    )
    .unwrap();
    assert!(matches!(typed, CxfValue::Typed { .. }));
    assert!(matches!(
        serde_json::from_str::<CxfValue>(r#""kCooCoi""#).unwrap(),
        CxfValue::Expr(_)
    ));
    // Re-serialization keeps bare literal forms (not wrapped).
    assert_eq!(serde_json::to_string(&CxfValue::Int(0)).unwrap(), "0");
    assert_eq!(
        serde_json::to_string(&CxfValue::Bool(false)).unwrap(),
        "false"
    );
}

#[test]
fn minimal_loop_parses_with_expected_shape() {
    let doc = parse_document(FIXTURE.as_bytes()).expect("fixture must parse");

    // The composite top node: @type S231:Block, 5 children, one cosmetic passthrough key.
    let top = &doc.graph[0];
    assert_eq!(top.id, "http://example.org#MinLoop");
    let ttype = top.r#type.as_ref().expect("top has @type");
    assert_eq!(ttype.len(), 1);
    assert_eq!(ttype.as_slice()[0], "S231:Block");
    assert_eq!(top.contains_block.len(), 5);
    assert!(
        top.other.contains_key("cdlLineNumStart"),
        "unmodeled key must flow through `other` (R-8)"
    );

    // G-3: an instance node's @type is a concrete class IRI (not an S231:* class).
    let con = doc
        .graph
        .iter()
        .find(|n| n.id.ends_with(".con"))
        .expect("con node");
    assert!(
        con.r#type.as_ref().unwrap().as_slice()[0].contains("CDL.Reals.Sources.Constant"),
        "instance @type must be the class IRI (G-3)"
    );

    // Connections are source-anchored OneOrMany: del.y fans out (Many=2), con.y is single (One=1).
    let del_y = doc
        .graph
        .iter()
        .find(|n| n.id.ends_with(".del.y"))
        .expect("del.y");
    assert_eq!(
        del_y.is_connected_to.len(),
        2,
        "del.y fans out to gain.u and gt.u1"
    );
    let con_y = doc
        .graph
        .iter()
        .find(|n| n.id.ends_with(".con.y"))
        .expect("con.y");
    assert_eq!(con_y.is_connected_to.len(), 1);

    // CxfValue forms: del.y_start is a bare Int, con.k is a typed-literal double.
    let y_start = doc
        .graph
        .iter()
        .find(|n| n.id.ends_with(".del.y_start"))
        .expect("y_start");
    assert!(matches!(y_start.value, Some(CxfValue::Int(0))));
    let k = doc
        .graph
        .iter()
        .find(|n| n.id.ends_with(".con.k"))
        .expect("con.k");
    assert!(matches!(k.value, Some(CxfValue::Typed { .. })));
}

#[test]
fn typed_literal_preserves_extra_jsonld_keys() {
    // R-8: a JSON-LD value object with keys beyond @value/@type (e.g. @index) must NOT lose them.
    let v: CxfValue = serde_json::from_str(
        r#"{"@value":"5","@type":"http://www.w3.org/2001/XMLSchema#double","@index":"3"}"#,
    )
    .unwrap();
    let back = serde_json::to_value(&v).unwrap();
    assert_eq!(
        back["@index"],
        serde_json::json!("3"),
        "extra key must survive"
    );
    assert_eq!(back["@value"], serde_json::json!("5"));
}

#[test]
fn iri_ref_preserves_extra_keys() {
    // R-8: a reference object carrying @index/@type beyond @id round-trips losslessly.
    let node: Node =
        serde_json::from_str(r#"{"@id":"x","S231:hasInput":{"@id":"a","@index":2}}"#).unwrap();
    let v = serde_json::to_value(&node).unwrap();
    assert_eq!(v["S231:hasInput"]["@index"], serde_json::json!(2));
    assert_eq!(v["S231:hasInput"]["@id"], serde_json::json!("a"));
}

#[test]
fn empty_array_edge_normalizes_to_absent() {
    // Documented RT-1 normalization (F1): an empty edge array ≡ absent (no members).
    let node: Node = serde_json::from_str(r#"{"@id":"x","S231:hasInput":[]}"#).unwrap();
    assert!(node.has_input.is_empty());
    let v = serde_json::to_value(&node).unwrap();
    assert!(
        v.get("S231:hasInput").is_none(),
        "empty array normalizes to absent (intentional)"
    );
}

#[test]
fn parse_document_rejects_invalid_json() {
    let err = parse_document(b"{ this is not json").unwrap_err();
    assert!(matches!(err, CxfError::Json(_)));
}

#[test]
fn cxf_error_validation_wraps_shared_diagnostics() {
    // The Validation variant carries the shared oce-diag vocabulary (AD-4 integration).
    let diags = vec![oce_diag::Diagnostic::error(
        oce_diag::DiagCode::DuplicateId,
        "duplicate @id",
    )];
    let err = CxfError::Validation(diags);
    assert!(err.to_string().contains("1 diagnostic"));
}
