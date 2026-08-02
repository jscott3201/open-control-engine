//! G36 ReliefFanGroup Tier-A independent-oracle check through the B3 facade driver.

use std::path::{Path, PathBuf};

use oce_conformance::{
    CombiTimeTable, ComparisonMode, ComparisonResult, DriveCadence, DriverInputReplay,
    DriverOptions, PointEnd, PointMapEntry, ReferenceSpec, Tolerances, ValueKind, VerifyConfig,
    drive_trace_with_options,
};
use serde_json::Value;

#[path = "g36_funnel_band/policy.rs"]
mod funnel_band_policy;

const RELIEF_FAN_GROUP: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/multizone_vav_relief_fan_group.jsonld");

const GOLDEN_DIR: &str = "../../tools/golden-gen/goldens/G36";
const SEQUENCE: &str = "multizone_vav_relief_fan_group";
const ROWS: usize = 161;
const SAMPLE_STEP: f64 = 15.0;

const SUPPLY_FAN_1: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan_group.u1SupFan_1";
const SUPPLY_FAN_2: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan_group.u1SupFan_2";
const BUILDING_PRESSURE: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan_group.dpBui";
const ALARM_1: &str = "http://example.org#g36.source.multizone_vav_relief_fan_group.uRelFanAla_1";
const ALARM_2: &str = "http://example.org#g36.source.multizone_vav_relief_fan_group.uRelFanAla_2";
const ALARM_3: &str = "http://example.org#g36.source.multizone_vav_relief_fan_group.uRelFanAla_3";
const ALARM_4: &str = "http://example.org#g36.source.multizone_vav_relief_fan_group.uRelFanAla_4";
const PROOF_1: &str = "http://example.org#g36.source.multizone_vav_relief_fan_group.u1RelFan_1";
const PROOF_2: &str = "http://example.org#g36.source.multizone_vav_relief_fan_group.u1RelFan_2";
const PROOF_3: &str = "http://example.org#g36.source.multizone_vav_relief_fan_group.u1RelFan_3";
const PROOF_4: &str = "http://example.org#g36.source.multizone_vav_relief_fan_group.u1RelFan_4";

const AVERAGED_PRESSURE: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan_group.movMea.y";
const RELIEF_FAN_1_SPEED: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan_group.pro3_1.y";
const RELIEF_FAN_2_SPEED: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan_group.pro3_2.y";
const RELIEF_FAN_3_SPEED: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan_group.pro3_3.y";
const RELIEF_FAN_4_SPEED: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan_group.pro3_4.y";
const RELIEF_DAMPER_1: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan_group.mul_1.y";
const RELIEF_DAMPER_2: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan_group.mul_2.y";
const RELIEF_DAMPER_3: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan_group.mul_3.y";
const RELIEF_DAMPER_4: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan_group.mul_4.y";

const INPUTS: &[PointSpec] = &[
    PointSpec::boolean("supply_fan_1_status", SUPPLY_FAN_1),
    PointSpec::boolean("supply_fan_2_status", SUPPLY_FAN_2),
    PointSpec::real("building_pressure", BUILDING_PRESSURE),
    PointSpec::integer("relief_fan_1_alarm", ALARM_1),
    PointSpec::integer("relief_fan_2_alarm", ALARM_2),
    PointSpec::integer("relief_fan_3_alarm", ALARM_3),
    PointSpec::integer("relief_fan_4_alarm", ALARM_4),
    PointSpec::boolean("relief_fan_1_proof", PROOF_1),
    PointSpec::boolean("relief_fan_2_proof", PROOF_2),
    PointSpec::boolean("relief_fan_3_proof", PROOF_3),
    PointSpec::boolean("relief_fan_4_proof", PROOF_4),
];
const OUTPUTS: &[PointSpec] = &[
    PointSpec::real("averaged_building_pressure", AVERAGED_PRESSURE),
    PointSpec::real("relief_fan_1_speed", RELIEF_FAN_1_SPEED),
    PointSpec::real("relief_fan_2_speed", RELIEF_FAN_2_SPEED),
    PointSpec::real("relief_fan_3_speed", RELIEF_FAN_3_SPEED),
    PointSpec::real("relief_fan_4_speed", RELIEF_FAN_4_SPEED),
    PointSpec::real("relief_damper_1_command", RELIEF_DAMPER_1),
    PointSpec::real("relief_damper_2_command", RELIEF_DAMPER_2),
    PointSpec::real("relief_damper_3_command", RELIEF_DAMPER_3),
    PointSpec::real("relief_damper_4_command", RELIEF_DAMPER_4),
];
const REFERENCE_COLUMNS: &[&str] = &[
    "time",
    "supply_fan_1_status",
    "supply_fan_2_status",
    "building_pressure",
    "relief_fan_1_alarm",
    "relief_fan_2_alarm",
    "relief_fan_3_alarm",
    "relief_fan_4_alarm",
    "relief_fan_1_proof",
    "relief_fan_2_proof",
    "relief_fan_3_proof",
    "relief_fan_4_proof",
    "averaged_building_pressure",
    "relief_fan_1_speed",
    "relief_fan_2_speed",
    "relief_fan_3_speed",
    "relief_fan_4_speed",
    "relief_damper_1_command",
    "relief_damper_2_command",
    "relief_damper_3_command",
    "relief_damper_4_command",
];

#[test]
fn g36_relief_fan_group_tier_a_oracle_matches_engine_output() {
    let reference = CombiTimeTable::read(&reference_path())
        .unwrap_or_else(|err| panic!("ReliefFanGroup reference read failed: {err}"));
    assert_eq!(
        reference.name,
        "G36_multizone_vav_relief_fan_group_reference"
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
        RELIEF_FAN_GROUP.as_bytes(),
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
    .unwrap_or_else(|err| panic!("ReliefFanGroup exact driver run failed: {err}"));

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
                    "ReliefFanGroup exact comparison failed for {}: {:?}",
                    output.reference_name, result.first_mismatch
                );
                assert_eq!(result.compared_points, ROWS);
                assert_eq!(result.first_mismatch, None);
            }
            other => panic!("ReliefFanGroup used non-exact comparison: {other:?}"),
        }
    }
}

/// Route this sequence's Real outputs through the L1 funnel band with the recorded per-signal
/// tolerance (`_spec/07 §8`); any Boolean/Integer outputs stay on the exact oracle above and are
/// excluded from the funnel entirely. Additive to that oracle, which is unchanged.
#[test]
fn funnel_band_routes_relief_fan_group_real_outputs() {
    let inputs = funnel_points(INPUTS);
    let outputs = funnel_points(OUTPUTS);
    let instants: Vec<f64> = (0..ROWS).map(|tick| tick as f64 * SAMPLE_STEP).collect();
    funnel_band_policy::route_real_outputs_through_funnel_band(
        &funnel_band_policy::FunnelRouting {
            sequence: SEQUENCE,
            cxf: RELIEF_FAN_GROUP.as_bytes(),
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
