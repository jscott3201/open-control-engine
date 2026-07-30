//! Source-verified G36 Economizers.Controller composite import tests.

mod bless;

use std::fmt::Write as _;
use std::path::PathBuf;

use oce_cxf::{ResolveOptions, import_cxf};
use oce_model::{ModelGraph, Value};
use serde_json::{Value as JsonValue, json};

const G36_ECONOMIZER_CONTROLLER: &str = include_str!(
    "fixtures/g36/multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.jsonld"
);
const G36_ECONOMIZER_CONTROLLER_GOLDEN_REL: &str = "tests/fixtures/golden/g36_multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.modelgraph.txt";
const G36_ECONOMIZER_CONTROLLER_MODEL: &str = "http://example.org#g36.source.multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21";
const G36_ECONOMIZER_CONTROLLER_CLASS: &str = "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.Economizers.Controller";

fn import_ok(src: &str) -> ModelGraph {
    let (graph, report) = import_cxf(src.as_bytes(), &ResolveOptions::default())
        .expect("G36 Economizers.Controller fixture should import");
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
fn source_verified_g36_multizone_vav_economizer_controller_imports_restricted_variant() {
    let parsed: JsonValue =
        serde_json::from_str(G36_ECONOMIZER_CONTROLLER).expect("G36 fixture JSON");
    let top = parsed["@graph"]
        .as_array()
        .expect("@graph array")
        .iter()
        .find(|node| node["@id"] == json!(G36_ECONOMIZER_CONTROLLER_MODEL))
        .expect("top G36 Economizers.Controller composite node");
    assert_eq!(top["@type"], json!(G36_ECONOMIZER_CONTROLLER_CLASS));
    assert_eq!(
        top["S231:containsBlock"]
            .as_array()
            .expect("children")
            .len(),
        4,
        "restricted controller variant should keep only damLim, enaDis, modRel, and ecoHigLim"
    );
    assert_eq!(
        top["S231:hasInput"].as_array().expect("inputs").len(),
        7,
        "single-damper fixed-dry-bulb variant should expose only active controller inputs"
    );
    assert_eq!(
        top["S231:hasOutput"].as_array().expect("outputs").len(),
        4,
        "single-damper relief-damper variant should expose only active controller outputs"
    );

    let graph = import_ok(G36_ECONOMIZER_CONTROLLER);
    assert_eq!(
        graph.blocks.len(),
        44,
        "restricted controller should flatten 16 Limits.Common leaves, 19 Enable leaves, 8 Reliefs leaves, and one high-limit constant"
    );
    let instances: Vec<&str> = graph
        .blocks
        .iter()
        .map(|block| block.instance_iri.as_deref().expect("source path"))
        .collect();
    assert!(
        instances
            .iter()
            .any(|iri| iri.ends_with(".damLim.damLimCon"))
    );
    assert!(
        instances
            .iter()
            .any(|iri| iri.ends_with(".enaDis.truFalHol"))
    );
    assert!(
        instances
            .iter()
            .any(|iri| iri.ends_with(".modRel.outDamPos"))
    );
    assert!(instances.iter().any(|iri| iri.ends_with(".ecoHigLim.con1")));
    assert!(
        !instances.iter().any(|iri| iri.contains(".sepAFMS.")),
        "DedicatedDampersAirflow branch must remain inactive"
    );
    assert!(
        !instances.iter().any(|iri| iri.contains(".sepDp.")),
        "DedicatedDampersPressure branch must remain inactive"
    );
    assert!(
        !instances.iter().any(|iri| iri.contains(".modRet.")),
        "return-fan pressure-control branch must remain inactive"
    );

    assert!(
        block_param(&graph, ".ecoHigLim.con1", "k").bit_eq(&Value::Real(294.15)),
        "ASHRAE 90.1 Zone_5A fixed dry-bulb high limit must ground to 294.15 K"
    );
    assert!(
        block_param(&graph, ".damLim.damLimCon", "k").bit_eq(&Value::Real(1.0)),
        "common-damper controller gain must inherit kMinOA"
    );
    assert!(
        block_param(&graph, ".enaDis.delOutDamOsc", "delayTime").bit_eq(&Value::Real(15.0)),
        "disable delay must inherit disDel"
    );
    assert!(
        block_param(&graph, ".modRel.outDamMinLimSig", "k").bit_eq(&Value::Real(-0.25)),
        "relief modulation lower signal bound must inherit uHeaMax"
    );

    let counts = connector_iri_counts(&graph);
    assert!(counts.contains(&(format!("{G36_ECONOMIZER_CONTROLLER_MODEL}.u1SupFan"), 3)));
    assert!(counts.contains(&(format!("{G36_ECONOMIZER_CONTROLLER_MODEL}.uOpeMod"), 1)));
    assert!(counts.contains(&(format!("{G36_ECONOMIZER_CONTROLLER_MODEL}.uTSup"), 2)));
    assert!(counts.contains(&(format!("{G36_ECONOMIZER_CONTROLLER_MODEL}.TOut"), 1)));
    assert!(
        !counts.iter().any(|(iri, _)| iri.ends_with(".hAirOut")),
        "fixed dry-bulb high-limit variant must not expose enthalpy inputs"
    );
}

#[test]
fn golden_g36_multizone_vav_economizer_controller_modelgraph() {
    let actual = render(&import_ok(G36_ECONOMIZER_CONTROLLER));
    let path = golden_path(G36_ECONOMIZER_CONTROLLER_GOLDEN_REL);
    if bless::enabled() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(path)
        .expect("golden snapshot missing; regenerate with OCE_BLESS=1");
    assert_eq!(
        actual, expected,
        "source-verified G36 Economizers.Controller ModelGraph diverged from golden"
    );
}
