//! Golden-trace driver coverage through the frozen `oce-api` facade.

use std::fs;
use std::path::Path;

use oce_api::{Engine, OcError, PointDirection, PointValueType};
use oce_conformance::{
    CombiTimeTable, ComparisonMode, ComparisonResult, ConfigError, DriveCadence, DriveMode,
    DriverError, DriverInputReplay, DriverOptions, IndicatorPattern, PartialTolerances, PointEnd,
    PointMapEntry, ReferenceSpec, Tolerances, VerifyConfig, drive_trace, drive_trace_with_options,
};

const FREE_ADD: &str = include_str!("fixtures/driver/free_add.jsonld");
const FREE_DIVIDE: &str = include_str!("fixtures/driver/free_divide.jsonld");
const U1: &str = "http://example.org#DriverAdd.u1";
const U2: &str = "http://example.org#DriverAdd.u2";
const DIVIDE_U1: &str = "http://example.org#DriverDivide.u1";
const DIVIDE_U2: &str = "http://example.org#DriverDivide.u2";
const Y: &str = "conn#2";

fn table(rows: &[(f64, f64, f64)]) -> CombiTimeTable {
    let mut data = Vec::with_capacity(rows.len() * 4);
    for &(t, u1, u2) in rows {
        data.extend([t, u1, u2, u1 + u2]);
    }
    CombiTimeTable {
        name: "driver_reference".to_string(),
        n_rows: rows.len(),
        n_cols: 4,
        data,
        col_names: Some(vec![
            "time".to_string(),
            "u1".to_string(),
            "u2".to_string(),
            "y".to_string(),
        ]),
    }
}

fn config() -> VerifyConfig {
    VerifyConfig {
        references: vec![ReferenceSpec {
            model: "driver-free-add".to_string(),
            sequence: "single-block-add".to_string(),
            point_name_mapping: vec![map("u1", U1), map("u2", U2), map("y", Y)],
        }],
        tolerances: Tolerances {
            atolx: 0.0,
            atoly: 0.0,
            rtolx: 0.0,
            rtoly: 0.0,
            ltolx: 0.0,
            ltoly: 0.0,
        },
        outputs: vec![oce_conformance::OutputPattern {
            pattern: "conn#2".to_string(),
            tolerances: PartialTolerances::default(),
        }],
        indicators: Vec::new(),
        sampling: Some(1.0),
        run_controller: true,
    }
}

fn divide_config() -> VerifyConfig {
    VerifyConfig {
        references: vec![ReferenceSpec {
            model: "driver-free-divide".to_string(),
            sequence: "single-block-divide".to_string(),
            point_name_mapping: vec![map("u1", DIVIDE_U1), map("u2", DIVIDE_U2), map("y", Y)],
        }],
        tolerances: Tolerances {
            atolx: 0.0,
            atoly: 0.0,
            rtolx: 0.0,
            rtoly: 0.0,
            ltolx: 0.0,
            ltoly: 0.0,
        },
        outputs: vec![oce_conformance::OutputPattern {
            pattern: Y.to_string(),
            tolerances: PartialTolerances::default(),
        }],
        indicators: Vec::new(),
        sampling: Some(1.0),
        run_controller: true,
    }
}

fn config_with_indicator(signal: &str) -> VerifyConfig {
    let mut config = config();
    config.indicators = vec![IndicatorPattern {
        pattern: Y.to_string(),
        signals: vec![signal.to_string()],
    }];
    config
}

fn indicator_output_paths() -> (String, String) {
    let mut engine = Engine::in_memory();
    engine.load_cxf(FREE_ADD.as_bytes()).expect("fixture loads");
    let indicators = engine
        .io()
        .iter()
        .filter(|point| {
            point.direction == PointDirection::Out && point.value_type == PointValueType::Bool
        })
        .map(|point| point.path)
        .collect::<Vec<_>>();
    assert_eq!(
        indicators.len(),
        2,
        "fixture should expose active and inactive Boolean indicators"
    );
    (indicators[0].clone(), indicators[1].clone())
}

fn map(device: &str, cdl: &str) -> PointMapEntry {
    PointMapEntry {
        cdl: PointEnd {
            name: cdl.to_string(),
            unit: None,
            kind: Some("Real".to_string()),
        },
        device: PointEnd {
            name: device.to_string(),
            unit: None,
            kind: Some("Real".to_string()),
        },
    }
}

