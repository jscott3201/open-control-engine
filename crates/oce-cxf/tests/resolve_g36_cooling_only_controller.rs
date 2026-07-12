//! Source-verified G36 CoolingOnly.Controller nested-composite import tests.
//!
//! The configured ASHRAE 62.1 controller contains eight inlined sub-controller graphs. The
//! Title 24 branch is pruned before import, so only the 213 active leaf blocks participate in
//! the flattened runtime graph.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::PathBuf;

use oce_cxf::{ResolveOptions, import_cxf};
use oce_model::{EnumClassId, ModelGraph, Value};
use serde_json::{Value as JsonValue, json};

const FIXTURE: &str = include_str!("fixtures/g36/cooling_only_controller.jsonld");
const GOLDEN_REL: &str = "tests/fixtures/golden/g36_cooling_only_controller.modelgraph.txt";
const MODEL: &str = "http://example.org#g36.source.cooling_only_controller";
const CLASS: &str =
    "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.TerminalUnits.CoolingOnly.Controller";

enum ExpectedParameter<'a> {
    Real(f64),
    Boolean(bool),
    Enumeration { class: &'a str, value: &'a str },
}

fn import_ok(src: &str) -> ModelGraph {
    let (graph, report) = import_cxf(src.as_bytes(), &ResolveOptions::default())
        .expect("G36 CoolingOnly.Controller fixture should import");
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

fn json_node<'a>(parsed: &'a JsonValue, id: &str) -> &'a JsonValue {
    parsed["@graph"]
        .as_array()
        .expect("@graph array")
        .iter()
        .find(|node| node["@id"] == json!(id))
        .unwrap_or_else(|| panic!("missing JSON node {id:?}"))
}

fn reference_ids(value: &JsonValue) -> Vec<&str> {
    match value {
        JsonValue::Array(values) => values
            .iter()
            .map(|value| value["@id"].as_str().expect("reference @id"))
            .collect(),
        JsonValue::Object(_) => vec![value["@id"].as_str().expect("reference @id")],
        other => panic!("expected reference or reference array, got {other:?}"),
    }
}

