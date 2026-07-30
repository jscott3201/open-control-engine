//! Nested composite import tests for the restricted G36 CXF profile subset.

mod bless;

use std::fmt::Write as _;
use std::path::PathBuf;

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::DiagCode;
use oce_model::{EnumClassId, ModelGraph, Value};
use serde_json::{Value as JsonValue, json};

const NESTED: &str = include_str!("fixtures/nested_composite.jsonld");
const G36_TRIM_AND_RESPOND: &str =
    include_str!("fixtures/g36/trim_and_respond_have_hol_false.jsonld");
const G36_SUPPLY_TEMPERATURE: &str =
    include_str!("fixtures/g36/multizone_vav_supply_temperature.jsonld");
const G36_SUPPLY_FAN: &str = include_str!("fixtures/g36/multizone_vav_supply_fan.jsonld");
const G36_SUPPLY_SIGNALS: &str = include_str!("fixtures/g36/multizone_vav_supply_signals.jsonld");
const NESTED_GOLDEN_REL: &str = "tests/fixtures/golden/nested_composite.modelgraph.txt";
const G36_TRIM_AND_RESPOND_GOLDEN_REL: &str =
    "tests/fixtures/golden/g36_trim_and_respond_have_hol_false.modelgraph.txt";
const G36_SUPPLY_TEMPERATURE_GOLDEN_REL: &str =
    "tests/fixtures/golden/g36_multizone_vav_supply_temperature.modelgraph.txt";
const G36_SUPPLY_FAN_GOLDEN_REL: &str =
    "tests/fixtures/golden/g36_multizone_vav_supply_fan.modelgraph.txt";
const G36_SUPPLY_SIGNALS_GOLDEN_REL: &str =
    "tests/fixtures/golden/g36_multizone_vav_supply_signals.modelgraph.txt";
const MODEL: &str = "http://example.org#g36.profile.nested_composite";
const G36_TRIM_AND_RESPOND_MODEL: &str =
    "http://example.org#g36.source.trim_and_respond_have_hol_false";
const G36_TRIM_AND_RESPOND_CLASS: &str =
    "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.Generic.TrimAndRespond";
const G36_SUPPLY_TEMPERATURE_MODEL: &str =
    "http://example.org#g36.source.multizone_vav_supply_temperature";
const G36_SUPPLY_TEMPERATURE_CLASS: &str = "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.SupplyTemperature";
const G36_SUPPLY_FAN_MODEL: &str = "http://example.org#g36.source.multizone_vav_supply_fan";
const G36_SUPPLY_FAN_CLASS: &str =
    "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.SupplyFan";
const G36_SUPPLY_SIGNALS_MODEL: &str = "http://example.org#g36.source.multizone_vav_supply_signals";
const G36_SUPPLY_SIGNALS_CLASS: &str = "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.SupplySignals";

fn import_ok(src: &str) -> ModelGraph {
    let (graph, report) = import_cxf(src.as_bytes(), &ResolveOptions::default())
        .expect("nested composite fixture should import");
    assert!(
        report.is_empty(),
        "fixture should not warn: {:?}",
        report.diagnostics
    );
    graph
}

fn import_doc(doc: &JsonValue) -> Result<ModelGraph, CxfError> {
    let bytes = serde_json::to_vec(doc).expect("serialize fixture");
    import_cxf(&bytes, &ResolveOptions::default()).map(|(graph, report)| {
        assert!(
            report.is_empty(),
            "fixture should not warn: {:?}",
            report.diagnostics
        );
        graph
    })
}

fn validation_codes(doc: &JsonValue) -> Vec<DiagCode> {
    let bytes = serde_json::to_vec(doc).expect("serialize fixture");
    match import_cxf(&bytes, &ResolveOptions::default()) {
        Err(CxfError::Validation(diags)) => diags.iter().map(|diag| diag.code).collect(),
        other => panic!("expected validation error, got {other:?}"),
    }
}

