//! Source-verified ASHRAE G36 ReliefDamper through the frozen facade.

use oce_api::{CollectSpec, Engine, InputSource, PointDirection, SimMetrics, SimSpec, Value};

const RELIEF_DAMPER: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/multizone_vav_relief_damper.jsonld");

const BUILDING_PRESSURE: &str = "conn#4";
const SUPPLY_FAN_ON: &str = "conn#1";
const RELIEF_DAMPER_COMMAND: &str = "conn#3";

const EXPECTED_TIMES: [f64; 6] = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0];

type ScheduleSignature = (Vec<u32>, Vec<u32>, Vec<u32>);

fn pair(path: &str, value: Value) -> (String, Value) {
    (path.to_string(), value)
}

fn input_row(t: f64) -> (f64, bool) {
    match t as u32 {
        0 => (10.0, false),
        1 => (12.0, true),
        2 => (13.0, true),
        3 => (14.0, true),
        4 => (15.0, true),
        _ => (20.0, false),
    }
}

fn relief_damper_inputs(t: f64) -> Vec<(String, Value)> {
    let (building_pressure, supply_fan_on) = input_row(t);
    vec![
        pair(BUILDING_PRESSURE, Value::Real(building_pressure)),
        pair(SUPPLY_FAN_ON, Value::Boolean(supply_fan_on)),
    ]
}

fn expected_output(t: f64) -> f64 {
    const DP_BUI_SET: f64 = 12.0;
    const GAIN: f64 = 0.5;

    let (building_pressure, supply_fan_on) = input_row(t);
    if supply_fan_on {
        (GAIN * (building_pressure - DP_BUI_SET)).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn load_relief_damper() -> Engine {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(RELIEF_DAMPER.as_bytes())
        .expect("source-verified G36 ReliefDamper fixture loads");
    assert_eq!(report.block_count, 6);
    assert_eq!(report.stateful_blocks, 0);
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
            t_stop: 5.0,
            step: 1.0,
            inputs: InputSource::Closure(Box::new(relief_damper_inputs)),
            collect: CollectSpec::Named {
                points: vec![RELIEF_DAMPER_COMMAND.to_string()],
                stride: 1,
            },
        })
        .expect("G36 ReliefDamper simulates");
    assert_eq!(metrics.ticks, 6);
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

fn expected_column() -> Vec<f64> {
    EXPECTED_TIMES.iter().map(|&t| expected_output(t)).collect()
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
fn multizone_vav_relief_damper_loads_simulates_and_is_deterministic() {
    let engine = load_relief_damper();
    let paths = engine
        .io()
        .iter()
        .map(|point| point.path.clone())
        .collect::<Vec<_>>();
    for input in [BUILDING_PRESSURE, SUPPLY_FAN_ON] {
        assert!(
            paths.contains(&input.to_string()),
            "missing facade input {input}"
        );
    }
    assert!(
        paths.contains(&RELIEF_DAMPER_COMMAND.to_string()),
        "missing facade output {RELIEF_DAMPER_COMMAND}"
    );
    let output_count = engine
        .io()
        .iter()
        .filter(|point| point.direction == PointDirection::Out)
        .count();
    assert_eq!(
        output_count, 6,
        "each active source block should expose one output"
    );

    let (schedule_a, metrics_a) = simulate(engine);
    let (schedule_b, metrics_b) = simulate(load_relief_damper());

    assert_eq!(
        schedule_a, schedule_b,
        "ReliefDamper schedule is deterministic"
    );
    assert_trace_bit_eq(&metrics_a, &metrics_b);
    assert_real_bits(
        &real_column(&metrics_a, RELIEF_DAMPER_COMMAND),
        &expected_column(),
        "relief damper command",
    );
}
