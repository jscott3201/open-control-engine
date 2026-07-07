//! G36 Generic.AirEconomizerHighLimits Tier-A independent-oracle checks through the B3 facade driver.

use std::path::{Path, PathBuf};

use oce_conformance::{
    CombiTimeTable, ComparisonMode, ComparisonResult, DriveCadence, DriverInputReplay,
    DriverOptions, PointEnd, PointMapEntry, ReferenceSpec, Tolerances, ValueKind, VerifyConfig,
    drive_trace_with_options,
};
use serde_json::Value;

#[path = "g36_funnel_band/policy.rs"]
mod funnel_band_policy;

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
const HIGH_LIMIT_ASHRAE_DIFFERENTIAL: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_ashrae_differential.jsonld"
);
const HIGH_LIMIT_TITLE24_DIFFERENTIAL_OFFSET_0: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_title24_differential_offset_0.jsonld"
);
const HIGH_LIMIT_TITLE24_DIFFERENTIAL_OFFSET_1: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_title24_differential_offset_1.jsonld"
);
const HIGH_LIMIT_TITLE24_DIFFERENTIAL_OFFSET_2: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_title24_differential_offset_2.jsonld"
);
const HIGH_LIMIT_TITLE24_DIFFERENTIAL_OFFSET_3: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_title24_differential_offset_3.jsonld"
);

const GOLDEN_DIR: &str = "../../tools/golden-gen/goldens/G36";
const SAMPLE_STEP: f64 = 1.0;

const RETURN_AIR_TEMPERATURE: PointSpec = PointSpec::real("return_air_temperature", "");
const FIXED_TEMPERATURE_CUTOFF: PointSpec = PointSpec::real("temperature_cutoff", "conn#0");
const DIFFERENTIAL_TEMPERATURE_CUTOFF: PointSpec = PointSpec::real("temperature_cutoff", "conn#1");
const FIXED_REFERENCE_COLUMNS: &[&str] = &["time", "temperature_cutoff"];
const DIFFERENTIAL_REFERENCE_COLUMNS: &[&str] =
    &["time", "return_air_temperature", "temperature_cutoff"];

#[derive(Clone, Copy)]
struct Case {
    sequence: &'static str,
    fixture: &'static str,
    input: Option<PointSpec>,
    output: PointSpec,
    rows: usize,
    reference_columns: &'static [&'static str],
}