fn uniform_reference() -> CombiTimeTable {
    table(&[
        (0.0, 1.0, 2.0),
        (1.0, 2.0, 4.0),
        (2.0, 3.0, 6.0),
        (3.0, 4.0, 8.0),
    ])
}

fn nonuniform_reference() -> CombiTimeTable {
    table(&[
        (0.0, 1.0, 2.0),
        (0.5, 3.0, 5.0),
        (2.0, -1.0, 7.0),
        (3.5, 4.5, 0.25),
    ])
}

fn zero_row_reference() -> CombiTimeTable {
    CombiTimeTable::parse("#1\n# columns: time u1 u2 y\ndouble driver_reference(0,4)\n")
        .expect("zero-row CombiTimeTable is syntactically valid CSV")
}

fn divide_reference() -> CombiTimeTable {
    CombiTimeTable::parse(
        "\
#1
# columns: time u1 u2 y
double driver_divide_reference(4,4)
0 1 2 0.5
1 1 0 inf
2 -1 0 -inf
3 0 0 NaN
",
    )
    .expect("non-finite divide reference parses")
}

fn trace_bits(run: &oce_conformance::DriverRun) -> Vec<Vec<u64>> {
    let mut out = Vec::new();
    out.push(run.trace.times.iter().map(|t| t.to_bits()).collect());
    for column in &run.trace.columns {
        out.push(column.values.iter().map(|v| v.to_bits()).collect());
    }
    out
}

fn assert_trace_bit_eq(left: &oce_conformance::DriverRun, right: &oce_conformance::DriverRun) {
    assert_eq!(left.trace.columns.len(), right.trace.columns.len());
    assert_eq!(trace_bits(left), trace_bits(right));
    for (l, r) in left.trace.columns.iter().zip(&right.trace.columns) {
        assert_eq!(l.name, r.name);
        assert_eq!(l.kind, r.kind);
    }
}

#[test]
fn uniform_fast_path_and_event_aligned_path_agree_bit_exactly() {
    let reference = uniform_reference();
    let config = config();
    let fast = drive_trace(FREE_ADD.as_bytes(), &config, &reference).expect("fast path runs");
    assert!(matches!(
        fast.drive_mode,
        DriveMode::Uniform {
            t_start,
            t_stop,
            step
        } if t_start.to_bits() == 0.0f64.to_bits()
            && t_stop.to_bits() == 3.0f64.to_bits()
            && step.to_bits() == 1.0f64.to_bits()
    ));
    assert_eq!(fast.comparisons.len(), 1);
    assert!(
        fast.comparisons[0].result.passed(),
        "engine trace should match the hand-authored reference: {:?}",
        fast.comparisons[0].result
    );
    assert!(
        fast.comparisons[0].result.compared_points() > 0,
        "comparison must not be a vacuous pass"
    );

    let event = drive_trace_with_options(
        FREE_ADD.as_bytes(),
        &config,
        &reference,
        &DriverOptions {
            cadence: DriveCadence::EventAligned {
                instants: vec![0.0, 1.0, 2.0, 3.0],
            },
            input_replay: DriverInputReplay::ReferenceTable,
            comparison: ComparisonMode::Funnel,
        },
    )
    .expect("event path runs");
    assert_trace_bit_eq(&fast, &event);
}

#[test]
fn event_aligned_path_ticks_every_reference_instant() {
    let reference = nonuniform_reference();
    let config = config();
    let run = drive_trace(FREE_ADD.as_bytes(), &config, &reference).expect("event path runs");
    let DriveMode::EventAligned { instants } = &run.drive_mode else {
        panic!("nonuniform reference times must select the event-aligned path");
    };
    let expected_times = [0.0_f64, 0.5, 2.0, 3.5];
    assert_eq!(
        instants.iter().map(|t| t.to_bits()).collect::<Vec<_>>(),
        expected_times
            .iter()
            .map(|t| t.to_bits())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        run.trace
            .times
            .iter()
            .map(|t| t.to_bits())
            .collect::<Vec<_>>(),
        expected_times
            .iter()
            .map(|t| t.to_bits())
            .collect::<Vec<_>>()
    );
    let y = run.trace.column(Y).expect("captured output y");
    let expected_y = [3.0_f64, 8.0, 6.0, 4.75];
    assert_eq!(
        y.values.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
        expected_y.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
    );
}

