//! Representative ASHRAE G36 sequence fixtures through the frozen facade:
//! CXF ingest -> validation -> schedule -> simulate. E1 is a smoke/sanity gate for real multi-block
//! sequences; bit-exact oracle comparison against reference traces is deferred to the E2 oracle.

use oce_api::{
    CollectSpec, Engine, InputSource, LoadReport, PointDirection, SimMetrics, SimSpec, Value,
};

const AHU_SAT_RESET: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/ahu_supply_air_temp_reset.jsonld");
const AHU_ECONOMIZER: &str = include_str!("../../oce-cxf/tests/fixtures/g36/ahu_economizer.jsonld");
const VAV_SINGLE_ZONE: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/vav_single_zone.jsonld");
const SUPPLY_TEMPERATURE: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/multizone_vav_supply_temperature.jsonld");
const SUPPLY_FAN: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/multizone_vav_supply_fan.jsonld");

const SAT_ZONE_TEMP: &str = "http://example.org#g36.ahu_supply_air_temp_reset.zone_temp";
const SAT_COOLING_SETPOINT: &str =
    "http://example.org#g36.ahu_supply_air_temp_reset.cooling_setpoint";
const ECON_RETURN_AIR_TEMP: &str = "http://example.org#g36.ahu_economizer.return_air_temp";
const ECON_OUTDOOR_AIR_TEMP: &str = "http://example.org#g36.ahu_economizer.outdoor_air_temp";
const ECON_OPERATING_MODE: &str = "http://example.org#g36.ahu_economizer.operating_mode";

const VAV_ZONE_TEMP: &str = "http://example.org#g36.vav_single_zone.zone_temp";
const VAV_COOLING_SETPOINT: &str = "http://example.org#g36.vav_single_zone.cooling_setpoint";
const VAV_HEATING_SETPOINT: &str = "http://example.org#g36.vav_single_zone.heating_setpoint";
const SUPPLY_TEMPERATURE_OUTDOOR_AIR: &str =
    "http://example.org#g36.source.multizone_vav_supply_temperature.TOut";
const SUPPLY_TEMPERATURE_FAN_STATUS: &str =
    "http://example.org#g36.source.multizone_vav_supply_temperature.u1SupFan";
const SUPPLY_TEMPERATURE_OPERATING_MODE: &str =
    "http://example.org#g36.source.multizone_vav_supply_temperature.uOpeMod";
const SUPPLY_TEMPERATURE_REQUESTS: &str =
    "http://example.org#g36.source.multizone_vav_supply_temperature.uZonTemResReq";
const SUPPLY_FAN_OPERATING_MODE: &str =
    "http://example.org#g36.source.multizone_vav_supply_fan.uOpeMod";
const SUPPLY_FAN_DUCT_PRESSURE: &str =
    "http://example.org#g36.source.multizone_vav_supply_fan.dpDuc";
const SUPPLY_FAN_PRESSURE_REQUESTS: &str =
    "http://example.org#g36.source.multizone_vav_supply_fan.uZonPreResReq";

type ScheduleSignature = (Vec<u32>, Vec<u32>, Vec<u32>);

#[derive(Clone, Copy)]
struct OutputRef {
    label: &'static str,
    ordinal: usize,
}

fn pair(path: &str, value: Value) -> (String, Value) {
    (path.to_string(), value)
}

fn schedule_signature(engine: &Engine) -> ScheduleSignature {
    let schedule = engine.schedule();
    (
        schedule.order.iter().map(|id| id.0).collect(),
        schedule.connector_order.iter().map(|id| id.0).collect(),
        schedule.driver_of.iter().map(|id| id.0).collect(),
    )
}

fn load(src: &str, expected_blocks: usize, expected_stateful: usize) -> (Engine, LoadReport) {
    let mut engine = Engine::in_memory();
    let report = engine.load_cxf(src.as_bytes()).expect("G36 fixture loads");
    assert_eq!(report.block_count, expected_blocks);
    assert_eq!(report.stateful_blocks, expected_stateful);
    assert!(
        report.warnings.is_empty(),
        "G36 fixture should load cleanly, got {:?}",
        report.warnings
    );
    (engine, report)
}

