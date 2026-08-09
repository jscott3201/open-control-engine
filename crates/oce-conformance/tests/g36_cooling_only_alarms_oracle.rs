//! G36 CoolingOnly Alarms Tier-A exact oracle check.
//!
//! There is intentionally no funnel-band test: all three outputs are Integer signals, and the
//! type-blind funnel excludes Integer/Boolean signals by policy (`g36_funnel_band/policy.rs:16`,
//! the all-Integer CoolingOnly SystemRequests precedent, and `_spec/07 §9.3`). A vacuous
//! zero-comparison funnel run would provide no evidence, so every alarm remains on this exact
//! Tier-A path.

use std::path::{Path, PathBuf};

use oce_conformance::{
    CombiTimeTable, ComparisonMode, ComparisonResult, DriveCadence, DriverInputReplay,
    DriverOptions, PointEnd, PointMapEntry, ReferenceSpec, Tolerances, ValueKind, VerifyConfig,
    drive_trace_with_options,
};
use serde_json::Value;

const ALARMS: &str = include_str!("../../oce-cxf/tests/fixtures/g36/cooling_only_alarms.jsonld");

const GOLDEN_DIR: &str = "../../tools/golden-gen/goldens/G36";
const SEQUENCE: &str = "cooling_only_alarms";
const ROWS: usize = 58;
const SAMPLE_STEP: f64 = 60.0;

const DISCHARGE_AIRFLOW: &str = "http://example.org#g36.source.cooling_only_alarms.VDis_flow";
const ACTIVE_AIRFLOW_SETPOINT: &str =
    "http://example.org#g36.source.cooling_only_alarms.VActSet_flow";
const SUPPLY_FAN_STATUS: &str = "http://example.org#g36.source.cooling_only_alarms.u1Fan";
const OPERATION_MODE: &str = "http://example.org#g36.source.cooling_only_alarms.uOpeMod";
const DAMPER_POSITION: &str = "http://example.org#g36.source.cooling_only_alarms.uDam";

const LOW_AIRFLOW_ALARM: &str = "http://example.org#g36.source.cooling_only_alarms.proInt.y";
const AIRFLOW_SENSOR_ALARM: &str = "http://example.org#g36.source.cooling_only_alarms.booToInt2.y";
const LEAKING_DAMPER_ALARM: &str = "http://example.org#g36.source.cooling_only_alarms.booToInt3.y";

const INPUTS: &[PointSpec] = &[
    PointSpec::real("discharge_airflow", DISCHARGE_AIRFLOW),
    PointSpec::real("active_airflow_setpoint", ACTIVE_AIRFLOW_SETPOINT),
    PointSpec::boolean("supply_fan_status", SUPPLY_FAN_STATUS),
    PointSpec::integer("operation_mode", OPERATION_MODE),
    PointSpec::real("damper_position", DAMPER_POSITION),
];
const OUTPUTS: &[PointSpec] = &[
    PointSpec::integer("low_airflow_alarm", LOW_AIRFLOW_ALARM),
    PointSpec::integer("airflow_sensor_alarm", AIRFLOW_SENSOR_ALARM),
    PointSpec::integer("leaking_damper_alarm", LEAKING_DAMPER_ALARM),
];
const REFERENCE_COLUMNS: &[&str] = &[
    "time",
    "discharge_airflow",
    "active_airflow_setpoint",
    "supply_fan_status",
    "operation_mode",
    "damper_position",
    "low_airflow_alarm",
    "airflow_sensor_alarm",
    "leaking_damper_alarm",
];

#[test]
fn g36_cooling_only_alarms_tier_a_oracle_matches_engine_output() {
    let reference = CombiTimeTable::read(&reference_path())
        .unwrap_or_else(|err| panic!("CoolingOnly Alarms reference read failed: {err}"));
    assert_eq!(reference.name, "G36_cooling_only_alarms_reference");
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
        ALARMS.as_bytes(),
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
    .unwrap_or_else(|err| panic!("CoolingOnly Alarms exact driver run failed: {err}"));

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
                    "CoolingOnly Alarms exact comparison failed for {}: {:?}",
                    output.reference_name, result.first_mismatch
                );
                assert_eq!(result.compared_points, ROWS);
                assert_eq!(result.first_mismatch, None);
            }
            other => panic!("CoolingOnly Alarms used non-exact comparison: {other:?}"),
        }
    }
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
