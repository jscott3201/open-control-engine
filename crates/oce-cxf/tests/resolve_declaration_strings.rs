//! Whole-document regressions for String literals in composite declaration scopes.

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::DiagCode;
use oce_model::ModelGraph;
use serde_json::{Value as Json, json};

fn import(doc: &Json) -> Result<ModelGraph, Vec<oce_diag::Diagnostic>> {
    let bytes = serde_json::to_vec(doc).expect("serializable test document");
    match import_cxf(&bytes, &ResolveOptions::default()) {
        Ok((graph, report)) => {
            assert!(report.is_empty(), "expected no warnings: {report:?}");
            Ok(graph)
        }
        Err(CxfError::Validation(diags)) => Err(diags),
        Err(other) => panic!("unexpected import failure: {other:?}"),
    }
}

fn document(declarations: &[(&str, &str)]) -> Json {
    let refs: Vec<Json> = declarations
        .iter()
        .map(|(name, _)| json!({ "@id": format!("http://example.org#M.{name}") }))
        .collect();
    let mut graph = vec![
        json!({
            "@id": "http://example.org#M",
            "@type": "S231:Block",
            "S231:hasParameter": refs,
            "S231:containsBlock": { "@id": "http://example.org#M.con" }
        }),
        json!({
            "@id": "http://example.org#M.con",
            "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
            "S231:hasParameter": { "@id": "http://example.org#M.con.k" },
            "S231:hasOutput": { "@id": "http://example.org#M.con.y" }
        }),
        json!({ "@id": "http://example.org#M.con.k", "S231:value": 1.0 }),
        json!({
            "@id": "http://example.org#M.con.y",
            "@type": "S231:RealOutput",
            "S231:isOfDataType": { "@id": "S231:Real" }
        }),
    ];
    graph.extend(declarations.iter().map(|(name, value)| {
        json!({
            "@id": format!("http://example.org#M.{name}"),
            "S231:value": value
        })
    }));
    json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" },
        "@graph": graph
    })
}

#[test]
fn sibling_named_string_literal_loads_without_a_cycle_in_both_orders() {
    let mut baseline = None;
    for declarations in [[("a", "\"b\""), ("b", "a")], [("b", "a"), ("a", "\"b\"")]] {
        let graph = import(&document(&declarations))
            .unwrap_or_else(|diags| panic!("{declarations:?} must load: {diags:#?}"));
        assert_eq!(graph.blocks.len(), 1);
        assert_eq!(
            graph.blocks[0].class_iri.as_ref(),
            "CDL.Reals.Sources.Constant"
        );
        let rendered = format!("{graph:#?}");
        if let Some(expected) = &baseline {
            assert_eq!(
                &rendered, expected,
                "declaration order must not alter the flattened graph"
            );
        } else {
            baseline = Some(rendered);
        }
    }
}

#[test]
fn unterminated_self_named_literal_reaches_the_expression_diagnostic() {
    let diags = import(&document(&[("a", "\"a")])).expect_err("the expression is malformed");
    assert_eq!(diags.len(), 1, "{diags:#?}");
    assert_eq!(diags[0].code, DiagCode::GroundingFailed);
    assert_eq!(diags[0].subject.as_deref(), Some("http://example.org#M.a"));
    assert_eq!(
        diags[0].message,
        "expression binding did not ground: expression parse error: unterminated string literal"
    );
}
