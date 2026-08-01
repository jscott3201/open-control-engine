//! Source-verified G36 CoolingOnly Alarms composite import tests.
//!
//! Upstream `Alarms.mo` lines 91-94 and 195-198 bind comparator hysteresis as half of
//! `floHys` and `damPosHys`. The fixture pre-grounds both expressions to `0.005` from the
//! canonical Validation bindings while retaining references for every direct parameter binding.

mod bless;

use std::fmt::Write as _;
use std::path::PathBuf;

use oce_cxf::{ResolveOptions, import_cxf};
use oce_model::{ModelGraph, Value};
use serde_json::{Value as JsonValue, json};

const G36_COOLING_ONLY_ALARMS: &str = include_str!("fixtures/g36/cooling_only_alarms.jsonld");
const G36_COOLING_ONLY_ALARMS_GOLDEN_REL: &str =
    "tests/fixtures/golden/g36_cooling_only_alarms.modelgraph.txt";
const G36_COOLING_ONLY_ALARMS_MODEL: &str = "http://example.org#g36.source.cooling_only_alarms";
const G36_COOLING_ONLY_ALARMS_CLASS: &str = "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.TerminalUnits.CoolingOnly.Subsequences.Alarms";

fn import_ok(src: &str) -> ModelGraph {
    let (graph, report) = import_cxf(src.as_bytes(), &ResolveOptions::default())
        .expect("G36 CoolingOnly Alarms fixture should import");
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
fn source_verified_cooling_only_alarms_preserves_topology_bindings_and_messages() {
    let parsed: JsonValue =
        serde_json::from_str(G36_COOLING_ONLY_ALARMS).expect("G36 fixture JSON");
    let top = parsed["@graph"]
        .as_array()
        .expect("@graph array")
        .iter()
        .find(|node| node["@id"] == json!(G36_COOLING_ONLY_ALARMS_MODEL))
        .expect("top G36 CoolingOnly Alarms composite node");
    assert_eq!(top["@type"], json!(G36_COOLING_ONLY_ALARMS_CLASS));
    assert_eq!(
        top["S231:hasParameter"]
            .as_array()
            .expect("parameters")
            .len(),
        8
    );
    assert_eq!(
        top["S231:containsBlock"]
            .as_array()
            .expect("children")
            .len(),
        47
    );
    assert_eq!(top["S231:hasInput"].as_array().expect("inputs").len(), 5);
    assert_eq!(top["S231:hasOutput"].as_array().expect("outputs").len(), 3);

    let graph = import_ok(G36_COOLING_ONLY_ALARMS);
    assert_eq!(graph.blocks.len(), 47);
    let instances = graph
        .blocks
        .iter()
        .map(|block| block.instance_iri.as_deref().expect("source path"))
        .collect::<Vec<_>>();
    for suffix in [
        ".gai",
        ".les",
        ".truDel",
        ".greThr",
        ".gre",
        ".gai1",
        ".truDel1",
        ".and2",
        ".and1",
        ".lowFloAla",
        ".conInt",
        ".booToInt",
        ".conInt1",
        ".greThr1",
        ".booToInt1",
        ".proInt",
        ".and8",
        ".not1",
        ".assMes",
        ".and4",
        ".not2",
        ".assMes1",
        ".cooMaxFlo",
        ".gai2",
        ".not3",
        ".truDel2",
        ".gre1",
        ".and5",
        ".not4",
        ".assMes2",
        ".booToInt2",
        ".truDel3",
        ".cloDam",
        ".leaDamAla1",
        ".leaDamAla2",
        ".not5",
        ".assMes3",
        ".booToInt3",
        ".truDel4",
        ".and11",
        ".and10",
        ".fanIni",
        ".occMod",
        ".isOcc",
        ".and6",
        ".and7",
        ".and3",
    ] {
        assert!(
            instances.iter().any(|iri| iri.ends_with(suffix)),
            "missing source component {suffix}"
        );
    }

    for (suffix, name, expected) in [
        (".greThr", "t", 0.01),
        (".greThr", "h", 0.005),
        (".cloDam", "t", 0.01),
        (".cloDam", "h", 0.005),
        (".les", "h", 0.01),
        (".gre", "h", 0.01),
        (".gre1", "h", 0.01),
        (".truDel", "delayTime", 300.0),
        (".truDel1", "delayTime", 300.0),
        (".truDel2", "delayTime", 600.0),
        (".truDel3", "delayTime", 600.0),
        (".truDel4", "delayTime", 300.0),
        (".fanIni", "delayTime", 1800.0),
        (".gai", "k", 0.5),
        (".gai1", "k", 0.7),
        (".gai2", "k", 0.1),
        (".conInt1", "k", 1.0),
        (".cooMaxFlo", "k", 0.5),
        (".greThr1", "t", 0.0),
        (".greThr1", "h", 0.0),
    ] {
        assert!(
            block_param(&graph, suffix, name).bit_eq(&Value::Real(expected)),
            "{suffix}.{name}"
        );
    }

    assert!(block_param(&graph, ".occMod", "k").bit_eq(&Value::Integer(1)));
    assert!(block_param(&graph, ".conInt", "k").bit_eq(&Value::Integer(2)));
    for (suffix, expected_true) in [
        (".booToInt", 3),
        (".booToInt1", 1),
        (".booToInt2", 3),
        (".booToInt3", 4),
    ] {
        assert!(
            block_param(&graph, suffix, "integerTrue").bit_eq(&Value::Integer(expected_true)),
            "{suffix}.integerTrue"
        );
        assert!(
            block_param(&graph, suffix, "integerFalse").bit_eq(&Value::Integer(0)),
            "{suffix}.integerFalse"
        );
    }

    for (suffix, expected) in [
        (
            ".assMes",
            "Warning: airflow is less than 50% of the setpoint.",
        ),
        (
            ".assMes1",
            "Warning: airflow is less than 70% of the setpoint.",
        ),
        (".assMes2", "Warning: airflow sensor should be calibrated."),
        (".assMes3", "Warning: the damper is leaking."),
    ] {
        assert!(
            block_param(&graph, suffix, "message").bit_eq(&Value::String(expected.into())),
            "{suffix}.message"
        );
    }
}

#[test]
fn cooling_only_alarms_modelgraph_is_stable() {
    let actual = render(&import_ok(G36_COOLING_ONLY_ALARMS));
    let path = golden_path(G36_COOLING_ONLY_ALARMS_GOLDEN_REL);
    if bless::enabled() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(path)
        .expect("golden snapshot missing; regenerate with OCE_BLESS=1");
    assert_eq!(
        actual, expected,
        "source-verified G36 CoolingOnly Alarms ModelGraph diverged from golden"
    );
}