#[test]
fn driver_trace_is_deterministic_across_identical_runs() {
    let reference = nonuniform_reference();
    let config = config();
    let a = drive_trace(FREE_ADD.as_bytes(), &config, &reference).expect("first run");
    let b = drive_trace(FREE_ADD.as_bytes(), &config, &reference).expect("second run");
    assert_trace_bit_eq(&a, &b);
    assert_eq!(a.comparisons, b.comparisons);
}

#[test]
fn driver_masked_path_compares_when_indicator_is_active() {
    let reference = uniform_reference();
    let (active_mask, _) = indicator_output_paths();
    let config = config_with_indicator(&active_mask);
    let run = drive_trace(FREE_ADD.as_bytes(), &config, &reference)
        .expect("active indicator should drive masked comparison");
    let comparison = &run.comparisons[0];
    assert!(comparison.masked);
    assert!(comparison.result.passed());
    assert!(
        comparison.result.compared_points() > 0,
        "active mask must still compare points"
    );
    let indicator = run
        .trace
        .column(&active_mask)
        .expect("captured active mask");
    assert_eq!(
        indicator
            .values
            .iter()
            .map(|value| value.to_bits())
            .collect::<Vec<_>>(),
        vec![1.0f64.to_bits(); reference.n_rows]
    );
}

#[test]
fn driver_masked_path_preserves_all_dont_care_vacuous_pass() {
    let reference = uniform_reference();
    let (_, inactive_mask) = indicator_output_paths();
    let config = config_with_indicator(&inactive_mask);
    let run = drive_trace(FREE_ADD.as_bytes(), &config, &reference)
        .expect("inactive indicator should produce a deliberate masked no-op");
    let comparison = &run.comparisons[0];
    assert!(comparison.masked);
    assert!(comparison.result.passed());
    assert_eq!(comparison.result.compared_points(), 0);
    let indicator = run
        .trace
        .column(&inactive_mask)
        .expect("captured inactive mask");
    assert_eq!(
        indicator
            .values
            .iter()
            .map(|value| value.to_bits())
            .collect::<Vec<_>>(),
        vec![0.0f64.to_bits(); reference.n_rows]
    );
}

#[test]
fn driver_rejects_empty_reference_before_vacuous_compare() {
    let reference = zero_row_reference();
    let config = config();
    let err = drive_trace(FREE_ADD.as_bytes(), &config, &reference)
        .expect_err("empty reference must not produce a vacuous passing comparison");
    assert!(matches!(err, DriverError::EmptyReference));
}

#[test]
fn driver_errors_surface_facade_unknown_point_before_any_silent_trace() {
    let reference = uniform_reference();
    let mut config = config();
    config.references[0].point_name_mapping[2].cdl.name =
        "http://example.org#DriverAdd.noSuchOutput".to_string();
    let err = drive_trace(FREE_ADD.as_bytes(), &config, &reference)
        .expect_err("unknown output must propagate as a typed facade error");
    assert!(
        matches!(err, DriverError::Engine(OcError::UnknownPoint(point))
        if point == "http://example.org#DriverAdd.noSuchOutput")
    );
}

#[test]
fn driver_errors_surface_uniform_bad_step_and_csv_deferred() {
    let reference = uniform_reference();
    let config = config();
    let bad_step = drive_trace_with_options(
        FREE_ADD.as_bytes(),
        &config,
        &reference,
        &DriverOptions {
            cadence: DriveCadence::Uniform {
                t_start: 0.0,
                t_stop: 1.0,
                step: 0.0,
            },
            input_replay: DriverInputReplay::ReferenceTable,
            comparison: ComparisonMode::Funnel,
        },
    )
    .expect_err("step <= 0 must be a typed facade Load error");
    assert!(matches!(
        bad_step,
        DriverError::Engine(OcError::Load { .. })
    ));

    let csv = drive_trace_with_options(
        FREE_ADD.as_bytes(),
        &config,
        &reference,
        &DriverOptions {
            cadence: DriveCadence::Uniform {
                t_start: 0.0,
                t_stop: 1.0,
                step: 1.0,
            },
            input_replay: DriverInputReplay::FacadeCsv {
                path: Path::new("reference.csv").to_path_buf(),
                bindings: vec![(U1.to_string(), 1)],
            },
            comparison: ComparisonMode::Funnel,
        },
    )
    .expect_err("facade Csv input source remains deferred");
    assert!(matches!(csv, DriverError::Engine(OcError::Load { .. })));
}

