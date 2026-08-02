//! Source-verified ASHRAE G36 Economizers.Controller restricted variant through the frozen facade.

use oce_api::{CollectSpec, Engine, InputSource, SimMetrics, SimSpec, Value};

const ECONOMIZER_CONTROLLER: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.jsonld"
);

const OUTDOOR_AIRFLOW_NORMALIZED: &str = "http://example.org#g36.source.multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.VOut_flow_normalized";
const MINIMUM_OUTDOOR_AIRFLOW_SETPOINT_NORMALIZED: &str = "http://example.org#g36.source.multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.VOutMinSet_flow_normalized";
const SUPPLY_TEMPERATURE_SIGNAL: &str = "http://example.org#g36.source.multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.uTSup";
const OUTDOOR_AIR_TEMPERATURE: &str = "http://example.org#g36.source.multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.TOut";
const SUPPLY_FAN_STATUS: &str = "http://example.org#g36.source.multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.u1SupFan";
const OPERATION_MODE: &str = "http://example.org#g36.source.multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.uOpeMod";
const FREEZE_PROTECTION_STAGE: &str = "http://example.org#g36.source.multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.uFreProSta";

const OUTDOOR_DAMPER_MIN_LIMIT: &str = "conn#16";
const MINIMUM_OUTDOOR_AIR_LOOP_ENABLED: &str = "conn#39";
const OUTDOOR_DAMPER_COMMAND: &str = "conn#109";
const RETURN_DAMPER_COMMAND: &str = "conn#112";

const EXPECTED_TIMES: [f64; 24] = [
    0.0, 60.0, 120.0, 180.0, 240.0, 300.0, 360.0, 420.0, 480.0, 540.0, 600.0, 660.0, 720.0, 780.0,
    840.0, 900.0, 960.0, 1020.0, 1080.0, 1140.0, 1200.0, 1260.0, 1320.0, 1380.0,
];
const OUTDOOR_AIRFLOW_NORMALIZED_VALUES: [f64; 24] = [0.0; 24];
const MINIMUM_OUTDOOR_AIRFLOW_SETPOINT_NORMALIZED_VALUES: [f64; 24] = [
    0.2, 0.2, 0.2, 0.2, 0.2, 0.2, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.2, 0.2, 0.2, 0.2,
    0.2, 0.2, 0.2, 0.2, 0.2,
];
const SUPPLY_TEMPERATURE_SIGNAL_VALUES: [f64; 24] = [
    -0.5, -0.25, -0.125, 0.0, 0.125, 0.25, 0.5, -0.5, -0.25, -0.125, 0.0, 0.125, 0.25, 0.5, -0.5,
    -0.25, -0.125, 0.0, 0.125, 0.25, 0.5, -0.5, -0.25, -0.125,
];
const OUTDOOR_AIR_TEMPERATURE_VALUES: [f64; 24] = [
    293.0, 295.0, 295.0, 295.0, 295.0, 295.0, 295.0, 295.0, 295.0, 295.0, 295.0, 295.0, 295.0,
    295.0, 295.0, 295.0, 293.0, 293.0, 293.0, 293.0, 293.0, 293.0, 293.0, 293.0,
];
const SUPPLY_FAN_STATUS_VALUES: [bool; 24] = [
    true, true, true, true, true, true, true, true, true, true, true, false, false, false, false,
    true, true, true, true, true, true, true, true, true,
];
const OPERATION_MODE_VALUES: [i64; 24] = [
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 1, 1,
];
const FREEZE_PROTECTION_STAGE_VALUES: [i64; 24] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0,
];

const EXPECTED_OUTDOOR_DAMPER_MIN_LIMIT: [f64; 24] = [
    0.4, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0,
    1.0, 0.0, 0.0, 0.0, 1.0,
];
const EXPECTED_MINIMUM_OUTDOOR_AIR_LOOP_ENABLED: [bool; 24] = [
    true, true, true, true, true, true, true, true, true, true, true, false, false, false, false,
    true, true, true, true, true, false, false, true, true,
];
const EXPECTED_OUTDOOR_DAMPER_COMMAND: [f64; 24] = [
    0.4, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.5, 1.0, 1.0,
    1.0, 0.0, 0.0, 0.0, 1.0,
];
const EXPECTED_RETURN_DAMPER_COMMAND: [f64; 24] = [
    1.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.5,
    0.0, 1.0, 1.0, 1.0, 0.0,
];

type ScheduleSignature = (Vec<u32>, Vec<u32>, Vec<u32>);

