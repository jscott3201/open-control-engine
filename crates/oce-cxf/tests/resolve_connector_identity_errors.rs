//! Authored connector identity rejection contracts.

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::DiagCode;
use serde_json::{Value, json};

const FIXTURE: &str = include_str!("fixtures/minimal_loop.jsonld");

fn diagnostics(document: &Value) -> Vec<oce_diag::Diagnostic> {
    let bytes = serde_json::to_vec(document).expect("document serializes");
    match import_cxf(&bytes, &ResolveOptions::default()) {
        Err(CxfError::Validation(diagnostics)) => diagnostics,
        other => panic!("expected validation diagnostics, got {other:?}"),
    }
}

#[test]
fn connector_without_authored_id_is_rejected_with_its_graph_position() {
    let mut document: Value = serde_json::from_str(FIXTURE).expect("fixture parses");
    document["@graph"][3]
        .as_object_mut()
        .expect("connector node")
        .remove("@id");

    let diagnostics = diagnostics(&document);
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.is_error()
                && diagnostic.code == DiagCode::MissingConnectorId
                && diagnostic.message == "connector at @graph[3] is missing its authored @id"
                && diagnostic.subject.as_deref() == Some("connector at @graph[3]")
        }),
        "{diagnostics:#?}"
    );
}

#[test]
fn duplicate_authored_id_names_both_graph_nodes() {
    let mut document: Value = serde_json::from_str(FIXTURE).expect("fixture parses");
    document["@graph"][7]["@id"] = json!("http://example.org#MinLoop.con.y");

    let diagnostics = diagnostics(&document);
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.is_error()
                && diagnostic.code == DiagCode::DuplicateId
                && diagnostic.message
                    == "duplicate @id `http://example.org#MinLoop.con.y` on @graph nodes 3 and 7"
                && diagnostic.subject.as_deref() == Some("http://example.org#MinLoop.con.y")
        }),
        "{diagnostics:#?}"
    );
}
