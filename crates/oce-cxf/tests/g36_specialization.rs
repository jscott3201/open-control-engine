//! G36 enum and load-time conditional specialization tests.

use oce_cxf::{CxfError, CxfValue, ResolveOptions, import_cxf};
use oce_diag::DiagCode;
use oce_model::{EnumClassId, Value, g36_enum_literals};
use serde_json::{Value as JsonValue, json};

const MODEL: &str = "http://example.org#G36Spec";
const G36_TYPES: &str = "Buildings.Controls.OBC.ASHRAE.G36.Types";
const VENTILATION_STANDARD: &str = "Buildings.Controls.OBC.ASHRAE.G36.Types.VentilationStandard";

fn import_doc(doc: JsonValue) -> Result<oce_model::ModelGraph, CxfError> {
    let bytes = serde_json::to_vec(&doc).expect("serialize fixture");
    import_cxf(&bytes, &ResolveOptions::default()).map(|(graph, report)| {
        assert!(
            report.is_empty(),
            "test fixtures should not warn: {:?}",
            report.diagnostics
        );
        graph
    })
}

fn validation_codes(doc: JsonValue) -> Vec<DiagCode> {
    let bytes = serde_json::to_vec(&doc).expect("serialize fixture");
    match import_cxf(&bytes, &ResolveOptions::default()) {
        Err(CxfError::Validation(diags)) => diags.iter().map(|diag| diag.code).collect(),
        other => panic!("expected validation error, got {other:?}"),
    }
}

fn top_node(children: &[&str]) -> JsonValue {
    top_node_with_params(children, &["venStd", "hasFeature"])
}

fn top_node_with_params(children: &[&str], params: &[&str]) -> JsonValue {
    json!({
        "@id": MODEL,
        "@type": "S231:Block",
        "S231:hasParameter": params
            .iter()
            .map(|param| json!({ "@id": format!("{MODEL}.{param}") }))
            .collect::<Vec<_>>(),
        "S231:containsBlock": children
            .iter()
            .map(|child| json!({ "@id": format!("{MODEL}.{child}") }))
            .collect::<Vec<_>>()
    })
}

fn top_params(ven_std_value: JsonValue, has_feature: bool) -> [JsonValue; 2] {
    [
        json!({
            "@id": format!("{MODEL}.venStd"),
            "@type": "S231:Parameter",
            "S231:isOfDataType": { "@id": format!("http://example.org#{VENTILATION_STANDARD}") },
            "S231:value": ven_std_value
        }),
        bool_param("hasFeature", has_feature),
    ]
}

fn enum_param(name: &str, class: &str, literal: &str) -> JsonValue {
    json!({
        "@id": format!("{MODEL}.{name}"),
        "@type": "S231:Parameter",
        "S231:isOfDataType": { "@id": format!("http://example.org#{class}") },
        "S231:value": format!("{class}.{literal}")
    })
}

fn bool_param(name: &str, value: bool) -> JsonValue {
    json!({
        "@id": format!("{MODEL}.{name}"),
        "@type": "S231:Parameter",
        "S231:isOfDataType": { "@id": "S231:Boolean" },
        "S231:value": value
    })
}

fn constant_block(name: &str, guard: Option<&str>) -> Vec<JsonValue> {
    let id = format!("{MODEL}.{name}");
    let mut block = json!({
        "@id": id,
        "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
        "S231:hasParameter": { "@id": format!("{MODEL}.{name}.k") },
        "S231:hasOutput": { "@id": format!("{MODEL}.{name}.y") }
    });
    if let Some(expr) = guard {
        block["S231:isConditionalComponent"] = json!(true);
        block["S231:conditionalExpression"] = json!(expr);
    }
    vec![
        block,
        json!({
            "@id": format!("{MODEL}.{name}.k"),
            "@type": "S231:Parameter",
            "S231:value": { "@value": "1.0", "@type": "http://www.w3.org/2001/XMLSchema#double" }
        }),
        json!({
            "@id": format!("{MODEL}.{name}.y"),
            "@type": "S231:RealOutput",
            "S231:isOfDataType": { "@id": "S231:Real" }
        }),
    ]
}

