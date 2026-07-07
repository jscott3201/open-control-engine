//! G36 Economizers.Controller restricted variant Tier-A independent-oracle check.

use std::path::{Path, PathBuf};

use oce_conformance::{
    CombiTimeTable, ComparisonMode, ComparisonResult, DriveCadence, DriverInputReplay,
    DriverOptions, PointEnd, PointMapEntry, ReferenceSpec, Tolerances, ValueKind, VerifyConfig,
    drive_trace_with_options,
};
use serde_json::Value;

#[path = "g36_funnel_band/policy.rs"]
mod funnel_band_policy;

const ECONOMIZER_CONTROLLER: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.jsonld"
);

const GOLDEN_DIR: &str = "../../tools/golden-gen/goldens/G36";
const SEQUENCE: &str = "multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21";
const ROWS: usize = 24;
const SAMPLE_STEP: f64 = 60.0;

const OUTDOOR_AIRFLOW_NORMALIZED: &str = "http://example.org#g36.source.multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.VOut_flow_normalized";
const MINIMUM_OUTDOOR_AIRFLOW_SETPOINT_NORMALIZED: &str = "http://example.org#g36.source.multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.VOutMinSet_flow_normalized";
const SUPPLY_TEMPERATURE_SIGNAL: &str = "http://example.org#g36.source.multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.uTSup";
const OUTDOOR_AIR_TEMPERATURE: &str = "http://example.org#g36.source.multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.TOut";
const SUPPLY_FAN_STATUS: &str = "http://example.org#g36.source.multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.u1SupFan";
const OPERATION_MODE: &str = "http://example.org#g36.source.multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.uOpeMod";
const FREEZE_PROTECTION_STAGE: &str = "http://example.org#g36.source.multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.uFreProSta";

const OUTDOOR_DAMPER_MIN_LIMIT: &str = "conn#16";
const MINIMUM_OUTDOOR_AIR_LOOP_ENABLED: &str = "conn#39";
const OUTDOOR_DAMPER_COMMAND: &str = "conn#109";
const RETURN_DAMPER_COMMAND: &str = "conn#112";

const INPUTS: &[PointSpec] = &[
    PointSpec::real("outdoor_airflow_normalized", OUTDOOR_AIRFLOW_NORMALIZED),
    PointSpec::real(
        "minimum_outdoor_airflow_setpoint_normalized",
        MINIMUM_OUTDOOR_AIRFLOW_SETPOINT_NORMALIZED,
    ),
    PointSpec::real("supply_temperature_signal", SUPPLY_TEMPERATURE_SIGNAL),
    PointSpec::real("outdoor_air_temperature", OUTDOOR_AIR_TEMPERATURE),
    PointSpec::boolean("supply_fan_status", SUPPLY_FAN_STATUS),
    PointSpec::integer("operation_mode", OPERATION_MODE),
    PointSpec::integer("freeze_protection_stage", FREEZE_PROTECTION_STAGE),
];
const OUTPUTS: &[PointSpec] = &[
    PointSpec::real("outdoor_damper_min_limit", OUTDOOR_DAMPER_MIN_LIMIT),
    PointSpec::boolean(
        "minimum_outdoor_air_loop_enabled",
        MINIMUM_OUTDOOR_AIR_LOOP_ENABLED,
    ),
    PointSpec::real("outdoor_damper_command", OUTDOOR_DAMPER_COMMAND),
    PointSpec::real("return_damper_command", RETURN_DAMPER_COMMAND),
];
const REFERENCE_COLUMNS: &[&str] = &[
    "time",
    "outdoor_airflow_normalized",
    "minimum_outdoor_airflow_setpoint_normalized",
    "supply_temperature_signal",
    "outdoor_air_temperature",
    "supply_fan_status",
    "operation_mode",
    "freeze_protection_stage",
    "outdoor_damper_min_limit",
    "minimum_outdoor_air_loop_enabled",
    "outdoor_damper_command",
    "return_damper_command",
];

#[test]
fn g36_economizer_controller_tier_a_oracle_matches_engine_output() {
    let reference = CombiTimeTable::read(&reference_path())
        .unwrap_or_else(|err| panic!("Economizers.Controller reference read failed: {err}"));
    assert_eq!(
        reference.name,
        "G36_multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21_reference"
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
        ECONOMIZER_CONTROLLER.as_bytes(),
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
    .unwrap_or_else(|err| panic!("Economizers.Controller exact driver run failed: {err}"));

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
                    "Economizers.Controller exact comparison failed for {}: {:?}",
                    output.reference_name, result.first_mismatch
                );
                assert_eq!(result.compared_points, ROWS);
                assert_eq!(result.first_mismatch, None);
            }
            other => panic!("Economizers.Controller used non-exact comparison: {other:?}"),
        }
    }
}

/// Route this sequence's Real outputs through the L1 funnel band with the recorded per-signal
/// tolerance (`_spec/07 §8`); any Boolean/Integer outputs stay on the exact oracle above and are
/// excluded from the funnel entirely. Additive to that oracle, which is unchanged.
#[test]
fn funnel_band_routes_economizer_controller_real_outputs() {
    let inputs = funnel_points(INPUTS);
    let outputs = funnel_points(OUTPUTS);
    let instants: Vec<f64> = (0..ROWS).map(|tick| tick as f64 * SAMPLE_STEP).collect();
    funnel_band_policy::route_real_outputs_through_funnel_band(
        &funnel_band_policy::FunnelRouting {
            sequence: SEQUENCE,
            cxf: ECONOMIZER_CONTROLLER.as_bytes(),
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
        ValueKind::Real => "real",
        ValueKind::Integer => "integer",
        ValueKind::Boolean => "boolean",
    }
}
