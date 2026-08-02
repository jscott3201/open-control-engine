//! Source-verified ASHRAE G36 PlantRequests through the frozen facade.

use oce_api::{CollectSpec, Engine, InputSource, PointDirection, SimMetrics, SimSpec, Value};

const PLANT_REQUESTS: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/multizone_vav_plant_requests.jsonld");

const SUPPLY_AIR_TEMP: &str = "http://example.org#g36.source.multizone_vav_plant_requests.TAirSup";
const SUPPLY_AIR_SETPOINT: &str =
    "http://example.org#g36.source.multizone_vav_plant_requests.TAirSupSet";
const COOLING_COIL_VALVE: &str =
    "http://example.org#g36.source.multizone_vav_plant_requests.uCooCoiSet";
const HEATING_COIL_VALVE: &str =
    "http://example.org#g36.source.multizone_vav_plant_requests.uHeaCoiSet";
const CHILLED_WATER_RESET: &str =
    "http://example.org#g36.source.multizone_vav_plant_requests.chiWatRes3.y";
const CHILLER_PLANT: &str = "http://example.org#g36.source.multizone_vav_plant_requests.intSwi3.y";
const HOT_WATER_RESET: &str =
    "http://example.org#g36.source.multizone_vav_plant_requests.hotWatRes3.y";
const HOT_WATER_PLANT: &str =
    "http://example.org#g36.source.multizone_vav_plant_requests.intSwi1.y";

const EXPECTED_TIMES: [f64; 20] = [
    0.0, 60.0, 120.0, 180.0, 240.0, 300.0, 360.0, 420.0, 480.0, 540.0, 600.0, 660.0, 720.0, 780.0,
    840.0, 900.0, 960.0, 1020.0, 1080.0, 1140.0,
];
const EXPECTED_CHILLED_WATER_RESET: [i64; 20] =
    [0, 0, 0, 3, 2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const EXPECTED_CHILLER_PLANT: [i64; 20] =
    [0, 0, 0, 0, 0, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const EXPECTED_HOT_WATER_RESET: [i64; 20] =
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 2, 1, 1, 0, 0];
const EXPECTED_HOT_WATER_PLANT: [i64; 20] =
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 0];

type ScheduleSignature = (Vec<u32>, Vec<u32>, Vec<u32>);

fn pair(path: &str, value: Value) -> (String, Value) {
    (path.to_string(), value)
}

fn load_plant_requests() -> Engine {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(PLANT_REQUESTS.as_bytes())
        .expect("source-verified G36 PlantRequests fixture loads");
    assert_eq!(report.block_count, 32);
    assert_eq!(report.stateful_blocks, 18);
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

fn plant_requests_inputs(t: f64) -> Vec<(String, Value)> {
    let row = ((t / 60.0).round() as usize).min(EXPECTED_TIMES.len() - 1);
    let supply_air_setpoint = [
        300.0, 295.0, 295.0, 295.0, 295.0, 300.0, 300.0, 300.0, 300.0, 320.0, 320.0, 320.0, 320.0,
        320.0, 320.0, 310.0, 300.0, 300.0, 300.0, 300.0,
    ][row];
    let supply_air_temperature = [
        300.0, 299.0, 299.0, 299.0, 297.5, 300.0, 300.0, 300.0, 300.0, 300.0, 300.0, 300.0, 300.0,
        300.0, 300.0, 300.0, 300.0, 300.0, 300.0, 300.0,
    ][row];
    let cooling_coil_valve = [
        0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.9, 0.8, 0.05, 0.05, 0.05, 0.05, 0.05, 0.05, 0.05, 0.05,
        0.05, 0.05, 0.05, 0.05,
    ][row];
    let heating_coil_valve = [
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.9,
        0.8, 0.05,
    ][row];

    vec![
        pair(SUPPLY_AIR_TEMP, Value::Real(supply_air_temperature)),
        pair(SUPPLY_AIR_SETPOINT, Value::Real(supply_air_setpoint)),
        pair(COOLING_COIL_VALVE, Value::Real(cooling_coil_valve)),
        pair(HEATING_COIL_VALVE, Value::Real(heating_coil_valve)),
    ]
}

fn simulate(mut engine: Engine) -> (ScheduleSignature, SimMetrics) {
    let schedule = schedule_signature(&engine);
    let metrics = engine
        .simulate(&SimSpec {
            t_start: 0.0,
            t_stop: 1140.0,
            step: 60.0,
            inputs: InputSource::Closure(Box::new(plant_requests_inputs)),
            collect: CollectSpec::Named {
                points: vec![
                    CHILLED_WATER_RESET.to_string(),
                    CHILLER_PLANT.to_string(),
                    HOT_WATER_RESET.to_string(),
                    HOT_WATER_PLANT.to_string(),
                ],
                stride: 1,
            },
        })
        .expect("G36 PlantRequests simulates");
    assert_eq!(metrics.ticks, 20);
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

fn int_column(metrics: &SimMetrics, path: &str) -> Vec<i64> {
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
            Value::Integer(x) => *x,
            other => panic!("{path} must be Integer, got {other:?}"),
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
fn multizone_vav_plant_requests_loads_simulates_and_is_deterministic() {
    let engine = load_plant_requests();
    let paths = engine
        .io()
        .iter()
        .map(|point| point.path.clone())
        .collect::<Vec<_>>();
    for input in [
        SUPPLY_AIR_TEMP,
        SUPPLY_AIR_SETPOINT,
        COOLING_COIL_VALVE,
        HEATING_COIL_VALVE,
    ] {
        assert!(
            paths.contains(&input.to_string()),
            "missing facade input {input}"
        );
    }
    for output in [
        CHILLED_WATER_RESET,
        CHILLER_PLANT,
        HOT_WATER_RESET,
        HOT_WATER_PLANT,
    ] {
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
        output_count, 32,
        "each active source block should expose one output"
    );

    let (schedule_a, metrics_a) = simulate(engine);
    let (schedule_b, metrics_b) = simulate(load_plant_requests());

    assert_eq!(
        schedule_a, schedule_b,
        "PlantRequests schedule is deterministic"
    );
    assert_trace_bit_eq(&metrics_a, &metrics_b);
    assert_eq!(
        int_column(&metrics_a, CHILLED_WATER_RESET),
        EXPECTED_CHILLED_WATER_RESET
    );
    assert_eq!(
        int_column(&metrics_a, CHILLER_PLANT),
        EXPECTED_CHILLER_PLANT
    );
    assert_eq!(
        int_column(&metrics_a, HOT_WATER_RESET),
        EXPECTED_HOT_WATER_RESET
    );
    assert_eq!(
        int_column(&metrics_a, HOT_WATER_PLANT),
        EXPECTED_HOT_WATER_PLANT
    );
}