fn node_mut<'a>(doc: &'a mut JsonValue, suffix: &str) -> &'a mut JsonValue {
    doc["@graph"]
        .as_array_mut()
        .expect("@graph array")
        .iter_mut()
        .find(|node| node["@id"].as_str().is_some_and(|id| id.ends_with(suffix)))
        .unwrap_or_else(|| panic!("missing node ending in {suffix:?}"))
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
fn nested_composite_flattens_to_leaf_blocks_and_expanded_connections() {
    let graph = import_ok(NESTED);
    let instances: Vec<&str> = graph
        .blocks
        .iter()
        .map(|block| block.instance_iri.as_deref().expect("source path"))
        .collect();
    assert_eq!(
        instances,
        vec![
            format!("{MODEL}.sub.gain"),
            format!("{MODEL}.sub.enumCarrier"),
            format!("{MODEL}.post"),
        ]
    );
    assert_eq!(graph.external_inputs, vec![oce_model::ConnectorId(0)]);
    assert_eq!(
        graph.connectors[0].iri.as_deref(),
        Some(format!("{MODEL}.u").as_str()),
        "nested boundary input should collapse to the top boundary input path"
    );
    assert_eq!(
        graph
            .connections
            .iter()
            .map(|edge| (edge.from.0, edge.to.0))
            .collect::<Vec<_>>(),
        vec![(1, 3)],
        "sub.gain.y should drive post.u after nested boundary-output expansion"
    );

    let gain_k = &graph.blocks[0].params.values[0].1;
    assert!(gain_k.bit_eq(&Value::Real(0.5)));
    let extra = graph.blocks[1]
        .params
        .values
        .iter()
        .find(|(name, _)| name.as_ref() == "extra")
        .expect("enum propagation proof")
        .1
        .clone();
    assert!(extra.bit_eq(&Value::Enum {
        class: EnumClassId::G36_VENTILATION_STANDARD,
        ordinal: 2,
    }));
}

#[test]
fn golden_nested_composite_modelgraph() {
    let actual = render(&import_ok(NESTED));
    let path = golden_path(NESTED_GOLDEN_REL);
    if bless::enabled() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(path)
        .expect("golden snapshot missing; regenerate with OCE_BLESS=1");
    assert_eq!(
        actual, expected,
        "nested composite ModelGraph diverged from golden"
    );
}

#[test]
fn source_verified_g36_trim_and_respond_have_hol_false_imports_as_explicit_composite() {
    let parsed: JsonValue = serde_json::from_str(G36_TRIM_AND_RESPOND).expect("G36 fixture JSON");
    let top = parsed["@graph"]
        .as_array()
        .expect("@graph array")
        .iter()
        .find(|node| node["@id"] == json!(G36_TRIM_AND_RESPOND_MODEL))
        .expect("top G36 composite node");
    assert_eq!(top["@type"], json!(G36_TRIM_AND_RESPOND_CLASS));

    let graph = import_ok(G36_TRIM_AND_RESPOND);
    assert_eq!(
        graph.blocks.len(),
        44,
        "have_hol=false should prune only the optional TrueFalseHold component"
    );
    let instances: Vec<&str> = graph
        .blocks
        .iter()
        .map(|block| block.instance_iri.as_deref().expect("source path"))
        .collect();
    assert!(instances.iter().any(|iri| iri.ends_with(".fal")));
    assert!(!instances.iter().any(|iri| iri.ends_with(".truHol")));

    assert!(
        block_param(&graph, ".tim", "delayTime").bit_eq(&Value::Real(720.0)),
        "final delayTime=delTim + samplePeriod must ground through the parent scope"
    );
    assert!(
        block_param(&graph, ".uniDel", "y_start").bit_eq(&Value::Real(10.0)),
        "UnitDelay.y_start must inherit iniSet"
    );
    assert!(
        block_param(&graph, ".numIgnReqCon", "k").bit_eq(&Value::Integer(2)),
        "source Integer numIgnReq feeds the Real constant parameter and is promoted by the block"
    );

    let counts = connector_iri_counts(&graph);
    assert!(counts.contains(&(format!("{G36_TRIM_AND_RESPOND_MODEL}.numOfReq"), 1)));
    assert!(counts.contains(&(format!("{G36_TRIM_AND_RESPOND_MODEL}.uDevSta"), 2)));
    assert!(
        !counts
            .iter()
            .any(|(iri, _)| iri == &format!("{G36_TRIM_AND_RESPOND_MODEL}.uHol")),
        "inactive optional hold input must not become an external input"
    );
}