fn doc_with_children(
    children: Vec<(&str, Option<String>)>,
    ven_std: &str,
    has_feature: bool,
) -> JsonValue {
    let child_names: Vec<&str> = children.iter().map(|(name, _)| *name).collect();
    let mut graph = vec![top_node(&child_names)];
    graph.extend(top_params(
        json!(format!("{VENTILATION_STANDARD}.{ven_std}")),
        has_feature,
    ));
    for (name, guard) in children {
        graph.extend(constant_block(name, guard.as_deref()));
    }
    json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" },
        "@graph": graph
    })
}

fn single_constant_with_extra_param(extra_type: &str, extra_value: JsonValue) -> JsonValue {
    let mut graph = vec![json!({
        "@id": MODEL,
        "@type": "S231:Block",
        "S231:containsBlock": { "@id": format!("{MODEL}.con") }
    })];
    graph.extend(vec![
        json!({
            "@id": format!("{MODEL}.con"),
            "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
            "S231:hasParameter": [
                { "@id": format!("{MODEL}.con.k") },
                { "@id": format!("{MODEL}.con.extra") }
            ],
            "S231:hasOutput": { "@id": format!("{MODEL}.con.y") }
        }),
        json!({
            "@id": format!("{MODEL}.con.k"),
            "@type": "S231:Parameter",
            "S231:value": { "@value": "1.0", "@type": "http://www.w3.org/2001/XMLSchema#double" }
        }),
        json!({
            "@id": format!("{MODEL}.con.extra"),
            "@type": "S231:Parameter",
            "S231:isOfDataType": { "@id": extra_type },
            "S231:value": extra_value
        }),
        json!({
            "@id": format!("{MODEL}.con.y"),
            "@type": "S231:RealOutput",
            "S231:isOfDataType": { "@id": "S231:Real" }
        }),
    ]);
    json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" },
        "@graph": graph
    })
}

#[test]
fn g36_enum_literal_values_round_trip_through_cxf_serde() {
    let cases = [
        (
            "Buildings.Controls.OBC.ASHRAE.G36.Types.ASHRAEClimateZone",
            EnumClassId::G36_ASHRAE_CLIMATE_ZONE,
        ),
        (
            "Buildings.Controls.OBC.ASHRAE.G36.Types.ControlEconomizer",
            EnumClassId::G36_CONTROL_ECONOMIZER,
        ),
        (
            "Buildings.Controls.OBC.ASHRAE.G36.Types.CoolingCoil",
            EnumClassId::G36_COOLING_COIL,
        ),
        (
            "Buildings.Controls.OBC.ASHRAE.G36.Types.EnergyStandard",
            EnumClassId::G36_ENERGY_STANDARD,
        ),
        (
            "Buildings.Controls.OBC.ASHRAE.G36.Types.FreezeStat",
            EnumClassId::G36_FREEZE_STAT,
        ),
        (
            "Buildings.Controls.OBC.ASHRAE.G36.Types.HeatingCoil",
            EnumClassId::G36_HEATING_COIL,
        ),
        (
            "Buildings.Controls.OBC.ASHRAE.G36.Types.OutdoorAirSection",
            EnumClassId::G36_OUTDOOR_AIR_SECTION,
        ),
        (
            "Buildings.Controls.OBC.ASHRAE.G36.Types.PressureControl",
            EnumClassId::G36_PRESSURE_CONTROL,
        ),
        (
            "Buildings.Controls.OBC.ASHRAE.G36.Types.Title24ClimateZone",
            EnumClassId::G36_TITLE24_CLIMATE_ZONE,
        ),
        (
            "Buildings.Controls.OBC.ASHRAE.G36.Types.VentilationStandard",
            EnumClassId::G36_VENTILATION_STANDARD,
        ),
    ];

    for (class_path, class) in cases {
        for literal in g36_enum_literals(class).expect("source-pinned literals") {
            let qualified = format!("{class_path}.{literal}");
            let parsed: CxfValue =
                serde_json::from_value(json!(qualified)).expect("enum literal parses as Expr");
            let serialized = serde_json::to_value(&parsed).expect("serialize enum literal");
            assert_eq!(serialized, json!(qualified));
            assert_eq!(parsed, CxfValue::Expr(qualified));
        }
    }
}

