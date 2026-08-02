//! Source-verified ASHRAE G36 Generic.AirEconomizerHighLimits through the frozen facade.

use oce_api::{CollectSpec, Engine, InputSource, PointDirection, SimMetrics, SimSpec, Value};

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

const TEMPERATURE_CUTOFF_RUNTIME: &str = "conn#0";
const DIFFERENTIAL_TEMPERATURE_CUTOFF_RUNTIME: &str = "conn#1";
const FIXED_TIMES: [f64; 1] = [0.0];
const DIFFERENTIAL_TIMES: [f64; 4] = [0.0, 1.0, 2.0, 3.0];
const RETURN_AIR_TEMPERATURES: [f64; 4] = [289.25, 293.15, 297.5, 301.75];

type ScheduleSignature = (Vec<u32>, Vec<u32>, Vec<u32>);

#[derive(Clone, Copy)]
struct Case {
    name: &'static str,
    fixture: &'static str,
    input_path: Option<&'static str>,
    output_path: &'static str,
    expected_cutoff: ExpectedCutoff,
}

#[derive(Clone, Copy)]
enum ExpectedCutoff {
    Fixed(f64),
    ReturnAirOffset(f64),
}

const CASES: &[Case] = &[
    Case {
        name: "ASHRAE90_1 FixedDryBulb 1B/2B/3B/3C/4B/4C/5B/5C/6B/7/8",
        fixture: HIGH_LIMIT_FIXED_24,
        input_path: None,
        output_path: TEMPERATURE_CUTOFF_RUNTIME,
        expected_cutoff: ExpectedCutoff::Fixed(297.15),
    },
    Case {
        name: "ASHRAE90_1 FixedDryBulb 5A/6A",
        fixture: HIGH_LIMIT_FIXED_21,
        input_path: None,
        output_path: TEMPERATURE_CUTOFF_RUNTIME,
        expected_cutoff: ExpectedCutoff::Fixed(294.15),
    },
    Case {
        name: "ASHRAE90_1 FixedDryBulb 1A/2A/3A/4A",
        fixture: HIGH_LIMIT_FIXED_18,
        input_path: None,
        output_path: TEMPERATURE_CUTOFF_RUNTIME,
        expected_cutoff: ExpectedCutoff::Fixed(291.15),
    },
    Case {
        name: "California_Title_24 FixedDryBulb 1/3/5/11-16",
        fixture: HIGH_LIMIT_TITLE24_FIXED_24,
        input_path: None,
        output_path: TEMPERATURE_CUTOFF_RUNTIME,
        expected_cutoff: ExpectedCutoff::Fixed(297.15),
    },
    Case {
        name: "California_Title_24 FixedDryBulb 2/4/10",
        fixture: HIGH_LIMIT_TITLE24_FIXED_23,
        input_path: None,
        output_path: TEMPERATURE_CUTOFF_RUNTIME,
        expected_cutoff: ExpectedCutoff::Fixed(296.15),
    },
    Case {
        name: "California_Title_24 FixedDryBulb 6/8/9",
        fixture: HIGH_LIMIT_TITLE24_FIXED_22,
        input_path: None,
        output_path: TEMPERATURE_CUTOFF_RUNTIME,
        expected_cutoff: ExpectedCutoff::Fixed(295.15),
    },
    Case {
        name: "California_Title_24 FixedDryBulb 7",
        fixture: HIGH_LIMIT_TITLE24_FIXED_21,
        input_path: None,
        output_path: TEMPERATURE_CUTOFF_RUNTIME,
        expected_cutoff: ExpectedCutoff::Fixed(294.15),
    },
    Case {
        name: "ASHRAE90_1 DifferentialDryBulb 5A",
        fixture: HIGH_LIMIT_ASHRAE_DIFFERENTIAL,
        input_path: Some(
            "http://example.org#g36.source.generic_air_economizer_high_limits_ashrae_differential.TRet",
        ),
        output_path: DIFFERENTIAL_TEMPERATURE_CUTOFF_RUNTIME,
        expected_cutoff: ExpectedCutoff::ReturnAirOffset(0.0),
    },
    Case {
        name: "California_Title_24 DifferentialDryBulb 1/3/5/11-16",
        fixture: HIGH_LIMIT_TITLE24_DIFFERENTIAL_OFFSET_0,
        input_path: Some(
            "http://example.org#g36.source.generic_air_economizer_high_limits_title24_differential_offset_0.TRet",
        ),
        output_path: DIFFERENTIAL_TEMPERATURE_CUTOFF_RUNTIME,
        expected_cutoff: ExpectedCutoff::ReturnAirOffset(0.0),
    },
    Case {
        name: "California_Title_24 DifferentialDryBulb 2/4/10",
        fixture: HIGH_LIMIT_TITLE24_DIFFERENTIAL_OFFSET_1,
        input_path: Some(
            "http://example.org#g36.source.generic_air_economizer_high_limits_title24_differential_offset_1.TRet",
        ),
        output_path: DIFFERENTIAL_TEMPERATURE_CUTOFF_RUNTIME,
        expected_cutoff: ExpectedCutoff::ReturnAirOffset(-1.0),
    },
    Case {
        name: "California_Title_24 DifferentialDryBulb 6/8/9",
        fixture: HIGH_LIMIT_TITLE24_DIFFERENTIAL_OFFSET_2,
        input_path: Some(
            "http://example.org#g36.source.generic_air_economizer_high_limits_title24_differential_offset_2.TRet",
        ),
        output_path: DIFFERENTIAL_TEMPERATURE_CUTOFF_RUNTIME,
        expected_cutoff: ExpectedCutoff::ReturnAirOffset(-2.0),
    },
    Case {
        name: "California_Title_24 DifferentialDryBulb 7",
        fixture: HIGH_LIMIT_TITLE24_DIFFERENTIAL_OFFSET_3,
        input_path: Some(
            "http://example.org#g36.source.generic_air_economizer_high_limits_title24_differential_offset_3.TRet",
        ),
        output_path: DIFFERENTIAL_TEMPERATURE_CUTOFF_RUNTIME,
        expected_cutoff: ExpectedCutoff::ReturnAirOffset(-3.0),
    },
];