#[test]
fn driver_errors_surface_event_aligned_time_regression() {
    let reference = uniform_reference();
    let config = config();
    let err = drive_trace_with_options(
        FREE_ADD.as_bytes(),
        &config,
        &reference,
        &DriverOptions {
            cadence: DriveCadence::EventAligned {
                instants: vec![0.0, 1.0, 0.5],
            },
            input_replay: DriverInputReplay::ReferenceTable,
            comparison: ComparisonMode::Funnel,
        },
    )
    .expect_err("decreasing explicit event instants must surface the facade time guard");
    assert!(matches!(
        err,
        DriverError::Engine(OcError::TimeRegression { .. })
    ));
}

#[test]
fn conformance_has_no_direct_graph_or_blocks_dependency() {
    let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let manifest = fs::read_to_string(manifest_path).expect("read oce-conformance manifest");
    let dependencies = dependencies_section(&manifest);
    assert!(
        dependencies.iter().any(|line| line.starts_with("oce-api")),
        "B3 must use the frozen oce-api facade directly"
    );
    for forbidden in ["oce-graph", "oce-blocks"] {
        assert!(
            !dependencies.iter().any(|line| line.starts_with(forbidden)),
            "oce-conformance must not directly depend on {forbidden}; transitive deps through \
             oce-api are expected and intentionally ignored"
        );
    }
}

fn dependencies_section(manifest: &str) -> Vec<&str> {
    let mut in_dependencies = false;
    let mut lines = Vec::new();
    for raw in manifest.lines() {
        let line = raw.trim();
        if line == "[dependencies]" {
            in_dependencies = true;
            continue;
        }
        if in_dependencies && line.starts_with('[') {
            break;
        }
        if in_dependencies && !line.is_empty() && !line.starts_with('#') {
            lines.push(line);
        }
    }
    lines
}

#[test]
fn driver_rejects_bad_regex_config_without_running() {
    let reference = uniform_reference();
    let mut config = config();
    config.outputs[0].pattern = "[".to_string();
    let err = drive_trace(FREE_ADD.as_bytes(), &config, &reference)
        .expect_err("invalid regex must be a config error");
    assert!(
        matches!(err, DriverError::Config(ConfigError::Invalid(message))
        if message.contains("regex"))
    );
}

#[test]
fn exact_mode_compares_free_add_bit_exactly() {
    let reference = uniform_reference();
    let config = config();
    let run = drive_trace_with_options(
        FREE_ADD.as_bytes(),
        &config,
        &reference,
        &DriverOptions {
            comparison: ComparisonMode::Exact,
            ..DriverOptions::default()
        },
    )
    .expect("exact mode compares finite add output");

    assert_eq!(run.comparisons.len(), 1);
    assert!(!run.comparisons[0].masked);
    let ComparisonResult::Exact(result) = &run.comparisons[0].result else {
        panic!("exact mode must return an exact verdict");
    };
    assert!(result.passed);
    assert_eq!(result.compared_points, reference.n_rows);
    assert_eq!(result.first_mismatch, None);
}

#[test]
fn exact_mode_compares_non_finite_divide_reference_without_no_compared_points() {
    let reference = divide_reference();
    let config = divide_config();
    let run = drive_trace_with_options(
        FREE_DIVIDE.as_bytes(),
        &config,
        &reference,
        &DriverOptions {
            comparison: ComparisonMode::Exact,
            ..DriverOptions::default()
        },
    )
    .expect("exact mode compares non-finite divide output");

    assert_eq!(run.comparisons.len(), 1);
    let ComparisonResult::Exact(result) = &run.comparisons[0].result else {
        panic!("exact mode must return an exact verdict");
    };
    assert!(result.passed);
    assert_eq!(result.compared_points, reference.n_rows);

    let y = run.trace.column(Y).expect("captured divide output");
    assert_eq!(y.values[0].to_bits(), 0.5_f64.to_bits());
    assert_eq!(y.values[1].to_bits(), f64::INFINITY.to_bits());
    assert_eq!(y.values[2].to_bits(), f64::NEG_INFINITY.to_bits());
    assert!(y.values[3].is_nan());
}