fn output_paths(engine: &Engine, refs: &[OutputRef]) -> Vec<String> {
    let outputs = engine
        .io()
        .iter()
        .filter(|point| point.direction == PointDirection::Out)
        .collect::<Vec<_>>();
    refs.iter()
        .map(|r| {
            outputs
                .get(r.ordinal)
                .unwrap_or_else(|| {
                    panic!(
                        "fixture output ordinal {} for {} is missing; outputs: {:?}",
                        r.ordinal, r.label, outputs
                    )
                })
                .path
                .clone()
        })
        .collect()
}

fn simulate(
    mut engine: Engine,
    inputs: InputSource,
    outputs: &[OutputRef],
    t_stop: f64,
) -> (ScheduleSignature, Vec<String>, SimMetrics) {
    let schedule = schedule_signature(&engine);
    let paths = output_paths(&engine, outputs);
    let metrics = engine
        .simulate(&SimSpec {
            t_start: 0.0,
            t_stop,
            step: 1.0,
            inputs,
            collect: CollectSpec::Named {
                points: paths.clone(),
                stride: 1,
            },
        })
        .expect("G36 fixture simulates");
    assert_eq!(metrics.ticks, t_stop as u64 + 1);
    assert_eq!(metrics.trace.columns(), paths.as_slice());
    assert_eq!(metrics.trace.rows(), metrics.ticks as usize);
    assert_all_finite(&metrics);
    (schedule, paths, metrics)
}

fn assert_all_finite(metrics: &SimMetrics) {
    for (j, path) in metrics.trace.columns().iter().enumerate() {
        for (i, value) in metrics.trace.column(j).unwrap().iter().enumerate() {
            if let Value::Real(x) = value {
                assert!(x.is_finite(), "{path} at row {i} must stay finite, got {x}");
            }
        }
    }
}

fn column<'a>(metrics: &'a SimMetrics, path: &str) -> &'a [Value] {
    let j = metrics
        .trace
        .columns()
        .iter()
        .position(|p| p == path)
        .unwrap_or_else(|| panic!("missing trace column {path}"));
    metrics.trace.column(j).expect("column index is in range")
}

fn real_at(metrics: &SimMetrics, path: &str, row: usize) -> f64 {
    match &column(metrics, path)[row] {
        Value::Real(x) => *x,
        other => panic!("{path}[{row}] must be Real, got {other:?}"),
    }
}

fn bool_at(metrics: &SimMetrics, path: &str, row: usize) -> bool {
    match &column(metrics, path)[row] {
        Value::Boolean(x) => *x,
        other => panic!("{path}[{row}] must be Boolean, got {other:?}"),
    }
}

fn assert_real_bounds(metrics: &SimMetrics, path: &str, min: f64, max: f64) {
    for (row, value) in column(metrics, path).iter().enumerate() {
        match value {
            Value::Real(x) => assert!(
                (min..=max).contains(x),
                "{path}[{row}]={x} outside [{min}, {max}]"
            ),
            other => panic!("{path}[{row}] must be Real, got {other:?}"),
        }
    }
}

fn assert_close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "expected {actual} to be within {tolerance} of {expected}"
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
        assert_eq!(l.len(), r.len());
        for (row, (lv, rv)) in l.iter().zip(r).enumerate() {
            assert!(
                lv.bit_eq(rv),
                "{} row {row} diverged: {lv:?} vs {rv:?}",
                left.trace.columns()[j]
            );
        }
    }
}

fn sat_inputs(t: f64) -> Vec<(String, Value)> {
    let zone_temp = match t as u32 {
        0 => 22.0,
        1 => 24.0,
        2 => 24.5,
        _ => 25.5,
    };
    vec![
        pair(SAT_ZONE_TEMP, Value::Real(zone_temp)),
        pair(SAT_COOLING_SETPOINT, Value::Real(24.0)),
    ]
}

fn economizer_inputs(t: f64) -> Vec<(String, Value)> {
    let (return_temp, outdoor_temp, operating_mode) = match t as u32 {
        0 => (24.0, 23.0, 1),
        1..=3 => (24.0, 19.0, 1),
        4 => (24.0, 24.0, 1),
        _ => (24.0, 19.0, 0),
    };
    vec![
        pair(ECON_RETURN_AIR_TEMP, Value::Real(return_temp)),
        pair(ECON_OUTDOOR_AIR_TEMP, Value::Real(outdoor_temp)),
        pair(ECON_OPERATING_MODE, Value::Integer(operating_mode)),
    ]
}

