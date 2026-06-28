//! G36 OutdoorAirFlow Title 24 SumZone Tier-A independent-oracle check through the B3 facade driver.

use std::path::{Path, PathBuf};

use oce_conformance::{
    CombiTimeTable, ComparisonMode, ComparisonResult, DriveCadence, DriverInputReplay,
    DriverOptions, PointEnd, PointMapEntry, ReferenceSpec, Tolerances, ValueKind, VerifyConfig,
    drive_trace_with_options,
};
use serde_json::Value;

const OUTDOOR_AIRFLOW_TITLE24_SUMZONE: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/multizone_vav_outdoor_airflow_title24_sumzone.jsonld"
);

const GOLDEN_DIR: &str = "../../tools/golden-gen/goldens/G36";
const SEQUENCE: &str = "multizone_vav_outdoor_airflow_title24_sumzone";
const U_OPE_MOD_1: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_sumzone.uOpeMod_1";
const U_OPE_MOD_2: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_sumzone.uOpeMod_2";
const ABS_MIN_1: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_sumzone.VZonAbsMin_flow_1";
const ABS_MIN_2: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_sumzone.VZonAbsMin_flow_2";
const ABS_MIN_3: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_sumzone.VZonAbsMin_flow_3";
const DES_MIN_1: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_sumzone.VZonDesMin_flow_1";
const DES_MIN_2: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_sumzone.VZonDesMin_flow_2";
const DES_MIN_3: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_sumzone.VZonDesMin_flow_3";
const CO2_1: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_sumzone.uCO2_1";
const CO2_2: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_sumzone.uCO2_2";
const CO2_3: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_sumzone.uCO2_3";
const SUMMED_ABSOLUTE_MIN_FLOW: &str = "conn#28";
const SUMMED_DESIGN_MIN_FLOW: &str = "conn#31";
const MAX_CO2: &str = "conn#43";

const INPUTS: &[PointSpec] = &[
    PointSpec::integer("operation_mode_1", U_OPE_MOD_1),
    PointSpec::integer("operation_mode_2", U_OPE_MOD_2),
    PointSpec::real("absolute_min_flow_1", ABS_MIN_1),
    PointSpec::real("absolute_min_flow_2", ABS_MIN_2),
    PointSpec::real("absolute_min_flow_3", ABS_MIN_3),
    PointSpec::real("design_min_flow_1", DES_MIN_1),
    PointSpec::real("design_min_flow_2", DES_MIN_2),
    PointSpec::real("design_min_flow_3", DES_MIN_3),
    PointSpec::real("co2_1", CO2_1),
    PointSpec::real("co2_2", CO2_2),
    PointSpec::real("co2_3", CO2_3),
];
const OUTPUTS: &[PointSpec] = &[
    PointSpec::real(
        "summed_absolute_minimum_outdoor_airflow",
        SUMMED_ABSOLUTE_MIN_FLOW,
    ),
    PointSpec::real(
        "summed_design_minimum_outdoor_airflow",
        SUMMED_DESIGN_MIN_FLOW,
    ),
    PointSpec::real("maximum_co2_loop", MAX_CO2),
];
const REFERENCE_COLUMNS: &[&str] = &[
    "time",
    "operation_mode_1",
    "operation_mode_2",
    "absolute_min_flow_1",
    "absolute_min_flow_2",
    "absolute_min_flow_3",
    "design_min_flow_1",
    "design_min_flow_2",
    "design_min_flow_3",
    "co2_1",
    "co2_2",
    "co2_3",
    "summed_absolute_minimum_outdoor_airflow",
    "summed_design_minimum_outdoor_airflow",
    "maximum_co2_loop",
];

#[test]
fn g36_outdoor_airflow_title24_sumzone_tier_a_oracle_matches_engine_output() {
    let reference = CombiTimeTable::read(&reference_path()).unwrap_or_else(|err| {
        panic!("OutdoorAirFlow Title24 SumZone reference read failed: {err}")
    });
    assert_eq!(
        reference.name,
        "G36_multizone_vav_outdoor_airflow_title24_sumzone_reference"
    );
    assert_eq!(reference.n_rows, 6);
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
        OUTDOOR_AIRFLOW_TITLE24_SUMZONE.as_bytes(),
        &config(),
        &reference,
        &DriverOptions {
            cadence: DriveCadence::EventAligned {
                instants: (0..=5).map(f64::from).collect(),
            },
            input_replay: DriverInputReplay::ReferenceTable,
            comparison: ComparisonMode::Exact,
        },
    )
    .unwrap_or_else(|err| panic!("OutdoorAirFlow Title24 SumZone exact driver run failed: {err}"));

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
                    "OutdoorAirFlow Title24 SumZone exact comparison failed for {}: {:?}",
                    output.reference_name, result.first_mismatch
                );
                assert_eq!(result.compared_points, 6);
                assert_eq!(result.first_mismatch, None);
            }
            other => panic!("OutdoorAirFlow Title24 SumZone used non-exact comparison: {other:?}"),
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
