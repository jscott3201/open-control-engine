//! Source-verified G36 Economizers.Subsequences.Enable composite import tests.

mod bless;

use std::fmt::Write as _;
use std::path::PathBuf;

use oce_cxf::{ResolveOptions, import_cxf};
use oce_model::{ModelGraph, Value};
use serde_json::{Value as JsonValue, json};

const G36_ECONOMIZER_ENABLE: &str =
    include_str!("fixtures/g36/multizone_vav_economizer_enable.jsonld");
const G36_ECONOMIZER_ENABLE_GOLDEN_REL: &str =
    "tests/fixtures/golden/g36_multizone_vav_economizer_enable.modelgraph.txt";
const G36_ECONOMIZER_ENABLE_MODEL: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable";
const G36_ECONOMIZER_ENABLE_CLASS: &str = "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.Economizers.Subsequences.Enable";

fn import_ok(src: &str) -> ModelGraph {
    let (graph, report) = import_cxf(src.as_bytes(), &ResolveOptions::default())
        .expect("G36 Economizers.Subsequences.Enable fixture should import");
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
fn source_verified_g36_multizone_vav_economizer_enable_imports_no_enthalpy_variant() {
    let parsed: JsonValue = serde_json::from_str(G36_ECONOMIZER_ENABLE).expect("G36 fixture JSON");
    let top = parsed["@graph"]
        .as_array()
        .expect("@graph array")
        .iter()
        .find(|node| node["@id"] == json!(G36_ECONOMIZER_ENABLE_MODEL))
        .expect("top G36 Economizers.Subsequences.Enable composite node");
    assert_eq!(top["@type"], json!(G36_ECONOMIZER_ENABLE_CLASS));
    assert_eq!(
        top["S231:containsBlock"]
            .as_array()
            .expect("children")
            .len(),
        19,
        "use_enthalpy=false should keep 19 active source children"
    );
    assert_eq!(top["S231:hasInput"].as_array().expect("inputs").len(), 9);
    assert_eq!(top["S231:hasOutput"].as_array().expect("outputs").len(), 3);

    let graph = import_ok(G36_ECONOMIZER_ENABLE);
    assert_eq!(
        graph.blocks.len(),
        19,
        "economizer enable sequence should import all active source children"
    );
    let instances = graph
        .blocks
        .iter()
        .map(|block| block.instance_iri.as_deref().expect("source path"))
        .collect::<Vec<_>>();
    for suffix in [
        ".truFalHol",
        ".andEnaDis",
        ".sub1",
        ".hysOutTem",
        ".outDamSwitch",
        ".retDamSwitch",
        ".maxRetDamSwitch",
        ".minRetDamSwitch",
        ".not2",
        ".and2",
        ".and1",
        ".and3",
        ".intEqu",
        ".delOutDamOsc",
        ".delRetDam",
        ".not1",
        ".conInt",
        ".entSubst1",
        ".or2",
    ] {
        assert!(
            instances.iter().any(|iri| iri.ends_with(suffix)),
            "missing source component {suffix}"
        );
    }
    for pruned in [".sub2", ".hysOutEnt"] {
        assert!(
            instances.iter().all(|iri| !iri.ends_with(pruned)),
            "use_enthalpy=false fixture must prune {pruned}"
        );
    }

    assert!(block_param(&graph, ".hysOutTem", "uLow").bit_eq(&Value::Real(-1.0)));
    assert!(block_param(&graph, ".hysOutTem", "uHigh").bit_eq(&Value::Real(0.0)));
    assert!(block_param(&graph, ".truFalHol", "trueHoldDuration").bit_eq(&Value::Real(600.0)));
    assert!(block_param(&graph, ".truFalHol", "falseHoldDuration").bit_eq(&Value::Real(600.0)));
    assert!(block_param(&graph, ".delOutDamOsc", "delayTime").bit_eq(&Value::Real(15.0)));
    assert!(block_param(&graph, ".delRetDam", "delayTime").bit_eq(&Value::Real(180.0)));
    assert!(
        block_param(&graph, ".conInt", "k").bit_eq(&Value::Integer(0)),
        "FreezeProtectionStages.stage0 must ground to integer zero"
    );
    assert!(
        block_param(&graph, ".entSubst1", "k").bit_eq(&Value::Boolean(false)),
        "no-enthalpy branch must feed false into the enthalpy side of the OR"
    );
}

#[test]
fn golden_g36_multizone_vav_economizer_enable_modelgraph() {
    let actual = render(&import_ok(G36_ECONOMIZER_ENABLE));
    let path = golden_path(G36_ECONOMIZER_ENABLE_GOLDEN_REL);
    if bless::enabled() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(path)
        .expect("golden snapshot missing; regenerate with OCE_BLESS=1");
    assert_eq!(
        actual, expected,
        "source-verified G36 Economizers.Subsequences.Enable ModelGraph diverged from golden"
    );
}
