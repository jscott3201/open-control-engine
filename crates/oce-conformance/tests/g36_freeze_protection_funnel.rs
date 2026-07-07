//! G36 FreezeProtection Tier-A independent-oracle check through the B3 facade driver.

use std::path::{Path, PathBuf};

use oce_conformance::{
    CombiTimeTable, ComparisonMode, ComparisonResult, DriveCadence, DriverInputReplay,
    DriverOptions, PointEnd, PointMapEntry, ReferenceSpec, Tolerances, ValueKind, VerifyConfig,
    drive_trace_with_options,
};
use serde_json::Value;

#[path = "g36_funnel_band/policy.rs"]
mod funnel_band_policy;

const FREEZE_PROTECTION: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/multizone_vav_freeze_protection.jsonld");

const GOLDEN_DIR: &str = "../../tools/golden-gen/goldens/G36";
const SEQUENCE: &str = "multizone_vav_freeze_protection";
const ROWS: usize = 111;
const SAMPLE_STEP: f64 = 60.0;

const OUTDOOR_DAMPER_MIN_POSITION: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.uOutDamPosMin";
const OUTDOOR_DAMPER: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.uOutDam";
const HEATING_COIL: &str = "http://example.org#g36.source.multizone_vav_freeze_protection.uHeaCoi";
const MINIMUM_OUTDOOR_DAMPER: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.uMinOutDam";
const RETURN_DAMPER: &str = "http://example.org#g36.source.multizone_vav_freeze_protection.uRetDam";
const SUPPLY_AIR_TEMPERATURE: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.TAirSup";
const SOFTWARE_RESET: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.u1SofSwiRes";
const SUPPLY_FAN_STATUS_INPUT: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.u1SupFan";
const SUPPLY_FAN_SPEED_INPUT: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.uSupFan";
const COOLING_COIL: &str = "http://example.org#g36.source.multizone_vav_freeze_protection.uCooCoi";
const MIXED_AIR_TEMPERATURE: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.TAirMix";

const FREEZE_PROTECTION_STAGE: &str = "conn#137";
const CHILLED_WATER_PUMP_ENABLE: &str = "conn#72";
const RETURN_DAMPER_COMMAND: &str = "conn#132";
const OUTDOOR_DAMPER_COMMAND: &str = "conn#81";
const MINIMUM_OUTDOOR_DAMPER_COMMAND: &str = "conn#119";
const SUPPLY_FAN_STATUS: &str = "conn#153";
const SUPPLY_FAN_SPEED: &str = "conn#76";
const COOLING_COIL_COMMAND: &str = "conn#85";
const HEATING_COIL_COMMAND: &str = "conn#101";
const HOT_WATER_PLANT_REQUEST: &str = "conn#89";
const ALARM_LEVEL: &str = "conn#105";

