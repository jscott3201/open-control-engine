//! Source-verified ASHRAE G36 ReliefFan through the frozen facade.

use oce_api::{CollectSpec, Engine, InputSource, PointDirection, SimMetrics, SimSpec, Value};

const RELIEF_FAN: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/multizone_vav_relief_fan.jsonld");

const BUILDING_PRESSURE: &str = "http://example.org#g36.source.multizone_vav_relief_fan.dpBui";
const SUPPLY_FAN_ON: &str = "http://example.org#g36.source.multizone_vav_relief_fan.u1SupFan";
const AVERAGED_PRESSURE: &str = "conn#1";
const RELIEF_DAMPER_STATUS: &str = "conn#38";
const RELIEF_FAN_STATUS: &str = "conn#41";
const RELIEF_FAN_SPEED: &str = "conn#46";

const EXPECTED_TIMES: [f64; 30] = [
    0.0, 60.0, 120.0, 180.0, 240.0, 300.0, 360.0, 420.0, 480.0, 540.0, 600.0, 660.0, 720.0, 780.0,
    840.0, 900.0, 960.0, 1020.0, 1080.0, 1140.0, 1200.0, 1260.0, 1320.0, 1380.0, 1440.0, 1500.0,
    1560.0, 1620.0, 1680.0, 1740.0,
];
const EXPECTED_AVERAGED_PRESSURE: [f64; 30] = [
    0.0,
    11.999800003333279,
    11.999900000833327,
    11.999933333703702,
    11.999950000208333,
    13.2,
    14.4,
    15.6,
    16.8,
    18.0,
    18.0,
    18.0,
    18.0,
    18.0,
    18.0,
    18.0,
    18.0,
    18.0,
    16.8,
    15.6,
    14.4,
    13.2,
    12.0,
    12.0,
    12.0,
    12.0,
    12.0,
    12.0,
    12.0,
    12.0,
];
const EXPECTED_RELIEF_DAMPER_STATUS: [bool; 30] = [
    false, false, false, false, false, true, true, true, true, true, true, true, true, true, true,
    true, true, true, true, true, true, true, true, true, true, true, true, false, false, false,
];
const EXPECTED_RELIEF_FAN_STATUS: [bool; 30] = [
    false, false, false, false, false, false, false, false, false, false, false, false, false,
    false, true, true, true, true, true, true, true, true, true, true, true, true, false, false,
    false, false,
];
const EXPECTED_RELIEF_FAN_SPEED: [f64; 30] = [
    0.0,
    0.0,
    0.0,
    0.0,
    0.0,
    0.0,
    0.0,
    0.0,
    0.0,
    0.0,
    0.0,
    0.0,
    0.0,
    0.0,
    0.5,
    0.5,
    0.5,
    0.5,
    0.40000000000000013,
    0.30000000000000004,
    0.19999999999999996,
    0.09999999999999987,
    0.0,
    0.0,
    0.0,
    0.0,
    0.0,
    0.0,
    0.0,
    0.0,
];

type ScheduleSignature = (Vec<u32>, Vec<u32>, Vec<u32>);

fn pair(path: &str, value: Value) -> (String, Value) {
    (path.to_string(), value)
}

fn input_row(t: f64) -> (f64, bool) {
    let building_pressure = if t < 300.0 {
        12.0
    } else if t <= 1020.0 {
        18.0
    } else {
        12.0
    };
    (building_pressure, t >= 300.0)
}

fn relief_fan_inputs(t: f64) -> Vec<(String, Value)> {
    let (building_pressure, supply_fan_on) = input_row(t);
    vec![
        pair(BUILDING_PRESSURE, Value::Real(building_pressure)),
        pair(SUPPLY_FAN_ON, Value::Boolean(supply_fan_on)),
    ]
}

fn load_relief_fan() -> Engine {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(RELIEF_FAN.as_bytes())
        .expect("source-verified G36 ReliefFan fixture loads");
    assert_eq!(report.block_count, 19);
    assert_eq!(report.stateful_blocks, 10);
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
            t_stop: 1740.0,
            step: 60.0,
            inputs: InputSource::Closure(Box::new(relief_fan_inputs)),
            collect: CollectSpec::Named {
                points: vec![
                    AVERAGED_PRESSURE.to_string(),
                    RELIEF_DAMPER_STATUS.to_string(),
                    RELIEF_FAN_STATUS.to_string(),
                    RELIEF_FAN_SPEED.to_string(),
                ],
                stride: 1,
            },
        })
        .expect("G36 ReliefFan simulates");
    assert_eq!(metrics.ticks, 30);
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
fn multizone_vav_relief_fan_loads_simulates_and_is_deterministic() {
    let engine = load_relief_fan();
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
    for output in [
        AVERAGED_PRESSURE,
        RELIEF_DAMPER_STATUS,
        RELIEF_FAN_STATUS,
        RELIEF_FAN_SPEED,
    ] {
        assert!(
            paths.contains(&output.to_string()),
            "missing facade output {output}"
        );
    }
    assert_eq!(
        paths
            .iter()
            .filter(|path| path.as_str() == SUPPLY_FAN_ON)
            .count(),
        1,
        "u1SupFan should expose one logical facade input while fanning out internally"
    );
    let output_count = engine
        .io()
        .iter()
        .filter(|point| point.direction == PointDirection::Out)
        .count();
    assert_eq!(
        output_count, 22,
        "all active block outputs should be exposed"
    );

    let (schedule_a, metrics_a) = simulate(engine);
    let (schedule_b, metrics_b) = simulate(load_relief_fan());

    assert_eq!(
        schedule_a, schedule_b,
        "ReliefFan schedule is deterministic"
    );
    assert_trace_bit_eq(&metrics_a, &metrics_b);
    assert_real_bits(
        &real_column(&metrics_a, AVERAGED_PRESSURE),
        &EXPECTED_AVERAGED_PRESSURE,
        "averaged building pressure",
    );
    assert_eq!(
        bool_column(&metrics_a, RELIEF_DAMPER_STATUS),
        EXPECTED_RELIEF_DAMPER_STATUS
    );
    assert_eq!(
        bool_column(&metrics_a, RELIEF_FAN_STATUS),
        EXPECTED_RELIEF_FAN_STATUS
    );
    assert_real_bits(
        &real_column(&metrics_a, RELIEF_FAN_SPEED),
        &EXPECTED_RELIEF_FAN_SPEED,
        "relief fan speed",
    );
}
