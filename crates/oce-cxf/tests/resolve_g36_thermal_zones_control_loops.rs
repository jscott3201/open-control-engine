//! Source-verified G36 ThermalZones.ControlLoops composite import tests.
//!
//! Pinned `ControlLoops.mo` lines 56-69 bind two PI `PIDWithReset` blocks with opposite acting
//! directions: cooling explicitly uses `reverseActing=false`, while heating uses the upstream
//! default `true`, made explicit in the fixture. Lines 109-116 bind each `LessThreshold.h` to
//! `0.8*looHys`, pre-grounded here to `0.008`. The reset targets and both Boolean-to-Real pairs
//! use typed Real whole-number literals so their exact IEEE-754 encodings remain observable.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::PathBuf;

use oce_cxf::{ResolveOptions, import_cxf};
use oce_model::{EnumClassId, ModelGraph, Value};
use serde_json::{Value as JsonValue, json};

const G36_THERMAL_ZONES_CONTROL_LOOPS: &str =
    include_str!("fixtures/g36/thermal_zones_control_loops.jsonld");
const G36_THERMAL_ZONES_CONTROL_LOOPS_GOLDEN_REL: &str =
    "tests/fixtures/golden/g36_thermal_zones_control_loops.modelgraph.txt";
const G36_THERMAL_ZONES_CONTROL_LOOPS_MODEL: &str =
    "http://example.org#g36.source.thermal_zones_control_loops";
const G36_THERMAL_ZONES_CONTROL_LOOPS_CLASS: &str =
    "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.ThermalZones.ControlLoops";

fn import_ok(src: &str) -> ModelGraph {
    let (graph, report) = import_cxf(src.as_bytes(), &ResolveOptions::default())
        .expect("G36 ThermalZones ControlLoops fixture should import");
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
fn thermal_zone_control_loop_topology_and_bindings_match_pinned_source() {
    let parsed: JsonValue =
        serde_json::from_str(G36_THERMAL_ZONES_CONTROL_LOOPS).expect("G36 fixture JSON");
    let top = parsed["@graph"]
        .as_array()
        .expect("@graph array")
        .iter()
        .find(|node| node["@id"] == json!(G36_THERMAL_ZONES_CONTROL_LOOPS_MODEL))
        .expect("top G36 ThermalZones ControlLoops composite node");
    assert_eq!(top["@type"], json!(G36_THERMAL_ZONES_CONTROL_LOOPS_CLASS));
    assert_eq!(
        top["S231:hasParameter"]
            .as_array()
            .expect("parameters")
            .len(),
        7
    );
    assert_eq!(
        top["S231:containsBlock"]
            .as_array()
            .expect("children")
            .len(),
        16
    );
    assert_eq!(top["S231:hasInput"].as_array().expect("inputs").len(), 3);
    assert_eq!(top["S231:hasOutput"].as_array().expect("outputs").len(), 2);

    let graph = import_ok(G36_THERMAL_ZONES_CONTROL_LOOPS);
    assert_eq!(graph.blocks.len(), 16);
    assert_eq!(
        graph.external_inputs.len(),
        8,
        "the three top-level inputs fan out to eight leaf inputs"
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
            "http://example.org#g36.source.thermal_zones_control_loops.TCooSet",
            "http://example.org#g36.source.thermal_zones_control_loops.TZon",
            "http://example.org#g36.source.thermal_zones_control_loops.THeaSet",
        ])
    );

    let instances = graph
        .blocks
        .iter()
        .map(|block| block.instance_iri.as_deref().expect("source path"))
        .collect::<Vec<_>>();
    for suffix in [
        ".conCoo",
        ".conHea",
        ".enaHeaLoo",
        ".enaCooLoo",
        ".disCoo",
        ".colZon",
        ".zerCoo",
        ".cooConSig",
        ".holZon",
        ".disHea",
        ".zerHea",
        ".heaConSig",
        ".zerCon",
        ".zerCon1",
        ".disCooCon",
        ".disHeaCon",
    ] {
        assert!(
            instances.iter().any(|iri| iri.ends_with(suffix)),
            "missing source component {suffix}"
        );
    }

    for suffix in [".conCoo", ".conHea"] {
        assert!(
            block_param(&graph, suffix, "controllerType").bit_eq(&Value::Enum {
                class: EnumClassId::SIMPLE_CONTROLLER,
                ordinal: 2,
            }),
            "{suffix}.controllerType"
        );
        assert!(
            block_param(&graph, suffix, "k").bit_eq(&Value::Real(0.1)),
            "{suffix}.k"
        );
        assert!(
            block_param(&graph, suffix, "Ti").bit_eq(&Value::Real(900.0)),
            "{suffix}.Ti"
        );
        assert!(
            block_param(&graph, suffix, "y_reset").bit_eq(&Value::Real(0.0)),
            "{suffix}.y_reset must remain typed Real zero"
        );
    }
    assert!(block_param(&graph, ".conCoo", "reverseActing").bit_eq(&Value::Boolean(false)));
    assert!(block_param(&graph, ".conHea", "reverseActing").bit_eq(&Value::Boolean(true)));

    for suffix in [".enaCooLoo", ".enaHeaLoo"] {
        assert!(
            block_param(&graph, suffix, "h").bit_eq(&Value::Real(0.25)),
            "{suffix}.h"
        );
    }
    for suffix in [".zerCon", ".zerCon1"] {
        assert!(
            block_param(&graph, suffix, "t").bit_eq(&Value::Real(0.01)),
            "{suffix}.t"
        );
        assert!(
            block_param(&graph, suffix, "h").bit_eq(&Value::Real(0.008)),
            "{suffix}.h"
        );
    }
    for suffix in [".disCoo", ".disHea"] {
        assert!(
            block_param(&graph, suffix, "delayTime").bit_eq(&Value::Real(30.0)),
            "{suffix}.delayTime"
        );
        assert!(
            block_param(&graph, suffix, "delayOnInit").bit_eq(&Value::Boolean(false)),
            "{suffix}.delayOnInit"
        );
    }
    for suffix in [".zerCoo", ".zerHea"] {
        assert!(
            block_param(&graph, suffix, "realTrue").bit_eq(&Value::Real(0.0)),
            "{suffix}.realTrue must remain Real(0x0000000000000000)"
        );
        assert!(
            block_param(&graph, suffix, "realFalse").bit_eq(&Value::Real(1.0)),
            "{suffix}.realFalse must remain Real(0x3ff0000000000000)"
        );
    }
}

#[test]
fn thermal_zone_control_loop_modelgraph_is_stable() {
    let actual = render(&import_ok(G36_THERMAL_ZONES_CONTROL_LOOPS));
    let path = golden_path(G36_THERMAL_ZONES_CONTROL_LOOPS_GOLDEN_REL);
    if std::env::var_os("OCE_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(path)
        .expect("golden snapshot missing; regenerate with OCE_BLESS=1");
    assert_eq!(
        actual, expected,
        "source-verified G36 ThermalZones ControlLoops ModelGraph diverged from golden"
    );
}
