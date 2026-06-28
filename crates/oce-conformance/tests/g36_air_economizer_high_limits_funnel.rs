//! G36 Generic.AirEconomizerHighLimits Tier-A independent-oracle checks through the B3 facade driver.

use std::path::{Path, PathBuf};

use oce_conformance::{
    CombiTimeTable, ComparisonMode, ComparisonResult, DriveCadence, DriverInputReplay,
    DriverOptions, PointEnd, PointMapEntry, ReferenceSpec, Tolerances, ValueKind, VerifyConfig,
    drive_trace_with_options,
};
use serde_json::Value;

const HIGH_LIMIT_FIXED_24: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_ashrae_fixed_24.jsonld"
);
const HIGH_LIMIT_FIXED_21: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_ashrae_fixed_21.jsonld"
);
const HIGH_LIMIT_FIXED_18: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_ashrae_fixed_18.jsonld"
);
const HIGH_LIMIT_TITLE24_FIXED_24: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_title24_fixed_24.jsonld"
);
const HIGH_LIMIT_TITLE24_FIXED_23: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_title24_fixed_23.jsonld"
);
const HIGH_LIMIT_TITLE24_FIXED_22: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_title24_fixed_22.jsonld"
);
const HIGH_LIMIT_TITLE24_FIXED_21: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_title24_fixed_21.jsonld"
);

const GOLDEN_DIR: &str = "../../tools/golden-gen/goldens/G36";
const ROWS: usize = 1;
const SAMPLE_STEP: f64 = 1.0;

const TEMPERATURE_CUTOFF: PointSpec = PointSpec::real("temperature_cutoff", "conn#0");
const REFERENCE_COLUMNS: &[&str] = &["time", "temperature_cutoff"];

#[derive(Clone, Copy)]
struct Case {
    sequence: &'static str,
    fixture: &'static str,
}

const CASES: &[Case] = &[
    Case {
        sequence: "generic_air_economizer_high_limits_ashrae_fixed_24",
        fixture: HIGH_LIMIT_FIXED_24,
    },
    Case {
        sequence: "generic_air_economizer_high_limits_ashrae_fixed_21",
        fixture: HIGH_LIMIT_FIXED_21,
    },
    Case {
        sequence: "generic_air_economizer_high_limits_ashrae_fixed_18",
        fixture: HIGH_LIMIT_FIXED_18,
    },
    Case {
        sequence: "generic_air_economizer_high_limits_title24_fixed_24",
        fixture: HIGH_LIMIT_TITLE24_FIXED_24,
    },
    Case {
        sequence: "generic_air_economizer_high_limits_title24_fixed_23",
        fixture: HIGH_LIMIT_TITLE24_FIXED_23,
    },
    Case {
        sequence: "generic_air_economizer_high_limits_title24_fixed_22",
        fixture: HIGH_LIMIT_TITLE24_FIXED_22,
    },
    Case {
        sequence: "generic_air_economizer_high_limits_title24_fixed_21",
        fixture: HIGH_LIMIT_TITLE24_FIXED_21,
    },
];

#[test]
fn g36_air_economizer_high_limits_tier_a_oracles_match_engine_output() {
    for &case in CASES {
        let reference =
            CombiTimeTable::read(&reference_path(case.sequence)).unwrap_or_else(|err| {
                panic!(
                    "AirEconomizerHighLimits {} reference read failed: {err}",
                    case.sequence
                )
            });
        assert_eq!(
            reference.name,
            format!("G36_{}_reference", case.sequence),
            "{} reference name",
            case.sequence
        );
        assert_eq!(reference.n_rows, ROWS, "{} reference rows", case.sequence);
        assert_eq!(
            reference.col_names.as_deref(),
            Some(
                REFERENCE_COLUMNS
                    .iter()
                    .map(|column| (*column).to_string())
                    .collect::<Vec<_>>()
                    .as_slice()
            ),
            "{} reference columns",
            case.sequence
        );
        assert_signal_provenance(case.sequence, &reference);

        let run = drive_trace_with_options(
            case.fixture.as_bytes(),
            &config(case.sequence),
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
            panic!(
                "AirEconomizerHighLimits {} exact driver run failed: {err}",
                case.sequence
            )
        });

        assert_eq!(
            run.comparisons.len(),
            1,
            "{} comparison count",
            case.sequence
        );
        let comparison = &run.comparisons[0];
        assert_eq!(comparison.output, TEMPERATURE_CUTOFF.cdl_name);
        assert_eq!(
            comparison.reference_column,
            TEMPERATURE_CUTOFF.reference_name
        );
        assert!(!comparison.masked);
        match &comparison.result {
            ComparisonResult::Exact(result) => {
                assert!(
                    result.passed,
                    "AirEconomizerHighLimits {} exact comparison failed: {:?}",
                    case.sequence, result.first_mismatch
                );
                assert_eq!(result.compared_points, ROWS);
                assert_eq!(result.first_mismatch, None);
            }
            other => panic!(
                "AirEconomizerHighLimits {} used non-exact comparison: {other:?}",
                case.sequence
            ),
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
}

fn config(sequence: &str) -> VerifyConfig {
    VerifyConfig {
        references: vec![ReferenceSpec {
            model: "g36".to_string(),
            sequence: sequence.to_string(),
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
    vec![PointMapEntry {
        cdl: point_end(TEMPERATURE_CUTOFF.cdl_name, TEMPERATURE_CUTOFF.kind),
        device: point_end(TEMPERATURE_CUTOFF.reference_name, TEMPERATURE_CUTOFF.kind),
    }]
}

fn point_end(name: &str, kind: ValueKind) -> PointEnd {
    PointEnd {
        name: name.to_string(),
        unit: None,
        kind: Some(kind_name(kind).to_string()),
    }
}

fn assert_signal_provenance(sequence: &str, reference: &CombiTimeTable) {
    let prov = read_json(&signal_provenance_path(
        sequence,
        TEMPERATURE_CUTOFF.reference_name,
    ));
    assert_eq!(prov["class_path"], "G36");
    assert_eq!(prov["scenario"], sequence);
    assert_eq!(prov["signal"], TEMPERATURE_CUTOFF.reference_name);
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

fn reference_path(sequence: &str) -> PathBuf {
    reference_dir(sequence).join("reference.csv")
}

fn signal_provenance_path(sequence: &str, signal: &str) -> PathBuf {
    reference_dir(sequence).join(format!("{signal}.prov.json"))
}

fn reference_dir(sequence: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(GOLDEN_DIR)
        .join(sequence)
}

fn kind_name(kind: ValueKind) -> &'static str {
    match kind {
        ValueKind::Real => "Real",
        ValueKind::Integer => "Integer",
        ValueKind::Boolean => "Boolean",
    }
}
