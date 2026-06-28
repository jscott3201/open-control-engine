//! Source-verified G36 Economizers.Subsequences.Limits.Common composite import tests.

use std::fmt::Write as _;
use std::path::PathBuf;

use oce_cxf::{ResolveOptions, import_cxf};
use oce_model::{ModelGraph, Value};
use serde_json::{Value as JsonValue, json};

const G36_ECONOMIZER_LIMITS_COMMON: &str =
    include_str!("fixtures/g36/multizone_vav_economizer_limits_common.jsonld");
const G36_ECONOMIZER_LIMITS_COMMON_GOLDEN_REL: &str =
    "tests/fixtures/golden/g36_multizone_vav_economizer_limits_common.modelgraph.txt";
const G36_ECONOMIZER_LIMITS_COMMON_MODEL: &str =
    "http://example.org#g36.source.multizone_vav_economizer_limits_common";
const G36_ECONOMIZER_LIMITS_COMMON_CLASS: &str = "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.Economizers.Subsequences.Limits.Common";

fn import_ok(src: &str) -> ModelGraph {
    let (graph, report) = import_cxf(src.as_bytes(), &ResolveOptions::default())
        .expect("G36 Economizers.Subsequences.Limits.Common fixture should import");
    assert!(
        report.is_empty(),
        "fixture should not warn: {:?}",
        report.diagnostics
    );
    graph
}

fn render(graph: &ModelGraph) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "blocks: {}", graph.blocks.len());
    for block in &graph.blocks {
        let _ = writeln!(
            out,
            "  B{} decl={} class={} instance_iri={:?}",
            block.id.0,
            block.decl_order,
            block.class_iri,
            block.instance_iri.as_deref()
        );
        let _ = writeln!(
            out,
            "    inputs={:?} outputs={:?}",
            block.inputs.iter().map(|id| id.0).collect::<Vec<_>>(),
            block.outputs.iter().map(|id| id.0).collect::<Vec<_>>()
        );
        for (name, value) in &block.params.values {
            let _ = writeln!(out, "    param {name}={}", render_value(value));
        }
    }
    let _ = writeln!(out, "connectors: {}", graph.connectors.len());
    for connector in &graph.connectors {
        let _ = writeln!(
            out,
            "  C{} block=B{} dir={:?} type={:?} decl={} iri={:?}",
            connector.id.0,
            connector.block.0,
            connector.dir,
            connector.value_type,
            connector.decl_order,
            connector.iri.as_deref()
        );
    }
    let _ = writeln!(out, "connections: {}", graph.connections.len());
    for connection in &graph.connections {
        let _ = writeln!(out, "  C{} -> C{}", connection.from.0, connection.to.0);
    }
    let _ = writeln!(
        out,
        "external_inputs: {:?}",
        graph
            .external_inputs
            .iter()
            .map(|id| id.0)
            .collect::<Vec<_>>()
    );
    out
}

fn render_value(value: &Value) -> String {
    match value {
        Value::Real(x) => format!("Real(0x{:016x})", x.to_bits()),
        Value::Integer(x) => format!("Integer({x})"),
        Value::Boolean(x) => format!("Boolean({x})"),
        Value::String(x) => format!("String({x:?})"),
        Value::Enum { class, ordinal } => format!("Enum(class={},ordinal={ordinal})", class.0),
    }
}

fn golden_path(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel)
}

fn block_param<'a>(graph: &'a ModelGraph, instance_suffix: &str, name: &str) -> &'a Value {
    graph
        .blocks
        .iter()
        .find(|block| {
            block
                .instance_iri
                .as_deref()
                .is_some_and(|iri| iri.ends_with(instance_suffix))
        })
        .unwrap_or_else(|| panic!("missing block ending in {instance_suffix:?}"))
        .params
        .values
        .iter()
        .find_map(|(param, value)| (param.as_ref() == name).then_some(value))
        .unwrap_or_else(|| panic!("missing param {name:?} on block {instance_suffix:?}"))
}

