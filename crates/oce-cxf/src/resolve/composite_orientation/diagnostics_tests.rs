//! The memoized erased-relation expansion answers exactly what a fresh evaluation answers.

use std::collections::HashMap;

use serde_json::json;

use super::{CompositeOrientation, Polarity};
use crate::dto::{CxfDocument, Node};
use crate::resolve::specialize::Specialization;

/// A top composite whose nested composite input `sub.u` (erased by lowering) drives one leaf
/// input, so its expansion reaches a SINK: the verdict differs between the two polarities.
fn document() -> CxfDocument {
    serde_json::from_value(json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" },
        "@graph": [
            { "@id": "root", "@type": "S231:Block",
              "S231:containsBlock": { "@id": "sub" },
              "S231:hasInput": { "@id": "root.u" } },
            { "@id": "root.u", "@type": "S231:RealInput",
              "S231:isConnectedTo": { "@id": "sub.u" } },
            { "@id": "sub", "@type": "S231:Block",
              "S231:containsBlock": { "@id": "inner" },
              "S231:hasInput": { "@id": "sub.u" } },
            { "@id": "sub.u", "@type": "S231:RealInput",
              "S231:isConnectedTo": { "@id": "inner.u" } },
            { "@id": "inner",
              "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.MultiplyByParameter",
              "S231:hasParameter": { "@id": "inner.k" },
              "S231:hasInput": { "@id": "inner.u" },
              "S231:hasOutput": { "@id": "inner.y" } },
            { "@id": "inner.k", "@type": "S231:Parameter", "S231:value": 1.0 },
            { "@id": "inner.u", "@type": "S231:RealInput" },
            { "@id": "inner.y", "@type": "S231:RealOutput" }
        ]
    }))
    .expect("document")
}

#[test]
fn memoized_expansion_verdict_equals_a_fresh_evaluation_per_target_and_polarity() {
    let doc = document();
    let by_id: HashMap<&str, &Node> = doc
        .graph
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let specialization = Specialization::default();
    let orientation = CompositeOrientation::new(&doc, &by_id, "root", &specialization);
    let canonical = orientation.canonical_connections(&doc, &by_id, &specialization);
    let erased = canonical
        .endpoints
        .lookup("sub.u")
        .expect("sub.u is interned");
    assert!(canonical.endpoints.get(erased).elided);
    let fresh =
        |polarity| orientation.expansion_reaches_generic_refusal(&canonical, erased, polarity);
    // The leaf input below the erased boundary is a SINK, so the two polarities disagree: a memo
    // keyed on the target alone would answer one of them wrongly.
    assert!(fresh(Some(Polarity::Sink)));
    assert!(!fresh(Some(Polarity::Source)));
    assert!(fresh(None));
    let mut memo = HashMap::new();
    // One target reached with each polarity, then each again: every answer is the fresh one,
    // including the repeats served from the memo.
    for polarity in [
        Some(Polarity::Source),
        Some(Polarity::Sink),
        Some(Polarity::Source),
        None,
        Some(Polarity::Sink),
        None,
    ] {
        assert_eq!(
            orientation.memoized_expansion_reaches(&canonical, &mut memo, erased, polarity),
            fresh(polarity),
            "polarity {polarity:?}"
        );
    }
    assert_eq!(memo.len(), 3, "one entry per distinct (target, polarity)");
}
