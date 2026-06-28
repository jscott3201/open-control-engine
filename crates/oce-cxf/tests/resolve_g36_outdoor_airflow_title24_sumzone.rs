//! Source-verified G36 OutdoorAirFlow Title 24 SumZone composite import tests.

use std::fmt::Write as _;
use std::path::PathBuf;

use oce_cxf::{ResolveOptions, import_cxf};
use oce_model::{ModelGraph, Value};
use serde_json::{Value as JsonValue, json};

const G36_OUTDOOR_AIRFLOW_TITLE24_SUMZONE: &str =
    include_str!("fixtures/g36/multizone_vav_outdoor_airflow_title24_sumzone.jsonld");
const G36_OUTDOOR_AIRFLOW_TITLE24_SUMZONE_GOLDEN_REL: &str =
    "tests/fixtures/golden/g36_multizone_vav_outdoor_airflow_title24_sumzone.modelgraph.txt";
const G36_OUTDOOR_AIRFLOW_TITLE24_SUMZONE_MODEL: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_sumzone";
const G36_OUTDOOR_AIRFLOW_TITLE24_SUMZONE_CLASS: &str = "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.OutdoorAirFlow.Title24.SumZone";

fn import_ok(src: &str) -> ModelGraph {
    let (graph, report) = import_cxf(src.as_bytes(), &ResolveOptions::default())
        .expect("G36 OutdoorAirFlow Title24 SumZone fixture should import");
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
fn source_verified_g36_multizone_vav_outdoor_airflow_title24_sumzone_imports_fixed_group_matrix_variant()
 {
    let parsed: JsonValue =
        serde_json::from_str(G36_OUTDOOR_AIRFLOW_TITLE24_SUMZONE).expect("G36 fixture JSON");
    let top = parsed["@graph"]
        .as_array()
        .expect("@graph array")
        .iter()
        .find(|node| node["@id"] == json!(G36_OUTDOOR_AIRFLOW_TITLE24_SUMZONE_MODEL))
        .expect("top G36 OutdoorAirFlow Title24 SumZone composite node");
    assert_eq!(
        top["@type"],
        json!(G36_OUTDOOR_AIRFLOW_TITLE24_SUMZONE_CLASS)
    );
    assert_eq!(
        top["S231:containsBlock"]
            .as_array()
            .expect("children")
            .len(),
        15,
        "Title24 SumZone source transcription should preserve 15 active child components"
    );
    assert_eq!(
        top["S231:hasInput"].as_array().expect("inputs").len(),
        11,
        "nGro=2, nZon=3, have_CO2Sen=true variant should expose operation modes, zone flows, and CO2 inputs"
    );
    assert_eq!(
        top["S231:hasOutput"].as_array().expect("outputs").len(),
        3,
        "CO2-enabled SumZone variant should expose two airflow sums plus max CO2"
    );

    let graph = import_ok(G36_OUTDOOR_AIRFLOW_TITLE24_SUMZONE);
    assert_eq!(
        graph.blocks.len(),
        15,
        "Title24 SumZone aggregation should import all fixed-variant active children"
    );
    let instances = graph
        .blocks
        .iter()
        .map(|block| block.instance_iri.as_deref().expect("source path"))
        .collect::<Vec<_>>();
    for suffix in [
        ".groFlo",
        ".groFlo1",
        ".booToRea_1",
        ".booToRea_2",
        ".mul_1",
        ".mul_2",
        ".mul1_1",
        ".mul1_2",
        ".mulSum",
        ".mulSum1",
        ".occMod_1",
        ".occMod_2",
        ".intEqu1_1",
        ".intEqu1_2",
        ".mulMax",
    ] {
        assert!(
            instances.iter().any(|iri| iri.ends_with(suffix)),
            "missing source component {suffix}"
        );
    }

    assert!(block_param(&graph, ".groFlo", "nout").bit_eq(&Value::Integer(2)));
    assert!(block_param(&graph, ".groFlo", "nin").bit_eq(&Value::Integer(3)));
    assert!(
        block_param(&graph, ".groFlo", "K_1_1").bit_eq(&Value::Real(1.0))
            && block_param(&graph, ".groFlo", "K_1_2").bit_eq(&Value::Real(1.0))
            && block_param(&graph, ".groFlo", "K_1_3").bit_eq(&Value::Real(0.0))
            && block_param(&graph, ".groFlo", "K_2_1").bit_eq(&Value::Real(0.0))
            && block_param(&graph, ".groFlo", "K_2_2").bit_eq(&Value::Real(1.0))
            && block_param(&graph, ".groFlo", "K_2_3").bit_eq(&Value::Real(1.0)),
        "absolute-flow group matrix must ground row-major from zonGroMat"
    );
    assert!(
        block_param(&graph, ".groFlo1", "K_1_1").bit_eq(&Value::Real(1.0))
            && block_param(&graph, ".groFlo1", "K_2_3").bit_eq(&Value::Real(1.0)),
        "design-flow group matrix must use the same zonGroMat"
    );
    assert!(block_param(&graph, ".mulSum", "nin").bit_eq(&Value::Integer(2)));
    assert!(block_param(&graph, ".mulSum1", "nin").bit_eq(&Value::Integer(2)));
    assert!(block_param(&graph, ".mulMax", "nin").bit_eq(&Value::Integer(3)));
    assert!(block_param(&graph, ".occMod_1", "k").bit_eq(&Value::Integer(1)));
    assert!(block_param(&graph, ".occMod_2", "k").bit_eq(&Value::Integer(1)));
}

#[test]
fn golden_g36_multizone_vav_outdoor_airflow_title24_sumzone_modelgraph() {
    let actual = render(&import_ok(G36_OUTDOOR_AIRFLOW_TITLE24_SUMZONE));
    let path = golden_path(G36_OUTDOOR_AIRFLOW_TITLE24_SUMZONE_GOLDEN_REL);
    if std::env::var_os("OCE_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(path)
        .expect("golden snapshot missing; regenerate with OCE_BLESS=1");
    assert_eq!(
        actual, expected,
        "source-verified G36 OutdoorAirFlow Title24 SumZone ModelGraph diverged from golden"
    );
}
