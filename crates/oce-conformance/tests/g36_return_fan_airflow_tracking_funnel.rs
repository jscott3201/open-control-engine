//! G36 ReturnFanAirflowTracking Tier-A independent-oracle check through the B3 facade driver.

use std::path::{Path, PathBuf};

use oce_conformance::{
    CombiTimeTable, ComparisonMode, ComparisonResult, DriveCadence, DriverInputReplay,
    DriverOptions, PointEnd, PointMapEntry, ReferenceSpec, Tolerances, ValueKind, VerifyConfig,
    drive_trace_with_options,
};
use serde_json::Value;

#[path = "g36_funnel_band/policy.rs"]
mod funnel_band_policy;

const RETURN_FAN_AIRFLOW: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/multizone_vav_return_fan_airflow_tracking.jsonld"
);

const GOLDEN_DIR: &str = "../../tools/golden-gen/goldens/G36";
const SEQUENCE: &str = "multizone_vav_return_fan_airflow_tracking";
const SUPPLY_AIRFLOW: &str =
    "http://example.org#g36.source.multizone_vav_return_fan_airflow_tracking.VAirSup_flow";
const RETURN_AIRFLOW: &str =
    "http://example.org#g36.source.multizone_vav_return_fan_airflow_tracking.VAirRet_flow";
const SUPPLY_FAN_STATUS: &str =
    "http://example.org#g36.source.multizone_vav_return_fan_airflow_tracking.u1SupFan";
const RETURN_FAN_SPEED: &str = "conn#6";
const RETURN_FAN_STATUS: &str =
    "http://example.org#g36.source.multizone_vav_return_fan_airflow_tracking.y1RetFan";

const INPUTS: &[PointSpec] = &[
    PointSpec::real("supply_airflow", SUPPLY_AIRFLOW),
    PointSpec::real("return_airflow", RETURN_AIRFLOW),
    PointSpec::boolean("supply_fan_status", SUPPLY_FAN_STATUS),
];
const OUTPUTS: &[PointSpec] = &[
    PointSpec::real("return_fan_speed", RETURN_FAN_SPEED),
    PointSpec::boolean("return_fan_status", RETURN_FAN_STATUS),
];

#[test]
fn g36_return_fan_airflow_tracking_tier_a_oracle_matches_engine_output() {
    let reference = CombiTimeTable::read(&reference_path())
        .unwrap_or_else(|err| panic!("ReturnFanAirflowTracking reference read failed: {err}"));
    assert_eq!(
        reference.name,
        "G36_multizone_vav_return_fan_airflow_tracking_reference"
    );
    assert_eq!(reference.n_rows, 8);
    assert_eq!(
        reference.col_names.as_deref(),
        Some(
            [
                "time".to_string(),
                "supply_airflow".to_string(),
                "return_airflow".to_string(),
                "supply_fan_status".to_string(),
                "return_fan_speed".to_string(),
                "return_fan_status".to_string(),
            ]
            .as_slice()
        )
    );
    assert_signal_provenance(&reference);

    let run = drive_trace_with_options(
        RETURN_FAN_AIRFLOW.as_bytes(),
        &config(),
        &reference,
        &DriverOptions {
            cadence: DriveCadence::EventAligned {
                instants: (0..=7).map(f64::from).collect(),
            },
            input_replay: DriverInputReplay::ReferenceTable,
            comparison: ComparisonMode::Exact,
        },
    )
    .unwrap_or_else(|err| panic!("ReturnFanAirflowTracking exact driver run failed: {err}"));

    assert_eq!(run.comparisons.len(), OUTPUTS.len());
    for (comparison, output) in run.comparisons.iter().zip(OUTPUTS) {
        assert_eq!(comparison.output, output.cdl_name);
        assert_eq!(comparison.reference_column, output.reference_name);
        assert!(!comparison.masked);
        match &comparison.result {
            ComparisonResult::Exact(result) => {
                assert!(
                    result.passed,
                    "ReturnFanAirflowTracking exact comparison failed: {:?}",
                    result.first_mismatch
                );
                assert_eq!(result.compared_points, 8);
                assert_eq!(result.first_mismatch, None);
            }
            other => panic!("ReturnFanAirflowTracking used non-exact comparison: {other:?}"),
        }
    }
}

/// Route this sequence's Real outputs through the L1 funnel band with the recorded per-signal
/// tolerance (`_spec/07 §8`); any Boolean/Integer outputs stay on the exact oracle above and are
/// excluded from the funnel entirely. Additive to that oracle, which is unchanged.
#[test]
fn funnel_band_routes_return_fan_airflow_tracking_real_outputs() {
    let inputs = funnel_points(INPUTS);
    let outputs = funnel_points(OUTPUTS);
    let instants: Vec<f64> = (0..=7).map(f64::from).collect();
    funnel_band_policy::route_real_outputs_through_funnel_band(
        &funnel_band_policy::FunnelRouting {
            sequence: SEQUENCE,
            cxf: RETURN_FAN_AIRFLOW.as_bytes(),
            inputs: &inputs,
            outputs: &outputs,
            instants: &instants,
            sample_step: 1.0,
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
        sampling: Some(1.0),
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