#[test]
fn golden_g36_trim_and_respond_have_hol_false_modelgraph() {
    let actual = render(&import_ok(G36_TRIM_AND_RESPOND));
    let path = golden_path(G36_TRIM_AND_RESPOND_GOLDEN_REL);
    if bless::enabled() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(path)
        .expect("golden snapshot missing; regenerate with OCE_BLESS=1");
    assert_eq!(
        actual, expected,
        "source-verified G36 TrimAndRespond ModelGraph diverged from golden"
    );
}

#[test]
fn source_verified_g36_multizone_vav_supply_temperature_imports_nested_trim_and_respond() {
    let parsed: JsonValue = serde_json::from_str(G36_SUPPLY_TEMPERATURE).expect("G36 fixture JSON");
    let top = parsed["@graph"]
        .as_array()
        .expect("@graph array")
        .iter()
        .find(|node| node["@id"] == json!(G36_SUPPLY_TEMPERATURE_MODEL))
        .expect("top G36 SupplyTemperature composite node");
    assert_eq!(top["@type"], json!(G36_SUPPLY_TEMPERATURE_CLASS));

    let graph = import_ok(G36_SUPPLY_TEMPERATURE);
    assert_eq!(
        graph.blocks.len(),
        62,
        "SupplyTemperature should flatten 44 active TrimAndRespond leaves plus 18 top-level leaves"
    );
    let instances: Vec<&str> = graph
        .blocks
        .iter()
        .map(|block| block.instance_iri.as_deref().expect("source path"))
        .collect();
    assert!(
        instances
            .iter()
            .any(|iri| iri.ends_with(".maxSupTemRes.tim"))
    );
    assert!(instances.iter().any(|iri| iri.ends_with(".lin")));
    assert!(instances.iter().any(|iri| iri.ends_with(".swi3")));
    assert!(
        !instances
            .iter()
            .any(|iri| iri.ends_with(".maxSupTemRes.truHol"))
    );

    assert!(
        block_param(&graph, ".maxSupTemRes.tim", "delayTime").bit_eq(&Value::Real(720.0)),
        "nested TrimAndRespond delay must ground the source default delTim + samplePeriod"
    );
    assert!(
        block_param(&graph, ".maxSupTemRes.uniDel", "y_start").bit_eq(&Value::Real(291.15)),
        "nested TrimAndRespond initial setpoint must ground the source default TSupCoo_max"
    );
    assert!(
        block_param(&graph, ".minSupTem", "k").bit_eq(&Value::Real(285.15)),
        "minimum cooling SAT constant must ground the source default TSupCoo_min"
    );

    let counts = connector_iri_counts(&graph);
    assert!(counts.contains(&(format!("{G36_SUPPLY_TEMPERATURE_MODEL}.TOut"), 1)));
    assert!(counts.contains(&(format!("{G36_SUPPLY_TEMPERATURE_MODEL}.uZonTemResReq"), 1)));
    assert!(counts.contains(&(format!("{G36_SUPPLY_TEMPERATURE_MODEL}.uOpeMod"), 5)));
    assert!(counts.contains(&(format!("{G36_SUPPLY_TEMPERATURE_MODEL}.u1SupFan"), 3)));
    assert!(
        !counts
            .iter()
            .any(|(iri, _)| iri.ends_with(".maxSupTemRes.uHol")),
        "inactive nested have_hol=false optional input must not survive flattening"
    );
}

#[test]
fn golden_g36_multizone_vav_supply_temperature_modelgraph() {
    let actual = render(&import_ok(G36_SUPPLY_TEMPERATURE));
    let path = golden_path(G36_SUPPLY_TEMPERATURE_GOLDEN_REL);
    if bless::enabled() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(path)
        .expect("golden snapshot missing; regenerate with OCE_BLESS=1");
    assert_eq!(
        actual, expected,
        "source-verified G36 SupplyTemperature ModelGraph diverged from golden"
    );
}

