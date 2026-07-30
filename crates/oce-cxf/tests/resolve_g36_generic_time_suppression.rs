//! Source-verified G36 Generic.TimeSuppression composite import tests.
//!
//! Upstream `TimeSuppression.mo` lines 72-74 bind `greThr.h=0.5*dTHys`. The fixture
//! pre-grounds that expression to `0.125` from the source-default `dTHys=0.25` binding while
//! retaining references for every direct parameter binding.

mod bless;

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::PathBuf;

use oce_cxf::{ResolveOptions, import_cxf};
use oce_model::{ModelGraph, Value};
use serde_json::{Value as JsonValue, json};

const G36_GENERIC_TIME_SUPPRESSION: &str =
    include_str!("fixtures/g36/generic_time_suppression.jsonld");
const G36_GENERIC_TIME_SUPPRESSION_GOLDEN_REL: &str =
    "tests/fixtures/golden/g36_generic_time_suppression.modelgraph.txt";
const G36_GENERIC_TIME_SUPPRESSION_MODEL: &str =
    "http://example.org#g36.source.generic_time_suppression";
const G36_GENERIC_TIME_SUPPRESSION_CLASS: &str =
    "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.Generic.TimeSuppression";

fn import_ok(src: &str) -> ModelGraph {
    let (graph, report) = import_cxf(src.as_bytes(), &ResolveOptions::default())
        .expect("G36 Generic TimeSuppression fixture should import");
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
fn source_verified_generic_time_suppression_preserves_topology_and_grounded_bindings() {
    let parsed: JsonValue =
        serde_json::from_str(G36_GENERIC_TIME_SUPPRESSION).expect("G36 fixture JSON");
    let top = parsed["@graph"]
        .as_array()
        .expect("@graph array")
        .iter()
        .find(|node| node["@id"] == json!(G36_GENERIC_TIME_SUPPRESSION_MODEL))
        .expect("top G36 Generic TimeSuppression composite node");
    assert_eq!(top["@type"], json!(G36_GENERIC_TIME_SUPPRESSION_CLASS));
    assert_eq!(
        top["S231:hasParameter"]
            .as_array()
            .expect("parameters")
            .len(),
        4
    );
    assert_eq!(
        top["S231:containsBlock"]
            .as_array()
            .expect("children")
            .len(),
        24
    );
    assert_eq!(top["S231:hasInput"].as_array().expect("inputs").len(), 2);
    assert_eq!(
        top["S231:hasOutput"]["@id"],
        json!("http://example.org#g36.source.generic_time_suppression.yAftSup")
    );

    let graph = import_ok(G36_GENERIC_TIME_SUPPRESSION);
    assert_eq!(graph.blocks.len(), 24);
    assert_eq!(
        graph.external_inputs.len(),
        3,
        "TSet fans out to two leaf inputs while TZon drives one"
    );
    let external_input_iris: BTreeSet<&str> = graph
        .external_inputs
        .iter()
        .map(|connector_id| {
            graph.connectors[connector_id.0 as usize]
                .iri
                .as_deref()
                .expect("external input source IRI")
        })
        .collect();
    assert_eq!(
        external_input_iris,
        BTreeSet::from([
            "http://example.org#g36.source.generic_time_suppression.TSet",
            "http://example.org#g36.source.generic_time_suppression.TZon",
        ])
    );
    let instances = graph
        .blocks
        .iter()
        .map(|block| block.instance_iri.as_deref().expect("source path"))
        .collect::<Vec<_>>();
    for suffix in [
        ".supTim",
        ".samSet",
        ".uniDel",
        ".abs1",
        ".triSam",
        ".edg",
        ".lat",
        ".lat1",
        ".tim",
        ".greThr",
        ".gai",
        ".sub1",
        ".conZer",
        ".maxSupTim",
        ".con5",
        ".swi",
        ".pre1",
        ".pasSupTim",
        ".pasSup",
        ".temDif",
        ".triSam1",
        ".abs2",
        ".truDel",
        ".con1",
    ] {
        assert!(
            instances.iter().any(|iri| iri.ends_with(suffix)),
            "missing source component {suffix}"
        );
    }

    for (suffix, name, expected) in [
        (".greThr", "h", 0.125),
        (".greThr", "t", 0.25),
        (".gai", "k", 540.0),
        (".maxSupTim", "k", 1800.0),
        (".samSet", "samplePeriod", 120.0),
        (".uniDel", "samplePeriod", 120.0),
        (".truDel", "delayTime", 120.0),
    ] {
        assert!(
            block_param(&graph, suffix, name).bit_eq(&Value::Real(expected)),
            "{suffix}.{name}"
        );
    }

    for (suffix, name) in [
        (".conZer", "k"),
        (".pasSup", "h"),
        (".uniDel", "y_start"),
        (".triSam", "y_start"),
        (".triSam1", "y_start"),
        (".tim", "t"),
    ] {
        assert!(
            block_param(&graph, suffix, name).bit_eq(&Value::Real(0.0)),
            "{suffix}.{name} must remain a typed Real zero"
        );
    }

    assert!(block_param(&graph, ".truDel", "delayOnInit").bit_eq(&Value::Boolean(true)));
    assert!(block_param(&graph, ".pre1", "pre_u_start").bit_eq(&Value::Boolean(false)));
    assert!(block_param(&graph, ".con1", "k").bit_eq(&Value::Boolean(true)));
    assert!(block_param(&graph, ".con5", "k").bit_eq(&Value::Boolean(true)));
}

#[test]
fn generic_time_suppression_modelgraph_is_stable() {
    let actual = render(&import_ok(G36_GENERIC_TIME_SUPPRESSION));
    let path = golden_path(G36_GENERIC_TIME_SUPPRESSION_GOLDEN_REL);
    if bless::enabled() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(path)
        .expect("golden snapshot missing; regenerate with OCE_BLESS=1");
    assert_eq!(
        actual, expected,
        "source-verified G36 Generic TimeSuppression ModelGraph diverged from golden"
    );
}
