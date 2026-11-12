//! Interning identity of lowering endpoints, shared lowered allocations, and repeat determinism.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::json;

use super::{EndpointIx, LoweringEndpoints};
use crate::dto::{CxfDocument, Node};
use crate::resolve::composite::{LoweredCxf, lower};
use crate::resolve::composite_orientation::CompositeOrientation;
use crate::resolve::declaration_scope::WithheldFindings;
use crate::resolve::parameter_attrs::CheckedParameters;
use crate::resolve::specialize::Specialization;

/// A top composite with a nested composite, a kept nested-input edge, a re-anchored top-input
/// edge, and a leaf input that names the top input twice (swapped onto the top input).
fn document() -> CxfDocument {
    let leaf = |id: &str| {
        json!({
            "@id": id,
            "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.MultiplyByParameter",
            "S231:hasParameter": { "@id": format!("{id}.k") },
            "S231:hasInput": { "@id": format!("{id}.u") },
            "S231:hasOutput": { "@id": format!("{id}.y") }
        })
    };
    let port = |id: &str, kind: &str| json!({ "@id": id, "@type": format!("S231:{kind}") });
    let parameter = |id: &str| json!({ "@id": id, "@type": "S231:Parameter", "S231:value": 1.0 });
    let mut graph = vec![
        json!({ "@id": "root", "@type": "S231:Block",
                "S231:containsBlock": [ { "@id": "sub" }, { "@id": "post" } ],
                "S231:hasInput": { "@id": "root.u" } }),
        json!({ "@id": "root.u", "@type": "S231:RealInput",
                "S231:isConnectedTo": { "@id": "sub.u" } }),
        json!({ "@id": "sub", "@type": "S231:Block",
                "S231:containsBlock": { "@id": "inner" },
                "S231:hasInput": { "@id": "sub.u" } }),
        json!({ "@id": "sub.u", "@type": "S231:RealInput",
                "S231:isConnectedTo": { "@id": "inner.u" } }),
        leaf("inner"),
        parameter("inner.k"),
        port("inner.u", "RealInput"),
        port("inner.y", "RealOutput"),
        leaf("post"),
        parameter("post.k"),
        port("post.y", "RealOutput"),
    ];
    graph.push(json!({ "@id": "post.u", "@type": "S231:RealInput",
                       "S231:isConnectedTo": [ { "@id": "root.u" }, { "@id": "root.u" } ] }));
    serde_json::from_value(json!({
        "@context": { "S231": "http://data.ashrae.org/S231#" },
        "@graph": graph
    }))
    .expect("document")
}

fn by_id(doc: &CxfDocument) -> HashMap<&str, &Node> {
    doc.graph
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect()
}

fn lowered(doc: &CxfDocument) -> (LoweredCxf, Vec<oce_diag::Diagnostic>) {
    let mut diags = Vec::new();
    let lowered = lower(
        doc,
        &by_id(doc),
        &Specialization::default(),
        WithheldFindings::default(),
        &mut CheckedParameters::default(),
        &mut diags,
    );
    (lowered, diags)
}

fn rendered_edges(lowered: &LoweredCxf) -> Vec<(String, String)> {
    lowered
        .connection_edges()
        .map(|(source, target)| (source.to_owned(), target.to_owned()))
        .collect()
}

#[test]
fn byte_equal_iris_share_one_endpoint_whatever_their_allocation() {
    let doc = document();
    let by_id = by_id(&doc);
    let specialization = Specialization::default();
    let orientation = CompositeOrientation::new(&doc, &by_id, "root", &specialization);
    let first = String::from("sub.u");
    let second = String::from("sub.u");
    let other = String::from("inner.u");
    let mut endpoints = LoweringEndpoints::default();
    let id = endpoints.intern(&first, &orientation, &by_id, &specialization);
    assert_eq!(id, EndpointIx(0));
    assert_eq!(
        endpoints.intern(&second, &orientation, &by_id, &specialization),
        id
    );
    assert_eq!(
        endpoints.intern(&other, &orientation, &by_id, &specialization),
        EndpointIx(1)
    );
    assert_eq!(endpoints.lookup("sub.u"), Some(id));
    assert_eq!(endpoints.lookup("missing"), None);
    // The classification is the one the string-keyed index answers.
    let nested = endpoints.get(id);
    assert!(nested.elided && nested.node && !nested.inactive && !nested.synthesized);
    assert_eq!(nested.position, orientation.position("sub.u"));
    let leaf = endpoints.get(EndpointIx(1));
    assert!(!leaf.elided && leaf.node);
}

/// Lowering one document twice in one process yields identical edges, sidecar and diagnostics:
/// endpoint ids never leak into the output.
#[test]
fn lowering_one_document_twice_yields_identical_edges() {
    let doc = document();
    let (first, first_diags) = lowered(&doc);
    let (second, second_diags) = lowered(&doc);
    assert_eq!(rendered_edges(&first), rendered_edges(&second));
    assert_eq!(first_diags, second_diags);
    assert_eq!(
        rendered_edges(&first),
        [
            ("root.u", "inner.u"),
            ("root.u", "post.u"),
            ("root.u", "post.u")
        ]
        .map(|(source, target)| (source.to_owned(), target.to_owned()))
    );
}

/// A swapped fan-out lowers every copy of its canonical target to one shared allocation, so a
/// long target is held once however many edges name it.
#[test]
fn swapped_fanout_lowers_to_one_shared_allocation() {
    let doc = document();
    let (lowered, _) = lowered(&doc);
    let (_, targets) = lowered
        .graph_edge_groups()
        .find(|(source, _)| *source == "root.u")
        .expect("the top input drives the lowered edges");
    let copies: Vec<&Arc<str>> = targets
        .iter()
        .filter(|target| &***target == "post.u")
        .collect();
    assert_eq!(copies.len(), 2);
    assert!(Arc::ptr_eq(copies[0], copies[1]));
}
