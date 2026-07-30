//! Source-verified G36 Economizers.Subsequences.Modulations.ReturnFan composite import tests.

mod bless;

use std::fmt::Write as _;
use std::path::PathBuf;

use oce_cxf::{ResolveOptions, import_cxf};
use oce_model::{ModelGraph, Value};
use serde_json::{Value as JsonValue, json};

const G36_ECONOMIZER_MODULATIONS_RETURN_FAN: &str =
    include_str!("fixtures/g36/multizone_vav_economizer_modulations_return_fan.jsonld");
const G36_ECONOMIZER_MODULATIONS_RETURN_FAN_RELIEF_DAMPER: &str = include_str!(
    "fixtures/g36/multizone_vav_economizer_modulations_return_fan_relief_damper.jsonld"
);
const G36_ECONOMIZER_MODULATIONS_RETURN_FAN_GOLDEN_REL: &str =
    "tests/fixtures/golden/g36_multizone_vav_economizer_modulations_return_fan.modelgraph.txt";
const G36_ECONOMIZER_MODULATIONS_RETURN_FAN_RELIEF_DAMPER_GOLDEN_REL: &str = "tests/fixtures/golden/g36_multizone_vav_economizer_modulations_return_fan_relief_damper.modelgraph.txt";
const G36_ECONOMIZER_MODULATIONS_RETURN_FAN_MODEL: &str =
    "http://example.org#g36.source.multizone_vav_economizer_modulations_return_fan";
const G36_ECONOMIZER_MODULATIONS_RETURN_FAN_RELIEF_DAMPER_MODEL: &str =
    "http://example.org#g36.source.multizone_vav_economizer_modulations_return_fan_relief_damper";
const G36_ECONOMIZER_MODULATIONS_RETURN_FAN_CLASS: &str = "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.Economizers.Subsequences.Modulations.ReturnFan";

fn import_ok(src: &str) -> ModelGraph {
    let (graph, report) = import_cxf(src.as_bytes(), &ResolveOptions::default())
        .expect("G36 Economizers.Subsequences.Modulations.ReturnFan fixture should import");
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

fn top_node<'a>(parsed: &'a JsonValue, model: &str) -> &'a JsonValue {
    parsed["@graph"]
        .as_array()
        .expect("@graph array")
        .iter()
        .find(|node| node["@id"] == json!(model))
        .unwrap_or_else(|| {
            panic!("top G36 Economizers.Subsequences.Modulations.ReturnFan node {model}")
        })
}

#[test]
fn source_verified_g36_multizone_vav_economizer_modulations_return_fan_imports_default_variant() {
    let parsed: JsonValue =
        serde_json::from_str(G36_ECONOMIZER_MODULATIONS_RETURN_FAN).expect("G36 fixture JSON");
    let top = top_node(&parsed, G36_ECONOMIZER_MODULATIONS_RETURN_FAN_MODEL);
    assert_eq!(
        top["@type"],
        json!(G36_ECONOMIZER_MODULATIONS_RETURN_FAN_CLASS)
    );
    assert_eq!(
        top["S231:containsBlock"]
            .as_array()
            .expect("children")
            .len(),
        4,
        "ReturnFan source-default have_dirCon=true variant should keep four active child blocks"
    );
    assert_eq!(top["S231:hasInput"].as_array().expect("inputs").len(), 3);
    assert_eq!(top["S231:hasOutput"].as_array().expect("outputs").len(), 2);

    let graph = import_ok(G36_ECONOMIZER_MODULATIONS_RETURN_FAN);
    assert_eq!(
        graph.blocks.len(),
        4,
        "economizer return-fan modulation sequence should import all active source children"
    );
    let instances = graph
        .blocks
        .iter()
        .map(|block| block.instance_iri.as_deref().expect("source path"))
        .collect::<Vec<_>>();
    for suffix in [".damMinLimSig", ".damMaxLimSig", ".retDamPos", ".one"] {
        assert!(
            instances.iter().any(|iri| iri.ends_with(suffix)),
            "missing source component {suffix}"
        );
    }
    for inactive_suffix in [".relDamPos", ".zer"] {
        assert!(
            !instances.iter().any(|iri| iri.ends_with(inactive_suffix)),
            "inactive have_dirCon=false source component should be absent: {inactive_suffix}"
        );
    }

    assert!(block_param(&graph, ".damMinLimSig", "k").bit_eq(&Value::Real(-0.25)));
    assert!(block_param(&graph, ".damMaxLimSig", "k").bit_eq(&Value::Real(0.25)));
    assert!(block_param(&graph, ".retDamPos", "limitBelow").bit_eq(&Value::Boolean(true)));
    assert!(block_param(&graph, ".retDamPos", "limitAbove").bit_eq(&Value::Boolean(true)));
    assert!(block_param(&graph, ".one", "k").bit_eq(&Value::Real(1.0)));
}

