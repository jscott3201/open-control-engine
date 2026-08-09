//! Source-verified ASHRAE G36 Economizers.Subsequences.Limits.Common through the frozen facade.

use oce_api::{CollectSpec, Engine, InputSource, PointDirection, SimMetrics, SimSpec, Value};

const ECONOMIZER_LIMITS_COMMON: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/multizone_vav_economizer_limits_common.jsonld");

const OUTDOOR_AIRFLOW_NORMALIZED: &str =
    "http://example.org#g36.source.multizone_vav_economizer_limits_common.VOut_flow_normalized";
const MINIMUM_OUTDOOR_AIRFLOW_SETPOINT_NORMALIZED: &str = "http://example.org#g36.source.multizone_vav_economizer_limits_common.VOutMinSet_flow_normalized";
const OPERATION_MODE: &str =
    "http://example.org#g36.source.multizone_vav_economizer_limits_common.uOpeMod";
const SUPPLY_FAN_STATUS: &str =
    "http://example.org#g36.source.multizone_vav_economizer_limits_common.u1SupFan";

const OUTDOOR_DAMPER_MIN_LIMIT: &str =
    "http://example.org#g36.source.multizone_vav_economizer_limits_common.minOutDam.y";
const OUTDOOR_DAMPER_MAX_LIMIT: &str =
    "http://example.org#g36.source.multizone_vav_economizer_limits_common.outDamPosMaxSwitch.y";
const RETURN_DAMPER_MIN_LIMIT: &str =
    "http://example.org#g36.source.multizone_vav_economizer_limits_common.retDamPosMinSwitch.y";
const RETURN_DAMPER_MAX_LIMIT: &str =
    "http://example.org#g36.source.multizone_vav_economizer_limits_common.minRetDam.y";
const RETURN_DAMPER_PHYSICAL_MAX_LIMIT: &str =
    "http://example.org#g36.source.multizone_vav_economizer_limits_common.retDamPhyPosMaxSig.y";
const MINIMUM_OUTDOOR_AIR_LOOP_ENABLED: &str =
    "http://example.org#g36.source.multizone_vav_economizer_limits_common.and3.y";

const EXPECTED_TIMES: [f64; 8] = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
const OUTDOOR_AIRFLOW_NORMALIZED_VALUES: [f64; 8] = [0.0; 8];
const MINIMUM_OUTDOOR_AIRFLOW_SETPOINT_NORMALIZED_VALUES: [f64; 8] =
    [1.0, 1.0, 1.0, 12.0, 24.0, 8.0, 8.0, 8.0];
const OPERATION_MODE_VALUES: [i64; 8] = [1, 1, 1, 1, 1, 0, 1, 1];
const SUPPLY_FAN_STATUS_VALUES: [bool; 8] = [false, true, true, true, true, true, false, true];

type ScheduleSignature = (Vec<u32>, Vec<u32>, Vec<u32>);

#[derive(Default)]
struct ExpectedTrace {
    outdoor_damper_min_limit: Vec<f64>,
    outdoor_damper_max_limit: Vec<f64>,
    return_damper_min_limit: Vec<f64>,
    return_damper_max_limit: Vec<f64>,
    return_damper_physical_max_limit: Vec<f64>,
    minimum_outdoor_air_loop_enabled: Vec<bool>,
}

fn pair(path: &str, value: Value) -> (String, Value) {
    (path.to_string(), value)
}

fn row_index(t: f64) -> usize {
    EXPECTED_TIMES
        .iter()
        .position(|expected| expected.to_bits() == t.to_bits())
        .unwrap_or_else(|| panic!("unexpected test instant {t}"))
}

fn economizer_limits_common_inputs(t: f64) -> Vec<(String, Value)> {
    let row = row_index(t);
    vec![
        pair(
            OUTDOOR_AIRFLOW_NORMALIZED,
            Value::Real(OUTDOOR_AIRFLOW_NORMALIZED_VALUES[row]),
        ),
        pair(
            MINIMUM_OUTDOOR_AIRFLOW_SETPOINT_NORMALIZED,
            Value::Real(MINIMUM_OUTDOOR_AIRFLOW_SETPOINT_NORMALIZED_VALUES[row]),
        ),
        pair(OPERATION_MODE, Value::Integer(OPERATION_MODE_VALUES[row])),
        pair(
            SUPPLY_FAN_STATUS,
            Value::Boolean(SUPPLY_FAN_STATUS_VALUES[row]),
        ),
    ]
}