fn pair(path: &str, value: Value) -> (String, Value) {
    (path.to_string(), value)
}

fn row_index(t: f64) -> usize {
    EXPECTED_TIMES
        .iter()
        .position(|expected| expected.to_bits() == t.to_bits())
        .unwrap_or_else(|| panic!("unexpected test instant {t}"))
}

fn economizer_controller_inputs(t: f64) -> Vec<(String, Value)> {
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
        pair(
            SUPPLY_TEMPERATURE_SIGNAL,
            Value::Real(SUPPLY_TEMPERATURE_SIGNAL_VALUES[row]),
        ),
        pair(
            OUTDOOR_AIR_TEMPERATURE,
            Value::Real(OUTDOOR_AIR_TEMPERATURE_VALUES[row]),
        ),
        pair(
            SUPPLY_FAN_STATUS,
            Value::Boolean(SUPPLY_FAN_STATUS_VALUES[row]),
        ),
        pair(OPERATION_MODE, Value::Integer(OPERATION_MODE_VALUES[row])),
        pair(
            FREEZE_PROTECTION_STAGE,
            Value::Integer(FREEZE_PROTECTION_STAGE_VALUES[row]),
        ),
    ]
}

fn load_economizer_controller() -> Engine {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(ECONOMIZER_CONTROLLER.as_bytes())
        .expect("source-verified G36 Economizers.Controller fixture loads");
    assert_eq!(report.block_count, 44);
    assert_eq!(report.stateful_blocks, 5);
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
            t_stop: 1380.0,
            step: 60.0,
            inputs: InputSource::Closure(Box::new(economizer_controller_inputs)),
            collect: CollectSpec::Named {
                points: vec![
                    OUTDOOR_DAMPER_MIN_LIMIT.to_string(),
                    MINIMUM_OUTDOOR_AIR_LOOP_ENABLED.to_string(),
                    OUTDOOR_DAMPER_COMMAND.to_string(),
                    RETURN_DAMPER_COMMAND.to_string(),
                ],
                stride: 1,
            },
        })
        .expect("G36 Economizers.Controller simulates");
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
fn multizone_vav_economizer_controller_loads_simulates_and_is_deterministic() {
    let engine = load_economizer_controller();
    let points = engine
        .io()
        .iter()
        .map(|point| point.path.clone())
        .collect::<Vec<_>>();
    for input in [
        OUTDOOR_AIRFLOW_NORMALIZED,
        MINIMUM_OUTDOOR_AIRFLOW_SETPOINT_NORMALIZED,
        SUPPLY_TEMPERATURE_SIGNAL,
        OUTDOOR_AIR_TEMPERATURE,
        SUPPLY_FAN_STATUS,
        OPERATION_MODE,
        FREEZE_PROTECTION_STAGE,
    ] {
        assert!(
            points.contains(&input.to_string()),
            "missing facade input {input}"
        );
        assert_eq!(
            points.iter().filter(|path| path.as_str() == input).count(),
            1,
            "source input {input} should expose one logical facade point"
        );
    }
    for output in [
        OUTDOOR_DAMPER_MIN_LIMIT,
        MINIMUM_OUTDOOR_AIR_LOOP_ENABLED,
        OUTDOOR_DAMPER_COMMAND,
        RETURN_DAMPER_COMMAND,
    ] {
        assert!(
            points.contains(&output.to_string()),
            "missing runtime output {output}"
        );
    }

    let (schedule, metrics) = simulate(engine);
    assert_real_bits(
        &real_column(&metrics, OUTDOOR_DAMPER_MIN_LIMIT),
        &EXPECTED_OUTDOOR_DAMPER_MIN_LIMIT,
        "outdoor damper min limit",
    );
    assert_eq!(
        bool_column(&metrics, MINIMUM_OUTDOOR_AIR_LOOP_ENABLED),
        EXPECTED_MINIMUM_OUTDOOR_AIR_LOOP_ENABLED,
        "minimum outdoor air loop enabled diverged"
    );
    assert_real_bits(
        &real_column(&metrics, OUTDOOR_DAMPER_COMMAND),
        &EXPECTED_OUTDOOR_DAMPER_COMMAND,
        "outdoor damper command",
    );
    assert_real_bits(
        &real_column(&metrics, RETURN_DAMPER_COMMAND),
        &EXPECTED_RETURN_DAMPER_COMMAND,
        "return damper command",
    );

    let (second_schedule, second_metrics) = simulate(load_economizer_controller());
    assert_eq!(schedule, second_schedule);
    assert_trace_bit_eq(&metrics, &second_metrics);
}
