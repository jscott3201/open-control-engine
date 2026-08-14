//! Refusals for boundary relations that lowering would otherwise erase.

use super::*;

/// An unreachable contradictory edge authored from a non-root boundary source must reject after
/// the bounded walk. The direct top-input arm keeps both leaf inputs driven, so dropping the dead
/// boundary chain would otherwise accept.
#[test]
fn contradictory_unreachable_boundary_source_remains_loud() {
    let mut document = sibling_document();
    let model = "http://example.org#siblings";
    node_mut(&mut document, "#siblings")
        .as_object_mut()
        .expect("root node")
        .remove("S231:hasOutput");
    set_absolute_targets(
        &mut document,
        ".u",
        &[
            &format!("{model}.subA.gain.u"),
            &format!("{model}.subB.gain.u"),
        ],
    );
    clear_targets(&mut document, ".subA.gain.y");
    set_absolute_targets(&mut document, ".subA.y", &[&format!("{model}.subB.u")]);
    set_absolute_targets(&mut document, ".subB.u", &[&format!("{model}.subB.gain.y")]);
    let diagnostics = import(&document).expect_err("contradictory boundary source must reject");
    assert_eq!(
        diagnostics,
        vec![Diagnostic::error(
            DiagCode::DirectionMismatch,
            "boundary connection has contradictory endpoint directions",
        )]
    );
}

/// A reached sibling input-to-input relation remains contradictory even when expansion would turn
/// it into a valid-looking top-input fanout with every leaf input driven.
#[test]
fn contradictory_reached_boundary_edge_cannot_flatten_into_valid_fanout() {
    let mut document = sibling_document();
    let model = "http://example.org#siblings";
    node_mut(&mut document, "#siblings")
        .as_object_mut()
        .expect("root node")
        .remove("S231:hasOutput");
    set_absolute_targets(
        &mut document,
        ".subA.u",
        &[&format!("{model}.subA.gain.u"), &format!("{model}.subB.u")],
    );
    let diagnostics = import(&document).expect_err("contradictory reached edge must reject");
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagCode::DirectionMismatch
            && diagnostic.message == "boundary connection has contradictory endpoint directions"
            && diagnostic.subject.is_none()
    }));
}

/// A contradictory edge targeting a nested boundary is erased when that boundary has no
/// continuation, but the malformed relation must still reject.
#[test]
fn contradictory_boundary_target_cannot_disappear_at_empty_boundary() {
    let model = "http://example.org#target_boundary";
    let mut graph = vec![json!({
        "@id": model,
        "@type": "S231:Block",
        "S231:containsBlock": { "@id": format!("{model}.sub") }
    })];
    graph.extend(constant_composite(model, "sub", false));
    let mut document = json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" },
        "@graph": graph
    });
    set_absolute_targets(&mut document, ".sub.src.y", &[&format!("{model}.sub.u")]);
    let diagnostics = import(&document).expect_err("contradictory boundary target must reject");
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagCode::DirectionMismatch
            && diagnostic.message == "boundary connection has contradictory endpoint directions"
            && diagnostic.subject.is_none()
    }));
}

/// An active elided boundary source must retain the existing inactive-target refusal.
#[test]
fn inactive_target_of_elided_boundary_source_remains_loud() {
    let mut document = sibling_document();
    let model = "http://example.org#siblings";
    node_mut(&mut document, "#siblings")
        .as_object_mut()
        .expect("root node")
        .remove("S231:hasOutput");
    set_absolute_targets(&mut document, ".u", &[&format!("{model}.subA.gain.u")]);
    set_absolute_targets(&mut document, ".subB.u", &[&format!("{model}.subB.gain.y")]);
    node_mut(&mut document, ".subB")["S231:hasParameter"] =
        json!({ "@id": format!("{model}.subB.have") });
    document["@graph"]
        .as_array_mut()
        .expect("@graph")
        .push(json!({
            "@id": format!("{model}.subB.have"),
            "@type": "S231:Parameter",
            "S231:isOfDataType": { "@id": "S231:Boolean" },
            "S231:value": false
        }));
    let inactive = node_mut(&mut document, ".subB.gain");
    inactive["S231:isConditionalComponent"] = json!(true);
    inactive["S231:conditionalExpression"] = json!("have");
    let diagnostics = import(&document).expect_err("inactive target must reject");
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.code == DiagCode::InactiveConditionalNode
                && diagnostic.message == "connection targets an inactive conditional node"
        }),
        "{diagnostics:?}"
    );
}