fn expected_trace() -> ExpectedTrace {
    const K: f64 = 0.05;
    const TI: f64 = 120.0;
    const NI: f64 = 0.9;
    const Y_MIN: f64 = 0.0;
    const Y_MAX: f64 = 1.0;
    const RESET: f64 = 0.0;
    const U_RET_DAM_MIN: f64 = 0.5;
    const RET_DAM_PHY_MAX: f64 = 1.0;
    const RET_DAM_PHY_MIN: f64 = 0.0;
    const OUT_DAM_PHY_MAX: f64 = 1.0;
    const OUT_DAM_PHY_MIN: f64 = 0.0;
    const OCCUPIED: i64 = 1;

    let mut integral = 0.0;
    let mut previous_time: Option<f64> = None;
    let mut previous_trigger = false;
    let mut trace = ExpectedTrace::default();

    for ((((&t, &measured), &setpoint), &mode), &fan_on) in EXPECTED_TIMES
        .iter()
        .zip(&OUTDOOR_AIRFLOW_NORMALIZED_VALUES)
        .zip(&MINIMUM_OUTDOOR_AIRFLOW_SETPOINT_NORMALIZED_VALUES)
        .zip(&OPERATION_MODE_VALUES)
        .zip(&SUPPLY_FAN_STATUS_VALUES)
    {
        let error = setpoint - measured;
        let proportional = K * error;
        let unlimited = proportional + integral;
        let loop_signal = unlimited.clamp(Y_MIN, Y_MAX);

        let enabled = fan_on && mode == OCCUPIED;
        let disabled = !enabled;
        let outdoor_damper_max = if disabled {
            OUT_DAM_PHY_MIN
        } else {
            OUT_DAM_PHY_MAX
        };
        let return_damper_min = if disabled {
            RET_DAM_PHY_MAX
        } else {
            RET_DAM_PHY_MIN
        };

        trace.outdoor_damper_min_limit.push(buildings_line(
            Y_MIN,
            OUT_DAM_PHY_MIN,
            U_RET_DAM_MIN,
            outdoor_damper_max,
            loop_signal,
        ));
        trace.outdoor_damper_max_limit.push(outdoor_damper_max);
        trace.return_damper_min_limit.push(return_damper_min);
        trace.return_damper_max_limit.push(buildings_line(
            U_RET_DAM_MIN,
            RET_DAM_PHY_MAX,
            Y_MAX,
            return_damper_min,
            loop_signal,
        ));
        trace.return_damper_physical_max_limit.push(RET_DAM_PHY_MAX);
        trace.minimum_outdoor_air_loop_enabled.push(enabled);

        let dt = previous_time.map_or(0.0, |previous| (t - previous).max(0.0));
        if fan_on && !previous_trigger {
            integral = RESET - proportional;
        } else {
            let anti_windup_gain = (unlimited - loop_signal) / (K * NI);
            let corrected_error = error - anti_windup_gain;
            integral += (K / TI) * corrected_error * dt;
        }
        previous_time = Some(t);
        previous_trigger = fan_on;
    }

    trace
}

fn buildings_line(x1: f64, f1: f64, x2: f64, f2: f64, u: f64) -> f64 {
    let x_lim = u.max(x1).min(x2);
    let slope = (f2 - f1) / (x2 - x1);
    let intercept = f2 - slope * x2;
    intercept + slope * x_lim
}

