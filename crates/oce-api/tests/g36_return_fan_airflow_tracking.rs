//! Source-verified ASHRAE G36 ReturnFanAirflowTracking through the frozen facade.

use oce_api::{CollectSpec, Engine, InputSource, PointDirection, SimMetrics, SimSpec, Value};

const RETURN_FAN_AIRFLOW: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/multizone_vav_return_fan_airflow_tracking.jsonld"
);

const SUPPLY_AIRFLOW: &str =
    "http://example.org#g36.source.multizone_vav_return_fan_airflow_tracking.VAirSup_flow";
const RETURN_AIRFLOW: &str =
    "http://example.org#g36.source.multizone_vav_return_fan_airflow_tracking.VAirRet_flow";
const SUPPLY_FAN_ON: &str =
    "http://example.org#g36.source.multizone_vav_return_fan_airflow_tracking.u1SupFan";
const RETURN_FAN_SPEED: &str = "conn#6";
const RETURN_FAN_STATUS: &str =
    "http://example.org#g36.source.multizone_vav_return_fan_airflow_tracking.y1RetFan";

const EXPECTED_TIMES: [f64; 8] = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];

type ScheduleSignature = (Vec<u32>, Vec<u32>, Vec<u32>);

fn pair(path: &str, value: Value) -> (String, Value) {
    (path.to_string(), value)
}

fn input_row(t: f64) -> (f64, f64, bool) {
    match t as u32 {
        0 => (5.0, 4.0, false),
        1 => (5.25, 4.0, true),
        2 => (5.0, 4.0, true),
        3 => (4.75, 4.0, true),
        4 => (5.5, 4.0, false),
        5 => (5.0, 4.0, true),
        _ => (4.5, 4.0, true),
    }
}

fn return_fan_inputs(t: f64) -> Vec<(String, Value)> {
    let (supply_airflow, return_airflow, supply_fan_on) = input_row(t);
    vec![
        pair(SUPPLY_AIRFLOW, Value::Real(supply_airflow)),
        pair(RETURN_AIRFLOW, Value::Real(return_airflow)),
        pair(SUPPLY_FAN_ON, Value::Boolean(supply_fan_on)),
    ]
}

fn expected_trace() -> (Vec<f64>, Vec<bool>) {
    const DIF_FLOW_SET: f64 = 1.0;
    const K: f64 = 1.0;
    const TI: f64 = 0.5;
    const NI: f64 = 0.9;
    const Y_MIN: f64 = 0.0;
    const Y_MAX: f64 = 1.0;

    let mut integral = 0.0;
    let mut previous_time: Option<f64> = None;
    let mut speed = Vec::with_capacity(EXPECTED_TIMES.len());
    let mut status = Vec::with_capacity(EXPECTED_TIMES.len());

    for &t in &EXPECTED_TIMES {
        let (supply_airflow, return_airflow, supply_fan_on) = input_row(t);
        let error = (supply_airflow - DIF_FLOW_SET) - return_airflow;
        let proportional = K * error;
        let unlimited = proportional + integral;
        let pid = unlimited.clamp(Y_MIN, Y_MAX);
        speed.push(if supply_fan_on { pid } else { 0.0 });
        status.push(supply_fan_on);

        let dt = previous_time.map_or(0.0, |previous| (t - previous).max(0.0));
        let anti_windup_gain = (unlimited - pid) / (K * NI);
        let corrected_error = error - anti_windup_gain;
        integral += (K / TI) * corrected_error * dt;
        previous_time = Some(t);
    }

    (speed, status)
}

fn load_return_fan_airflow() -> Engine {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(RETURN_FAN_AIRFLOW.as_bytes())
        .expect("source-verified G36 ReturnFanAirflowTracking fixture loads");
    assert_eq!(report.block_count, 6);
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
            inputs: InputSource::Closure(Box::new(return_fan_inputs)),
            collect: CollectSpec::Named {
                points: vec![RETURN_FAN_SPEED.to_string(), RETURN_FAN_STATUS.to_string()],
                stride: 1,
            },
        })
        .expect("G36 ReturnFanAirflowTracking simulates");
    assert_eq!(metrics.ticks, 8);
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
fn multizone_vav_return_fan_airflow_tracking_loads_simulates_and_is_deterministic() {
    let engine = load_return_fan_airflow();
    let paths = engine
        .io()
        .iter()
        .map(|point| point.path.clone())
        .collect::<Vec<_>>();
    for input in [SUPPLY_AIRFLOW, RETURN_AIRFLOW, SUPPLY_FAN_ON] {
        assert!(
            paths.contains(&input.to_string()),
            "missing facade input {input}"
        );
    }
    for output in [RETURN_FAN_SPEED, RETURN_FAN_STATUS] {
        assert!(
            paths.contains(&output.to_string()),
            "missing facade output {output}"
        );
    }
    let output_count = engine
        .io()
        .iter()
        .filter(|point| point.direction == PointDirection::Out)
        .count();
    assert_eq!(
        output_count, 6,
        "each active block should expose one output"
    );

    let (schedule_a, metrics_a) = simulate(engine);
    let (schedule_b, metrics_b) = simulate(load_return_fan_airflow());
    let (expected_speed, expected_status) = expected_trace();

    assert_eq!(
        schedule_a, schedule_b,
        "ReturnFanAirflowTracking schedule is deterministic"
    );
    assert_trace_bit_eq(&metrics_a, &metrics_b);
    assert_real_bits(
        &real_column(&metrics_a, RETURN_FAN_SPEED),
        &expected_speed,
        "return fan speed",
    );
    assert_eq!(
        bool_column(&metrics_a, RETURN_FAN_STATUS),
        expected_status,
        "return fan status diverged"
    );
}
