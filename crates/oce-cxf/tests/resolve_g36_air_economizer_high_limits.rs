//! Source-verified G36 Generic.AirEconomizerHighLimits composite import tests.

use std::fmt::Write as _;
use std::path::PathBuf;

use oce_cxf::{ResolveOptions, import_cxf};
use oce_model::{ModelGraph, Value};
use serde_json::{Value as JsonValue, json};

const G36_HIGH_LIMIT_FIXED_24: &str =
    include_str!("fixtures/g36/generic_air_economizer_high_limits_ashrae_fixed_24.jsonld");
const G36_HIGH_LIMIT_FIXED_21: &str =
    include_str!("fixtures/g36/generic_air_economizer_high_limits_ashrae_fixed_21.jsonld");
const G36_HIGH_LIMIT_FIXED_18: &str =
    include_str!("fixtures/g36/generic_air_economizer_high_limits_ashrae_fixed_18.jsonld");
const G36_HIGH_LIMIT_TITLE24_FIXED_24: &str =
    include_str!("fixtures/g36/generic_air_economizer_high_limits_title24_fixed_24.jsonld");
const G36_HIGH_LIMIT_TITLE24_FIXED_23: &str =
    include_str!("fixtures/g36/generic_air_economizer_high_limits_title24_fixed_23.jsonld");
const G36_HIGH_LIMIT_TITLE24_FIXED_22: &str =
    include_str!("fixtures/g36/generic_air_economizer_high_limits_title24_fixed_22.jsonld");
const G36_HIGH_LIMIT_TITLE24_FIXED_21: &str =
    include_str!("fixtures/g36/generic_air_economizer_high_limits_title24_fixed_21.jsonld");
const G36_HIGH_LIMIT_CLASS: &str =
    "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.Generic.AirEconomizerHighLimits";

#[derive(Clone, Copy)]
struct Case {
    source: &'static str,
    model: &'static str,
    golden_rel: &'static str,
    child: &'static str,
    cutoff: f64,
    climate_zone: &'static str,
}

const CASES: &[Case] = &[
    Case {
        source: G36_HIGH_LIMIT_FIXED_24,
        model: "http://example.org#g36.source.generic_air_economizer_high_limits_ashrae_fixed_24",
        golden_rel: "tests/fixtures/golden/g36_generic_air_economizer_high_limits_ashrae_fixed_24.modelgraph.txt",
        child: ".con",
        cutoff: 297.15,
        climate_zone: "Zone_1B",
    },
    Case {
        source: G36_HIGH_LIMIT_FIXED_21,
        model: "http://example.org#g36.source.generic_air_economizer_high_limits_ashrae_fixed_21",
        golden_rel: "tests/fixtures/golden/g36_generic_air_economizer_high_limits_ashrae_fixed_21.modelgraph.txt",
        child: ".con1",
        cutoff: 294.15,
        climate_zone: "Zone_5A",
    },
    Case {
        source: G36_HIGH_LIMIT_FIXED_18,
        model: "http://example.org#g36.source.generic_air_economizer_high_limits_ashrae_fixed_18",
        golden_rel: "tests/fixtures/golden/g36_generic_air_economizer_high_limits_ashrae_fixed_18.modelgraph.txt",
        child: ".con2",
        cutoff: 291.15,
        climate_zone: "Zone_1A",
    },
    Case {
        source: G36_HIGH_LIMIT_TITLE24_FIXED_24,
        model: "http://example.org#g36.source.generic_air_economizer_high_limits_title24_fixed_24",
        golden_rel: "tests/fixtures/golden/g36_generic_air_economizer_high_limits_title24_fixed_24.modelgraph.txt",
        child: ".con5",
        cutoff: 297.15,
        climate_zone: "Title24 Zone_1",
    },
    Case {
        source: G36_HIGH_LIMIT_TITLE24_FIXED_23,
        model: "http://example.org#g36.source.generic_air_economizer_high_limits_title24_fixed_23",
        golden_rel: "tests/fixtures/golden/g36_generic_air_economizer_high_limits_title24_fixed_23.modelgraph.txt",
        child: ".con6",
        cutoff: 296.15,
        climate_zone: "Title24 Zone_2",
    },
    Case {
        source: G36_HIGH_LIMIT_TITLE24_FIXED_22,
        model: "http://example.org#g36.source.generic_air_economizer_high_limits_title24_fixed_22",
        golden_rel: "tests/fixtures/golden/g36_generic_air_economizer_high_limits_title24_fixed_22.modelgraph.txt",
        child: ".con7",
        cutoff: 295.15,
        climate_zone: "Title24 Zone_6",
    },
    Case {
        source: G36_HIGH_LIMIT_TITLE24_FIXED_21,
        model: "http://example.org#g36.source.generic_air_economizer_high_limits_title24_fixed_21",
        golden_rel: "tests/fixtures/golden/g36_generic_air_economizer_high_limits_title24_fixed_21.modelgraph.txt",
        child: ".con8",
        cutoff: 294.15,
        climate_zone: "Title24 Zone_7",
    },
];

fn import_ok(src: &str) -> ModelGraph {
    let (graph, report) = import_cxf(src.as_bytes(), &ResolveOptions::default())
        .expect("G36 Generic.AirEconomizerHighLimits fixture should import");
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
fn source_verified_g36_air_economizer_high_limits_imports_fixed_dry_bulb_buckets() {
    for case in CASES {
        let parsed: JsonValue =
            serde_json::from_str(case.source).expect("G36 high-limit fixture JSON");
        let top = parsed["@graph"]
            .as_array()
            .expect("@graph array")
            .iter()
            .find(|node| node["@id"] == json!(case.model))
            .expect("top G36 Generic.AirEconomizerHighLimits composite node");
        assert_eq!(top["@type"], json!(G36_HIGH_LIMIT_CLASS));
        assert_eq!(
            top["S231:containsBlock"]
                .as_array()
                .expect("children")
                .len(),
            1,
            "fixed dry-bulb bucket should keep only the selected source constant"
        );
        assert!(
            top.get("S231:hasInput").is_none(),
            "fixed dry-bulb high-limit bucket should have no active runtime inputs"
        );
        assert_eq!(
            top["S231:hasOutput"],
            json!({"@id": format!("{}.TCut", case.model)})
        );

        let graph = import_ok(case.source);
        assert_eq!(
            graph.blocks.len(),
            1,
            "fixed dry-bulb bucket should import one active source child"
        );
        assert!(
            block_param(&graph, case.child, "k").bit_eq(&Value::Real(case.cutoff)),
            "{} should map to source cutoff {} K",
            case.climate_zone,
            case.cutoff
        );
    }
}

#[test]
fn golden_g36_air_economizer_high_limits_modelgraphs() {
    for case in CASES {
        let actual = render(&import_ok(case.source));
        let path = golden_path(case.golden_rel);
        if std::env::var_os("OCE_BLESS").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, &actual).unwrap();
            continue;
        }
        let expected = std::fs::read_to_string(path)
            .expect("golden snapshot missing; regenerate with OCE_BLESS=1");
        assert_eq!(
            actual, expected,
            "source-verified G36 Generic.AirEconomizerHighLimits ModelGraph diverged from golden"
        );
    }
}