#[test]
fn source_verified_g36_multizone_vav_economizer_limits_common_imports_default_variant() {
    let parsed: JsonValue =
        serde_json::from_str(G36_ECONOMIZER_LIMITS_COMMON).expect("G36 fixture JSON");
    let top = parsed["@graph"]
        .as_array()
        .expect("@graph array")
        .iter()
        .find(|node| node["@id"] == json!(G36_ECONOMIZER_LIMITS_COMMON_MODEL))
        .expect("top G36 Economizers.Subsequences.Limits.Common composite node");
    assert_eq!(top["@type"], json!(G36_ECONOMIZER_LIMITS_COMMON_CLASS));
    assert_eq!(
        top["S231:containsBlock"]
            .as_array()
            .expect("children")
            .len(),
        16,
        "Limits.Common source-default variant should keep sixteen active child blocks"
    );
    assert_eq!(top["S231:hasInput"].as_array().expect("inputs").len(), 4);
    assert_eq!(top["S231:hasOutput"].as_array().expect("outputs").len(), 6);

    let graph = import_ok(G36_ECONOMIZER_LIMITS_COMMON);
    assert_eq!(
        graph.blocks.len(),
        16,
        "economizer common damper limits sequence should import all active source children"
    );
    let instances = graph
        .blocks
        .iter()
        .map(|block| block.instance_iri.as_deref().expect("source path"))
        .collect::<Vec<_>>();
    for suffix in [
        ".damLimCon",
        ".outDamPhyPosMinSig",
        ".outDamPhyPosMaxSig",
        ".retDamPhyPosMinSig",
        ".retDamPhyPosMaxSig",
        ".minSigLim",
        ".maxSigLim",
        ".sigFraForOutDam",
        ".minOutDam",
        ".minRetDam",
        ".retDamPosMinSwitch",
        ".outDamPosMaxSwitch",
        ".not1",
        ".conInt1",
        ".intEqu",
        ".and3",
    ] {
        assert!(
            instances.iter().any(|iri| iri.ends_with(suffix)),
            "missing source component {suffix}"
        );
    }

    assert!(block_param(&graph, ".damLimCon", "k").bit_eq(&Value::Real(0.05)));
    assert!(block_param(&graph, ".damLimCon", "Ti").bit_eq(&Value::Real(120.0)));
    assert!(block_param(&graph, ".damLimCon", "Td").bit_eq(&Value::Real(0.1)));
    assert!(block_param(&graph, ".damLimCon", "yMax").bit_eq(&Value::Real(1.0)));
    assert!(block_param(&graph, ".damLimCon", "yMin").bit_eq(&Value::Real(0.0)));
    assert!(block_param(&graph, ".damLimCon", "y_reset").bit_eq(&Value::Real(0.0)));
    assert!(block_param(&graph, ".minOutDam", "limitBelow").bit_eq(&Value::Boolean(true)));
    assert!(block_param(&graph, ".minOutDam", "limitAbove").bit_eq(&Value::Boolean(true)));
    assert!(block_param(&graph, ".minRetDam", "limitBelow").bit_eq(&Value::Boolean(true)));
    assert!(block_param(&graph, ".minRetDam", "limitAbove").bit_eq(&Value::Boolean(true)));
    assert!(block_param(&graph, ".sigFraForOutDam", "k").bit_eq(&Value::Real(0.5)));
    assert!(block_param(&graph, ".retDamPhyPosMaxSig", "k").bit_eq(&Value::Real(1.0)));
    assert!(block_param(&graph, ".outDamPhyPosMinSig", "k").bit_eq(&Value::Real(0.0)));
}

#[test]
fn golden_g36_multizone_vav_economizer_limits_common_modelgraph() {
    let actual = render(&import_ok(G36_ECONOMIZER_LIMITS_COMMON));
    let path = golden_path(G36_ECONOMIZER_LIMITS_COMMON_GOLDEN_REL);
    if std::env::var_os("OCE_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(path)
        .expect("golden snapshot missing; regenerate with OCE_BLESS=1");
    assert_eq!(
        actual, expected,
        "source-verified G36 Economizers.Subsequences.Limits.Common ModelGraph diverged from golden"
    );
}