/// An underivable edge from an elided boundary source must remain loud without copying its subject.
#[test]
fn underivable_boundary_source_edge_rejects_without_subject_copy() {
    let mut document = sibling_document();
    let model = "http://example.org#siblings";
    node_mut(&mut document, "#siblings")
        .as_object_mut()
        .expect("root node")
        .remove("S231:hasOutput");
    set_absolute_targets(
        &mut document,
        ".u",
        &[
            &format!("{model}.subA.gain.u"),
            &format!("{model}.subB.gain.u"),
        ],
    );
    set_absolute_targets(&mut document, ".subB.u", &[&format!("{model}.subB.gain")]);
    let diagnostics = import(&document).expect_err("underivable boundary edge must reject");
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagCode::DirectionMismatch
            && diagnostic.message == "boundary connection direction cannot be derived"
            && diagnostic.subject.is_none()
    }));
}

/// A reverse spelling whose missing canonical source is not synthesized must remain loud when its
/// authored boundary source would otherwise be removed.
#[test]
fn swap_blocked_elided_source_rejects_without_subject_copy() {
    let mut document = sibling_document();
    let model = "http://example.org#siblings";
    node_mut(&mut document, "#siblings")
        .as_object_mut()
        .expect("root node")
        .remove("S231:hasOutput");
    set_absolute_targets(
        &mut document,
        ".u",
        &[
            &format!("{model}.subA.gain.u"),
            &format!("{model}.subB.gain.u"),
        ],
    );
    clear_targets(&mut document, ".subA.gain.y");
    set_absolute_targets(&mut document, ".subB.u", &[&format!("{model}.subA.y")]);
    let missing = format!("{model}.subA.y");
    document["@graph"]
        .as_array_mut()
        .expect("@graph")
        .retain(|node| node["@id"].as_str() != Some(missing.as_str()));
    let diagnostics = import(&document).expect_err("swap-blocked boundary source must reject");
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagCode::DirectionMismatch
            && diagnostic.message == "boundary connection direction cannot be derived"
            && diagnostic.subject.is_none()
    }));
}

fn derived_output_document(node_bearing: bool) -> Value {
    let model = "http://example.org#derived_output";
    let output = format!("{model}.sub.con.y");
    let mut graph = vec![
        json!({
            "@id": model,
            "@type": "S231:Block",
            "S231:containsBlock": [
                { "@id": format!("{model}.sub") },
                { "@id": format!("{model}.post") }
            ]
        }),
        json!({
            "@id": format!("{model}.sub"),
            "@type": "S231:Block",
            "S231:containsBlock": { "@id": format!("{model}.sub.con") },
            "S231:hasOutput": { "@id": format!("{model}.sub.y") }
        }),
        json!({
            "@id": format!("{model}.sub.y"),
            "@type": "S231:RealOutput",
            "S231:isOfDataType": { "@id": "S231:Real" },
            "S231:isConnectedTo": [
                { "@id": output.clone() },
                { "@id": format!("{model}.post.u") }
            ]
        }),
        json!({
            "@id": format!("{model}.sub.con"),
            "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
            "S231:hasInstance": [
                { "@id": format!("{model}.sub.con.k") },
                { "@id": output.clone() }
            ]
        }),
        json!({
            "@id": format!("{model}.sub.con.k"),
            "@type": "S231:Parameter",
            "S231:isOfDataType": { "@id": "S231:Real" },
            "S231:value": 1.0
        }),
        json!({
            "@id": format!("{model}.post"),
            "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.MultiplyByParameter",
            "S231:hasParameter": { "@id": format!("{model}.post.k") },
            "S231:hasInput": { "@id": format!("{model}.post.u") },
            "S231:hasOutput": { "@id": format!("{model}.post.y") }
        }),
        json!({
            "@id": format!("{model}.post.k"),
            "@type": "S231:Parameter",
            "S231:isOfDataType": { "@id": "S231:Real" },
            "S231:value": 1.0
        }),
        json!({
            "@id": format!("{model}.post.u"),
            "@type": "S231:RealInput",
            "S231:isOfDataType": { "@id": "S231:Real" }
        }),
        json!({
            "@id": format!("{model}.post.y"),
            "@type": "S231:RealOutput",
            "S231:isOfDataType": { "@id": "S231:Real" }
        }),
    ];
    if node_bearing {
        graph.push(json!({
            "@id": output,
            "@type": "S231:RealOutput",
            "S231:isOfDataType": { "@id": "S231:Real" }
        }));
    }
    json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" },
        "@graph": graph
    })
}

/// A node-less output listed by a derivation-shaped leaf is synthesized later in resolution. Its
/// reverse-spelled boundary edge must orient the same way as the node-bearing control.
#[test]
fn synthesized_output_can_drive_through_nested_boundary() {
    for node_bearing in [false, true] {
        let graph = import(&derived_output_document(node_bearing))
            .unwrap_or_else(|diagnostics| panic!("node_bearing={node_bearing}: {diagnostics:?}"));
        assert!(rendered_edges(&graph).iter().any(|(source, target)| {
            source.starts_with("http://example.org#derived_output.sub.con:")
                && target.starts_with("http://example.org#derived_output.post:")
        }));
    }
}
