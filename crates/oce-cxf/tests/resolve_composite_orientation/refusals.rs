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
        &[
            &format!("{model}.subA.gain.u"),
            &format!("{model}.subB.u"),
            &format!("{model}.subB.gain.u"),
        ],
    );
    let diagnostics = import(&document).expect_err("contradictory reached edge must reject");
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagCode::DirectionMismatch
            && diagnostic.message == "boundary connection has contradictory endpoint directions"
            && diagnostic.subject.is_none()
    }));
}

/// A reached contradictory chain relies on its one emitted output-to-output direction refusal.
#[test]
fn reached_contradictory_output_chain_reports_once() {
    let mut document = sibling_document();
    let model = "http://example.org#siblings";
    let root = node_mut(&mut document, "#siblings");
    root.as_object_mut()
        .expect("root node")
        .remove("S231:hasOutput");
    root["S231:containsBlock"]
        .as_array_mut()
        .expect("containsBlock")
        .push(json!({ "@id": format!("{model}.other") }));
    set_absolute_targets(
        &mut document,
        ".u",
        &[
            &format!("{model}.subA.gain.u"),
            &format!("{model}.subB.gain.u"),
        ],
    );
    set_absolute_targets(&mut document, ".subA.y", &[&format!("{model}.subB.y")]);
    set_absolute_targets(&mut document, ".subB.y", &[&format!("{model}.other.y")]);
    clear_targets(&mut document, ".subB.gain.y");
    document["@graph"].as_array_mut().expect("@graph").extend([
        json!({
            "@id": format!("{model}.other"),
            "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
            "S231:hasParameter": { "@id": format!("{model}.other.k") },
            "S231:hasOutput": { "@id": format!("{model}.other.y") }
        }),
        json!({
            "@id": format!("{model}.other.k"),
            "@type": "S231:Parameter",
            "S231:value": 1
        }),
        json!({
            "@id": format!("{model}.other.y"),
            "@type": "S231:RealOutput",
            "S231:isOfDataType": { "@id": "S231:Real" }
        }),
    ]);
    let diagnostics = import(&document).expect_err("output chain must reject");
    assert_eq!(
        diagnostics,
        vec![
            Diagnostic::error(
                DiagCode::DirectionMismatch,
                "connection is not output→input",
            )
            .with_subject(format!("{model}.subA.gain.y"))
        ]
    );
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

/// A reached boundary source leaves its inactive target for the ordinary subject-bearing refusal.
#[test]
fn reached_boundary_source_reports_inactive_target_once() {
    let mut document = sibling_document();
    let model = "http://example.org#siblings";
    node_mut(&mut document, "#siblings")
        .as_object_mut()
        .expect("root node")
        .remove("S231:hasOutput");
    set_absolute_targets(
        &mut document,
        ".u",
        &[&format!("{model}.subA.gain.u"), &format!("{model}.subB.u")],
    );
    set_absolute_targets(&mut document, ".subB.u", &[&format!("{model}.subB.gain.y")]);
    clear_targets(&mut document, ".subB.gain.y");
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
    let target = format!("{model}.subB.gain.y");
    let diagnostics = import(&document).expect_err("inactive target must reject");
    assert_eq!(
        diagnostics,
        vec![
            Diagnostic::error(
                DiagCode::InactiveConditionalNode,
                "connection targets an inactive conditional node",
            )
            .with_subject(target)
        ]
    );
}

fn inactive_boundary_target_document(target: &str, repeats: usize) -> Value {
    let model = "http://example.org#inactive_boundary_target";
    let targets = std::iter::repeat_n(json!({ "@id": target }), repeats).collect::<Vec<_>>();
    json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" },
        "@graph": [
            {
                "@id": model,
                "@type": "S231:Block",
                "S231:containsBlock": [
                    { "@id": format!("{model}.src") },
                    { "@id": format!("{model}.sub") }
                ]
            },
            {
                "@id": format!("{model}.src"),
                "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
                "S231:hasParameter": { "@id": format!("{model}.src.k") },
                "S231:hasOutput": { "@id": format!("{model}.src.y") }
            },
            {
                "@id": format!("{model}.src.k"),
                "@type": "S231:Parameter",
                "S231:value": 1
            },
            {
                "@id": format!("{model}.src.y"),
                "@type": "S231:RealOutput",
                "S231:isOfDataType": { "@id": "S231:Real" },
                "S231:isConnectedTo": targets
            },
            {
                "@id": format!("{model}.sub"),
                "@type": "S231:Block",
                "S231:containsBlock": { "@id": format!("{model}.sub.keep") },
                "S231:hasInput": { "@id": target },
                "S231:hasParameter": { "@id": format!("{model}.sub.have") }
            },
            {
                "@id": format!("{model}.sub.have"),
                "@type": "S231:Parameter",
                "S231:isOfDataType": { "@id": "S231:Boolean" },
                "S231:value": false
            },
            {
                "@id": target,
                "@type": "S231:RealInput",
                "S231:isOfDataType": { "@id": "S231:Real" },
                "S231:isConditionalComponent": true,
                "S231:conditionalExpression": "have"
            },
            {
                "@id": format!("{model}.sub.keep"),
                "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
                "S231:hasParameter": { "@id": format!("{model}.sub.keep.k") },
                "S231:hasOutput": { "@id": format!("{model}.sub.keep.y") }
            },
            {
                "@id": format!("{model}.sub.keep.k"),
                "@type": "S231:Parameter",
                "S231:value": 1
            },
            {
                "@id": format!("{model}.sub.keep.y"),
                "@type": "S231:RealOutput",
                "S231:isOfDataType": { "@id": "S231:Real" }
            }
        ]
    })
}