#[test]
fn source_verified_g36_multizone_vav_supply_fan_imports_nested_trim_and_respond() {
    let parsed: JsonValue = serde_json::from_str(G36_SUPPLY_FAN).expect("G36 fixture JSON");
    let top = parsed["@graph"]
        .as_array()
        .expect("@graph array")
        .iter()
        .find(|node| node["@id"] == json!(G36_SUPPLY_FAN_MODEL))
        .expect("top G36 SupplyFan composite node");
    assert_eq!(top["@type"], json!(G36_SUPPLY_FAN_CLASS));
    assert_eq!(
        top["S231:containsBlock"]
            .as_array()
            .expect("children")
            .len(),
        23,
        "SupplyFan source transcription should preserve the 23 declared child components"
    );

    let graph = import_ok(G36_SUPPLY_FAN);
    assert_eq!(
        graph.blocks.len(),
        65,
        "SupplyFan should flatten 44 active TrimAndRespond leaves plus 21 top-level leaves"
    );
    let instances: Vec<&str> = graph
        .blocks
        .iter()
        .map(|block| block.instance_iri.as_deref().expect("source path"))
        .collect();
    assert!(
        instances
            .iter()
            .any(|iri| iri.ends_with(".staPreSetRes.tim"))
    );
    assert!(instances.iter().any(|iri| iri.ends_with(".conSpe")));
    assert!(instances.iter().any(|iri| iri.ends_with(".firOrdHol")));
    assert!(instances.iter().any(|iri| iri.ends_with(".con")));
    assert!(!instances.iter().any(|iri| iri.ends_with(".or2")));
    assert!(
        !instances
            .iter()
            .any(|iri| iri.ends_with(".staPreSetRes.truHol"))
    );

    assert!(
        block_param(&graph, ".staPreSetRes.tim", "delayTime").bit_eq(&Value::Real(720.0)),
        "nested TrimAndRespond delay must ground delTim + samplePeriod"
    );
    assert!(
        block_param(&graph, ".staPreSetRes.uniDel", "y_start").bit_eq(&Value::Real(120.0)),
        "nested TrimAndRespond initial pressure setpoint must inherit iniSet"
    );
    assert!(
        block_param(&graph, ".staPreSetRes.maxSetCon", "k").bit_eq(&Value::Real(410.0)),
        "nested TrimAndRespond maximum pressure setpoint must inherit explicit maxSet"
    );
    assert!(
        block_param(&graph, ".conSpe", "yMax").bit_eq(&Value::Real(1.0)),
        "PIDWithReset yMax must inherit maxSpe"
    );
    assert!(
        block_param(&graph, ".conSpe", "yMin").bit_eq(&Value::Real(0.1)),
        "PIDWithReset yMin must inherit minSpe"
    );
    assert!(
        block_param(&graph, ".conSpe", "y_reset").bit_eq(&Value::Real(0.1)),
        "PIDWithReset reset target must inherit iniSpe"
    );
    assert!(
        block_param(&graph, ".firOrdHol", "samplePeriod").bit_eq(&Value::Real(120.0)),
        "FirstOrderHold must inherit samplePeriod"
    );
    assert!(
        block_param(&graph, ".gaiNor", "k").bit_eq(&Value::Real(410.0)),
        "normalization gain must use the explicit duct-pressure maxSet"
    );
    assert!(
        block_param(&graph, ".con", "k").bit_eq(&Value::Boolean(false)),
        "default have_perZonRehBox=false branch should keep source Constant false"
    );

    let counts = connector_iri_counts(&graph);
    assert!(counts.contains(&(format!("{G36_SUPPLY_FAN_MODEL}.uOpeMod"), 5)));
    assert!(counts.contains(&(format!("{G36_SUPPLY_FAN_MODEL}.dpDuc"), 1)));
    assert!(counts.contains(&(format!("{G36_SUPPLY_FAN_MODEL}.uZonPreResReq"), 1)));
    assert!(
        !counts
            .iter()
            .any(|(iri, _)| iri.ends_with(".staPreSetRes.uHol")),
        "inactive nested have_hol=false optional input must not survive flattening"
    );
}

