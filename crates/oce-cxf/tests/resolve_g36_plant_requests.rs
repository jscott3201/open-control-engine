//! Source-verified G36 PlantRequests composite import tests.

use std::fmt::Write as _;
use std::path::PathBuf;

use oce_cxf::{ResolveOptions, import_cxf};
use oce_model::{ModelGraph, Value};
use serde_json::{Value as JsonValue, json};

const G36_PLANT_REQUESTS: &str = include_str!("fixtures/g36/multizone_vav_plant_requests.jsonld");
const G36_PLANT_REQUESTS_GOLDEN_REL: &str =
    "tests/fixtures/golden/g36_multizone_vav_plant_requests.modelgraph.txt";
const G36_PLANT_REQUESTS_MODEL: &str = "http://example.org#g36.source.multizone_vav_plant_requests";
const G36_PLANT_REQUESTS_CLASS: &str = "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.PlantRequests";

fn import_ok(src: &str) -> ModelGraph {
    let (graph, report) = import_cxf(src.as_bytes(), &ResolveOptions::default())
        .expect("G36 PlantRequests fixture should import");
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

fn connector_iri_counts(graph: &ModelGraph) -> Vec<(String, usize)> {
    let mut counts = std::collections::BTreeMap::new();
    for connector in &graph.connectors {
        if let Some(iri) = connector.iri.as_deref() {
            *counts.entry(iri.to_owned()).or_insert(0usize) += 1;
        }
    }
    counts.into_iter().collect()
}

#[test]
fn source_verified_g36_multizone_vav_plant_requests_imports_water_based_variant() {
    let parsed: JsonValue = serde_json::from_str(G36_PLANT_REQUESTS).expect("G36 fixture JSON");
    let top = parsed["@graph"]
        .as_array()
        .expect("@graph array")
        .iter()
        .find(|node| node["@id"] == json!(G36_PLANT_REQUESTS_MODEL))
        .expect("top G36 PlantRequests composite node");
    assert_eq!(top["@type"], json!(G36_PLANT_REQUESTS_CLASS));
    assert_eq!(
        top["S231:containsBlock"]
            .as_array()
            .expect("children")
            .len(),
        32,
        "PlantRequests source transcription should preserve the 32 declared child components"
    );

    let graph = import_ok(G36_PLANT_REQUESTS);
    assert_eq!(
        graph.blocks.len(),
        32,
        "default WaterBased heating/cooling variant should keep both optional branches"
    );
    let instances: Vec<&str> = graph
        .blocks
        .iter()
        .map(|block| block.instance_iri.as_deref().expect("source path"))
        .collect();
    for suffix in [
        ".cooSupTemDif",
        ".greThr",
        ".truDel",
        ".lat",
        ".intSwi3",
        ".heaSupTemDif",
        ".truDel2",
        ".lat3",
        ".intSwi1",
    ] {
        assert!(
            instances.iter().any(|iri| iri.ends_with(suffix)),
            "missing source component {suffix}"
        );
    }

    assert!(
        block_param(&graph, ".greThr", "t").bit_eq(&Value::Real(3.0)),
        "chilled-water 3-request threshold must ground exactly"
    );
    assert!(
        block_param(&graph, ".greThr", "h").bit_eq(&Value::Real(0.1)),
        "temperature hysteresis must inherit THys"
    );
    assert!(
        block_param(&graph, ".truDel", "delayTime").bit_eq(&Value::Real(120.0)),
        "chilled-water reset delay must be 120 seconds"
    );
    assert!(
        block_param(&graph, ".truDel2", "delayTime").bit_eq(&Value::Real(300.0)),
        "hot-water reset delay must be 300 seconds"
    );
    assert!(
        block_param(&graph, ".greThr5", "t").bit_eq(&Value::Real(0.95)),
        "valve request set threshold must ground exactly"
    );
    assert!(
        block_param(&graph, ".lesThr3", "t").bit_eq(&Value::Real(0.1)),
        "plant-request clear threshold must ground exactly"
    );
    assert!(
        block_param(&graph, ".thr", "k").bit_eq(&Value::Integer(3)),
        "shared integer constant 3 must feed both reset ladders"
    );
    assert!(
        block_param(&graph, ".zer", "k").bit_eq(&Value::Integer(0)),
        "shared integer zero constant must feed all reset fallbacks"
    );

    let counts = connector_iri_counts(&graph);
    assert!(counts.contains(&(format!("{G36_PLANT_REQUESTS_MODEL}.TAirSup"), 2)));
    assert!(counts.contains(&(format!("{G36_PLANT_REQUESTS_MODEL}.TAirSupSet"), 2)));
    assert!(counts.contains(&(format!("{G36_PLANT_REQUESTS_MODEL}.uCooCoiSet"), 3)));
    assert!(counts.contains(&(format!("{G36_PLANT_REQUESTS_MODEL}.uHeaCoiSet"), 3)));
}

#[test]
fn golden_g36_multizone_vav_plant_requests_modelgraph() {
    let actual = render(&import_ok(G36_PLANT_REQUESTS));
    let path = golden_path(G36_PLANT_REQUESTS_GOLDEN_REL);
    if std::env::var_os("OCE_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(path)
        .expect("golden snapshot missing; regenerate with OCE_BLESS=1");
    assert_eq!(
        actual, expected,
        "source-verified G36 PlantRequests ModelGraph diverged from golden"
    );
}
