//! Source-verified G36 VentilationZones ASHRAE 62.1 Setpoints composite import tests.
//!
//! The capstone specialization enables every sensor and the typical-terminal-unit branch while
//! disabling the parallel-fan and single-zone-VAV branches. The fixture therefore contains only
//! the 34 active source blocks and 61 active source connections. Whole-number Real bindings stay
//! explicitly typed so resolver grounding cannot silently turn them into Integers.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::PathBuf;

use oce_cxf::{ResolveOptions, import_cxf};
use oce_model::{ModelGraph, Value};
use serde_json::{Value as JsonValue, json};

const FIXTURE: &str = include_str!("fixtures/g36/ventilation_zones_ashrae62_1_setpoints.jsonld");
const GOLDEN_REL: &str =
    "tests/fixtures/golden/g36_ventilation_zones_ashrae62_1_setpoints.modelgraph.txt";
const MODEL: &str = "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints";
const CLASS: &str =
    "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.VentilationZones.ASHRAE62_1.Setpoints";

fn import_ok(src: &str) -> ModelGraph {
    let (graph, report) = import_cxf(src.as_bytes(), &ResolveOptions::default())
        .expect("G36 VentilationZones ASHRAE 62.1 Setpoints fixture should import");
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

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_REL)
}

fn json_node<'a>(parsed: &'a JsonValue, suffix: &str) -> &'a JsonValue {
    parsed["@graph"]
        .as_array()
        .expect("@graph array")
        .iter()
        .find(|node| node["@id"].as_str().is_some_and(|id| id.ends_with(suffix)))
        .unwrap_or_else(|| panic!("missing JSON node ending in {suffix:?}"))
}

fn block<'a>(graph: &'a ModelGraph, suffix: &str) -> &'a oce_model::BlockInstance {
    graph
        .blocks
        .iter()
        .find(|block| {
            block
                .instance_iri
                .as_deref()
                .is_some_and(|iri| iri.ends_with(suffix))
        })
        .unwrap_or_else(|| panic!("missing block ending in {suffix:?}"))
}

fn block_param<'a>(graph: &'a ModelGraph, suffix: &str, name: &str) -> &'a Value {
    block(graph, suffix)
        .params
        .values
        .iter()
        .find_map(|(param, value)| (param.as_ref() == name).then_some(value))
        .unwrap_or_else(|| panic!("missing param {name:?} on block {suffix:?}"))
}

fn authored_connection_count(parsed: &JsonValue) -> usize {
    parsed["@graph"]
        .as_array()
        .expect("@graph array")
        .iter()
        .map(|node| match node.get("S231:isConnectedTo") {
            Some(JsonValue::Array(targets)) => targets.len(),
            Some(JsonValue::Object(_)) => 1,
            None => 0,
            Some(other) => panic!("unexpected isConnectedTo value: {other:?}"),
        })
        .sum()
}

fn assert_typed_real(parsed: &JsonValue, suffix: &str, expected_bits: u64) {
    let node = json_node(parsed, suffix);
    assert_eq!(node["S231:isOfDataType"], json!({ "@id": "S231:Real" }));
    assert_eq!(
        node["S231:value"]["@type"],
        json!("http://www.w3.org/2001/XMLSchema#double")
    );
    let lexical = node["S231:value"]["@value"]
        .as_str()
        .unwrap_or_else(|| panic!("{suffix} must use a typed Real lexical value"));
    assert_eq!(
        lexical
            .parse::<f64>()
            .expect("valid Real lexical")
            .to_bits(),
        expected_bits,
        "{suffix}"
    );
}

fn assert_boolean(parsed: &JsonValue, suffix: &str, expected: bool) {
    let node = json_node(parsed, suffix);
    assert_eq!(node["S231:isOfDataType"], json!({ "@id": "S231:Boolean" }));
    assert_eq!(node["S231:value"], json!(expected), "{suffix}");
}

