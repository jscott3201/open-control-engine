//! G36 VentilationZones ASHRAE 62.1 Setpoints Tier-A oracle and L1 funnel-band checks.
//!
//! Exact conformance covers all four Real outputs. The companion routing test sends every output
//! through the shared uniform `NEAR_ULP_RTOLY` band without selecting or modifying policy.

use std::path::{Path, PathBuf};

use oce_conformance::{
    CombiTimeTable, ComparisonMode, ComparisonResult, DriveCadence, DriverInputReplay,
    DriverOptions, PointEnd, PointMapEntry, ReferenceSpec, Tolerances, ValueKind, VerifyConfig,
    drive_trace_with_options,
};
use serde_json::Value;

#[path = "g36_funnel_band/policy.rs"]
mod funnel_band_policy;

const SETPOINTS: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/ventilation_zones_ashrae62_1_setpoints.jsonld");

const GOLDEN_DIR: &str = "../../tools/golden-gen/goldens/G36";
const SEQUENCE: &str = "ventilation_zones_ashrae62_1_setpoints";
const ROWS: usize = 60;
const SAMPLE_STEP: f64 = 60.0;

const WINDOW_STATUS: &str =
    "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.u1Win";
const OCCUPANCY_STATUS: &str =
    "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.u1Occ";
const OPERATING_MODE: &str =
    "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.uOpeMod";
const CO2_SETPOINT: &str =
    "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.ppmCO2Set";
const CO2_CONCENTRATION: &str =
    "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.ppmCO2";
const ZONE_TEMPERATURE: &str =
    "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.TZon";
const DISCHARGE_AIR_TEMPERATURE: &str =
    "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.TDis";

const ADJUSTED_POPULATION_FLOW: &str =
    "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.modPopBreAir.y";
const OCCUPIED_MINIMUM_FLOW: &str =
    "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.occMinAir.y";
const ADJUSTED_AREA_FLOW: &str =
    "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.modAreBreAir.y";
const MINIMUM_OUTDOOR_AIRFLOW: &str =
    "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.minOA.y";

const INPUTS: &[PointSpec] = &[
    PointSpec::boolean("window_status", WINDOW_STATUS),
    PointSpec::boolean("occupancy_status", OCCUPANCY_STATUS),
    PointSpec::integer("operating_mode", OPERATING_MODE),
    PointSpec::real("co2_setpoint", CO2_SETPOINT),
    PointSpec::real("co2_concentration", CO2_CONCENTRATION),
    PointSpec::real("zone_temperature", ZONE_TEMPERATURE),
    PointSpec::real("discharge_air_temperature", DISCHARGE_AIR_TEMPERATURE),
];
const OUTPUTS: &[PointSpec] = &[
    PointSpec::real("adjusted_population_flow", ADJUSTED_POPULATION_FLOW),
    PointSpec::real("occupied_minimum_flow", OCCUPIED_MINIMUM_FLOW),
    PointSpec::real("adjusted_area_flow", ADJUSTED_AREA_FLOW),
    PointSpec::real("minimum_outdoor_airflow", MINIMUM_OUTDOOR_AIRFLOW),
];
const REFERENCE_COLUMNS: &[&str] = &[
    "time",
    "window_status",
    "occupancy_status",
    "operating_mode",
    "co2_setpoint",
    "co2_concentration",
    "zone_temperature",
    "discharge_air_temperature",
    "adjusted_population_flow",
    "occupied_minimum_flow",
    "adjusted_area_flow",
    "minimum_outdoor_airflow",
];

#[test]
fn g36_ventilation_zones_ashrae62_1_setpoints_tier_a_oracle_matches_engine_output() {
    let reference = CombiTimeTable::read(&reference_path()).unwrap_or_else(|err| {
        panic!("VentilationZones ASHRAE 62.1 Setpoints reference read failed: {err}")
    });
    assert_eq!(
        reference.name,
        "G36_ventilation_zones_ashrae62_1_setpoints_reference"
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
        SETPOINTS.as_bytes(),
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
    .unwrap_or_else(|err| {
        panic!("VentilationZones ASHRAE 62.1 Setpoints exact driver run failed: {err}")
    });

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
                    "VentilationZones ASHRAE 62.1 Setpoints exact comparison failed for {}: {:?}",
                    output.reference_name, result.first_mismatch
                );
                assert_eq!(result.compared_points, ROWS);
                assert_eq!(result.first_mismatch, None);
            }
            other => panic!(
                "VentilationZones ASHRAE 62.1 Setpoints used non-exact comparison: {other:?}"
            ),
        }
    }
}

#[test]
fn funnel_band_routes_ventilation_zones_ashrae62_1_setpoints_real_outputs() {
    let inputs = funnel_points(INPUTS);
    let outputs = funnel_points(OUTPUTS);
    let instants = (0..ROWS)
        .map(|tick| tick as f64 * SAMPLE_STEP)
        .collect::<Vec<_>>();
    funnel_band_policy::route_real_outputs_through_funnel_band(
        &funnel_band_policy::FunnelRouting {
            sequence: SEQUENCE,
            cxf: SETPOINTS.as_bytes(),
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
        assert_eq!(prov["n_samples"], ROWS);
        assert_eq!(prov["depends_on_oce_blocks"], false);
        assert_eq!(
            prov["source_files"],
            "Buildings/Controls/OBC/ASHRAE/G36/VentilationZones/ASHRAE62_1/Setpoints.mo"
        );
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