const CASES: &[Case] = &[
    Case {
        sequence: "generic_air_economizer_high_limits_ashrae_fixed_24",
        fixture: HIGH_LIMIT_FIXED_24,
        input: None,
        output: FIXED_TEMPERATURE_CUTOFF,
        rows: 1,
        reference_columns: FIXED_REFERENCE_COLUMNS,
    },
    Case {
        sequence: "generic_air_economizer_high_limits_ashrae_fixed_21",
        fixture: HIGH_LIMIT_FIXED_21,
        input: None,
        output: FIXED_TEMPERATURE_CUTOFF,
        rows: 1,
        reference_columns: FIXED_REFERENCE_COLUMNS,
    },
    Case {
        sequence: "generic_air_economizer_high_limits_ashrae_fixed_18",
        fixture: HIGH_LIMIT_FIXED_18,
        input: None,
        output: FIXED_TEMPERATURE_CUTOFF,
        rows: 1,
        reference_columns: FIXED_REFERENCE_COLUMNS,
    },
    Case {
        sequence: "generic_air_economizer_high_limits_title24_fixed_24",
        fixture: HIGH_LIMIT_TITLE24_FIXED_24,
        input: None,
        output: FIXED_TEMPERATURE_CUTOFF,
        rows: 1,
        reference_columns: FIXED_REFERENCE_COLUMNS,
    },
    Case {
        sequence: "generic_air_economizer_high_limits_title24_fixed_23",
        fixture: HIGH_LIMIT_TITLE24_FIXED_23,
        input: None,
        output: FIXED_TEMPERATURE_CUTOFF,
        rows: 1,
        reference_columns: FIXED_REFERENCE_COLUMNS,
    },
    Case {
        sequence: "generic_air_economizer_high_limits_title24_fixed_22",
        fixture: HIGH_LIMIT_TITLE24_FIXED_22,
        input: None,
        output: FIXED_TEMPERATURE_CUTOFF,
        rows: 1,
        reference_columns: FIXED_REFERENCE_COLUMNS,
    },
    Case {
        sequence: "generic_air_economizer_high_limits_title24_fixed_21",
        fixture: HIGH_LIMIT_TITLE24_FIXED_21,
        input: None,
        output: FIXED_TEMPERATURE_CUTOFF,
        rows: 1,
        reference_columns: FIXED_REFERENCE_COLUMNS,
    },
    Case {
        sequence: "generic_air_economizer_high_limits_ashrae_differential",
        fixture: HIGH_LIMIT_ASHRAE_DIFFERENTIAL,
        input: Some(PointSpec::real(
            RETURN_AIR_TEMPERATURE.reference_name,
            "http://example.org#g36.source.generic_air_economizer_high_limits_ashrae_differential.TRet",
        )),
        output: DIFFERENTIAL_TEMPERATURE_CUTOFF,
        rows: 4,
        reference_columns: DIFFERENTIAL_REFERENCE_COLUMNS,
    },
    Case {
        sequence: "generic_air_economizer_high_limits_title24_differential_offset_0",
        fixture: HIGH_LIMIT_TITLE24_DIFFERENTIAL_OFFSET_0,
        input: Some(PointSpec::real(
            RETURN_AIR_TEMPERATURE.reference_name,
            "http://example.org#g36.source.generic_air_economizer_high_limits_title24_differential_offset_0.TRet",
        )),
        output: DIFFERENTIAL_TEMPERATURE_CUTOFF,
        rows: 4,
        reference_columns: DIFFERENTIAL_REFERENCE_COLUMNS,
    },
    Case {
        sequence: "generic_air_economizer_high_limits_title24_differential_offset_1",
        fixture: HIGH_LIMIT_TITLE24_DIFFERENTIAL_OFFSET_1,
        input: Some(PointSpec::real(
            RETURN_AIR_TEMPERATURE.reference_name,
            "http://example.org#g36.source.generic_air_economizer_high_limits_title24_differential_offset_1.TRet",
        )),
        output: DIFFERENTIAL_TEMPERATURE_CUTOFF,
        rows: 4,
        reference_columns: DIFFERENTIAL_REFERENCE_COLUMNS,
    },
    Case {
        sequence: "generic_air_economizer_high_limits_title24_differential_offset_2",
        fixture: HIGH_LIMIT_TITLE24_DIFFERENTIAL_OFFSET_2,
        input: Some(PointSpec::real(
            RETURN_AIR_TEMPERATURE.reference_name,
            "http://example.org#g36.source.generic_air_economizer_high_limits_title24_differential_offset_2.TRet",
        )),
        output: DIFFERENTIAL_TEMPERATURE_CUTOFF,
        rows: 4,
        reference_columns: DIFFERENTIAL_REFERENCE_COLUMNS,
    },
    Case {
        sequence: "generic_air_economizer_high_limits_title24_differential_offset_3",
        fixture: HIGH_LIMIT_TITLE24_DIFFERENTIAL_OFFSET_3,
        input: Some(PointSpec::real(
            RETURN_AIR_TEMPERATURE.reference_name,
            "http://example.org#g36.source.generic_air_economizer_high_limits_title24_differential_offset_3.TRet",
        )),
        output: DIFFERENTIAL_TEMPERATURE_CUTOFF,
        rows: 4,
        reference_columns: DIFFERENTIAL_REFERENCE_COLUMNS,
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
        assert_eq!(
            reference.n_rows, case.rows,
            "{} reference rows",
            case.sequence
        );
        assert_eq!(
            reference.col_names.as_deref(),
            Some(
                case.reference_columns
                    .iter()
                    .map(|column| (*column).to_string())
                    .collect::<Vec<_>>()
                    .as_slice()
            ),
            "{} reference columns",
            case.sequence
        );
        assert_signal_provenance(case, &reference);

        let run = drive_trace_with_options(
            case.fixture.as_bytes(),
            &config(case),
            &reference,
            &DriverOptions {
                cadence: DriveCadence::EventAligned {
                    instants: (0..case.rows)
                        .map(|tick| tick as f64 * SAMPLE_STEP)
                        .collect(),
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
        assert_eq!(comparison.output, case.output.cdl_name);
        assert_eq!(comparison.reference_column, case.output.reference_name);
        assert!(!comparison.masked);
        match &comparison.result {
            ComparisonResult::Exact(result) => {
                assert!(
                    result.passed,
                    "AirEconomizerHighLimits {} exact comparison failed: {:?}",
                    case.sequence, result.first_mismatch
                );
                assert_eq!(result.compared_points, case.rows);
                assert_eq!(result.first_mismatch, None);
            }
            other => panic!(
                "AirEconomizerHighLimits {} used non-exact comparison: {other:?}",
                case.sequence
            ),
        }
    }
}

/// Route every AirEconomizerHighLimits `temperature_cutoff` (a Real-algebraic `[A]` signal) through
/// the L1 funnel band with the recorded per-signal tolerance (`_spec/07 §8`), asserting the band is
/// the real comparison mechanism and that it actually landed on the routed signal. This is additive
/// to — and file-separate from — the bit-exact oracle and determinism goldens, which stay untouched.
///
/// The 7 single-row fixed cases have `range_y == 0`, so their relative band collapses to exact; the
/// live, falsifiable band is exercised on the 5 multi-row differential cases (see
/// `recorded_last_ulp_band_rejects_out_of_band_cutoff_drift`).
#[test]
fn funnel_band_accepts_air_economizer_cutoff_within_recorded_last_ulp_slack() {
    for &case in CASES {
        let reference =
            CombiTimeTable::read(&reference_path(case.sequence)).unwrap_or_else(|err| {
                panic!(
                    "AirEconomizerHighLimits {} reference read failed: {err}",
                    case.sequence
                )
            });
        let run = drive_trace_with_options(
            case.fixture.as_bytes(),
            &funnel_config(case),
            &reference,
            &DriverOptions {
                cadence: DriveCadence::EventAligned {
                    instants: (0..case.rows)
                        .map(|tick| tick as f64 * SAMPLE_STEP)
                        .collect(),
                },
                input_replay: DriverInputReplay::ReferenceTable,
                comparison: ComparisonMode::Funnel,
            },
        )
        .unwrap_or_else(|err| {
            panic!(
                "AirEconomizerHighLimits {} funnel driver run failed: {err}",
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
        assert_eq!(comparison.output, case.output.cdl_name);
        assert!(
            !comparison.masked,
            "{} cutoff must be an unmasked funnel comparison",
            case.sequence
        );
        // The recorded Real-algebraic band actually resolved onto this signal (not a zero fall-through).
        assert_eq!(
            comparison.tolerance.rtoly.to_bits(),
            funnel_band_policy::REAL_ALGEBRAIC_LAST_ULP.to_bits(),
            "{} recorded rtoly",
            case.sequence
        );
        assert_eq!(
            comparison.tolerance.atoly.to_bits(),
            0.0f64.to_bits(),
            "{} exact-math atoly stays zero",
            case.sequence
        );
        match &comparison.result {
            ComparisonResult::Funnel(result) => {
                assert!(
                    result.passed,
                    "AirEconomizerHighLimits {} funnel comparison failed: {:?}",
                    case.sequence, result.first_failure_x
                );
                assert_eq!(
                    result.compared_points, case.rows,
                    "{} funnel compared points (non-vacuous)",
                    case.sequence
                );
                assert_eq!(
                    result.max_error.to_bits(),
                    0.0f64.to_bits(),
                    "{} funnel max error",
                    case.sequence
                );
                assert_eq!(
                    result.first_failure_x, None,
                    "{} funnel first failure",
                    case.sequence
                );
            }
            other => panic!(
                "AirEconomizerHighLimits {} used non-funnel comparison: {other:?}",
                case.sequence
            ),
        }
    }
}

/// Anti-tautology control: the recorded band is falsifiable, not decorative. Uses a multi-row
/// differential reference (`range_y > 0`, so the relative last-ULP band is live) and checks — on the
/// engine-independent oracle values only — that a within-band drift passes and an out-of-band drift
/// fails. Without this, routing the bit-exact-matching engine trace through the band would certify
/// nothing.
#[test]
fn recorded_last_ulp_band_rejects_out_of_band_cutoff_drift() {
    let case = CASES
        .iter()
        .find(|case| case.rows > 1)
        .expect("a multi-row differential case exercises the relative band");
    let reference = CombiTimeTable::read(&reference_path(case.sequence))
        .unwrap_or_else(|err| panic!("{} reference read failed: {err}", case.sequence));
    let columns = reference.col_names.as_ref().expect("reference columns");
    let cutoff_col = columns
        .iter()
        .position(|name| name == case.output.reference_name)
        .expect("temperature_cutoff column present");
    let times: Vec<f64> = (0..reference.n_rows)
        .map(|row| reference.data[row * reference.n_cols])
        .collect();
    let cutoff: Vec<f64> = (0..reference.n_rows)
        .map(|row| reference.data[row * reference.n_cols + cutoff_col])
        .collect();
    funnel_band_policy::assert_real_algebraic_band_is_falsifiable(&times, &cutoff);
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

fn config(case: Case) -> VerifyConfig {
    VerifyConfig {
        references: vec![ReferenceSpec {
            model: "g36".to_string(),
            sequence: case.sequence.to_string(),
            point_name_mapping: point_mapping(case),
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

fn funnel_config(case: Case) -> VerifyConfig {
    VerifyConfig {
        references: vec![ReferenceSpec {
            model: "g36".to_string(),
            sequence: case.sequence.to_string(),
            point_name_mapping: point_mapping(case),
        }],
        tolerances: funnel_band_policy::zero_base(),
        outputs: vec![funnel_band_policy::real_algebraic_override(
            case.output.cdl_name,
        )],
        indicators: Vec::new(),
        sampling: Some(SAMPLE_STEP),
        run_controller: true,
    }
}

fn point_mapping(case: Case) -> Vec<PointMapEntry> {
    let mut points = Vec::new();
    if let Some(input) = case.input {
        points.push(input);
    }
    points.push(case.output);
    points
        .into_iter()
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

fn assert_signal_provenance(case: Case, reference: &CombiTimeTable) {
    let prov = read_json(&signal_provenance_path(
        case.sequence,
        case.output.reference_name,
    ));
    assert_eq!(prov["class_path"], "G36");
    assert_eq!(prov["scenario"], case.sequence);
    assert_eq!(prov["signal"], case.output.reference_name);
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
