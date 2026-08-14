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

fn derived_output_document(node_bearing: bool, listed_output: bool) -> Value {
    let model = "http://example.org#derived_output";
    let output = format!("{model}.sub.con.y");
    let mut members = vec![json!({ "@id": format!("{model}.sub.con.k") })];
    if listed_output {
        members.push(json!({ "@id": output.clone() }));
    }
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
            "S231:hasInstance": members
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

fn replace_iri_prefix(value: &mut Value, replacement: &str) {
    const ORIGINAL: &str = "http://example.org#derived_output";
    match value {
        Value::Array(values) => {
            for value in values {
                replace_iri_prefix(value, replacement);
            }
        }
        Value::Object(fields) => {
            for value in fields.values_mut() {
                replace_iri_prefix(value, replacement);
            }
        }
        Value::String(text) if text.starts_with(ORIGINAL) => {
            *text = format!("{replacement}{}", &text[ORIGINAL.len()..]);
        }
        _ => {}
    }
}

fn sidecar_order_document() -> Value {
    let model = "http://example.org#derived_sidecar_order";
    let mut graph = vec![json!({
        "@id": model,
        "@type": "S231:Block",
        "S231:containsBlock": [
            { "@id": format!("{model}.listed.sub") },
            { "@id": format!("{model}.listed.post") },
            { "@id": format!("{model}.padded.sub") },
            { "@id": format!("{model}.padded.post") },
            { "@id": format!("{model}.direct") },
            { "@id": format!("{model}.directPost") }
        ]
    })];
    for (name, listed) in [("listed", true), ("padded", false)] {
        let mut document = derived_output_document(false, listed);
        replace_iri_prefix(&mut document, &format!("{model}.{name}"));
        let mut nodes = document["@graph"].as_array().expect("@graph").clone();
        nodes.remove(0);
        graph.extend(nodes);
    }
    graph.extend([
        json!({
            "@id": format!("{model}.direct"),
            "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
            "S231:hasParameter": { "@id": format!("{model}.direct.k") },
            "S231:hasOutput": { "@id": format!("{model}.direct.y") }
        }),
        json!({
            "@id": format!("{model}.direct.k"),
            "@type": "S231:Parameter",
            "S231:value": 2.0
        }),
        json!({
            "@id": format!("{model}.direct.y"),
            "@type": "S231:RealOutput",
            "S231:isOfDataType": { "@id": "S231:Real" },
            "S231:isConnectedTo": { "@id": format!("{model}.directPost.u") }
        }),
        json!({
            "@id": format!("{model}.directPost"),
            "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.MultiplyByParameter",
            "S231:hasParameter": { "@id": format!("{model}.directPost.k") },
            "S231:hasInput": { "@id": format!("{model}.directPost.u") },
            "S231:hasOutput": { "@id": format!("{model}.directPost.y") }
        }),
        json!({
            "@id": format!("{model}.directPost.k"),
            "@type": "S231:Parameter",
            "S231:value": 1.0
        }),
        json!({
            "@id": format!("{model}.directPost.u"),
            "@type": "S231:RealInput",
            "S231:isOfDataType": { "@id": "S231:Real" }
        }),
        json!({
            "@id": format!("{model}.directPost.y"),
            "@type": "S231:RealOutput",
            "S231:isOfDataType": { "@id": "S231:Real" }
        }),
    ]);
    json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" },
        "@graph": graph
    })
}

/// Listed node-less and omitted padded outputs are synthesized later in resolution. Their reverse-
/// spelled boundary edges must orient the same way as the node-bearing control.
#[test]
fn synthesized_output_can_drive_through_nested_boundary() {
    for (node_bearing, listed_output) in [(false, true), (true, true), (false, false)] {
        let graph = import(&derived_output_document(node_bearing, listed_output)).unwrap_or_else(
            |diagnostics| {
                panic!(
                    "node_bearing={node_bearing}, listed_output={listed_output}: {diagnostics:?}"
                )
            },
        );
        assert!(rendered_edges(&graph).iter().any(|(source, target)| {
            source.starts_with("http://example.org#derived_output.sub.con:")
                && target.starts_with("http://example.org#derived_output.post:")
        }));
    }
}

/// Authored connections precede listed and padded synthesized-driver sidecars in block order.
#[test]
fn synthesized_sidecar_order_is_bit_exact_and_repeatable() {
    let document = sidecar_order_document();
    let first = super::render::render(&import(&document).expect("sidecar fixture imports"));
    let second = super::render::render(&import(&document).expect("repeated sidecar import"));
    assert_eq!(
        first, second,
        "repeated import changed the ModelGraph bytes"
    );

    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/golden/derived_boundary_sidecar.modelgraph.txt");
    if super::bless::enabled() {
        std::fs::write(path, &first).expect("write sidecar golden");
        return;
    }
    let expected = std::fs::read_to_string(path).expect("sidecar golden missing");
    assert_eq!(first, expected, "synthesized sidecar golden diverged");
}
