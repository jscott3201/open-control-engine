//! Source-verified G36 CoolingOnly ActiveAirFlow composite import tests.

use std::fmt::Write as _;
use std::path::PathBuf;

use oce_cxf::{ResolveOptions, import_cxf};
use oce_model::{ModelGraph, Value};
use serde_json::{Value as JsonValue, json};

const G36_COOLING_ONLY_ACTIVE_AIR_FLOW: &str =
    include_str!("fixtures/g36/cooling_only_active_air_flow.jsonld");
const G36_COOLING_ONLY_ACTIVE_AIR_FLOW_GOLDEN_REL: &str =
    "tests/fixtures/golden/g36_cooling_only_active_air_flow.modelgraph.txt";
const G36_COOLING_ONLY_ACTIVE_AIR_FLOW_MODEL: &str =
    "http://example.org#g36.source.cooling_only_active_air_flow";
const G36_COOLING_ONLY_ACTIVE_AIR_FLOW_CLASS: &str = "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.TerminalUnits.CoolingOnly.Subsequences.ActiveAirFlow";

fn import_ok(src: &str) -> ModelGraph {
    let (graph, report) = import_cxf(src.as_bytes(), &ResolveOptions::default())
        .expect("G36 CoolingOnly ActiveAirFlow fixture should import");
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
fn source_verified_cooling_only_active_air_flow_preserves_topology_and_bindings() {
    let parsed: JsonValue =
        serde_json::from_str(G36_COOLING_ONLY_ACTIVE_AIR_FLOW).expect("G36 fixture JSON");
    let top = parsed["@graph"]
        .as_array()
        .expect("@graph array")
        .iter()
        .find(|node| node["@id"] == json!(G36_COOLING_ONLY_ACTIVE_AIR_FLOW_MODEL))
        .expect("top G36 CoolingOnly ActiveAirFlow composite node");
    assert_eq!(top["@type"], json!(G36_COOLING_ONLY_ACTIVE_AIR_FLOW_CLASS));
    assert_eq!(
        top["S231:containsBlock"]
            .as_array()
            .expect("children")
            .len(),
        11
    );
    assert_eq!(top["S231:hasInput"].as_array().expect("inputs").len(), 2);
    assert_eq!(top["S231:hasOutput"].as_array().expect("outputs").len(), 3);

    let graph = import_ok(G36_COOLING_ONLY_ACTIVE_AIR_FLOW);
    assert_eq!(graph.blocks.len(), 11);
    let instances = graph
        .blocks
        .iter()
        .map(|block| block.instance_iri.as_deref().expect("source path"))
        .collect::<Vec<_>>();
    for suffix in [
        ".actCooMax",
        ".or3",
        ".booToRea",
        ".actMin",
        ".occMod",
        ".intEqu",
        ".cooDowMod",
        ".setUpMod",
        ".intEqu2",
        ".intEqu1",
        ".or2",
    ] {
        assert!(
            instances.iter().any(|iri| iri.ends_with(suffix)),
            "missing source component {suffix}"
        );
    }

    assert!(block_param(&graph, ".actCooMax", "realTrue").bit_eq(&Value::Real(0.94)));
    assert!(block_param(&graph, ".occMod", "k").bit_eq(&Value::Integer(1)));
    assert!(block_param(&graph, ".cooDowMod", "k").bit_eq(&Value::Integer(2)));
    assert!(block_param(&graph, ".setUpMod", "k").bit_eq(&Value::Integer(3)));
}

#[test]
fn cooling_only_active_air_flow_modelgraph_is_stable() {
    let actual = render(&import_ok(G36_COOLING_ONLY_ACTIVE_AIR_FLOW));
    let path = golden_path(G36_COOLING_ONLY_ACTIVE_AIR_FLOW_GOLDEN_REL);
    if std::env::var_os("OCE_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(path)
        .expect("golden snapshot missing; regenerate with OCE_BLESS=1");
    assert_eq!(
        actual, expected,
        "source-verified G36 CoolingOnly ActiveAirFlow ModelGraph diverged from golden"
    );
}