#[test]
fn return_fan_imports_relief_damper_branch() {
    let parsed: JsonValue =
        serde_json::from_str(G36_ECONOMIZER_MODULATIONS_RETURN_FAN_RELIEF_DAMPER)
            .expect("G36 relief-damper variant fixture JSON");
    let top = top_node(
        &parsed,
        G36_ECONOMIZER_MODULATIONS_RETURN_FAN_RELIEF_DAMPER_MODEL,
    );
    assert_eq!(
        top["@type"],
        json!(G36_ECONOMIZER_MODULATIONS_RETURN_FAN_CLASS)
    );
    assert_eq!(
        top["S231:containsBlock"]
            .as_array()
            .expect("children")
            .len(),
        6,
        "ReturnFan have_dirCon=false variant should keep six active child blocks"
    );
    assert_eq!(top["S231:hasInput"].as_array().expect("inputs").len(), 3);
    assert_eq!(top["S231:hasOutput"].as_array().expect("outputs").len(), 3);

    let graph = import_ok(G36_ECONOMIZER_MODULATIONS_RETURN_FAN_RELIEF_DAMPER);
    assert_eq!(
        graph.blocks.len(),
        6,
        "economizer return-fan relief-damper variant should import all active source children"
    );
    let instances = graph
        .blocks
        .iter()
        .map(|block| block.instance_iri.as_deref().expect("source path"))
        .collect::<Vec<_>>();
    for suffix in [
        ".damMinLimSig",
        ".damMaxLimSig",
        ".retDamPos",
        ".relDamPos",
        ".zer",
        ".one",
    ] {
        assert!(
            instances.iter().any(|iri| iri.ends_with(suffix)),
            "missing source component {suffix}"
        );
    }

    assert!(block_param(&graph, ".damMinLimSig", "k").bit_eq(&Value::Real(-0.25)));
    assert!(block_param(&graph, ".damMaxLimSig", "k").bit_eq(&Value::Real(0.25)));
    assert!(block_param(&graph, ".retDamPos", "limitBelow").bit_eq(&Value::Boolean(true)));
    assert!(block_param(&graph, ".retDamPos", "limitAbove").bit_eq(&Value::Boolean(true)));
    assert!(block_param(&graph, ".relDamPos", "limitBelow").bit_eq(&Value::Boolean(true)));
    assert!(block_param(&graph, ".relDamPos", "limitAbove").bit_eq(&Value::Boolean(true)));
    assert!(block_param(&graph, ".zer", "k").bit_eq(&Value::Real(0.0)));
    assert!(block_param(&graph, ".one", "k").bit_eq(&Value::Real(1.0)));
}

#[test]
fn golden_g36_multizone_vav_economizer_modulations_return_fan_modelgraph() {
    let actual = render(&import_ok(G36_ECONOMIZER_MODULATIONS_RETURN_FAN));
    let path = golden_path(G36_ECONOMIZER_MODULATIONS_RETURN_FAN_GOLDEN_REL);
    if bless::enabled() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(path)
        .expect("golden snapshot missing; regenerate with OCE_BLESS=1");
    assert_eq!(
        actual, expected,
        "source-verified G36 Economizers.Subsequences.Modulations.ReturnFan ModelGraph diverged from golden"
    );
}

#[test]
fn golden_g36_multizone_vav_economizer_modulations_return_fan_relief_damper_modelgraph() {
    let actual = render(&import_ok(
        G36_ECONOMIZER_MODULATIONS_RETURN_FAN_RELIEF_DAMPER,
    ));
    let path = golden_path(G36_ECONOMIZER_MODULATIONS_RETURN_FAN_RELIEF_DAMPER_GOLDEN_REL);
    if bless::enabled() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(path)
        .expect("golden snapshot missing; regenerate with OCE_BLESS=1");
    assert_eq!(
        actual, expected,
        "source-verified G36 Economizers.Subsequences.Modulations.ReturnFan relief-damper ModelGraph diverged from golden"
    );
}