const INPUTS: &[PointSpec] = &[
    PointSpec::real("outdoor_damper_min_position", OUTDOOR_DAMPER_MIN_POSITION),
    PointSpec::real("outdoor_damper", OUTDOOR_DAMPER),
    PointSpec::real("heating_coil", HEATING_COIL),
    PointSpec::real("minimum_outdoor_damper", MINIMUM_OUTDOOR_DAMPER),
    PointSpec::real("return_damper", RETURN_DAMPER),
    PointSpec::real("supply_air_temperature", SUPPLY_AIR_TEMPERATURE),
    PointSpec::boolean("software_reset", SOFTWARE_RESET),
    PointSpec::boolean("supply_fan_status_input", SUPPLY_FAN_STATUS_INPUT),
    PointSpec::real("supply_fan_speed_input", SUPPLY_FAN_SPEED_INPUT),
    PointSpec::real("cooling_coil", COOLING_COIL),
    PointSpec::real("mixed_air_temperature", MIXED_AIR_TEMPERATURE),
];
const OUTPUTS: &[PointSpec] = &[
    PointSpec::integer("freeze_protection_stage", FREEZE_PROTECTION_STAGE),
    PointSpec::boolean("chilled_water_pump_enable", CHILLED_WATER_PUMP_ENABLE),
    PointSpec::real("return_damper_command", RETURN_DAMPER_COMMAND),
    PointSpec::real("outdoor_damper_command", OUTDOOR_DAMPER_COMMAND),
    PointSpec::real(
        "minimum_outdoor_damper_command",
        MINIMUM_OUTDOOR_DAMPER_COMMAND,
    ),
    PointSpec::boolean("supply_fan_status", SUPPLY_FAN_STATUS),
    PointSpec::real("supply_fan_speed", SUPPLY_FAN_SPEED),
    PointSpec::real("cooling_coil_command", COOLING_COIL_COMMAND),
    PointSpec::real("heating_coil_command", HEATING_COIL_COMMAND),
    PointSpec::integer("hot_water_plant_request", HOT_WATER_PLANT_REQUEST),
    PointSpec::integer("alarm_level", ALARM_LEVEL),
];
const REFERENCE_COLUMNS: &[&str] = &[
    "time",
    "outdoor_damper_min_position",
    "outdoor_damper",
    "heating_coil",
    "minimum_outdoor_damper",
    "return_damper",
    "supply_air_temperature",
    "software_reset",
    "supply_fan_status_input",
    "supply_fan_speed_input",
    "cooling_coil",
    "mixed_air_temperature",
    "freeze_protection_stage",
    "chilled_water_pump_enable",
    "return_damper_command",
    "outdoor_damper_command",
    "minimum_outdoor_damper_command",
    "supply_fan_status",
    "supply_fan_speed",
    "cooling_coil_command",
    "heating_coil_command",
    "hot_water_plant_request",
    "alarm_level",
];

#[test]
fn g36_freeze_protection_tier_a_oracle_matches_engine_output() {
    let reference = CombiTimeTable::read(&reference_path())
        .unwrap_or_else(|err| panic!("FreezeProtection reference read failed: {err}"));
    assert_eq!(
        reference.name,
        "G36_multizone_vav_freeze_protection_reference"
    );
    assert_eq!(reference.n_rows, ROWS);
    assert_eq!(
        reference.col_names.as_deref(),
        Some(
            REFERENCE_COLUMNS
                .iter()
                .map(|column| (*column).to_string())
                .collect::<Vec<_>>()
                .as_slice()
        )
    );
    assert_signal_provenance(&reference);

    let run = drive_trace_with_options(
        FREEZE_PROTECTION.as_bytes(),
        &config(),
        &reference,
        &DriverOptions {
            cadence: DriveCadence::EventAligned {
                instants: (0..ROWS).map(|tick| tick as f64 * SAMPLE_STEP).collect(),
            },
            input_replay: DriverInputReplay::ReferenceTable,
            comparison: ComparisonMode::Exact,
        },
    )
    .unwrap_or_else(|err| panic!("FreezeProtection exact driver run failed: {err}"));

    assert_eq!(run.comparisons.len(), OUTPUTS.len());
    for output in OUTPUTS {
        let comparison = run
            .comparisons
            .iter()
            .find(|comparison| comparison.reference_column == output.reference_name)
            .unwrap_or_else(|| panic!("missing comparison for {}", output.reference_name));
        assert_eq!(comparison.output, output.cdl_name);
        assert_eq!(comparison.reference_column, output.reference_name);
        assert!(!comparison.masked);
        match &comparison.result {
            ComparisonResult::Exact(result) => {
                assert!(
                    result.passed,
                    "FreezeProtection exact comparison failed for {}: {:?}",
                    output.reference_name, result.first_mismatch
                );
                assert_eq!(result.compared_points, ROWS);
                assert_eq!(result.first_mismatch, None);
            }
            other => panic!("FreezeProtection used non-exact comparison: {other:?}"),
        }
    }
}

/// Route this sequence's Real outputs through the L1 funnel band with the recorded per-signal
/// tolerance (`_spec/07 §8`); any Boolean/Integer outputs stay on the exact oracle above and are
/// excluded from the funnel entirely. Additive to that oracle, which is unchanged.
#[test]
fn funnel_band_routes_freeze_protection_real_outputs() {
    let inputs = funnel_points(INPUTS);
    let outputs = funnel_points(OUTPUTS);
    let instants: Vec<f64> = (0..ROWS).map(|tick| tick as f64 * SAMPLE_STEP).collect();
    funnel_band_policy::route_real_outputs_through_funnel_band(
        &funnel_band_policy::FunnelRouting {
            sequence: SEQUENCE,
            cxf: FREEZE_PROTECTION.as_bytes(),
            inputs: &inputs,
            outputs: &outputs,
            instants: &instants,
            sample_step: SAMPLE_STEP,
            reference_csv: &reference_path(),
            kind_to_str: kind_name,
        },
    );
}