#[test]
fn g36_enum_parameter_grounds_to_source_ordered_ordinal() {
    let graph = import_doc(single_constant_with_extra_param(
        &format!("http://example.org#{VENTILATION_STANDARD}"),
        json!(format!("{VENTILATION_STANDARD}.California_Title_24")),
    ))
    .expect("G36 enum parameter should import");
    let extra = graph.blocks[0]
        .params
        .values
        .iter()
        .find(|(name, _)| name.as_ref() == "extra")
        .expect("extra param")
        .1
        .clone();
    assert!(extra.bit_eq(&Value::Enum {
        class: EnumClassId::G36_VENTILATION_STANDARD,
        ordinal: 2,
    }));
}

#[test]
fn g36_integer_constant_reference_grounds_to_integer_value() {
    let graph = import_doc(single_constant_with_extra_param(
        &format!("http://example.org#{G36_TYPES}.OperationModes"),
        json!(format!("{G36_TYPES}.OperationModes.occupied")),
    ))
    .expect("G36 integer constant should import");
    let extra = graph.blocks[0]
        .params
        .values
        .iter()
        .find(|(name, _)| name.as_ref() == "extra")
        .expect("extra param")
        .1
        .clone();
    assert!(extra.bit_eq(&Value::Integer(1)));
}

#[test]
fn g36_enum_parameter_malformed_values_have_typed_diagnostics() {
    let unknown_literal = validation_codes(single_constant_with_extra_param(
        &format!("http://example.org#{VENTILATION_STANDARD}"),
        json!(format!("{VENTILATION_STANDARD}.Bogus")),
    ));
    assert!(unknown_literal.contains(&DiagCode::UnknownEnumLiteral));

    let integer_standin = validation_codes(single_constant_with_extra_param(
        &format!("http://example.org#{VENTILATION_STANDARD}"),
        json!(2),
    ));
    assert!(integer_standin.contains(&DiagCode::EnumIntegerStandin));

    let unknown_type = validation_codes(single_constant_with_extra_param(
        &format!("http://example.org#{G36_TYPES}.NotAType"),
        json!(format!("{G36_TYPES}.NotAType.Value")),
    ));
    assert!(unknown_type.contains(&DiagCode::UnknownEnumType));
}

#[test]
fn conditional_components_are_pruned_before_block_id_assignment() {
    let doc = doc_with_children(
        vec![
            (
                "title24",
                Some(format!(
                    "venStd == {VENTILATION_STANDARD}.California_Title_24"
                )),
            ),
            (
                "notAshrae",
                Some(format!(
                    "venStd != {VENTILATION_STANDARD}.ASHRAE62_1 and hasFeature"
                )),
            ),
            ("disabled", Some("!hasFeature".to_owned())),
        ],
        "California_Title_24",
        true,
    );
    let graph = import_doc(doc).expect("specialized fixture should import");
    let instances: Vec<String> = graph
        .blocks
        .iter()
        .filter_map(|block| block.instance_iri.as_deref().map(str::to_owned))
        .collect();
    assert_eq!(
        instances,
        vec![format!("{MODEL}.title24"), format!("{MODEL}.notAshrae")]
    );
    assert_eq!(graph.blocks[0].id.0, 0);
    assert_eq!(graph.blocks[1].id.0, 1);
}

