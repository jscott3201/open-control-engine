//! Source-verified G36 ReliefFan composite import tests.

use std::fmt::Write as _;
use std::path::PathBuf;

use oce_cxf::{ResolveOptions, import_cxf};
use oce_model::{EnumClassId, ModelGraph, Value};
use serde_json::{Value as JsonValue, json};

const G36_RELIEF_FAN: &str = include_str!("fixtures/g36/multizone_vav_relief_fan.jsonld");
const G36_RELIEF_FAN_GOLDEN_REL: &str =
    "tests/fixtures/golden/g36_multizone_vav_relief_fan.modelgraph.txt";
const G36_RELIEF_FAN_MODEL: &str = "http://example.org#g36.source.multizone_vav_relief_fan";
const G36_RELIEF_FAN_CLASS: &str =
    "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.ReliefFan";

fn import_ok(src: &str) -> ModelGraph {
    let (graph, report) = import_cxf(src.as_bytes(), &ResolveOptions::default())
        .expect("G36 ReliefFan fixture should import");
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
fn source_verified_g36_multizone_vav_relief_fan_imports_default_variant() {
    let parsed: JsonValue = serde_json::from_str(G36_RELIEF_FAN).expect("G36 fixture JSON");
    let top = parsed["@graph"]
        .as_array()
        .expect("@graph array")
        .iter()
        .find(|node| node["@id"] == json!(G36_RELIEF_FAN_MODEL))
        .expect("top G36 ReliefFan composite node");
    assert_eq!(top["@type"], json!(G36_RELIEF_FAN_CLASS));
    assert_eq!(
        top["S231:containsBlock"]
            .as_array()
            .expect("children")
            .len(),
        19,
        "ReliefFan source transcription should preserve the nineteen declared child components"
    );
    assert_eq!(top["S231:hasInput"].as_array().expect("inputs").len(), 2);
    assert_eq!(top["S231:hasOutput"].as_array().expect("outputs").len(), 4);

    let graph = import_ok(G36_RELIEF_FAN);
    assert_eq!(
        graph.blocks.len(),
        19,
        "scalar ReliefFan sequence should import all source children"
    );
    let instances = graph
        .blocks
        .iter()
        .map(|block| block.instance_iri.as_deref().expect("source path"))
        .collect::<Vec<_>>();
    for suffix in [
        ".movMea",
        ".dpBuiSetPoi",
        ".div1",
        ".conOne",
        ".conP",
        ".greThr",
        ".lesThr",
        ".tim",
        ".and2",
        ".lat",
        ".greThr2",
        ".upTim",
        ".lesThr3",
        ".dowTim",
        ".lat1",
        ".relDam",
        ".relFan",
        ".booToRea2",
        ".pro1",
    ] {
        assert!(
            instances.iter().any(|iri| iri.ends_with(suffix)),
            "missing source component {suffix}"
        );
    }

    assert!(block_param(&graph, ".movMea", "delta").bit_eq(&Value::Real(300.0)));
    assert!(block_param(&graph, ".dpBuiSetPoi", "k").bit_eq(&Value::Real(12.0)));
    assert!(block_param(&graph, ".conOne", "k").bit_eq(&Value::Real(1.0)));
    assert!(block_param(&graph, ".conP", "k").bit_eq(&Value::Real(1.0)));
    assert!(block_param(&graph, ".conP", "reverseActing").bit_eq(&Value::Boolean(false)));
    assert!(
        block_param(&graph, ".conP", "controllerType").bit_eq(&Value::Enum {
            class: EnumClassId::SIMPLE_CONTROLLER,
            ordinal: 1
        }),
        "ReliefFan source fixes controllerType=P"
    );
    assert!(block_param(&graph, ".greThr", "t").bit_eq(&Value::Real(0.05)));
    assert!(block_param(&graph, ".greThr", "h").bit_eq(&Value::Real(0.005)));
    assert!(block_param(&graph, ".lesThr", "t").bit_eq(&Value::Real(0.005)));
    assert!(block_param(&graph, ".lesThr", "h").bit_eq(&Value::Real(0.005)));
    assert!(block_param(&graph, ".tim", "t").bit_eq(&Value::Real(300.0)));
    assert!(block_param(&graph, ".greThr2", "t").bit_eq(&Value::Real(0.25)));
    assert!(block_param(&graph, ".greThr2", "h").bit_eq(&Value::Real(0.005)));
    assert!(block_param(&graph, ".upTim", "t").bit_eq(&Value::Real(420.0)));
    assert!(block_param(&graph, ".lesThr3", "t").bit_eq(&Value::Real(0.1)));
    assert!(block_param(&graph, ".lesThr3", "h").bit_eq(&Value::Real(0.005)));
    assert!(block_param(&graph, ".dowTim", "t").bit_eq(&Value::Real(300.0)));
}

#[test]
fn golden_g36_multizone_vav_relief_fan_modelgraph() {
    let actual = render(&import_ok(G36_RELIEF_FAN));
    let path = golden_path(G36_RELIEF_FAN_GOLDEN_REL);
    if std::env::var_os("OCE_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(path)
        .expect("golden snapshot missing; regenerate with OCE_BLESS=1");
    assert_eq!(
        actual, expected,
        "source-verified G36 ReliefFan ModelGraph diverged from golden"
    );
}