fn load_high_limit(case: Case) -> Engine {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(case.fixture.as_bytes())
        .unwrap_or_else(|err| panic!("{} fixture loads: {err}", case.name));
    assert_eq!(report.block_count, 1, "{} block count", case.name);
    assert_eq!(
        report.stateful_blocks, 0,
        "{} stateful block count",
        case.name
    );
    assert!(
        report.warnings.is_empty(),
        "{} fixture should not warn: {:?}",
        case.name,
        report.warnings
    );
    engine
}

fn schedule_signature(engine: &Engine) -> ScheduleSignature {
    let schedule = engine.schedule();
    (
        schedule.order.iter().map(|id| id.0).collect(),
        schedule.connector_order.iter().map(|id| id.0).collect(),
        schedule.driver_of.iter().map(|id| id.0).collect(),
    )
}

fn simulate(case: Case, mut engine: Engine) -> (ScheduleSignature, SimMetrics) {
    let schedule = schedule_signature(&engine);
    let metrics = engine
        .simulate(&SimSpec {
            t_start: 0.0,
            t_stop: match case.expected_cutoff {
                ExpectedCutoff::Fixed(_) => 0.0,
                ExpectedCutoff::ReturnAirOffset(_) => 3.0,
            },
            step: 1.0,
            inputs: match case.input_path {
                Some(path) => InputSource::Closure(Box::new(move |t| return_air_inputs(path, t))),
                None => InputSource::None,
            },
            collect: CollectSpec::Named {
                points: vec![case.output_path.to_string()],
                stride: 1,
            },
        })
        .unwrap_or_else(|err| panic!("{} simulates: {err}", case.name));
    let expected_times = expected_times(case);
    assert_eq!(
        metrics.ticks,
        expected_times.len() as u64,
        "{} ticks",
        case.name
    );
    assert_eq!(
        metrics
            .trace
            .times()
            .iter()
            .map(|t| t.to_bits())
            .collect::<Vec<_>>(),
        expected_times
            .iter()
            .map(|t| t.to_bits())
            .collect::<Vec<_>>(),
        "{} trace times",
        case.name
    );
    (schedule, metrics)
}