#[test]
fn conditional_guard_matrix_covers_representative_g36_controller_enums() {
    let children = [
        "economizer",
        "freeze",
        "pressure",
        "cooling",
        "heating",
        "outdoor",
        "energy",
    ];
    let mut graph = vec![top_node_with_params(
        &children,
        &[
            "eco", "freSta", "preCon", "cooCoi", "heaCoi", "outSec", "eneStd",
        ],
    )];
    graph.extend(vec![
        enum_param(
            "eco",
            &format!("{G36_TYPES}.ControlEconomizer"),
            "FixedDryBulb",
        ),
        enum_param(
            "freSta",
            &format!("{G36_TYPES}.FreezeStat"),
            "Hardwired_to_BAS",
        ),
        enum_param(
            "preCon",
            &format!("{G36_TYPES}.PressureControl"),
            "ReturnFanDp",
        ),
        enum_param("cooCoi", &format!("{G36_TYPES}.CoolingCoil"), "WaterBased"),
        enum_param("heaCoi", &format!("{G36_TYPES}.HeatingCoil"), "None"),
        enum_param(
            "outSec",
            &format!("{G36_TYPES}.OutdoorAirSection"),
            "SingleDamper",
        ),
        enum_param(
            "eneStd",
            &format!("{G36_TYPES}.EnergyStandard"),
            "California_Title_24",
        ),
    ]);
    for (name, guard) in [
        (
            "economizer",
            format!("eco == {G36_TYPES}.ControlEconomizer.FixedDryBulb"),
        ),
        (
            "freeze",
            format!("freSta != {G36_TYPES}.FreezeStat.No_freeze_stat"),
        ),
        (
            "pressure",
            format!("preCon == {G36_TYPES}.PressureControl.ReturnFanDp"),
        ),
        (
            "cooling",
            format!("cooCoi == {G36_TYPES}.CoolingCoil.WaterBased"),
        ),
        (
            "heating",
            format!("heaCoi == {G36_TYPES}.HeatingCoil.Electric"),
        ),
        (
            "outdoor",
            format!("outSec == {G36_TYPES}.OutdoorAirSection.SingleDamper"),
        ),
        (
            "energy",
            format!("eneStd == {G36_TYPES}.EnergyStandard.California_Title_24"),
        ),
    ] {
        graph.extend(constant_block(name, Some(&guard)));
    }

    let graph = import_doc(json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" },
        "@graph": graph
    }))
    .expect("representative G36 guard matrix should import");
    let instances: Vec<String> = graph
        .blocks
        .iter()
        .filter_map(|block| block.instance_iri.as_deref().map(str::to_owned))
        .collect();
    assert_eq!(
        instances,
        vec![
            format!("{MODEL}.economizer"),
            format!("{MODEL}.freeze"),
            format!("{MODEL}.pressure"),
            format!("{MODEL}.cooling"),
            format!("{MODEL}.outdoor"),
            format!("{MODEL}.energy"),
        ]
    );
}

#[test]
fn malformed_guards_have_typed_diagnostics() {
    let unknown = validation_codes(doc_with_children(
        vec![("bad", Some("unknownFlag".to_owned()))],
        "California_Title_24",
        true,
    ));
    assert!(unknown.contains(&DiagCode::ConditionalGuardUnknownParameter));

    let arithmetic = validation_codes(doc_with_children(
        vec![("bad", Some("hasFeature + 1 == 2".to_owned()))],
        "California_Title_24",
        true,
    ));
    assert!(arithmetic.contains(&DiagCode::ConditionalGuardUnsupported));

    let function = validation_codes(doc_with_children(
        vec![("bad", Some("integer(1) == 1".to_owned()))],
        "California_Title_24",
        true,
    ));
    assert!(function.contains(&DiagCode::ConditionalGuardUnsupported));
}

#[test]
fn inactive_conditional_node_with_active_connection_is_rejected() {
    let mut doc = doc_with_children(
        vec![("disabled", Some("!hasFeature".to_owned()))],
        "California_Title_24",
        true,
    );
    doc["@graph"][0]["S231:hasOutput"] = json!({ "@id": format!("{MODEL}.y") });
    doc["@graph"]
        .as_array_mut()
        .expect("graph array")
        .push(json!({
            "@id": format!("{MODEL}.y"),
            "@type": "S231:RealOutput",
            "S231:isOfDataType": { "@id": "S231:Real" }
        }));
    for node in doc["@graph"].as_array_mut().expect("graph array") {
        if node["@id"] == json!(format!("{MODEL}.disabled.y")) {
            node["S231:isConnectedTo"] = json!({ "@id": format!("{MODEL}.y") });
        }
    }

    let codes = validation_codes(doc);
    assert!(codes.contains(&DiagCode::InactiveConditionalNode));
}