/// A surviving source targeting an inactive boundary gets one ordinary subject-bearing refusal.
#[test]
fn inactive_boundary_target_is_reported_once() {
    let target = "http://example.org#inactive_boundary_target.sub.u";
    let diagnostics = import(&inactive_boundary_target_document(target, 1))
        .expect_err("inactive boundary target must reject");
    assert_eq!(
        diagnostics,
        vec![
            Diagnostic::error(
                DiagCode::InactiveConditionalNode,
                "connection targets an inactive conditional node",
            )
            .with_subject(target.to_owned())
        ]
    );
}

/// Inactive boundary targets still consume count and byte budgets before activity classification.
#[test]
fn inactive_boundary_targets_remain_resource_bounded() {
    let target = "http://example.org#inactive_boundary_target.sub.u";
    let count = import(&inactive_boundary_target_document(target, 65_537))
        .expect_err("inactive target count must reject");
    assert_eq!(
        count,
        vec![Diagnostic::error(
            DiagCode::MalformedDocument,
            "composite boundary resolution exceeds the supported target examination count (65536)",
        )]
    );

    let oversized = format!(
        "http://example.org#inactive_boundary_target.sub.u{}",
        "x".repeat(8_388_608)
    );
    let bytes = import(&inactive_boundary_target_document(&oversized, 1))
        .expect_err("inactive target bytes must reject");
    assert_eq!(
        bytes,
        vec![Diagnostic::error(
            DiagCode::MalformedDocument,
            "composite boundary resolution exceeds the supported aggregate target IRI byte count \
             (8388608)",
        )]
    );
}

/// Exact-limit expansion reports one copied subject for a repeated ordinary missing target.
#[test]
fn expanded_missing_target_reports_once() {
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
    let missing = format!("{model}.missing");
    clear_targets(&mut document, ".subB.gain.y");
    node_mut(&mut document, ".subA.y")["S231:isConnectedTo"] =
        Value::Array(std::iter::repeat_n(json!({ "@id": missing }), 65_535).collect());
    let diagnostics = import(&document).expect_err("missing target must reject");
    assert_eq!(
        diagnostics,
        vec![
            Diagnostic::error(DiagCode::UnresolvedReference, "connection target not found",)
                .with_subject(missing)
        ]
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
    assert_eq!(
        diagnostics,
        vec![Diagnostic::error(
            DiagCode::DirectionMismatch,
            "boundary connection direction cannot be derived",
        )]
    );
}

/// A missing target behind an unreachable boundary source must remain an unresolved reference.
#[test]
fn missing_target_of_unreachable_boundary_source_remains_loud() {
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
    let missing = format!("{model}.subB.u");
    document["@graph"]
        .as_array_mut()
        .expect("@graph")
        .retain(|node| node["@id"].as_str() != Some(missing.as_str()));
    let diagnostics = import(&document).expect_err("missing boundary target must reject");
    assert_eq!(
        diagnostics,
        vec![Diagnostic::error(
            DiagCode::UnresolvedReference,
            "connection target not found",
        )]
    );
}

/// A surviving edge owns the one diagnostic for a missing endpoint also named by an erased edge.
#[test]
fn preserved_missing_target_suppresses_deferred_duplicate() {
    let mut document = sibling_document();
    let model = "http://example.org#siblings";
    node_mut(&mut document, "#siblings")
        .as_object_mut()
        .expect("root node")
        .remove("S231:hasOutput");
    let missing = format!("{model}.subB.missing");
    node_mut(&mut document, ".subB")["S231:hasInput"] = json!([
        { "@id": format!("{model}.subB.u") },
        { "@id": missing }
    ]);
    set_absolute_targets(
        &mut document,
        ".u",
        &[
            &format!("{model}.subA.gain.u"),
            &format!("{model}.subB.gain.u"),
        ],
    );
    set_absolute_targets(&mut document, ".subA.gain.y", &[&missing]);
    set_absolute_targets(&mut document, ".subA.y", &[&missing]);
    let diagnostics = import(&document).expect_err("missing target must reject once");
    assert_eq!(
        diagnostics,
        vec![
            Diagnostic::error(DiagCode::UnresolvedReference, "connection target not found",)
                .with_subject(missing)
        ]
    );
}

/// Conflicted ownership cannot hide an active edge authored from an elided boundary identity.
#[test]
fn conflicted_ownership_on_elided_source_remains_loud() {
    let mut document = sibling_document();
    let model = "http://example.org#siblings";
    node_mut(&mut document, "#siblings")
        .as_object_mut()
        .expect("root node")
        .remove("S231:hasOutput");
    node_mut(&mut document, ".subB")["S231:hasInput"] = json!([
        { "@id": format!("{model}.subB.u") },
        { "@id": format!("{model}.subA.y") }
    ]);
    set_absolute_targets(
        &mut document,
        ".u",
        &[
            &format!("{model}.subA.gain.u"),
            &format!("{model}.subB.gain.u"),
        ],
    );
    clear_targets(&mut document, ".subA.gain.y");
    clear_targets(&mut document, ".subB.gain.y");
    set_absolute_targets(&mut document, ".subA.y", &[&format!("{model}.subB.gain.y")]);
    let diagnostics = import(&document).expect_err("conflicted boundary source must reject");
    assert_eq!(
        diagnostics,
        vec![Diagnostic::error(
            DiagCode::DirectionMismatch,
            "boundary connection direction cannot be derived",
        )]
    );
}

/// A reached boundary cannot hide a conflicted boundary target with no continuation.
#[test]
fn reached_conflicted_boundary_target_remains_loud() {
    let mut document = sibling_document();
    let model = "http://example.org#siblings";
    node_mut(&mut document, "#siblings")
        .as_object_mut()
        .expect("root node")
        .remove("S231:hasOutput");
    node_mut(&mut document, ".subB")["S231:hasInput"] = json!([
        { "@id": format!("{model}.subB.u") },
        { "@id": format!("{model}.subA.y") }
    ]);
    set_absolute_targets(
        &mut document,
        ".u",
        &[&format!("{model}.subA.gain.u"), &format!("{model}.subB.u")],
    );
    clear_targets(&mut document, ".subA.gain.y");
    set_absolute_targets(&mut document, ".subB.u", &[&format!("{model}.subA.y")]);
    let diagnostics = import(&document).expect_err("conflicted empty boundary must reject");
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagCode::DirectionMismatch
            && diagnostic.message == "boundary connection direction cannot be derived"
            && diagnostic.subject.is_none()
    }));
}