fn funnel_points(specs: &[PointSpec]) -> Vec<funnel_band_policy::FunnelPoint> {
    specs
        .iter()
        .map(|point| funnel_band_policy::FunnelPoint {
            reference_name: point.reference_name,
            cdl_name: point.cdl_name,
            kind: point.kind,
        })
        .collect()
}

#[derive(Clone, Copy)]
struct PointSpec {
    reference_name: &'static str,
    cdl_name: &'static str,
    kind: ValueKind,
}

impl PointSpec {
    const fn real(reference_name: &'static str, cdl_name: &'static str) -> Self {
        Self {
            reference_name,
            cdl_name,
            kind: ValueKind::Real,
        }
    }

    const fn integer(reference_name: &'static str, cdl_name: &'static str) -> Self {
        Self {
            reference_name,
            cdl_name,
            kind: ValueKind::Integer,
        }
    }

    const fn boolean(reference_name: &'static str, cdl_name: &'static str) -> Self {
        Self {
            reference_name,
            cdl_name,
            kind: ValueKind::Boolean,
        }
    }
}

fn config() -> VerifyConfig {
    VerifyConfig {
        references: vec![ReferenceSpec {
            model: "g36".to_string(),
            sequence: SEQUENCE.to_string(),
            point_name_mapping: point_mapping(),
        }],
        tolerances: Tolerances {
            atolx: 0.0,
            atoly: 0.0,
            rtolx: 0.0,
            rtoly: 0.0,
            ltolx: 0.0,
            ltoly: 0.0,
        },
        outputs: Vec::new(),
        indicators: Vec::new(),
        sampling: Some(SAMPLE_STEP),
        run_controller: true,
    }
}

fn point_mapping() -> Vec<PointMapEntry> {
    INPUTS
        .iter()
        .chain(OUTPUTS.iter())
        .map(|point| PointMapEntry {
            cdl: point_end(point.cdl_name, point.kind),
            device: point_end(point.reference_name, point.kind),
        })
        .collect()
}

fn point_end(name: &str, kind: ValueKind) -> PointEnd {
    PointEnd {
        name: name.to_string(),
        unit: None,
        kind: Some(kind_name(kind).to_string()),
    }
}

fn assert_signal_provenance(reference: &CombiTimeTable) {
    for output in OUTPUTS {
        let prov = read_json(&signal_provenance_path(output.reference_name));
        assert_eq!(prov["class_path"], "G36");
        assert_eq!(prov["scenario"], SEQUENCE);
        assert_eq!(prov["signal"], output.reference_name);
        assert_eq!(prov["tier"], "A");
        assert_eq!(prov["depends_on_oce_blocks"], false);
        assert!(
            prov["source"]
                .as_str()
                .is_some_and(|source| source.contains("Buildings"))
        );
        assert_eq!(
            json_string_array(&prov["reference_columns"]),
            reference
                .col_names
                .as_ref()
                .expect("reference columns")
                .clone()
        );
    }
}

fn json_string_array(value: &Value) -> Vec<String> {
    value
        .as_array()
        .expect("JSON string array")
        .iter()
        .map(|item| item.as_str().expect("JSON string").to_string())
        .collect()
}

fn read_json(path: &Path) -> Value {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("read JSON {} failed: {err}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|err| panic!("parse JSON {} failed: {err}", path.display()))
}

fn reference_path() -> PathBuf {
    reference_dir().join("reference.csv")
}

fn signal_provenance_path(signal: &str) -> PathBuf {
    reference_dir().join(format!("{signal}.prov.json"))
}

fn reference_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(GOLDEN_DIR)
        .join(SEQUENCE)
}

fn kind_name(kind: ValueKind) -> &'static str {
    match kind {
        ValueKind::Real => "Real",
        ValueKind::Integer => "Integer",
        ValueKind::Boolean => "Boolean",
    }
}