#[test]
fn golden_g36_multizone_vav_supply_fan_modelgraph() {
    let actual = render(&import_ok(G36_SUPPLY_FAN));
    let path = golden_path(G36_SUPPLY_FAN_GOLDEN_REL);
    if bless::enabled() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(path)
        .expect("golden snapshot missing; regenerate with OCE_BLESS=1");
    assert_eq!(
        actual, expected,
        "source-verified G36 SupplyFan ModelGraph diverged from golden"
    );
}

#[test]
fn source_verified_g36_multizone_vav_supply_signals_imports_source_loop() {
    let parsed: JsonValue = serde_json::from_str(G36_SUPPLY_SIGNALS).expect("G36 fixture JSON");
    let top = parsed["@graph"]
        .as_array()
        .expect("@graph array")
        .iter()
        .find(|node| node["@id"] == json!(G36_SUPPLY_SIGNALS_MODEL))
        .expect("top G36 SupplySignals composite node");
    assert_eq!(top["@type"], json!(G36_SUPPLY_SIGNALS_CLASS));
    assert_eq!(
        top["S231:containsBlock"]
            .as_array()
            .expect("children")
            .len(),
        9,
        "SupplySignals source transcription should preserve the 9 default child components"
    );

    let graph = import_ok(G36_SUPPLY_SIGNALS);
    assert_eq!(
        graph.blocks.len(),
        9,
        "default have_heaCoi=true/have_cooCoi=true variant should keep both coil branches"
    );
    let instances: Vec<&str> = graph
        .blocks
        .iter()
        .map(|block| block.instance_iri.as_deref().expect("source path"))
        .collect();
    assert!(instances.iter().any(|iri| iri.ends_with(".conTSup")));
    assert!(instances.iter().any(|iri| iri.ends_with(".swi")));
    assert!(instances.iter().any(|iri| iri.ends_with(".conSigHea")));
    assert!(instances.iter().any(|iri| iri.ends_with(".conSigCoo")));

    assert!(
        block_param(&graph, ".conTSup", "controllerType").bit_eq(&Value::Enum {
            class: EnumClassId::SIMPLE_CONTROLLER,
            ordinal: 2,
        }),
        "SupplySignals source default controllerType=PI must ground through parent scope"
    );
    assert!(
        block_param(&graph, ".conTSup", "k").bit_eq(&Value::Real(0.05)),
        "PID gain must inherit kTSup"
    );
    assert!(
        block_param(&graph, ".conTSup", "Ti").bit_eq(&Value::Real(600.0)),
        "PID integral time must inherit TiTSup"
    );
    assert!(
        block_param(&graph, ".conTSup", "yMin").bit_eq(&Value::Real(-1.0)),
        "SupplySignals PID range must allow heating-side negative output"
    );
    assert!(
        block_param(&graph, ".conTSup", "reverseActing").bit_eq(&Value::Boolean(false)),
        "source reverseActing=false must override the registry default"
    );
    assert!(
        block_param(&graph, ".conSigHea", "limitBelow").bit_eq(&Value::Boolean(false)),
        "heating coil map must only clamp above uHea_max"
    );
    assert!(
        block_param(&graph, ".conSigHea", "limitAbove").bit_eq(&Value::Boolean(true)),
        "heating coil map must only clamp above uHea_max"
    );
    assert!(
        block_param(&graph, ".conSigCoo", "limitBelow").bit_eq(&Value::Boolean(true)),
        "cooling coil map must clamp below uCoo_min"
    );
    assert!(
        block_param(&graph, ".conSigCoo", "limitAbove").bit_eq(&Value::Boolean(false)),
        "cooling coil map must not clamp above because PID yMax already limits uTSup"
    );

    let counts = connector_iri_counts(&graph);
    assert!(counts.contains(&(format!("{G36_SUPPLY_SIGNALS_MODEL}.TAirSup"), 1)));
    assert!(counts.contains(&(format!("{G36_SUPPLY_SIGNALS_MODEL}.TAirSupSet"), 1)));
    assert!(counts.contains(&(format!("{G36_SUPPLY_SIGNALS_MODEL}.u1SupFan"), 2)));
}

