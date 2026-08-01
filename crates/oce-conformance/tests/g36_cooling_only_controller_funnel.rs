//! G36 CoolingOnly.Controller Tier-A exact and L1 funnel-band conformance.
//!
//! Exact mode covers all ten outputs. Funnel mode routes only the five Real outputs through the
//! shared near-ULP policy; all five Integer outputs remain Exact-only.

use std::path::{Path, PathBuf};

use oce_conformance::{
    CombiTimeTable, ComparisonMode, ComparisonResult, DriveCadence, DriverInputReplay,
    DriverOptions, PointEnd, PointMapEntry, ReferenceSpec, Tolerances, ValueKind, VerifyConfig,
    drive_trace_with_options,
};
use serde_json::Value;

#[path = "g36_funnel_band/policy.rs"]
mod funnel_band_policy;

const FIXTURE: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/cooling_only_controller.jsonld");
const GOLDEN_DIR: &str = "../../tools/golden-gen/goldens/G36";
const SEQUENCE: &str = "cooling_only_controller";
const ROWS: usize = 1_441;
const SAMPLE_STEP: f64 = 60.0;

const AIRFLOW_SETPOINT: &str = "conn#460";
const DAMPER_COMMAND: &str = "conn#485";
const ADJUSTED_POPULATION_FLOW: &str = "conn#352";
const ADJUSTED_AREA_FLOW: &str = "conn#356";
const MINIMUM_OUTDOOR_AIRFLOW: &str = "conn#388";
const ZONE_TEMPERATURE_RESET_REQUEST: &str = "conn#68";
const ZONE_PRESSURE_RESET_REQUEST: &str = "conn#76";
const LOW_AIRFLOW_ALARM: &str = "conn#182";
const AIRFLOW_SENSOR_ALARM: &str = "conn#212";
const LEAKING_DAMPER_ALARM: &str = "conn#227";

const INPUTS: &[PointSpec] = &[
    PointSpec::real(
        "zone_temperature",
        "http://example.org#g36.source.cooling_only_controller.TZon",
    ),
    PointSpec::real(
        "cooling_setpoint",
        "http://example.org#g36.source.cooling_only_controller.TCooSet",
    ),
    PointSpec::real(
        "heating_setpoint",
        "http://example.org#g36.source.cooling_only_controller.THeaSet",
    ),
    PointSpec::boolean(
        "window_status",
        "http://example.org#g36.source.cooling_only_controller.u1Win",
    ),
    PointSpec::boolean(
        "occupancy_status",
        "http://example.org#g36.source.cooling_only_controller.u1Occ",
    ),
    PointSpec::integer(
        "operating_mode",
        "http://example.org#g36.source.cooling_only_controller.uOpeMod",
    ),
    PointSpec::real(
        "co2_setpoint",
        "http://example.org#g36.source.cooling_only_controller.ppmCO2Set",
    ),
    PointSpec::real(
        "co2_concentration",
        "http://example.org#g36.source.cooling_only_controller.ppmCO2",
    ),
    PointSpec::real(
        "discharge_air_temperature",
        "http://example.org#g36.source.cooling_only_controller.TDis",
    ),
    PointSpec::real(
        "supply_air_temperature",
        "http://example.org#g36.source.cooling_only_controller.TSup",
    ),
    PointSpec::real(
        "discharge_airflow",
        "http://example.org#g36.source.cooling_only_controller.VDis_flow",
    ),
    PointSpec::integer(
        "airflow_override_index",
        "http://example.org#g36.source.cooling_only_controller.oveFloSet",
    ),
    PointSpec::integer(
        "damper_override_index",
        "http://example.org#g36.source.cooling_only_controller.oveDamPos",
    ),
    PointSpec::boolean(
        "supply_fan_status",
        "http://example.org#g36.source.cooling_only_controller.u1Fan",
    ),
];
const OUTPUTS: &[PointSpec] = &[
    PointSpec::runtime_real("airflow_setpoint", AIRFLOW_SETPOINT),
    PointSpec::runtime_real("damper_command", DAMPER_COMMAND),
    PointSpec::runtime_real("adjusted_population_flow", ADJUSTED_POPULATION_FLOW),
    PointSpec::runtime_real("adjusted_area_flow", ADJUSTED_AREA_FLOW),
    PointSpec::runtime_real("minimum_outdoor_airflow", MINIMUM_OUTDOOR_AIRFLOW),
    PointSpec::runtime_integer(
        "zone_temperature_reset_request",
        ZONE_TEMPERATURE_RESET_REQUEST,
    ),
    PointSpec::runtime_integer("zone_pressure_reset_request", ZONE_PRESSURE_RESET_REQUEST),
    PointSpec::runtime_integer("low_airflow_alarm", LOW_AIRFLOW_ALARM),
    PointSpec::runtime_integer("airflow_sensor_alarm", AIRFLOW_SENSOR_ALARM),
    PointSpec::runtime_integer("leaking_damper_alarm", LEAKING_DAMPER_ALARM),
];
const REFERENCE_COLUMNS: &[&str] = &[
    "time",
    "zone_temperature",
    "cooling_setpoint",
    "heating_setpoint",
    "window_status",
    "occupancy_status",
    "operating_mode",
    "co2_setpoint",
    "co2_concentration",
    "discharge_air_temperature",
    "supply_air_temperature",
    "discharge_airflow",
    "airflow_override_index",
    "damper_override_index",
    "supply_fan_status",
    "airflow_setpoint",
    "damper_command",
    "adjusted_population_flow",
    "adjusted_area_flow",
    "minimum_outdoor_airflow",
    "zone_temperature_reset_request",
    "zone_pressure_reset_request",
    "low_airflow_alarm",
    "airflow_sensor_alarm",
    "leaking_damper_alarm",
];