fn load_economizer_limits_common() -> Engine {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(ECONOMIZER_LIMITS_COMMON.as_bytes())
        .expect("source-verified G36 Economizers.Subsequences.Limits.Common fixture loads");
    assert_eq!(report.block_count, 16);
    assert_eq!(report.stateful_blocks, 1);
    assert!(
        report.warnings.is_empty(),
        "fixture should not warn: {:?}",
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

fn simulate(mut engine: Engine) -> (ScheduleSignature, SimMetrics) {
    let schedule = schedule_signature(&engine);
    let metrics = engine
        .simulate(&SimSpec {
            t_start: 0.0,
            t_stop: 7.0,
            step: 1.0,
            inputs: InputSource::Closure(Box::new(economizer_limits_common_inputs)),
            collect: CollectSpec::Named {
                points: vec![
                    OUTDOOR_DAMPER_MIN_LIMIT.to_string(),
                    OUTDOOR_DAMPER_MAX_LIMIT.to_string(),
                    RETURN_DAMPER_MIN_LIMIT.to_string(),
                    RETURN_DAMPER_MAX_LIMIT.to_string(),
                    RETURN_DAMPER_PHYSICAL_MAX_LIMIT.to_string(),
                    MINIMUM_OUTDOOR_AIR_LOOP_ENABLED.to_string(),
                ],
                stride: 1,
            },
        })
        .expect("G36 Economizers.Subsequences.Limits.Common simulates");
    assert_eq!(metrics.ticks, EXPECTED_TIMES.len() as u64);
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
            .collect::<Vec<_>>()
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

fn bool_column(metrics: &SimMetrics, path: &str) -> Vec<bool> {
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
            Value::Boolean(x) => *x,
            other => panic!("{path} must be Boolean, got {other:?}"),
        })
        .collect()
}

fn assert_real_bits(actual: &[f64], expected: &[f64], label: &str) {
    assert_eq!(
        actual.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
        expected.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
        "{label} diverged"
    );
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
fn multizone_vav_economizer_limits_common_loads_simulates_and_is_deterministic() {
    let engine = load_economizer_limits_common();
    let points = engine.io().iter().collect::<Vec<_>>();
    let paths = points
        .iter()
        .map(|point| point.path.clone())
        .collect::<Vec<_>>();
    for input in [
        OUTDOOR_AIRFLOW_NORMALIZED,
        MINIMUM_OUTDOOR_AIRFLOW_SETPOINT_NORMALIZED,
        OPERATION_MODE,
        SUPPLY_FAN_STATUS,
    ] {
        assert!(
            paths.contains(&input.to_string()),
            "missing facade input {input}"
        );
        assert_eq!(
            paths.iter().filter(|path| path.as_str() == input).count(),
            1,
            "source input {input} should expose one logical facade point"
        );
    }
    for output in [
        OUTDOOR_DAMPER_MIN_LIMIT,
        OUTDOOR_DAMPER_MAX_LIMIT,
        RETURN_DAMPER_MIN_LIMIT,
        RETURN_DAMPER_MAX_LIMIT,
        RETURN_DAMPER_PHYSICAL_MAX_LIMIT,
        MINIMUM_OUTDOOR_AIR_LOOP_ENABLED,
    ] {
        assert!(
            paths.contains(&output.to_string()),
            "missing runtime output {output}"
        );
    }
    assert_eq!(
        paths
            .iter()
            .filter(|path| path.as_str() == SUPPLY_FAN_STATUS)
            .count(),
        1,
        "u1SupFan should expose one logical facade input while fanning out internally"
    );
    let output_count = points
        .iter()
        .filter(|point| point.direction == PointDirection::Out)
        .count();
    assert_eq!(
        output_count, 16,
        "each active source child block should expose one runtime output"
    );

    let (schedule_a, metrics_a) = simulate(engine);
    let (schedule_b, metrics_b) = simulate(load_economizer_limits_common());
    let expected = expected_trace();

    assert_eq!(
        schedule_a, schedule_b,
        "Economizers.Subsequences.Limits.Common schedule is deterministic"
    );
    assert_trace_bit_eq(&metrics_a, &metrics_b);
    assert_real_bits(
        &real_column(&metrics_a, OUTDOOR_DAMPER_MIN_LIMIT),
        &expected.outdoor_damper_min_limit,
        "outdoor damper minimum limit",
    );
    assert_real_bits(
        &real_column(&metrics_a, OUTDOOR_DAMPER_MAX_LIMIT),
        &expected.outdoor_damper_max_limit,
        "outdoor damper maximum limit",
    );
    assert_real_bits(
        &real_column(&metrics_a, RETURN_DAMPER_MIN_LIMIT),
        &expected.return_damper_min_limit,
        "return damper minimum limit",
    );
    assert_real_bits(
        &real_column(&metrics_a, RETURN_DAMPER_MAX_LIMIT),
        &expected.return_damper_max_limit,
        "return damper maximum limit",
    );
    assert_real_bits(
        &real_column(&metrics_a, RETURN_DAMPER_PHYSICAL_MAX_LIMIT),
        &expected.return_damper_physical_max_limit,
        "return damper physical maximum limit",
    );
    assert_eq!(
        bool_column(&metrics_a, MINIMUM_OUTDOOR_AIR_LOOP_ENABLED),
        expected.minimum_outdoor_air_loop_enabled,
        "minimum outdoor air loop enabled diverged"
    );
}