#[test]
fn golden_g36_multizone_vav_supply_signals_modelgraph() {
    let actual = render(&import_ok(G36_SUPPLY_SIGNALS));
    let path = golden_path(G36_SUPPLY_SIGNALS_GOLDEN_REL);
    if bless::enabled() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(path)
        .expect("golden snapshot missing; regenerate with OCE_BLESS=1");
    assert_eq!(
        actual, expected,
        "source-verified G36 SupplySignals ModelGraph diverged from golden"
    );
}

#[test]
fn nested_composite_import_is_byte_identical_across_imports() {
    let first = render(&import_ok(NESTED));
    let second = render(&import_ok(NESTED));
    assert_eq!(first, second);

    let parsed: JsonValue = serde_json::from_str(NESTED).expect("fixture JSON");
    let rewritten = serde_json::to_string(&parsed).expect("rewrite JSON");
    assert_eq!(first, render(&import_ok(&rewritten)));
}

#[test]
fn nested_composite_runs_through_block_registry_with_propagated_parameter() {
    let graph = import_ok(NESTED);
    let entry = oce_blocks::lookup(&graph.blocks[0].class_iri).expect("gain block registered");
    let block = (entry.make)(&graph.blocks[0].params);
    let diag = oce_blocks::NoopDiagnostics;
    let cx = oce_blocks::Ctx::new(0.0, &diag);
    let mut emitted = None;
    block.step_algebraic(&cx, &[Value::Real(8.0)], &mut |idx, value| {
        if idx == 0 {
            emitted = Some(value);
        }
    });
    assert!(
        emitted.expect("gain output").bit_eq(&Value::Real(4.0)),
        "topK=0.5 must propagate through sub.innerK into sub.gain.k"
    );
}

#[test]
fn unsupported_replaceable_nested_component_is_rejected() {
    let mut doc: JsonValue = serde_json::from_str(NESTED).expect("fixture JSON");
    node_mut(&mut doc, ".sub")["S231:isReplaceable"] = json!(true);
    assert!(validation_codes(&doc).contains(&DiagCode::UnresolvedPolymorphism));
}

#[test]
fn inactive_nested_composite_connections_are_rejected() {
    let mut doc: JsonValue = serde_json::from_str(NESTED).expect("fixture JSON");
    let top = node_mut(&mut doc, "#g36.profile.nested_composite");
    top["S231:hasParameter"]
        .as_array_mut()
        .expect("top params")
        .push(json!({ "@id": format!("{MODEL}.hasFeature") }));
    doc["@graph"].as_array_mut().expect("graph").push(json!({
        "@id": format!("{MODEL}.hasFeature"),
        "@type": "S231:Parameter",
        "S231:isOfDataType": { "@id": "S231:Boolean" },
        "S231:value": false
    }));
    let sub = node_mut(&mut doc, ".sub");
    sub["S231:isConditionalComponent"] = json!(true);
    sub["S231:conditionalExpression"] = json!("hasFeature");

    assert!(validation_codes(&doc).contains(&DiagCode::InactiveConditionalNode));
}

#[test]
fn nested_composite_parent_parameter_references_fail_closed_when_unbound() {
    let mut doc: JsonValue = serde_json::from_str(NESTED).expect("fixture JSON");
    node_mut(&mut doc, ".sub.innerK")["S231:value"] = json!("missingParent");
    assert!(validation_codes(&doc).contains(&DiagCode::GroundingFailed));
}

#[test]
fn nested_composite_import_fixture_still_loads_after_json_round_trip() {
    let parsed: JsonValue = serde_json::from_str(NESTED).expect("fixture JSON");
    let graph = import_doc(&parsed).expect("round-tripped JSON imports");
    assert_eq!(graph.blocks.len(), 3);
}
