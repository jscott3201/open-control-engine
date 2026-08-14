//! Synthesized connector orientation, ordering, and resource bounds.

use super::*;

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

/// A node-less synthesized driver still charges its authored target IRI against the byte budget.
#[test]
fn synthesized_output_remains_resource_bounded() {
    let mut document = derived_output_document(false, false);
    let owner = format!("http://example.org#{}", "x".repeat(8_388_608));
    let parameter = format!("{owner}.k");
    let output = format!("{owner}.y");
    node_mut(&mut document, ".sub")["S231:containsBlock"] = json!({ "@id": owner });
    node_mut(&mut document, ".sub.con.k")["@id"] = json!(parameter);
    let instance = node_mut(&mut document, ".sub.con");
    instance["@id"] = json!(owner);
    instance["S231:hasInstance"] = json!([{ "@id": parameter }]);
    set_absolute_targets(
        &mut document,
        ".sub.y",
        &[&output, "http://example.org#derived_output.post.u"],
    );
    let diagnostics = import(&document).expect_err("oversized synthesized output must reject");
    assert_eq!(
        diagnostics,
        vec![Diagnostic::error(
            DiagCode::MalformedDocument,
            "composite boundary resolution exceeds the supported aggregate target IRI byte count \
             (8388608)",
        )]
    );
}

/// Distinct relations to one synthesized driver each charge the original long target identity.
#[test]
fn synthesized_output_relation_bytes_are_aggregated() {
    let mut document = derived_output_document(false, false);
    let owner = format!("http://example.org#{}", "x".repeat(4_194_304));
    let parameter = format!("{owner}.k");
    let output = format!("{owner}.y");
    node_mut(&mut document, ".sub")["S231:containsBlock"] = json!({ "@id": owner });
    node_mut(&mut document, ".sub")["S231:hasOutput"] = json!([
        { "@id": "http://example.org#derived_output.sub.y" },
        { "@id": "http://example.org#derived_output.sub.y2" }
    ]);
    node_mut(&mut document, ".sub.con.k")["@id"] = json!(parameter);
    let instance = node_mut(&mut document, ".sub.con");
    instance["@id"] = json!(owner);
    instance["S231:hasInstance"] = json!([{ "@id": parameter }]);
    set_absolute_targets(
        &mut document,
        ".sub.y",
        &[&output, "http://example.org#derived_output.post.u"],
    );
    document["@graph"]
        .as_array_mut()
        .expect("@graph")
        .push(json!({
            "@id": "http://example.org#derived_output.sub.y2",
            "@type": "S231:RealOutput",
            "S231:isOfDataType": { "@id": "S231:Real" },
            "S231:isConnectedTo": [
                { "@id": output },
                { "@id": "http://example.org#derived_output.post.u" }
            ]
        }));
    let diagnostics = import(&document).expect_err("aggregate synthesized bytes must reject");
    let summary = diagnostics
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.code,
                diagnostic.message.as_str(),
                diagnostic.subject.as_ref().map(|subject| subject.len()),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        summary,
        vec![(
            DiagCode::MalformedDocument,
            "composite boundary resolution exceeds the supported aggregate target IRI byte count \
             (8388608)",
            None,
        )]
    );
}

/// Exact-limit fanout from a derivation-skipped source emits one copied missing-source subject.
#[test]
fn skipped_synthesized_source_fanout_reports_source_once() {
    let mut document = derived_output_document(false, false);
    let owner = format!("http://example.org#{}.con", "x".repeat(65_536));
    let parameter = format!("{owner}.k");
    let output = format!("{owner}.y");
    node_mut(&mut document, ".sub")["S231:containsBlock"] = json!({ "@id": owner });
    node_mut(&mut document, ".sub.con.k")["@id"] = json!(parameter);
    let instance = node_mut(&mut document, ".sub.con");
    instance["@id"] = json!(owner);
    instance["S231:hasInstance"] = json!([
        { "@id": parameter },
        { "@id": output },
        { "@id": output }
    ]);
    let mut targets = vec![json!({ "@id": output })];
    targets.extend(std::iter::repeat_n(
        json!({ "@id": "http://example.org#derived_output.post.u" }),
        65_535,
    ));
    node_mut(&mut document, ".sub.y")["S231:isConnectedTo"] = Value::Array(targets);

    let diagnostics = import(&document).expect_err("skipped synthesized source must reject");
    let missing_sources = diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.code == DiagCode::UnresolvedReference
                && diagnostic.message == "connection source not found"
        })
        .collect::<Vec<_>>();
    assert_eq!(missing_sources.len(), 1, "{diagnostics:?}");
    assert_eq!(missing_sources[0].subject.as_deref(), Some(output.as_str()));
    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagCode::MalformedDocument)
    );
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