#[test]
fn capstone_configuration_preserves_source_topology_grounding_and_output_fanout() {
    let parsed: JsonValue = serde_json::from_str(FIXTURE).expect("G36 fixture JSON");
    let top = json_node(&parsed, "ventilation_zones_ashrae62_1_setpoints");
    assert_eq!(top["@id"], json!(MODEL));
    assert_eq!(top["@type"], json!(CLASS));
    assert_eq!(
        top["S231:hasParameter"]
            .as_array()
            .expect("parameters")
            .len(),
        14
    );
    assert_eq!(
        top["S231:containsBlock"]
            .as_array()
            .expect("children")
            .len(),
        34
    );
    assert_eq!(top["S231:hasInput"].as_array().expect("inputs").len(), 7);
    assert_eq!(top["S231:hasOutput"].as_array().expect("outputs").len(), 4);
    assert_eq!(
        authored_connection_count(&parsed),
        61,
        "the active specialization must transcribe exactly the 61 source connections"
    );

    let expected_instances = BTreeSet::from([
        ".airDisEff",
        ".lin",
        ".zer",
        ".one",
        ".addPar",
        ".inOccMod",
        ".booToRea",
        ".co2Con",
        ".occMinAirSet",
        ".zonMinFlo",
        ".zonCooMaxFlo",
        ".popBreOutAir",
        ".or2",
        ".notOccMod",
        ".perOccSta",
        ".zer1",
        ".modPopBreAir",
        ".modAreBreAir",
        ".occMinAir",
        ".notOcc",
        ".unpPopBreAir",
        ".booToRea1",
        ".unPopAreBreAir",
        ".unpMinZonFlo",
        ".unpAreBreAir",
        ".unpMinZonAir",
        ".reqBreAir",
        ".minOA",
        ".occMod",
        ".cooSup",
        ".gai2",
        ".desAreAir",
        ".desPopAir",
        ".winOpe",
    ]);
    let graph = import_ok(FIXTURE);
    assert_eq!(graph.blocks.len(), 34);
    let instances = graph
        .blocks
        .iter()
        .map(|block| {
            let iri = block.instance_iri.as_deref().expect("source path");
            &iri[iri.rfind('.').expect("instance suffix")..]
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(instances, expected_instances);
    assert_eq!(
        graph.connections.len(),
        49,
        "61 source connections minus 8 boundary-input edges and 4 boundary-output edges"
    );
    assert_eq!(
        graph.external_inputs.len(),
        8,
        "seven logical inputs fan out to eight leaf connectors"
    );
    let external_input_iris = graph
        .external_inputs
        .iter()
        .map(|id| {
            graph.connectors[id.0 as usize]
                .iri
                .as_deref()
                .expect("external input IRI")
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        external_input_iris,
        BTreeSet::from([
            "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.TDis",
            "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.TZon",
            "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.ppmCO2",
            "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.ppmCO2Set",
            "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.u1Occ",
            "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.u1Win",
            "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.uOpeMod",
        ])
    );

    for (suffix, expected) in [
        (".have_winSen", true),
        (".have_occSen", true),
        (".have_CO2Sen", true),
        (".have_typTerUni", true),
        (".have_parFanPowUni", false),
        (".have_SZVAV", false),
        (".permit_occStandby", true),
        (".perOccSta.k", true),
    ] {
        assert_boolean(&parsed, suffix, expected);
    }
    for (suffix, bits) in [
        (".VAreBreZon_flow", 0x3f78_9374_bc6a_7efa),
        (".VPopBreZon_flow", 0x3f74_7ae1_47ae_147b),
        (".VMin_flow", 0x3fe0_0000_0000_0000),
        (".VCooMax_flow", 0x3ff8_0000_0000_0000),
        (".zonDisEff_cool", 0x3ff0_0000_0000_0000),
        (".zonDisEff_heat", 0x3fe9_9999_9999_999a),
        (".dTHys", 0x3fd0_0000_0000_0000),
        (".zer.k", 0x0000_0000_0000_0000),
        (".zer1.k", 0x0000_0000_0000_0000),
        (".one.k", 0x3ff0_0000_0000_0000),
        (".airDisEff.realTrue", 0x3ff0_0000_0000_0000),
        (".booToRea1.realTrue", 0x0000_0000_0000_0000),
        (".booToRea1.realFalse", 0x3ff0_0000_0000_0000),
        (".gai2.k", 0x3ff0_0000_0000_0000),
        (".addPar.p", 0xc069_0000_0000_0000),
    ] {
        assert_typed_real(&parsed, suffix, bits);
    }

    for (suffix, name, expected) in [
        (".desAreAir", "k", 0.006),
        (".desPopAir", "k", 0.005),
        (".zonMinFlo", "k", 0.5),
        (".zonCooMaxFlo", "k", 1.5),
        (".airDisEff", "realTrue", 1.0),
        (".airDisEff", "realFalse", 0.8),
        (".cooSup", "h", 0.25),
        (".addPar", "p", -200.0),
        (".zer", "k", 0.0),
        (".zer1", "k", 0.0),
        (".one", "k", 1.0),
        (".booToRea1", "realTrue", 0.0),
        (".booToRea1", "realFalse", 1.0),
        (".gai2", "k", 1.0),
    ] {
        assert!(
            block_param(&graph, suffix, name).bit_eq(&Value::Real(expected)),
            "{suffix}.{name}"
        );
    }
    assert!(block_param(&graph, ".perOccSta", "k").bit_eq(&Value::Boolean(true)));
    assert!(block_param(&graph, ".occMod", "k").bit_eq(&Value::Integer(1)));
    for suffix in [".lin", ".occMinAirSet", ".popBreOutAir"] {
        assert!(block_param(&graph, suffix, "limitBelow").bit_eq(&Value::Boolean(true)));
        assert!(block_param(&graph, suffix, "limitAbove").bit_eq(&Value::Boolean(true)));
    }

    assert_eq!(
        json_node(&parsed, ".modPopBreAir.y")["S231:isConnectedTo"],
        json!([
            { "@id": "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.reqBreAir.u1" },
            { "@id": "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.VAdjPopBreZon_flow" }
        ])
    );
    assert_eq!(
        json_node(&parsed, ".modAreBreAir.y")["S231:isConnectedTo"],
        json!([
            { "@id": "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.reqBreAir.u2" },
            { "@id": "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.VAdjAreBreZon_flow" }
        ])
    );

    let mod_pop = block(&graph, ".modPopBreAir").outputs[0];
    let occupied_minimum = block(&graph, ".occMinAir").outputs[0];
    let mod_area = block(&graph, ".modAreBreAir").outputs[0];
    let minimum_outdoor_air = block(&graph, ".minOA").outputs[0];
    let output_edges = BTreeSet::from([
        mod_pop.0,
        occupied_minimum.0,
        mod_area.0,
        minimum_outdoor_air.0,
    ]);
    assert_eq!(
        output_edges.len(),
        4,
        "all four named outputs must resolve to distinct conn#N runtime edges"
    );
    let req_breathing_air = block(&graph, ".reqBreAir");
    assert!(
        graph
            .connections
            .iter()
            .any(|edge| edge.from == mod_pop && edge.to == req_breathing_air.inputs[0]),
        "modPopBreAir.y must retain its internal reqBreAir.u1 fan-out edge"
    );
    assert!(
        graph
            .connections
            .iter()
            .any(|edge| edge.from == mod_area && edge.to == req_breathing_air.inputs[1]),
        "modAreBreAir.y must retain its internal reqBreAir.u2 fan-out edge"
    );
}

#[test]
fn ventilation_zones_ashrae62_1_setpoints_modelgraph_is_stable() {
    let actual = render(&import_ok(FIXTURE));
    let path = golden_path();
    if std::env::var_os("OCE_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().expect("golden parent")).unwrap();
        std::fs::write(path, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(path)
        .expect("golden snapshot missing; regenerate with OCE_BLESS=1");
    assert_eq!(
        actual, expected,
        "source-verified G36 VentilationZones ASHRAE 62.1 Setpoints ModelGraph diverged from golden"
    );
}