/// A padded output is synthesized only when its owner reaches the derivation domain.
#[test]
fn protected_derivation_owner_cannot_hide_boundary_relation() {
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
            &format!("{model}.subB.u"),
            &format!("{model}.subB.gain.u"),
        ],
    );
    clear_targets(&mut document, ".subB.u");
    let ghost = format!("{model}.subA.gain.protected");
    node_mut(&mut document, ".subA.gain")["S231:containsBlock"] = json!({ "@id": ghost });
    let ghost_output = format!("{ghost}.y");
    set_absolute_targets(&mut document, ".subB.u", &[&ghost_output]);
    document["@graph"].as_array_mut().expect("@graph").extend([
        json!({
            "@id": ghost,
            "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
            "S231:hasInstance": { "@id": format!("{ghost}.k") }
        }),
        json!({
            "@id": format!("{ghost}.k"),
            "@type": "S231:Parameter",
            "S231:value": 1
        }),
    ]);
    let diagnostics = import(&document).expect_err("protected output relation must reject");
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagCode::UnresolvedReference
            && diagnostic.message == "boundary-input target not found"
            && diagnostic.subject.as_deref() == Some(ghost_output.as_str())
    }));
}

fn reached_malformed_boundary_document(target: &str) -> Value {
    let mut document = sibling_document();
    let model = "http://example.org#siblings";
    node_mut(&mut document, "#siblings")
        .as_object_mut()
        .expect("root node")
        .remove("S231:hasOutput");
    set_absolute_targets(
        &mut document,
        ".u",
        &[&format!("{model}.subA.gain.u"), &format!("{model}.subB.u")],
    );
    set_absolute_targets(
        &mut document,
        ".subB.u",
        &[&format!("{model}.subB.gain.u"), target],
    );
    document
}

/// A reached underivable target survives lowering for the ordinary subject-bearing refusal.
#[test]
fn reached_unknown_boundary_relation_reports_once() {
    let target = "http://example.org#siblings.subB.gain";
    let diagnostics = import(&reached_malformed_boundary_document(target))
        .expect_err("non-connector target must reject");
    assert_eq!(
        diagnostics,
        vec![
            Diagnostic::error(
                DiagCode::UnresolvedReference,
                "boundary-input target not found",
            )
            .with_subject(target.to_owned())
        ]
    );
}

/// A reached swap-blocked target survives lowering for the ordinary unresolved-reference refusal.
#[test]
fn reached_swap_blocked_boundary_relation_reports_once() {
    let target = "http://example.org#siblings.subA.y";
    let mut document = reached_malformed_boundary_document(target);
    clear_targets(&mut document, ".subA.gain.y");
    document["@graph"]
        .as_array_mut()
        .expect("@graph")
        .retain(|node| node["@id"].as_str() != Some(target));
    let diagnostics = import(&document).expect_err("missing canonical source must reject");
    assert_eq!(
        diagnostics,
        vec![
            Diagnostic::error(
                DiagCode::UnresolvedReference,
                "boundary-input target not found",
            )
            .with_subject(target.to_owned())
        ]
    );
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
