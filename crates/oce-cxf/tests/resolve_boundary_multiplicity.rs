//! Path multiplicity contracts for nested-composite boundary lowering.

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::DiagCode;
use oce_model::ModelGraph;
use serde_json::json;

fn import(document: &serde_json::Value) -> Result<ModelGraph, Vec<oce_diag::Diagnostic>> {
    let bytes = serde_json::to_vec(document).expect("serialize document");
    match import_cxf(&bytes, &ResolveOptions::default()) {
        Ok((graph, report)) => {
            assert!(report.is_empty(), "{report:?}");
            Ok(graph)
        }
        Err(CxfError::Validation(diagnostics)) => Err(diagnostics),
        Err(other) => panic!("expected validation result, got {other:?}"),
    }
}

/// Two independent drivers share the same boundary continuation. A global visited set would
/// expand the shared boundary only for the first source and hide the second drive.
#[test]
fn shared_boundary_continuation_preserves_distinct_source_drives() {
    let model = "http://example.org#shared-boundary";
    let mut graph = vec![
        json!({
            "@id": model, "@type": "S231:Block",
            "S231:containsBlock": [
                { "@id": format!("{model}.srcA") }, { "@id": format!("{model}.srcB") },
                { "@id": format!("{model}.sub") }, { "@id": format!("{model}.gain") }
            ]
        }),
        json!({
            "@id": format!("{model}.sub"), "@type": "S231:Block",
            "S231:containsBlock": { "@id": format!("{model}.sub.keep") },
            "S231:hasInput": { "@id": format!("{model}.sub.u") }
        }),
        json!({
            "@id": format!("{model}.sub.u"), "@type": "S231:RealInput",
            "S231:isOfDataType": { "@id": "S231:Real" },
            "S231:isConnectedTo": { "@id": format!("{model}.gain.u") }
        }),
        json!({
            "@id": format!("{model}.sub.keep"),
            "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
            "S231:hasParameter": { "@id": format!("{model}.sub.keep.k") },
            "S231:hasOutput": { "@id": format!("{model}.sub.keep.y") }
        }),
        json!({ "@id": format!("{model}.sub.keep.k"), "S231:value": 1 }),
        json!({
            "@id": format!("{model}.sub.keep.y"), "@type": "S231:RealOutput",
            "S231:isOfDataType": { "@id": "S231:Real" }
        }),
        json!({
            "@id": format!("{model}.gain"),
            "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.MultiplyByParameter",
            "S231:hasParameter": { "@id": format!("{model}.gain.k") },
            "S231:hasInput": { "@id": format!("{model}.gain.u") },
            "S231:hasOutput": { "@id": format!("{model}.gain.y") }
        }),
        json!({ "@id": format!("{model}.gain.k"), "S231:value": 1 }),
        json!({
            "@id": format!("{model}.gain.u"), "@type": "S231:RealInput",
            "S231:isOfDataType": { "@id": "S231:Real" }
        }),
        json!({
            "@id": format!("{model}.gain.y"), "@type": "S231:RealOutput",
            "S231:isOfDataType": { "@id": "S231:Real" }
        }),
    ];
    for source in ["srcA", "srcB"] {
        graph.extend([
            json!({
                "@id": format!("{model}.{source}"),
                "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
                "S231:hasParameter": { "@id": format!("{model}.{source}.k") },
                "S231:hasOutput": { "@id": format!("{model}.{source}.y") }
            }),
            json!({ "@id": format!("{model}.{source}.k"), "S231:value": 1 }),
            json!({
                "@id": format!("{model}.{source}.y"), "@type": "S231:RealOutput",
                "S231:isOfDataType": { "@id": "S231:Real" },
                "S231:isConnectedTo": { "@id": format!("{model}.sub.u") }
            }),
        ]);
    }
    let document = json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" }, "@graph": graph
    });
    let diagnostics = import(&document).expect_err("two drives must reject");
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagCode::SingleAssignment
            && diagnostic.subject.as_deref() == Some(format!("{model}.gain.u").as_str())
    }));
}