fn expected_times(case: Case) -> &'static [f64] {
    match case.expected_cutoff {
        ExpectedCutoff::Fixed(_) => &FIXED_TIMES,
        ExpectedCutoff::ReturnAirOffset(_) => &DIFFERENTIAL_TIMES,
    }
}

fn return_air_inputs(input_path: &str, t: f64) -> Vec<(String, Value)> {
    vec![(
        input_path.to_string(),
        Value::Real(RETURN_AIR_TEMPERATURES[t as usize]),
    )]
}

fn expected_cutoffs(case: Case) -> Vec<f64> {
    match case.expected_cutoff {
        ExpectedCutoff::Fixed(cutoff) => vec![cutoff],
        ExpectedCutoff::ReturnAirOffset(offset) => RETURN_AIR_TEMPERATURES
            .iter()
            .map(|&temperature| temperature + offset)
            .collect(),
    }
}

fn real_column(metrics: &SimMetrics, path: &str) -> Vec<f64> {
    let index = metrics
        .trace
        .columns()
        .iter()
        .position(|column| column == path)
        .unwrap_or_else(|| panic!("missing trace column {path}"));
    let column = metrics.trace.column(index).expect("column index is valid");
    column
        .iter()
        .map(|value| match value {
            Value::Real(x) => *x,
            other => panic!("{path} must be Real, got {other:?}"),
        })
        .collect()
}

fn assert_trace_bit_eq(left: &SimMetrics, right: &SimMetrics) {
    assert_eq!(left.trace.columns(), right.trace.columns());
    assert_eq!(
        left.trace
            .times()
            .iter()
            .map(|t| t.to_bits())
            .collect::<Vec<_>>(),
        right
            .trace
            .times()
            .iter()
            .map(|t| t.to_bits())
            .collect::<Vec<_>>()
    );
    for j in 0..left.trace.columns().len() {
        let l = left.trace.column(j).unwrap();
        let r = right.trace.column(j).unwrap();
        for (row, (lv, rv)) in l.iter().zip(r).enumerate() {
            assert!(
                lv.bit_eq(rv),
                "{} row {row} diverged: {lv:?} vs {rv:?}",
                left.trace.columns()[j]
            );
        }
    }
}

#[test]
fn g36_air_economizer_high_limits_loads_simulates_and_is_deterministic() {
    for &case in CASES {
        let engine = load_high_limit(case);
        let points = engine.io().iter().collect::<Vec<_>>();
        let expected_point_count = if case.input_path.is_some() { 2 } else { 1 };
        assert_eq!(
            points.len(),
            expected_point_count,
            "{} facade point count",
            case.name
        );
        if let Some(path) = case.input_path {
            assert!(
                points
                    .iter()
                    .any(|point| point.path == path && point.direction == PointDirection::In),
                "{} should expose return-air input {path}",
                case.name
            );
        }
        assert_eq!(
            points
                .iter()
                .filter(|point| point.path == case.output_path
                    && point.direction == PointDirection::Out)
                .count(),
            1,
            "{} should expose one temperature cutoff output {}",
            case.name,
            case.output_path
        );

        let (schedule_a, metrics_a) = simulate(case, engine);
        let (schedule_b, metrics_b) = simulate(case, load_high_limit(case));
        assert_eq!(
            schedule_a, schedule_b,
            "{} schedule is deterministic",
            case.name
        );
        assert_trace_bit_eq(&metrics_a, &metrics_b);
        assert_eq!(
            real_column(&metrics_a, case.output_path)
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>(),
            expected_cutoffs(case)
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>(),
            "{} cutoff diverged",
            case.name
        );
    }
}