fn vav_inputs(t: f64) -> Vec<(String, Value)> {
    let zone_temp = match t as u32 {
        0 => 22.0,
        1 => 27.0,
        2 => 27.5,
        3 => 19.0,
        4 => 19.3,
        _ => 21.0,
    };
    vec![
        pair(VAV_ZONE_TEMP, Value::Real(zone_temp)),
        pair(VAV_COOLING_SETPOINT, Value::Real(24.0)),
        pair(VAV_HEATING_SETPOINT, Value::Real(20.0)),
    ]
}

fn supply_temperature_inputs(t: f64) -> Vec<(String, Value)> {
    let requests = if t >= 840.0 {
        6
    } else if t >= 720.0 {
        3
    } else {
        0
    };
    vec![
        pair(SUPPLY_TEMPERATURE_OUTDOOR_AIR, Value::Real(289.15)),
        pair(SUPPLY_TEMPERATURE_FAN_STATUS, Value::Boolean(true)),
        pair(SUPPLY_TEMPERATURE_OPERATING_MODE, Value::Integer(1)),
        pair(SUPPLY_TEMPERATURE_REQUESTS, Value::Integer(requests)),
    ]
}

fn supply_fan_inputs(t: f64) -> Vec<(String, Value)> {
    let operating_mode = match t as u32 {
        300..=359 => 4,
        _ => 1,
    };
    let requests = if t >= 840.0 {
        5
    } else if t >= 720.0 {
        3
    } else {
        0
    };
    let duct_pressure = if t >= 720.0 { 80.0 } else { 120.0 };
    vec![
        pair(SUPPLY_FAN_OPERATING_MODE, Value::Integer(operating_mode)),
        pair(SUPPLY_FAN_DUCT_PRESSURE, Value::Real(duct_pressure)),
        pair(SUPPLY_FAN_PRESSURE_REQUESTS, Value::Integer(requests)),
    ]
}

#[test]
fn ahu_supply_air_temp_reset_loads_simulates_and_is_deterministic() {
    let (engine, _) = load(AHU_SAT_RESET, 7, 0);
    let outputs = [
        OutputRef {
            label: "sat_setpoint",
            ordinal: 6,
        },
        OutputRef {
            label: "cooling_demand",
            ordinal: 1,
        },
    ];
    let (schedule_a, paths, metrics_a) = simulate(
        engine,
        InputSource::Closure(Box::new(sat_inputs)),
        &outputs,
        4.0,
    );

    let (engine, _) = load(AHU_SAT_RESET, 7, 0);
    let (schedule_b, paths_b, metrics_b) = simulate(
        engine,
        InputSource::Closure(Box::new(sat_inputs)),
        &outputs,
        4.0,
    );

    assert_eq!(paths, paths_b);
    assert_eq!(
        schedule_a, schedule_b,
        "SAT-reset schedule is deterministic"
    );
    assert_trace_bit_eq(&metrics_a, &metrics_b);
    let sat_setpoint = paths[0].as_str();
    let sat_demand = paths[1].as_str();
    assert_real_bounds(&metrics_a, sat_setpoint, 12.0, 16.0);
    assert_real_bounds(&metrics_a, sat_demand, 0.0, 1.0);

    assert_eq!(
        real_at(&metrics_a, sat_setpoint, 0).to_bits(),
        16.0f64.to_bits()
    );
    assert!(
        real_at(&metrics_a, sat_setpoint, 2) < real_at(&metrics_a, sat_setpoint, 1),
        "SAT setpoint should reset downward as cooling load rises"
    );
    assert_eq!(
        real_at(&metrics_a, sat_setpoint, 4).to_bits(),
        12.0f64.to_bits()
    );
    assert!(
        real_at(&metrics_a, sat_demand, 4) > real_at(&metrics_a, sat_demand, 1),
        "cooling demand should increase with zone load"
    );
}

