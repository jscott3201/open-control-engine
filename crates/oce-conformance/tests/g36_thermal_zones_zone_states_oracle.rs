//! G36 ThermalZones.ZoneStates Tier-A exact oracle check.
//!
//! There is intentionally no funnel-band test. The policy header states that Boolean and Integer
//! G36 outputs stay on `ComparisonMode::Exact`, never the type-blind funnel. The single
//! `yZonSta` output is Integer, so a vacuous funnel run would add no evidence; this follows the
//! CoolingOnly Alarms and Generic.TimeSuppression exact-only precedents.

use std::path::{Path, PathBuf};

use oce_conformance::{
    CombiTimeTable, ComparisonMode, ComparisonResult, DriveCadence, DriverInputReplay,
    DriverOptions, PointEnd, PointMapEntry, ReferenceSpec, Tolerances, ValueKind, VerifyConfig,
    drive_trace_with_options,
};
use serde_json::Value;

const ZONE_STATES: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/thermal_zones_zone_states.jsonld");

const GOLDEN_DIR: &str = "../../tools/golden-gen/goldens/G36";
const SEQUENCE: &str = "thermal_zones_zone_states";
const ROWS: usize = 44;
const SAMPLE_STEP: f64 = 60.0;

const HEATING_CONTROL: &str = "http://example.org#g36.source.thermal_zones_zone_states.uHea";
const COOLING_CONTROL: &str = "http://example.org#g36.source.thermal_zones_zone_states.uCoo";
const ZONE_STATE: &str = "http://example.org#g36.source.thermal_zones_zone_states.addInt1.y";

const INPUTS: &[PointSpec] = &[
    PointSpec::real("heating_control", HEATING_CONTROL),
    PointSpec::real("cooling_control", COOLING_CONTROL),
];
const OUTPUTS: &[PointSpec] = &[PointSpec::integer("zone_state", ZONE_STATE)];
const REFERENCE_COLUMNS: &[&str] = &["time", "heating_control", "cooling_control", "zone_state"];

#[test]
fn g36_thermal_zones_zone_states_tier_a_oracle_matches_engine_output() {
    let reference = CombiTimeTable::read(&reference_path())
        .unwrap_or_else(|err| panic!("ThermalZones ZoneStates reference read failed: {err}"));
    assert_eq!(reference.name, "G36_thermal_zones_zone_states_reference");
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
        ZONE_STATES.as_bytes(),
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
    .unwrap_or_else(|err| panic!("ThermalZones ZoneStates exact driver run failed: {err}"));

    assert_eq!(run.comparisons.len(), 1);
    let comparison = &run.comparisons[0];
    assert_eq!(comparison.output, ZONE_STATE);
    assert_eq!(comparison.reference_column, "zone_state");
    assert!(!comparison.masked);
    match &comparison.result {
        ComparisonResult::Exact(result) => {
            assert!(
                result.passed,
                "ThermalZones ZoneStates exact comparison failed: {:?}",
                result.first_mismatch
            );
            assert_eq!(result.compared_points, ROWS);
            assert_eq!(result.first_mismatch, None);
        }
        other => panic!("ThermalZones ZoneStates used non-exact comparison: {other:?}"),
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
    let prov = read_json(&signal_provenance_path("zone_state"));
    assert_eq!(prov["class_path"], "G36");
    assert_eq!(prov["scenario"], SEQUENCE);
    assert_eq!(prov["signal"], "zone_state");
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
