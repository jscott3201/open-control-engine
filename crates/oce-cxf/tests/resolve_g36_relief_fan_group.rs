//! Source-verified G36 ReliefFanGroup composite import tests.

mod bless;

use std::fmt::Write as _;
use std::path::PathBuf;

use oce_cxf::{ResolveOptions, import_cxf};
use oce_model::{EnumClassId, ModelGraph, Value};
use serde_json::{Value as JsonValue, json};

const G36_RELIEF_FAN_GROUP: &str =
    include_str!("fixtures/g36/multizone_vav_relief_fan_group.jsonld");
const G36_RELIEF_FAN_GROUP_GOLDEN_REL: &str =
    "tests/fixtures/golden/g36_multizone_vav_relief_fan_group.modelgraph.txt";
const G36_RELIEF_FAN_GROUP_MODEL: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan_group";
const G36_RELIEF_FAN_GROUP_CLASS: &str = "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.ReliefFanGroup";

fn import_ok(src: &str) -> ModelGraph {
    let (graph, report) = import_cxf(src.as_bytes(), &ResolveOptions::default())
        .expect("G36 ReliefFanGroup fixture should import");
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
fn source_verified_g36_multizone_vav_relief_fan_group_imports_source_default_variant() {
    let parsed: JsonValue = serde_json::from_str(G36_RELIEF_FAN_GROUP).expect("G36 fixture JSON");
    let top = parsed["@graph"]
        .as_array()
        .expect("@graph array")
        .iter()
        .find(|node| node["@id"] == json!(G36_RELIEF_FAN_GROUP_MODEL))
        .expect("top G36 ReliefFanGroup composite node");
    assert_eq!(top["@type"], json!(G36_RELIEF_FAN_GROUP_CLASS));
    assert_eq!(
        top["S231:containsBlock"]
            .as_array()
            .expect("children")
            .len(),
        226,
        "ReliefFanGroup source-default expansion should preserve all active child instances"
    );
    assert_eq!(
        top["S231:hasInput"].as_array().expect("inputs").len(),
        11,
        "nSupFan=2 and nRelFan=4 exposes 2 supply proofs, dpBui, 4 alarms, and 4 relief proofs"
    );
    assert_eq!(
        top["S231:hasOutput"].as_array().expect("outputs").len(),
        9,
        "nRelFan=4 exposes yDpBui plus four fan speeds and four damper commands"
    );

    let graph = import_ok(G36_RELIEF_FAN_GROUP);
    assert_eq!(
        graph.blocks.len(),
        226,
        "ReliefFanGroup should import the fixed source-default expanded graph"
    );
    let instances = graph
        .blocks
        .iter()
        .map(|block| block.instance_iri.as_deref().expect("source path"))
        .collect::<Vec<_>>();
    for suffix in [
        ".enaRel",
        ".booToRea_1",
        ".booToRea_2",
        ".gai_1",
        ".gai_2",
        ".gai_3",
        ".gai_4",
        ".movMea",
        ".conP",
        ".mulMin",
        ".mulMin1",
        ".logSwi_1",
        ".logSwi1_4",
        ".logSwi2_3",
        ".logSwi3_2",
        ".truDel_1",
        ".truDel1_4",
        ".lim",
        ".mulAnd1",
        ".booToRea5_4",
    ] {
        assert!(
            instances.iter().any(|iri| iri.ends_with(suffix)),
            "missing source component {suffix}"
        );
    }

    assert!(block_param(&graph, ".enaRel", "nout").bit_eq(&Value::Integer(4)));
    assert!(block_param(&graph, ".enaRel", "nin").bit_eq(&Value::Integer(2)));
    assert!(
        block_param(&graph, ".enaRel", "K_1_1").bit_eq(&Value::Real(1.0))
            && block_param(&graph, ".enaRel", "K_1_2").bit_eq(&Value::Real(0.0))
            && block_param(&graph, ".enaRel", "K_2_1").bit_eq(&Value::Real(1.0))
            && block_param(&graph, ".enaRel", "K_2_2").bit_eq(&Value::Real(0.0))
            && block_param(&graph, ".enaRel", "K_3_1").bit_eq(&Value::Real(0.0))
            && block_param(&graph, ".enaRel", "K_3_2").bit_eq(&Value::Real(1.0))
            && block_param(&graph, ".enaRel", "K_4_1").bit_eq(&Value::Real(0.0))
            && block_param(&graph, ".enaRel", "K_4_2").bit_eq(&Value::Real(1.0)),
        "enaRel must ground default relFanMat row-major"
    );
    for (suffix, order) in [
        (".gai_1", 2.0),
        (".gai_2", 3.0),
        (".gai_3", 1.0),
        (".gai_4", 4.0),
    ] {
        assert!(
            block_param(&graph, suffix, "k").bit_eq(&Value::Real(order)),
            "{suffix} must ground default staVec"
        );
    }
    assert!(block_param(&graph, ".movMea", "delta").bit_eq(&Value::Real(300.0)));
    assert!(block_param(&graph, ".dpBuiSetPoi", "k").bit_eq(&Value::Real(12.0)));
    assert!(block_param(&graph, ".conP", "k").bit_eq(&Value::Real(1.0)));
    assert!(block_param(&graph, ".conP", "reverseActing").bit_eq(&Value::Boolean(false)));
    assert!(
        block_param(&graph, ".conP", "controllerType").bit_eq(&Value::Enum {
            class: EnumClassId::SIMPLE_CONTROLLER,
            ordinal: 1
        }),
        "ReliefFanGroup source fixes controllerType=P"
    );
    assert!(block_param(&graph, ".greThr", "t").bit_eq(&Value::Real(0.05)));
    assert!(block_param(&graph, ".tim", "t").bit_eq(&Value::Real(300.0)));
    assert!(block_param(&graph, ".upTim", "t").bit_eq(&Value::Real(420.0)));
    assert!(block_param(&graph, ".dowTim", "t").bit_eq(&Value::Real(300.0)));
    assert!(block_param(&graph, ".pre", "pre_u_start").bit_eq(&Value::Boolean(true)));
    assert!(block_param(&graph, ".pre1", "pre_u_start").bit_eq(&Value::Boolean(true)));
    assert!(block_param(&graph, ".lim", "uMin").bit_eq(&Value::Real(0.1)));
    assert!(block_param(&graph, ".lim", "uMax").bit_eq(&Value::Real(1.0)));
    for suffix in [".truDel_1", ".truDel_2", ".truDel_3", ".truDel_4"] {
        assert!(block_param(&graph, suffix, "delayTime").bit_eq(&Value::Real(2.0)));
    }
    for suffix in [".truDel1_1", ".truDel1_2", ".truDel1_3", ".truDel1_4"] {
        assert!(block_param(&graph, suffix, "delayTime").bit_eq(&Value::Real(2.0)));
    }
    for suffix in [".mulAnd", ".mulAnd1", ".mulAnd2", ".mulOr", ".mulOr1"] {
        assert!(block_param(&graph, suffix, "nin").bit_eq(&Value::Integer(4)));
    }
}

#[test]
fn golden_g36_multizone_vav_relief_fan_group_modelgraph() {
    let actual = render(&import_ok(G36_RELIEF_FAN_GROUP));
    let path = golden_path(G36_RELIEF_FAN_GROUP_GOLDEN_REL);
    if bless::enabled() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(path)
        .expect("golden snapshot missing; regenerate with OCE_BLESS=1");
    assert_eq!(
        actual, expected,
        "source-verified G36 ReliefFanGroup ModelGraph diverged from golden"
    );
}
