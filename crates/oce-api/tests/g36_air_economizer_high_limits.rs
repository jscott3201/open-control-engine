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

const TEMPERATURE_CUTOFF_RUNTIME: &str = "conn#0";
const EXPECTED_TIMES: [f64; 1] = [0.0];

type ScheduleSignature = (Vec<u32>, Vec<u32>, Vec<u32>);

#[derive(Clone, Copy)]
struct Case {
    name: &'static str,
    fixture: &'static str,
    expected_cutoff: f64,
}

const CASES: &[Case] = &[
    Case {
        name: "ASHRAE90_1 FixedDryBulb 1B/2B/3B/3C/4B/4C/5B/5C/6B/7/8",
        fixture: HIGH_LIMIT_FIXED_24,
        expected_cutoff: 297.15,
    },
    Case {
        name: "ASHRAE90_1 FixedDryBulb 5A/6A",
        fixture: HIGH_LIMIT_FIXED_21,
        expected_cutoff: 294.15,
    },
    Case {
        name: "ASHRAE90_1 FixedDryBulb 1A/2A/3A/4A",
        fixture: HIGH_LIMIT_FIXED_18,
        expected_cutoff: 291.15,
    },
    Case {
        name: "California_Title_24 FixedDryBulb 1/3/5/11-16",
        fixture: HIGH_LIMIT_TITLE24_FIXED_24,
        expected_cutoff: 297.15,
    },
    Case {
        name: "California_Title_24 FixedDryBulb 2/4/10",
        fixture: HIGH_LIMIT_TITLE24_FIXED_23,
        expected_cutoff: 296.15,
    },
    Case {
        name: "California_Title_24 FixedDryBulb 6/8/9",
        fixture: HIGH_LIMIT_TITLE24_FIXED_22,
        expected_cutoff: 295.15,
    },
    Case {
        name: "California_Title_24 FixedDryBulb 7",
        fixture: HIGH_LIMIT_TITLE24_FIXED_21,
        expected_cutoff: 294.15,
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
            t_stop: 0.0,
            step: 1.0,
            inputs: InputSource::None,
            collect: CollectSpec::Named {
                points: vec![TEMPERATURE_CUTOFF_RUNTIME.to_string()],
                stride: 1,
            },
        })
        .unwrap_or_else(|err| panic!("{} simulates: {err}", case.name));
    assert_eq!(
        metrics.ticks,
        EXPECTED_TIMES.len() as u64,
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
        EXPECTED_TIMES
            .iter()
            .map(|t| t.to_bits())
            .collect::<Vec<_>>(),
        "{} trace times",
        case.name
    );
    (schedule, metrics)
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
fn g36_air_economizer_high_limits_fixed_dry_bulb_loads_simulates_and_is_deterministic() {
    for &case in CASES {
        let engine = load_high_limit(case);
        let points = engine.io().iter().collect::<Vec<_>>();
        assert_eq!(
            points.len(),
            1,
            "{} should expose one facade point",
            case.name
        );
        assert_eq!(
            points[0].path, TEMPERATURE_CUTOFF_RUNTIME,
            "{} facade path",
            case.name
        );
        assert_eq!(
            points[0].direction,
            PointDirection::Out,
            "{} facade direction",
            case.name
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
            real_column(&metrics_a, TEMPERATURE_CUTOFF_RUNTIME)
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>(),
            vec![case.expected_cutoff.to_bits()],
            "{} cutoff diverged",
            case.name
        );
    }
}