#[test]
fn exact_oracle_compares_every_controller_output_at_every_validation_tick() {
    let reference = CombiTimeTable::read(&reference_path())
        .unwrap_or_else(|error| panic!("CoolingOnly.Controller reference read failed: {error}"));
    assert_eq!(reference.name, "G36_cooling_only_controller_reference");
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
        FIXTURE.as_bytes(),
        &config(),
        &reference,
        &DriverOptions {
            cadence: DriveCadence::EventAligned {
                instants: validation_instants(),
            },
            input_replay: DriverInputReplay::ReferenceTable,
            comparison: ComparisonMode::Exact,
        },
    )
    .unwrap_or_else(|error| panic!("CoolingOnly.Controller exact run failed: {error}"));

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
                    "exact comparison failed for {}: {:?}",
                    output.reference_name, result.first_mismatch
                );
                assert_eq!(result.compared_points, ROWS);
                assert_eq!(result.first_mismatch, None);
            }
            other => panic!("Controller used non-exact comparison: {other:?}"),
        }
    }
}

#[test]
fn funnel_band_routes_only_controller_real_outputs() {
    let inputs = funnel_points(INPUTS);
    let outputs = funnel_points(OUTPUTS);
    let instants = validation_instants();
    funnel_band_policy::route_real_outputs_through_funnel_band(
        &funnel_band_policy::FunnelRouting {
            sequence: SEQUENCE,
            cxf: FIXTURE.as_bytes(),
            inputs: &inputs,
            outputs: &outputs,
            instants: &instants,
            sample_step: SAMPLE_STEP,
            reference_csv: &reference_path(),
            kind_to_str: kind_name,
        },
    );
}

fn validation_instants() -> Vec<f64> {
    (0..ROWS).map(|index| index as f64 * SAMPLE_STEP).collect()
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
    const fn real(reference_name: &'static str, connector: &'static str) -> Self {
        Self {
            reference_name,
            cdl_name: connector,
            kind: ValueKind::Real,
        }
    }

    const fn integer(reference_name: &'static str, connector: &'static str) -> Self {
        Self {
            reference_name,
            cdl_name: connector,
            kind: ValueKind::Integer,
        }
    }

    const fn boolean(reference_name: &'static str, connector: &'static str) -> Self {
        Self {
            reference_name,
            cdl_name: connector,
            kind: ValueKind::Boolean,
        }
    }

    const fn runtime_real(reference_name: &'static str, cdl_name: &'static str) -> Self {
        Self {
            reference_name,
            cdl_name,
            kind: ValueKind::Real,
        }
    }

    const fn runtime_integer(reference_name: &'static str, cdl_name: &'static str) -> Self {
        Self {
            reference_name,
            cdl_name,
            kind: ValueKind::Integer,
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
        .copied()
        .chain(OUTPUTS.iter().copied())
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
        let provenance = read_json(&signal_provenance_path(output.reference_name));
        assert_eq!(provenance["class_path"], "G36");
        assert_eq!(provenance["scenario"], SEQUENCE);
        assert_eq!(provenance["signal"], output.reference_name);
        assert_eq!(provenance["tier"], "A");
        assert_eq!(provenance["n_samples"], ROWS);
        assert_eq!(provenance["depends_on_oce_blocks"], false);
        assert_eq!(
            provenance["source_files"],
            "Buildings/Controls/OBC/ASHRAE/G36/TerminalUnits/CoolingOnly/Controller.mo"
        );
        assert!(
            provenance["sampling_rationale"]
                .as_str()
                .is_some_and(|note| note.contains("86400-second"))
        );
        assert_eq!(
            json_string_array(&provenance["reference_columns"]),
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
        .unwrap_or_else(|error| panic!("read JSON {} failed: {error}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("parse JSON {} failed: {error}", path.display()))
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
