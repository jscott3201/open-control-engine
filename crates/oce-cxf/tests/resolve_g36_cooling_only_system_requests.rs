//! Source-verified G36 CoolingOnly SystemRequests composite import tests.
//!
//! Upstream `SystemRequests.mo` line 125 binds `greThr4.h=0.5*floHys`. The fixture deliberately
//! pre-grounds that expression to `0.005` from the canonical Validation binding `floHys=0.01`;
//! this test pins the resulting constant alongside every referenced timing and hysteresis value.

mod bless;

use std::fmt::Write as _;
use std::path::PathBuf;

use oce_cxf::{ResolveOptions, import_cxf};
use oce_model::{ModelGraph, Value};
use serde_json::{Value as JsonValue, json};

const G36_COOLING_ONLY_SYSTEM_REQUESTS: &str =
    include_str!("fixtures/g36/cooling_only_system_requests.jsonld");
const G36_COOLING_ONLY_SYSTEM_REQUESTS_GOLDEN_REL: &str =
    "tests/fixtures/golden/g36_cooling_only_system_requests.modelgraph.txt";
const G36_COOLING_ONLY_SYSTEM_REQUESTS_MODEL: &str =
    "http://example.org#g36.source.cooling_only_system_requests";
const G36_COOLING_ONLY_SYSTEM_REQUESTS_CLASS: &str = "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.TerminalUnits.CoolingOnly.Subsequences.SystemRequests";

fn import_ok(src: &str) -> ModelGraph {
    let (graph, report) = import_cxf(src.as_bytes(), &ResolveOptions::default())
        .expect("G36 CoolingOnly SystemRequests fixture should import");
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
fn source_verified_cooling_only_system_requests_preserves_topology_and_bindings() {
    let parsed: JsonValue =
        serde_json::from_str(G36_COOLING_ONLY_SYSTEM_REQUESTS).expect("G36 fixture JSON");
    let top = parsed["@graph"]
        .as_array()
        .expect("@graph array")
        .iter()
        .find(|node| node["@id"] == json!(G36_COOLING_ONLY_SYSTEM_REQUESTS_MODEL))
        .expect("top G36 CoolingOnly SystemRequests composite node");
    assert_eq!(top["@type"], json!(G36_COOLING_ONLY_SYSTEM_REQUESTS_CLASS));
    assert_eq!(
        top["S231:hasParameter"]
            .as_array()
            .expect("parameters")
            .len(),
        9
    );
    assert_eq!(
        top["S231:containsBlock"]
            .as_array()
            .expect("children")
            .len(),
        33
    );
    assert_eq!(top["S231:hasInput"].as_array().expect("inputs").len(), 7);
    assert_eq!(top["S231:hasOutput"].as_array().expect("outputs").len(), 2);

    let graph = import_ok(G36_COOLING_ONLY_SYSTEM_REQUESTS);
    assert_eq!(graph.blocks.len(), 33);
    let instances = graph
        .blocks
        .iter()
        .map(|block| block.instance_iri.as_deref().expect("source path"))
        .collect::<Vec<_>>();
    for suffix in [
        ".greThr1",
        ".greThr2",
        ".greThr3",
        ".greThr",
        ".greThr4",
        ".booToInt",
        ".booToInt1",
        ".gai1",
        ".gai2",
        ".sub2",
        ".sub3",
        ".and1",
        ".and2",
        ".and3",
        ".and4",
        ".thrCooResReq",
        ".twoCooResReq",
        ".thrPreResReq",
        ".twoPreResReq",
        ".intSwi",
        ".intSwi1",
        ".swi4",
        ".swi5",
        ".tim1",
        ".tim2",
        ".tim3",
        ".greEqu",
        ".greEqu1",
        ".and5",
        ".sampler",
        ".sampler1",
        ".sampler2",
        ".sampler3",
    ] {
        assert!(
            instances.iter().any(|iri| iri.ends_with(suffix)),
            "missing source component {suffix}"
        );
    }

    for (suffix, name, expected) in [
        (".greThr1", "t", 3.0),
        (".greThr1", "h", 0.25),
        (".greThr4", "t", 0.01),
        (".greThr4", "h", 0.005),
        (".greThr3", "h", 0.01),
        (".tim1", "delayTime", 120.0),
        (".tim3", "delayTime", 60.0),
        (".sampler", "samplePeriod", 120.0),
        (".sampler1", "samplePeriod", 120.0),
        (".sampler2", "samplePeriod", 120.0),
        (".sampler3", "samplePeriod", 120.0),
        (".gai1", "k", 0.5),
        (".gai2", "k", 0.7),
    ] {
        assert!(block_param(&graph, suffix, name).bit_eq(&Value::Real(expected)));
    }
    for (suffix, expected) in [
        (".thrCooResReq", 3),
        (".twoCooResReq", 2),
        (".thrPreResReq", 3),
        (".twoPreResReq", 2),
    ] {
        assert!(block_param(&graph, suffix, "k").bit_eq(&Value::Integer(expected)));
    }
}

#[test]
fn cooling_only_system_requests_modelgraph_is_stable() {
    let actual = render(&import_ok(G36_COOLING_ONLY_SYSTEM_REQUESTS));
    let path = golden_path(G36_COOLING_ONLY_SYSTEM_REQUESTS_GOLDEN_REL);
    if bless::enabled() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(path)
        .expect("golden snapshot missing; regenerate with OCE_BLESS=1");
    assert_eq!(
        actual, expected,
        "source-verified G36 CoolingOnly SystemRequests ModelGraph diverged from golden"
    );
}
