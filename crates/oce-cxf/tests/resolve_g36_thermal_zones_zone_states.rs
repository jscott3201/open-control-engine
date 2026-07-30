//! Source-verified G36 ThermalZones.ZoneStates composite import tests.
//!
//! Upstream `ZoneStates.mo` lines 58-62 bind `hysU.uLow=-uLow` and
//! `hysU.uHigh=uLow`. The fixture pre-grounds these expressions to `-0.01` and `+0.01`.
//! Pre-grounding the upper threshold preserves Modelica's enclosing-scope meaning: the resolver's
//! latest-wins `ParamScope` would otherwise shadow the top-level
//! `uLow=+0.01` with the earlier sibling `hysU.uLow=-0.01`. Resolver-design follow-up
//! `019f5431-047a` tracks an explicit enclosing-scope representation. The three enum literals
//! bound at `BooleanToInteger.integerTrue` must ground position-independently to Integer values
//! 1, 2, and 3.

mod bless;

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::PathBuf;

use oce_cxf::{ResolveOptions, import_cxf};
use oce_model::{ModelGraph, Value};
use serde_json::{Value as JsonValue, json};

const G36_THERMAL_ZONES_ZONE_STATES: &str =
    include_str!("fixtures/g36/thermal_zones_zone_states.jsonld");
const G36_THERMAL_ZONES_ZONE_STATES_GOLDEN_REL: &str =
    "tests/fixtures/golden/g36_thermal_zones_zone_states.modelgraph.txt";
const G36_THERMAL_ZONES_ZONE_STATES_MODEL: &str =
    "http://example.org#g36.source.thermal_zones_zone_states";
const G36_THERMAL_ZONES_ZONE_STATES_CLASS: &str =
    "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.ThermalZones.ZoneStates";

fn import_ok(src: &str) -> ModelGraph {
    let (graph, report) = import_cxf(src.as_bytes(), &ResolveOptions::default())
        .expect("G36 ThermalZones ZoneStates fixture should import");
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
fn source_verified_thermal_zones_zone_states_preserves_topology_and_grounded_bindings() {
    let parsed: JsonValue =
        serde_json::from_str(G36_THERMAL_ZONES_ZONE_STATES).expect("G36 fixture JSON");
    let top = parsed["@graph"]
        .as_array()
        .expect("@graph array")
        .iter()
        .find(|node| node["@id"] == json!(G36_THERMAL_ZONES_ZONE_STATES_MODEL))
        .expect("top G36 ThermalZones ZoneStates composite node");
    assert_eq!(top["@type"], json!(G36_THERMAL_ZONES_ZONE_STATES_CLASS));
    assert_eq!(
        top["S231:hasParameter"]
            .as_array()
            .expect("parameters")
            .len(),
        2
    );
    assert_eq!(
        top["S231:containsBlock"]
            .as_array()
            .expect("children")
            .len(),
        13
    );
    assert_eq!(top["S231:hasInput"].as_array().expect("inputs").len(), 2);
    assert_eq!(
        top["S231:hasOutput"]["@id"],
        json!("http://example.org#g36.source.thermal_zones_zone_states.yZonSta")
    );

    let graph = import_ok(G36_THERMAL_ZONES_ZONE_STATES);
    assert_eq!(graph.blocks.len(), 13);
    assert_eq!(graph.connections.len(), 15);
    assert_eq!(
        graph.external_inputs.len(),
        4,
        "both top-level inputs fan out to two leaf inputs"
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
            "http://example.org#g36.source.thermal_zones_zone_states.uHea",
            "http://example.org#g36.source.thermal_zones_zone_states.uCoo",
        ])
    );

    let instances = graph
        .blocks
        .iter()
        .map(|block| block.instance_iri.as_deref().expect("source path"))
        .collect::<Vec<_>>();
    for suffix in [
        ".booToIntHea",
        ".booToIntCoo",
        ".isDea",
        ".booToIntDea",
        ".isHea",
        ".hysUHea",
        ".hysUCoo",
        ".uHeaMinUCoo",
        ".isCoo",
        ".hysU",
        ".notHea",
        ".addInt",
        ".addInt1",
    ] {
        assert!(
            instances.iter().any(|iri| iri.ends_with(suffix)),
            "missing source component {suffix}"
        );
    }

    for (suffix, expected) in [
        (".booToIntHea", 1),
        (".booToIntDea", 2),
        (".booToIntCoo", 3),
    ] {
        assert!(
            block_param(&graph, suffix, "integerTrue").bit_eq(&Value::Integer(expected)),
            "{suffix}.integerTrue must ground the ZoneStates literal"
        );
        assert!(
            block_param(&graph, suffix, "integerFalse").bit_eq(&Value::Integer(0)),
            "{suffix}.integerFalse"
        );
    }

    for (suffix, expected_low, expected_high) in [
        (".hysUHea", 0.01, 0.05),
        (".hysUCoo", 0.01, 0.05),
        (".hysU", -0.01, 0.01),
    ] {
        assert!(
            block_param(&graph, suffix, "uLow").bit_eq(&Value::Real(expected_low)),
            "{suffix}.uLow"
        );
        assert!(
            block_param(&graph, suffix, "uHigh").bit_eq(&Value::Real(expected_high)),
            "{suffix}.uHigh"
        );
        assert!(
            block_param(&graph, suffix, "pre_y_start").bit_eq(&Value::Boolean(false)),
            "{suffix}.pre_y_start"
        );
    }
}

#[test]
fn thermal_zones_zone_states_modelgraph_is_stable() {
    let actual = render(&import_ok(G36_THERMAL_ZONES_ZONE_STATES));
    let path = golden_path(G36_THERMAL_ZONES_ZONE_STATES_GOLDEN_REL);
    if bless::enabled() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(path)
        .expect("golden snapshot missing; regenerate with OCE_BLESS=1");
    assert_eq!(
        actual, expected,
        "source-verified G36 ThermalZones ZoneStates ModelGraph diverged from golden"
    );
}