#[test]
fn multizone_vav_supply_temperature_loads_simulates_and_is_deterministic() {
    let (engine, _) = load(SUPPLY_TEMPERATURE, 62, 5);
    let outputs = [OutputRef {
        label: "supply_air_temperature_setpoint",
        ordinal: 50,
    }];
    let (schedule_a, paths, metrics_a) = simulate(
        engine,
        InputSource::Closure(Box::new(supply_temperature_inputs)),
        &outputs,
        900.0,
    );

    let (engine, _) = load(SUPPLY_TEMPERATURE, 62, 5);
    let (schedule_b, paths_b, metrics_b) = simulate(
        engine,
        InputSource::Closure(Box::new(supply_temperature_inputs)),
        &outputs,
        900.0,
    );

    assert_eq!(paths, paths_b);
    assert_eq!(
        schedule_a, schedule_b,
        "SupplyTemperature schedule is deterministic"
    );
    assert_trace_bit_eq(&metrics_a, &metrics_b);
    let sat_setpoint = paths[0].as_str();
    assert_real_bounds(&metrics_a, sat_setpoint, 285.15, 291.15);
    assert_close(real_at(&metrics_a, sat_setpoint, 0), 291.15, 1e-12);
    assert!(
        real_at(&metrics_a, sat_setpoint, 720) < real_at(&metrics_a, sat_setpoint, 719),
        "first active trim/respond sample should lower the reset endpoint"
    );
    assert!(
        real_at(&metrics_a, sat_setpoint, 840) < real_at(&metrics_a, sat_setpoint, 839),
        "larger request count should apply a larger negative response"
    );
}

#[test]
fn multizone_vav_supply_fan_loads_simulates_and_is_deterministic() {
    let (engine, _) = load(SUPPLY_FAN, 65, 7);
    let outputs = [
        OutputRef {
            label: "supply_fan_speed",
            ordinal: 44,
        },
        OutputRef {
            label: "supply_fan_status",
            ordinal: 45,
        },
    ];
    let (schedule_a, paths, metrics_a) = simulate(
        engine,
        InputSource::Closure(Box::new(supply_fan_inputs)),
        &outputs,
        900.0,
    );

    let (engine, _) = load(SUPPLY_FAN, 65, 7);
    let (schedule_b, paths_b, metrics_b) = simulate(
        engine,
        InputSource::Closure(Box::new(supply_fan_inputs)),
        &outputs,
        900.0,
    );

    assert_eq!(paths, paths_b);
    assert_eq!(
        schedule_a, schedule_b,
        "SupplyFan schedule is deterministic"
    );
    assert_trace_bit_eq(&metrics_a, &metrics_b);
    let fan_speed = paths[0].as_str();
    let fan_status = paths[1].as_str();
    assert_real_bounds(&metrics_a, fan_speed, 0.0, 1.0);
    assert!(bool_at(&metrics_a, fan_status, 0));
    assert!(!bool_at(&metrics_a, fan_status, 300));
    assert!(bool_at(&metrics_a, fan_status, 360));
    assert_eq!(
        real_at(&metrics_a, fan_speed, 300).to_bits(),
        0.0f64.to_bits()
    );
    assert!(
        real_at(&metrics_a, fan_speed, 360) >= 0.1,
        "occupied mode after warm-up should restore at least minSpe"
    );
    assert!(
        real_at(&metrics_a, fan_speed, 840) > real_at(&metrics_a, fan_speed, 719),
        "larger pressure reset requests and lower duct pressure should increase fan speed"
    );
}