#[test]
fn repeated_nested_continuation_remains_a_deterministic_double_drive() {
    let model = "http://example.org#repeated-boundary";
    let document = json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" },
        "@graph": [
            {
                "@id": model, "@type": "S231:Block",
                "S231:containsBlock": [
                    { "@id": format!("{model}.src") }, { "@id": format!("{model}.outer") }
                ]
            },
            {
                "@id": format!("{model}.src"),
                "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
                "S231:hasParameter": { "@id": format!("{model}.src.k") },
                "S231:hasOutput": { "@id": format!("{model}.src.y") }
            },
            { "@id": format!("{model}.src.k"), "S231:value": 1 },
            {
                "@id": format!("{model}.src.y"), "@type": "S231:RealOutput",
                "S231:isOfDataType": { "@id": "S231:Real" },
                "S231:isConnectedTo": { "@id": format!("{model}.outer.u") }
            },
            {
                "@id": format!("{model}.outer"), "@type": "S231:Block",
                "S231:containsBlock": { "@id": format!("{model}.outer.inner") },
                "S231:hasInput": { "@id": format!("{model}.outer.u") }
            },
            {
                "@id": format!("{model}.outer.u"), "@type": "S231:RealInput",
                "S231:isOfDataType": { "@id": "S231:Real" },
                "S231:isConnectedTo": [
                    { "@id": format!("{model}.outer.inner.u") },
                    { "@id": format!("{model}.outer.inner.u") }
                ]
            },
            {
                "@id": format!("{model}.outer.inner"), "@type": "S231:Block",
                "S231:containsBlock": { "@id": format!("{model}.gain") },
                "S231:hasInput": { "@id": format!("{model}.outer.inner.u") }
            },
            {
                "@id": format!("{model}.outer.inner.u"), "@type": "S231:RealInput",
                "S231:isOfDataType": { "@id": "S231:Real" },
                "S231:isConnectedTo": { "@id": format!("{model}.gain.u") }
            },
            {
                "@id": format!("{model}.gain"),
                "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.MultiplyByParameter",
                "S231:hasParameter": { "@id": format!("{model}.gain.k") },
                "S231:hasInput": { "@id": format!("{model}.gain.u") },
                "S231:hasOutput": { "@id": format!("{model}.gain.y") }
            },
            { "@id": format!("{model}.gain.k"), "S231:value": 1 },
            {
                "@id": format!("{model}.gain.u"), "@type": "S231:RealInput",
                "S231:isOfDataType": { "@id": "S231:Real" }
            },
            {
                "@id": format!("{model}.gain.y"), "@type": "S231:RealOutput",
                "S231:isOfDataType": { "@id": "S231:Real" }
            }
        ]
    });
    let first = import(&document).expect_err("repeated path must remain a double drive");
    let second = import(&document).expect_err("repeat must reject identically");
    assert_eq!(first, second);
    assert_eq!(first.len(), 1, "{first:?}");
    assert_eq!(first[0].code, DiagCode::SingleAssignment);
    assert_eq!(
        first[0].subject.as_deref(),
        Some(format!("{model}.gain.u").as_str())
    );
}

#[test]
fn accepted_nested_fanout_follows_target_graph_order() {
    let model = "http://example.org#ordered-boundary";
    let document = json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" },
        "@graph": [
            {
                "@id": model, "@type": "S231:Block",
                "S231:containsBlock": [
                    { "@id": format!("{model}.src") }, { "@id": format!("{model}.sub") }
                ]
            },
            {
                "@id": format!("{model}.src"),
                "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
                "S231:hasParameter": { "@id": format!("{model}.src.k") },
                "S231:hasOutput": { "@id": format!("{model}.src.y") }
            },
            { "@id": format!("{model}.src.k"), "S231:value": 1 },
            {
                "@id": format!("{model}.src.y"), "@type": "S231:RealOutput",
                "S231:isOfDataType": { "@id": "S231:Real" },
                "S231:isConnectedTo": { "@id": format!("{model}.sub.u") }
            },
            {
                "@id": format!("{model}.sub"), "@type": "S231:Block",
                "S231:containsBlock": [
                    { "@id": format!("{model}.first") }, { "@id": format!("{model}.second") }
                ],
                "S231:hasInput": { "@id": format!("{model}.sub.u") }
            },
            {
                "@id": format!("{model}.sub.u"), "@type": "S231:RealInput",
                "S231:isOfDataType": { "@id": "S231:Real" },
                "S231:isConnectedTo": [
                    { "@id": format!("{model}.second.u") },
                    { "@id": format!("{model}.first.u") }
                ]
            },
            {
                "@id": format!("{model}.first"),
                "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.MultiplyByParameter",
                "S231:hasParameter": { "@id": format!("{model}.first.k") },
                "S231:hasInput": { "@id": format!("{model}.first.u") },
                "S231:hasOutput": { "@id": format!("{model}.first.y") }
            },
            { "@id": format!("{model}.first.k"), "S231:value": 1 },
            {
                "@id": format!("{model}.first.u"), "@type": "S231:RealInput",
                "S231:isOfDataType": { "@id": "S231:Real" }
            },
            {
                "@id": format!("{model}.first.y"), "@type": "S231:RealOutput",
                "S231:isOfDataType": { "@id": "S231:Real" }
            },
            {
                "@id": format!("{model}.second"),
                "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.MultiplyByParameter",
                "S231:hasParameter": { "@id": format!("{model}.second.k") },
                "S231:hasInput": { "@id": format!("{model}.second.u") },
                "S231:hasOutput": { "@id": format!("{model}.second.y") }
            },
            { "@id": format!("{model}.second.k"), "S231:value": 1 },
            {
                "@id": format!("{model}.second.u"), "@type": "S231:RealInput",
                "S231:isOfDataType": { "@id": "S231:Real" }
            },
            {
                "@id": format!("{model}.second.y"), "@type": "S231:RealOutput",
                "S231:isOfDataType": { "@id": "S231:Real" }
            }
        ]
    });
    let graph = import(&document).expect("distinct fan-out targets must import");
    let targets = graph
        .connections
        .iter()
        .map(|connection| {
            graph.connectors[connection.to.0 as usize]
                .iri
                .as_deref()
                .expect("target IRI")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        targets,
        [
            format!("{model}.first.u").as_str(),
            format!("{model}.second.u").as_str()
        ]
    );
}
