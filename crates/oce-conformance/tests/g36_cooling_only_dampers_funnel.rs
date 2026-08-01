//! G36 CoolingOnly Dampers Tier-A oracle and L1 funnel-band checks.

use std::path::{Path, PathBuf};

use oce_conformance::{
    CombiTimeTable, ComparisonMode, ComparisonResult, DriveCadence, DriverInputReplay,
    DriverOptions, PointEnd, PointMapEntry, ReferenceSpec, Tolerances, ValueKind, VerifyConfig,
    drive_trace_with_options,
};
use serde_json::Value;

#[path = "g36_funnel_band/policy.rs"]
mod funnel_band_policy;

const COOLING_ONLY_DAMPERS: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/cooling_only_dampers.jsonld");

const GOLDEN_DIR: &str = "../../tools/golden-gen/goldens/G36";
const SEQUENCE: &str = "cooling_only_dampers";
const ROWS: usize = 34;
const SAMPLE_STEP: f64 = 60.0;

const ACTIVE_MINIMUM_AIRFLOW: &str =
    "http://example.org#g36.source.cooling_only_dampers.VActMin_flow";
const SUPPLY_AIR_TEMPERATURE: &str = "http://example.org#g36.source.cooling_only_dampers.TSup";
const ZONE_TEMPERATURE: &str = "http://example.org#g36.source.cooling_only_dampers.TZon";
const COOLING_LOOP: &str = "http://example.org#g36.source.cooling_only_dampers.uCoo";
const ACTIVE_COOLING_MAXIMUM_AIRFLOW: &str =
    "http://example.org#g36.source.cooling_only_dampers.VActCooMax_flow";
const ZONE_STATE: &str = "http://example.org#g36.source.cooling_only_dampers.uZonSta";
const AIRFLOW_OVERRIDE_INDEX: &str = "http://example.org#g36.source.cooling_only_dampers.oveFloSet";
const SUPPLY_FAN_STATUS: &str = "http://example.org#g36.source.cooling_only_dampers.u1Fan";
const DISCHARGE_AIRFLOW: &str = "http://example.org#g36.source.cooling_only_dampers.VDis_flow";
const DAMPER_OVERRIDE_INDEX: &str = "http://example.org#g36.source.cooling_only_dampers.oveDamPos";

const AIRFLOW_SETPOINT: &str = "conn#61";
const DAMPER_COMMAND: &str = "conn#86";

const INPUTS: &[PointSpec] = &[
    PointSpec::real("active_minimum_airflow", ACTIVE_MINIMUM_AIRFLOW),
    PointSpec::real("supply_air_temperature", SUPPLY_AIR_TEMPERATURE),
    PointSpec::real("zone_temperature", ZONE_TEMPERATURE),
    PointSpec::real("cooling_loop", COOLING_LOOP),
    PointSpec::real(
        "active_cooling_maximum_airflow",
        ACTIVE_COOLING_MAXIMUM_AIRFLOW,
    ),
    PointSpec::integer("zone_state", ZONE_STATE),
    PointSpec::integer("airflow_override_index", AIRFLOW_OVERRIDE_INDEX),
    PointSpec::boolean("supply_fan_status", SUPPLY_FAN_STATUS),
    PointSpec::real("discharge_airflow", DISCHARGE_AIRFLOW),
    PointSpec::integer("damper_override_index", DAMPER_OVERRIDE_INDEX),
];
const OUTPUTS: &[PointSpec] = &[
    PointSpec::real("airflow_setpoint", AIRFLOW_SETPOINT),
    PointSpec::real("damper_command", DAMPER_COMMAND),
];
const REFERENCE_COLUMNS: &[&str] = &[
    "time",
    "active_minimum_airflow",
    "supply_air_temperature",
    "zone_temperature",
    "cooling_loop",
    "active_cooling_maximum_airflow",
    "zone_state",
    "airflow_override_index",
    "supply_fan_status",
    "discharge_airflow",
    "damper_override_index",
    "airflow_setpoint",
    "damper_command",
];

#[test]
fn g36_cooling_only_dampers_tier_a_oracle_matches_engine_output() {
    let reference = CombiTimeTable::read(&reference_path())
        .unwrap_or_else(|err| panic!("CoolingOnly Dampers reference read failed: {err}"));
    assert_eq!(reference.name, "G36_cooling_only_dampers_reference");
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
        COOLING_ONLY_DAMPERS.as_bytes(),
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
    .unwrap_or_else(|err| panic!("CoolingOnly Dampers exact driver run failed: {err}"));

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
                    "CoolingOnly Dampers exact comparison failed for {}: {:?}",
                    output.reference_name, result.first_mismatch
                );
                assert_eq!(result.compared_points, ROWS);
                assert_eq!(result.first_mismatch, None);
            }
            other => panic!("CoolingOnly Dampers used non-exact comparison: {other:?}"),
        }
    }
}

#[test]
fn funnel_band_routes_cooling_only_dampers_real_outputs() {
    let inputs = funnel_points(INPUTS);
    let outputs = funnel_points(OUTPUTS);
    let instants: Vec<f64> = (0..ROWS).map(|tick| tick as f64 * SAMPLE_STEP).collect();
    funnel_band_policy::route_real_outputs_through_funnel_band(
        &funnel_band_policy::FunnelRouting {
            sequence: SEQUENCE,
            cxf: COOLING_ONLY_DAMPERS.as_bytes(),
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
            rtoly: 1e-9,
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