#[test]
fn ahu_economizer_loads_simulates_and_is_deterministic() {
    let (engine, _) = load(AHU_ECONOMIZER, 12, 3);
    let outputs = [
        OutputRef {
            label: "economizer_enabled",
            ordinal: 8,
        },
        OutputRef {
            label: "damper_command",
            ordinal: 11,
        },
        OutputRef {
            label: "operating_mode_real",
            ordinal: 4,
        },
        OutputRef {
            label: "oa_temperature_delta",
            ordinal: 0,
        },
    ];
    let (schedule_a, paths, metrics_a) = simulate(
        engine,
        InputSource::Closure(Box::new(economizer_inputs)),
        &outputs,
        5.0,
    );

    let (engine, _) = load(AHU_ECONOMIZER, 12, 3);
    let (schedule_b, paths_b, metrics_b) = simulate(
        engine,
        InputSource::Closure(Box::new(economizer_inputs)),
        &outputs,
        5.0,
    );

    assert_eq!(paths, paths_b);
    assert_eq!(
        schedule_a, schedule_b,
        "economizer schedule is deterministic"
    );
    assert_trace_bit_eq(&metrics_a, &metrics_b);
    let econ_enabled = paths[0].as_str();
    let econ_damper = paths[1].as_str();
    let econ_mode_real = paths[2].as_str();
    assert_real_bounds(&metrics_a, econ_damper, 0.2, 1.0);
    assert_real_bounds(&metrics_a, econ_mode_real, 0.0, 1.0);

    assert!(!bool_at(&metrics_a, econ_enabled, 0));
    assert!(!bool_at(&metrics_a, econ_enabled, 1));
    assert!(
        bool_at(&metrics_a, econ_enabled, 2),
        "favorable outdoor air should enable after the true-delay"
    );
    assert!(bool_at(&metrics_a, econ_enabled, 3));
    assert!(
        !bool_at(&metrics_a, econ_enabled, 4),
        "unfavorable changeover should clear the latch"
    );
    assert!(
        !bool_at(&metrics_a, econ_enabled, 5),
        "unoccupied mode should keep economizer disabled"
    );
    assert_eq!(
        real_at(&metrics_a, econ_damper, 2).to_bits(),
        1.0f64.to_bits()
    );
    assert_eq!(
        real_at(&metrics_a, econ_damper, 5).to_bits(),
        0.2f64.to_bits()
    );
    assert_eq!(
        real_at(&metrics_a, econ_mode_real, 5).to_bits(),
        0.0f64.to_bits()
    );
}

#[test]
fn vav_single_zone_loads_simulates_and_is_deterministic() {
    let (engine, _) = load(VAV_SINGLE_ZONE, 8, 2);
    let outputs = [
        OutputRef {
            label: "damper_command",
            ordinal: 7,
        },
        OutputRef {
            label: "airflow_setpoint",
            ordinal: 6,
        },
        OutputRef {
            label: "cooling_signal",
            ordinal: 1,
        },
        OutputRef {
            label: "heating_enabled",
            ordinal: 4,
        },
    ];
    let (schedule_a, paths, metrics_a) = simulate(
        engine,
        InputSource::Closure(Box::new(vav_inputs)),
        &outputs,
        5.0,
    );

    let (engine, _) = load(VAV_SINGLE_ZONE, 8, 2);
    let (schedule_b, paths_b, metrics_b) = simulate(
        engine,
        InputSource::Closure(Box::new(vav_inputs)),
        &outputs,
        5.0,
    );

    assert_eq!(paths, paths_b);
    assert_eq!(schedule_a, schedule_b, "VAV schedule is deterministic");
    assert_trace_bit_eq(&metrics_a, &metrics_b);
    let vav_damper = paths[0].as_str();
    let vav_airflow = paths[1].as_str();
    let vav_cooling = paths[2].as_str();
    let vav_heating = paths[3].as_str();
    assert_real_bounds(&metrics_a, vav_damper, 0.2, 1.0);
    assert_real_bounds(&metrics_a, vav_airflow, 0.25, 1.0);
    assert_real_bounds(&metrics_a, vav_cooling, 0.0, 1.0);

    assert!(
        real_at(&metrics_a, vav_cooling, 1) > real_at(&metrics_a, vav_cooling, 0),
        "cooling signal should rise when zone temperature exceeds the cooling setpoint"
    );
    let initial_airflow = real_at(&metrics_a, vav_airflow, 0);
    let cooling_airflow = real_at(&metrics_a, vav_airflow, 2);
    assert!(
        cooling_airflow > initial_airflow,
        "airflow should track increased cooling demand: initial={initial_airflow}, cooling={cooling_airflow}"
    );
    assert!(bool_at(&metrics_a, vav_heating, 3));
    assert!(
        bool_at(&metrics_a, vav_heating, 4),
        "heating hysteresis should hold inside the band"
    );
    assert!(!bool_at(&metrics_a, vav_heating, 5));
    assert_eq!(
        real_at(&metrics_a, vav_airflow, 3).to_bits(),
        0.6f64.to_bits()
    );
}
