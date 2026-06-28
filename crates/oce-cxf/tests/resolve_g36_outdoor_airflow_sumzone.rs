//! Source-verified G36 OutdoorAirFlow ASHRAE 62.1 SumZone composite import tests.

use std::fmt::Write as _;
use std::path::PathBuf;

use oce_cxf::{ResolveOptions, import_cxf};
use oce_model::{ModelGraph, Value};
use serde_json::{Value as JsonValue, json};

const G36_OUTDOOR_AIRFLOW_SUMZONE: &str =
    include_str!("fixtures/g36/multizone_vav_outdoor_airflow_sumzone.jsonld");
const G36_OUTDOOR_AIRFLOW_SUMZONE_GOLDEN_REL: &str =
    "tests/fixtures/golden/g36_multizone_vav_outdoor_airflow_sumzone.modelgraph.txt";
const G36_OUTDOOR_AIRFLOW_SUMZONE_MODEL: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_sumzone";
const G36_OUTDOOR_AIRFLOW_SUMZONE_CLASS: &str = "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.OutdoorAirFlow.ASHRAE62_1.SumZone";

fn import_ok(src: &str) -> ModelGraph {
    let (graph, report) = import_cxf(src.as_bytes(), &ResolveOptions::default())
        .expect("G36 OutdoorAirFlow ASHRAE62_1 SumZone fixture should import");
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
fn source_verified_g36_multizone_vav_outdoor_airflow_sumzone_imports_fixed_group_matrix_variant() {
    let parsed: JsonValue =
        serde_json::from_str(G36_OUTDOOR_AIRFLOW_SUMZONE).expect("G36 fixture JSON");
    let top = parsed["@graph"]
        .as_array()
        .expect("@graph array")
        .iter()
        .find(|node| node["@id"] == json!(G36_OUTDOOR_AIRFLOW_SUMZONE_MODEL))
        .expect("top G36 OutdoorAirFlow ASHRAE62_1 SumZone composite node");
    assert_eq!(top["@type"], json!(G36_OUTDOOR_AIRFLOW_SUMZONE_CLASS));
    assert_eq!(
        top["S231:containsBlock"]
            .as_array()
            .expect("children")
            .len(),
        35,
        "ASHRAE62_1 SumZone source transcription should preserve 35 active child components"
    );
    assert_eq!(
        top["S231:hasInput"].as_array().expect("inputs").len(),
        14,
        "nGro=2, nZon=3 variant should expose operation modes and four zone-flow vectors"
    );
    assert_eq!(
        top["S231:hasOutput"].as_array().expect("outputs").len(),
        4,
        "SumZone should expose three airflow sums plus the maximum outdoor-air fraction"
    );

    let graph = import_ok(G36_OUTDOOR_AIRFLOW_SUMZONE);
    assert_eq!(
        graph.blocks.len(),
        35,
        "ASHRAE62_1 SumZone aggregation should import all fixed-variant active children"
    );
    let instances = graph
        .blocks
        .iter()
        .map(|block| block.instance_iri.as_deref().expect("source path"))
        .collect::<Vec<_>>();
    for suffix in [
        ".groFlo",
        ".groFlo1",
        ".groFlo2",
        ".groFlo3",
        ".booToRea_1",
        ".booToRea_2",
        ".mul_1",
        ".mul_2",
        ".mul1_1",
        ".mul1_2",
        ".mul2_1",
        ".mul2_2",
        ".mulSum",
        ".mulSum1",
        ".mulSum2",
        ".occMod_1",
        ".occMod_2",
        ".intEqu1_1",
        ".intEqu1_2",
        ".div1_1",
        ".div1_2",
        ".div1_3",
        ".max2_1",
        ".max2_2",
        ".max2_3",
        ".min1_1",
        ".min1_2",
        ".min1_3",
        ".mul3_1",
        ".mul3_2",
        ".mul3_3",
        ".mulMax",
        ".neaZer_1",
        ".neaZer_2",
        ".neaZer_3",
    ] {
        assert!(
            instances.iter().any(|iri| iri.ends_with(suffix)),
            "missing source component {suffix}"
        );
    }

    for suffix in [".groFlo", ".groFlo1", ".groFlo2"] {
        assert!(block_param(&graph, suffix, "nout").bit_eq(&Value::Integer(2)));
        assert!(block_param(&graph, suffix, "nin").bit_eq(&Value::Integer(3)));
        assert!(
            block_param(&graph, suffix, "K_1_1").bit_eq(&Value::Real(1.0))
                && block_param(&graph, suffix, "K_1_2").bit_eq(&Value::Real(1.0))
                && block_param(&graph, suffix, "K_1_3").bit_eq(&Value::Real(0.0))
                && block_param(&graph, suffix, "K_2_1").bit_eq(&Value::Real(0.0))
                && block_param(&graph, suffix, "K_2_2").bit_eq(&Value::Real(1.0))
                && block_param(&graph, suffix, "K_2_3").bit_eq(&Value::Real(1.0)),
            "{suffix} must ground zonGroMat row-major"
        );
    }
    assert!(block_param(&graph, ".groFlo3", "nout").bit_eq(&Value::Integer(3)));
    assert!(block_param(&graph, ".groFlo3", "nin").bit_eq(&Value::Integer(2)));
    assert!(
        block_param(&graph, ".groFlo3", "K_1_1").bit_eq(&Value::Real(1.0))
            && block_param(&graph, ".groFlo3", "K_1_2").bit_eq(&Value::Real(0.0))
            && block_param(&graph, ".groFlo3", "K_2_1").bit_eq(&Value::Real(1.0))
            && block_param(&graph, ".groFlo3", "K_2_2").bit_eq(&Value::Real(1.0))
            && block_param(&graph, ".groFlo3", "K_3_1").bit_eq(&Value::Real(0.0))
            && block_param(&graph, ".groFlo3", "K_3_2").bit_eq(&Value::Real(1.0)),
        "groFlo3 must ground zonGroMatTra row-major"
    );
    assert!(block_param(&graph, ".mulMax", "nin").bit_eq(&Value::Integer(3)));
    for suffix in [".mulSum", ".mulSum1", ".mulSum2"] {
        assert!(block_param(&graph, suffix, "nin").bit_eq(&Value::Integer(2)));
    }
    for suffix in [".occMod_1", ".occMod_2"] {
        assert!(block_param(&graph, suffix, "k").bit_eq(&Value::Integer(1)));
    }
    for suffix in [".neaZer_1", ".neaZer_2", ".neaZer_3"] {
        assert!(block_param(&graph, suffix, "k").bit_eq(&Value::Real(1e-4)));
    }
}

#[test]
fn golden_g36_multizone_vav_outdoor_airflow_sumzone_modelgraph() {
    let actual = render(&import_ok(G36_OUTDOOR_AIRFLOW_SUMZONE));
    let path = golden_path(G36_OUTDOOR_AIRFLOW_SUMZONE_GOLDEN_REL);
    if std::env::var_os("OCE_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(path)
        .expect("golden snapshot missing; regenerate with OCE_BLESS=1");
    assert_eq!(
        actual, expected,
        "source-verified G36 OutdoorAirFlow ASHRAE62_1 SumZone ModelGraph diverged from golden"
    );
}
