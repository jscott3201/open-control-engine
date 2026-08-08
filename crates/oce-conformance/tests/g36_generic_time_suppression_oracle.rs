//! G36 Generic.TimeSuppression Tier-A exact oracle check.
//!
//! There is intentionally no funnel-band test. The policy header states:
//! “Boolean/Integer G36 outputs are compared exactly (`_spec/07 §9.3`) and are kept on
//! `ComparisonMode::Exact`, never routed through the type-blind funnel.” The single `yAftSup`
//! output is Boolean, so a vacuous funnel run would add no evidence; this follows the
//! CoolingOnly Alarms exact-only precedent.

use std::path::{Path, PathBuf};

use oce_conformance::{
    CombiTimeTable, ComparisonMode, ComparisonResult, DriveCadence, DriverInputReplay,
    DriverOptions, PointEnd, PointMapEntry, ReferenceSpec, Tolerances, ValueKind, VerifyConfig,
    drive_trace_with_options,
};
use serde_json::Value;

const TIME_SUPPRESSION: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/generic_time_suppression.jsonld");

const GOLDEN_DIR: &str = "../../tools/golden-gen/goldens/G36";
const SEQUENCE: &str = "generic_time_suppression";
const ROWS: usize = 91;
const SAMPLE_STEP: f64 = 60.0;

const SETPOINT_TEMPERATURE: &str = "http://example.org#g36.source.generic_time_suppression.TSet";
const ZONE_TEMPERATURE: &str = "http://example.org#g36.source.generic_time_suppression.TZon";
const AFTER_SUPPRESSION: &str =
    "http://example.org#g36.source.generic_time_suppression.pasSupTim.y";

const INPUTS: &[PointSpec] = &[
    PointSpec::real("setpoint_temperature", SETPOINT_TEMPERATURE),
    PointSpec::real("zone_temperature", ZONE_TEMPERATURE),
];
const OUTPUTS: &[PointSpec] = &[PointSpec::boolean("after_suppression", AFTER_SUPPRESSION)];
const REFERENCE_COLUMNS: &[&str] = &[
    "time",
    "setpoint_temperature",
    "zone_temperature",
    "after_suppression",
];

#[test]
fn g36_generic_time_suppression_tier_a_oracle_matches_engine_output() {
    let reference = CombiTimeTable::read(&reference_path())
        .unwrap_or_else(|err| panic!("Generic TimeSuppression reference read failed: {err}"));
    assert_eq!(reference.name, "G36_generic_time_suppression_reference");
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
        TIME_SUPPRESSION.as_bytes(),
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
    .unwrap_or_else(|err| panic!("Generic TimeSuppression exact driver run failed: {err}"));

    assert_eq!(run.comparisons.len(), 1);
    let comparison = &run.comparisons[0];
    assert_eq!(comparison.output, AFTER_SUPPRESSION);
    assert_eq!(comparison.reference_column, "after_suppression");
    assert!(!comparison.masked);
    match &comparison.result {
        ComparisonResult::Exact(result) => {
            assert!(
                result.passed,
                "Generic TimeSuppression exact comparison failed: {:?}",
                result.first_mismatch
            );
            assert_eq!(result.compared_points, ROWS);
            assert_eq!(result.first_mismatch, None);
        }
        other => panic!("Generic TimeSuppression used non-exact comparison: {other:?}"),
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
    let prov = read_json(&signal_provenance_path("after_suppression"));
    assert_eq!(prov["class_path"], "G36");
    assert_eq!(prov["scenario"], SEQUENCE);
    assert_eq!(prov["signal"], "after_suppression");
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