fn assert_parameter_scope(
    parsed: &JsonValue,
    scope: &str,
    expected: &[(&str, ExpectedParameter<'_>)],
) {
    let scope_id = format!("{MODEL}{scope}");
    let root = json_node(parsed, &scope_id);
    let prefix = format!("{scope_id}.");
    let parameter_ids = reference_ids(&root["S231:hasParameter"]);
    let names = parameter_ids
        .iter()
        .map(|id| {
            id.strip_prefix(&prefix)
                .unwrap_or_else(|| panic!("{id:?} is outside parameter scope {scope_id:?}"))
        })
        .collect::<BTreeSet<_>>();
    let expected_names = expected
        .iter()
        .map(|(name, _)| *name)
        .collect::<BTreeSet<_>>();
    assert_eq!(names, expected_names, "parameter census for {scope_id}");

    for (name, expected_value) in expected {
        let id = format!("{scope_id}.{name}");
        let node = json_node(parsed, &id);
        match expected_value {
            ExpectedParameter::Real(expected_real) => {
                assert_eq!(node["S231:isOfDataType"], json!({ "@id": "S231:Real" }));
                assert_eq!(
                    node["S231:value"]["@type"],
                    json!("http://www.w3.org/2001/XMLSchema#double")
                );
                let lexical = node["S231:value"]["@value"]
                    .as_str()
                    .unwrap_or_else(|| panic!("{id} must use a typed Real lexical value"));
                assert_eq!(
                    lexical
                        .parse::<f64>()
                        .expect("valid Real lexical")
                        .to_bits(),
                    expected_real.to_bits(),
                    "{id}"
                );
            }
            ExpectedParameter::Boolean(expected_boolean) => {
                assert_eq!(node["S231:isOfDataType"], json!({ "@id": "S231:Boolean" }));
                assert_eq!(node["S231:value"], json!(expected_boolean), "{id}");
            }
            ExpectedParameter::Enumeration { class, value } => {
                assert_eq!(
                    node["S231:isOfDataType"],
                    json!({ "@id": format!("http://example.org#{class}") })
                );
                assert_eq!(node["S231:value"], json!(value), "{id}");
            }
        }
    }
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
        .find_map(|(parameter, value)| (parameter.as_ref() == name).then_some(value))
        .unwrap_or_else(|| panic!("missing param {name:?} on block {suffix:?}"))
}

fn cross_scope_connection_count(parsed: &JsonValue) -> usize {
    parsed["@graph"]
        .as_array()
        .expect("@graph array")
        .iter()
        .filter_map(|node| {
            let source = node["@id"].as_str()?.strip_prefix(&format!("{MODEL}."))?;
            let targets = node.get("S231:isConnectedTo")?;
            Some(
                reference_ids(targets)
                    .into_iter()
                    .filter(|target| {
                        let target = target
                            .strip_prefix(&format!("{MODEL}."))
                            .expect("fixture-local connection");
                        source.split_once('.').map(|pair| pair.0)
                            != target.split_once('.').map(|pair| pair.0)
                    })
                    .count(),
            )
        })
        .sum()
}

fn assert_top_fanout(parsed: &JsonValue, source: &str, targets: &[&str]) {
    let node = json_node(parsed, &format!("{MODEL}.{source}"));
    let actual = reference_ids(&node["S231:isConnectedTo"])
        .into_iter()
        .map(|id| {
            id.strip_prefix(&format!("{MODEL}."))
                .expect("fixture-local target")
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        actual,
        targets.iter().copied().collect(),
        "{source} fan-out"
    );
}

#[test]
fn configured_controller_preserves_nested_topology_grounding_and_fanout() {
    let parsed: JsonValue = serde_json::from_str(FIXTURE).expect("G36 fixture JSON");
    let top = json_node(&parsed, MODEL);
    assert_eq!(top["@type"], json!(CLASS));
    assert_eq!(reference_ids(&top["S231:hasParameter"]).len(), 39);
    assert_eq!(reference_ids(&top["S231:containsBlock"]).len(), 8);
    assert_eq!(reference_ids(&top["S231:hasInput"]).len(), 14);
    assert_eq!(reference_ids(&top["S231:hasOutput"]).len(), 10);
    assert_eq!(
        cross_scope_connection_count(&parsed),
        48,
        "57 Controller.mo connects minus the nine pruned Title 24 edges"
    );
    assert!(!FIXTURE.contains("S231:isConditional"));
    assert!(!FIXTURE.contains("S231:conditionalExpression"));
    assert!(
        !parsed["@graph"]
            .as_array()
            .expect("@graph")
            .iter()
            .any(|node| {
                node["@id"]
                    .as_str()
                    .is_some_and(|id| id.starts_with(&format!("{MODEL}.minFlo.")))
            }),
        "the Title 24 minFlo composite must be absent"
    );

    assert_parameter_scope(
        &parsed,
        "",
        &[
            (
                "venStd",
                ExpectedParameter::Enumeration {
                    class: "Buildings.Controls.OBC.ASHRAE.G36.Types.VentilationStandard",
                    value: "Buildings.Controls.OBC.ASHRAE.G36.Types.VentilationStandard.ASHRAE62_1",
                },
            ),
            ("have_winSen", ExpectedParameter::Boolean(true)),
            ("have_occSen", ExpectedParameter::Boolean(true)),
            ("have_CO2Sen", ExpectedParameter::Boolean(true)),
            ("permit_occStandby", ExpectedParameter::Boolean(true)),
            ("VOccMin_flow", ExpectedParameter::Real(0.0)),
            ("VAreMin_flow", ExpectedParameter::Real(0.0)),
            ("VAreBreZon_flow", ExpectedParameter::Real(0.006)),
            ("VPopBreZon_flow", ExpectedParameter::Real(0.005)),
            ("VMin_flow", ExpectedParameter::Real(0.5)),
            ("VCooMax_flow", ExpectedParameter::Real(1.5)),
            ("kCooCon", ExpectedParameter::Real(0.1)),
            ("TiCooCon", ExpectedParameter::Real(900.0)),
            ("kHeaCon", ExpectedParameter::Real(0.1)),
            ("TiHeaCon", ExpectedParameter::Real(900.0)),
            (
                "damCon",
                ExpectedParameter::Enumeration {
                    class: "Buildings.Controls.OBC.CDL.Types.SimpleController",
                    value: "Buildings.Controls.OBC.CDL.Types.SimpleController.PI",
                },
            ),
            ("kDam", ExpectedParameter::Real(0.5)),
            ("TiDam", ExpectedParameter::Real(300.0)),
            ("TdDam", ExpectedParameter::Real(0.1)),
            ("thrTemDif", ExpectedParameter::Real(3.0)),
            ("twoTemDif", ExpectedParameter::Real(2.0)),
            ("durTimTem", ExpectedParameter::Real(120.0)),
            ("durTimFlo", ExpectedParameter::Real(60.0)),
            ("staPreMul", ExpectedParameter::Real(1.0)),
            ("lowFloTim", ExpectedParameter::Real(300.0)),
            ("fanOffTim", ExpectedParameter::Real(600.0)),
            ("leaFloTim", ExpectedParameter::Real(600.0)),
            ("samplePeriod", ExpectedParameter::Real(120.0)),
            ("chaRat", ExpectedParameter::Real(540.0)),
            ("maxSupTim", ExpectedParameter::Real(1800.0)),
            ("dTHys", ExpectedParameter::Real(0.25)),
            ("floHys", ExpectedParameter::Real(0.01)),
            ("looHys", ExpectedParameter::Real(0.01)),
            ("damPosHys", ExpectedParameter::Real(0.01)),
            ("staTim", ExpectedParameter::Real(1800.0)),
            ("iniDam", ExpectedParameter::Real(0.01)),
            ("timChe", ExpectedParameter::Real(30.0)),
            ("zonDisEff_cool", ExpectedParameter::Real(1.0)),
            ("zonDisEff_heat", ExpectedParameter::Real(0.8)),
        ],
    );
    assert_parameter_scope(
        &parsed,
        ".actAirSet",
        &[("VCooMax_flow", ExpectedParameter::Real(1.5))],
    );
    assert_parameter_scope(
        &parsed,
        ".sysReq",
        &[
            ("thrTemDif", ExpectedParameter::Real(3.0)),
            ("twoTemDif", ExpectedParameter::Real(2.0)),
            ("durTimTem", ExpectedParameter::Real(120.0)),
            ("durTimFlo", ExpectedParameter::Real(60.0)),
            ("dTHys", ExpectedParameter::Real(0.25)),
            ("floHys", ExpectedParameter::Real(0.01)),
            ("looHys", ExpectedParameter::Real(0.01)),
            ("damPosHys", ExpectedParameter::Real(0.01)),
            ("samplePeriod", ExpectedParameter::Real(120.0)),
        ],
    );
    assert_parameter_scope(
        &parsed,
        ".conLoo",
        &[
            ("kCooCon", ExpectedParameter::Real(0.1)),
            ("TiCooCon", ExpectedParameter::Real(900.0)),
            ("kHeaCon", ExpectedParameter::Real(0.1)),
            ("TiHeaCon", ExpectedParameter::Real(900.0)),
            ("timChe", ExpectedParameter::Real(30.0)),
            ("dTHys", ExpectedParameter::Real(0.25)),
            ("looHys", ExpectedParameter::Real(0.01)),
        ],
    );
    assert_parameter_scope(
        &parsed,
        ".ala",
        &[
            ("staPreMul", ExpectedParameter::Real(1.0)),
            ("VCooMax_flow", ExpectedParameter::Real(1.5)),
            ("lowFloTim", ExpectedParameter::Real(300.0)),
            ("fanOffTim", ExpectedParameter::Real(600.0)),
            ("leaFloTim", ExpectedParameter::Real(600.0)),
            ("floHys", ExpectedParameter::Real(0.01)),
            ("damPosHys", ExpectedParameter::Real(0.01)),
            ("staTim", ExpectedParameter::Real(1800.0)),
        ],
    );
    assert_parameter_scope(
        &parsed,
        ".timSup",
        &[
            ("chaRat", ExpectedParameter::Real(540.0)),
            ("maxTim", ExpectedParameter::Real(1800.0)),
            ("samplePeriod", ExpectedParameter::Real(120.0)),
            ("dTHys", ExpectedParameter::Real(0.25)),
        ],
    );
    assert_parameter_scope(
        &parsed,
        ".setPoi",
        &[
            ("have_winSen", ExpectedParameter::Boolean(true)),
            ("have_occSen", ExpectedParameter::Boolean(true)),
            ("have_CO2Sen", ExpectedParameter::Boolean(true)),
            ("have_typTerUni", ExpectedParameter::Boolean(true)),
            ("have_parFanPowUni", ExpectedParameter::Boolean(false)),
            ("have_SZVAV", ExpectedParameter::Boolean(false)),
            ("permit_occStandby", ExpectedParameter::Boolean(true)),
            ("VAreBreZon_flow", ExpectedParameter::Real(0.006)),
            ("VPopBreZon_flow", ExpectedParameter::Real(0.005)),
            ("VMin_flow", ExpectedParameter::Real(0.5)),
            ("VCooMax_flow", ExpectedParameter::Real(1.5)),
            ("zonDisEff_cool", ExpectedParameter::Real(1.0)),
            ("zonDisEff_heat", ExpectedParameter::Real(0.8)),
            ("dTHys", ExpectedParameter::Real(0.25)),
        ],
    );
    assert_parameter_scope(
        &parsed,
        ".dam",
        &[
            ("VMin_flow", ExpectedParameter::Real(0.5)),
            ("VCooMax_flow", ExpectedParameter::Real(1.5)),
            (
                "damCon",
                ExpectedParameter::Enumeration {
                    class: "Buildings.Controls.OBC.CDL.Types.SimpleController",
                    value: "Buildings.Controls.OBC.CDL.Types.SimpleController.PI",
                },
            ),
            ("kDam", ExpectedParameter::Real(0.5)),
            ("TiDam", ExpectedParameter::Real(300.0)),
            ("TdDam", ExpectedParameter::Real(0.1)),
            ("dTHys", ExpectedParameter::Real(0.25)),
            ("iniDam", ExpectedParameter::Real(0.01)),
        ],
    );
    assert_parameter_scope(
        &parsed,
        ".zonSta",
        &[
            ("uLow", ExpectedParameter::Real(0.01)),
            ("uHigh", ExpectedParameter::Real(0.05)),
        ],
    );

    assert_top_fanout(
        &parsed,
        "conLoo.yCoo",
        &["dam.uCoo", "sysReq.uCoo", "zonSta.uCoo"],
    );
    assert_top_fanout(
        &parsed,
        "dam.VSet_flow",
        &["VSet_flow", "ala.VActSet_flow", "sysReq.VSet_flow"],
    );
    assert_top_fanout(&parsed, "dam.yDam", &["ala.uDam", "sysReq.uDam", "yDam"]);

    let graph = import_ok(FIXTURE);
    assert_eq!(graph.blocks.len(), 213);
    assert_eq!(graph.connectors.len(), 521);
    assert_eq!(
        graph.connections.len(),
        268,
        "248 child-internal edges plus 20 leaf edges expanded from 13 cross-child connects"
    );
    let mut per_child = BTreeMap::new();
    for block in &graph.blocks {
        let path = block
            .instance_iri
            .as_deref()
            .expect("flattened source path")
            .strip_prefix(&format!("{MODEL}."))
            .expect("Controller child path");
        let child = path.split_once('.').expect("child leaf path").0;
        *per_child.entry(child).or_insert(0usize) += 1;
    }
    assert_eq!(
        per_child,
        BTreeMap::from([
            ("actAirSet", 11),
            ("ala", 47),
            ("conLoo", 16),
            ("dam", 35),
            ("setPoi", 34),
            ("sysReq", 33),
            ("timSup", 24),
            ("zonSta", 13),
        ])
    );

    for (suffix, name, expected) in [
        (".actAirSet.actCooMax", "realTrue", 1.5),
        (".conLoo.conCoo", "k", 0.1),
        (".conLoo.conCoo", "Ti", 900.0),
        (".dam.conPID", "k", 0.5),
        (".dam.conPID", "Ti", 300.0),
        (".dam.conPID", "y_reset", 0.01),
        (".dam.nomFlow", "k", 1.5),
        (".dam.cooMax", "realTrue", 1.5),
        (".dam.minFlo", "realTrue", 0.5),
        (".ala.cooMaxFlo", "k", 1.5),
        (".ala.fanIni", "delayTime", 1800.0),
        (".sysReq.greThr1", "t", 3.0),
        (".sysReq.greThr2", "t", 2.0),
        (".timSup.samSet", "samplePeriod", 120.0),
        (".setPoi.desAreAir", "k", 0.006),
        (".setPoi.desPopAir", "k", 0.005),
    ] {
        assert!(
            block_param(&graph, suffix, name).bit_eq(&Value::Real(expected)),
            "{suffix}.{name}"
        );
    }
    assert!(
        block_param(&graph, ".dam.conPID", "controllerType").bit_eq(&Value::Enum {
            class: EnumClassId::SIMPLE_CONTROLLER,
            ordinal: 2,
        })
    );

    let output_edges = [
        block(&graph, ".dam.swi1").outputs[0],
        block(&graph, ".dam.swi2").outputs[0],
        block(&graph, ".setPoi.modPopBreAir").outputs[0],
        block(&graph, ".setPoi.modAreBreAir").outputs[0],
        block(&graph, ".setPoi.minOA").outputs[0],
        block(&graph, ".sysReq.intSwi").outputs[0],
        block(&graph, ".sysReq.swi4").outputs[0],
        block(&graph, ".ala.proInt").outputs[0],
        block(&graph, ".ala.booToInt2").outputs[0],
        block(&graph, ".ala.booToInt3").outputs[0],
    ];
    assert_eq!(
        output_edges
            .into_iter()
            .map(|id| id.0)
            .collect::<BTreeSet<_>>()
            .len(),
        10,
        "all ten named outputs must resolve to distinct conn#N runtime edges"
    );
}

#[test]
fn cooling_only_controller_modelgraph_is_stable() {
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
        "source-verified G36 CoolingOnly.Controller ModelGraph diverged from golden"
    );
}
