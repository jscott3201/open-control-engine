//! Deterministic per-sequence input schedules for the whole-sequence determinism goldens.
//!
//! Each function is the `input_fn` of one [`crate::support::SequenceSpec`]: a pure map from
//! tick time to the staged input batch, so replaying a sequence is reproducible bit-for-bit.

use oce_api::Value;

use crate::support::pair;
// The authored input point identities live at the crate root next to the sequence table.
use crate::*;

pub(crate) fn sat_inputs(t: f64) -> Vec<(String, Value)> {
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

pub(crate) fn economizer_inputs(t: f64) -> Vec<(String, Value)> {
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

pub(crate) fn vav_inputs(t: f64) -> Vec<(String, Value)> {
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

pub(crate) fn supply_temperature_inputs(t: f64) -> Vec<(String, Value)> {
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

pub(crate) fn supply_fan_inputs(t: f64) -> Vec<(String, Value)> {
    let operating_mode = match t as u32 {
        300..=359 => 4,
        _ => 1,
    };
    let pressure_requests = if t >= 840.0 {
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
        pair(
            SUPPLY_FAN_PRESSURE_REQUESTS,
            Value::Integer(pressure_requests),
        ),
    ]
}

pub(crate) fn supply_signals_inputs(t: f64) -> Vec<(String, Value)> {
    let row = t as usize;
    let setpoint = [
        295.0, 295.0, 295.0, 300.0, 295.0, 295.0, 320.0, 320.0, 320.0, 320.0,
    ][row.min(9)];
    let measured = [
        300.0, 300.0, 300.0, 295.0, 310.0, 320.0, 295.0, 295.0, 295.0, 295.0,
    ][row.min(9)];
    let fan_status = [false, true, true, true, true, true, true, false, true, true][row.min(9)];
    vec![
        pair(SUPPLY_SIGNALS_MEASURED_TEMP, Value::Real(measured)),
        pair(SUPPLY_SIGNALS_SETPOINT, Value::Real(setpoint)),
        pair(SUPPLY_SIGNALS_FAN_STATUS, Value::Boolean(fan_status)),
    ]
}

pub(crate) fn plant_requests_inputs(t: f64) -> Vec<(String, Value)> {
    let row = ((t / 60.0).round() as usize).min(19);
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
        pair(
            PLANT_REQUESTS_SUPPLY_AIR,
            Value::Real(supply_air_temperature),
        ),
        pair(PLANT_REQUESTS_SETPOINT, Value::Real(supply_air_setpoint)),
        pair(
            PLANT_REQUESTS_COOLING_VALVE,
            Value::Real(cooling_coil_valve),
        ),
        pair(
            PLANT_REQUESTS_HEATING_VALVE,
            Value::Real(heating_coil_valve),
        ),
    ]
}

pub(crate) fn outdoor_airflow_inputs(t: f64) -> Vec<(String, Value)> {
    let (population, area, primary, fraction, measured) = match t as u32 {
        0 => (1.0, 1.0, 5.0, 0.2, 4.0),
        1 => (4.0, 5.0, 2.0, 0.2, 6.0),
        2 => (0.002, 0.001, 0.0, 0.4, 0.1),
        3 => (0.0, 0.0, 1.0, 1.5, 9.0),
        _ => (5.0, 5.0, 100.0, 0.99, 8.8),
    };

    vec![
        pair(OUTDOOR_AIRFLOW_POPULATION_FLOW, Value::Real(population)),
        pair(OUTDOOR_AIRFLOW_AREA_FLOW, Value::Real(area)),
        pair(OUTDOOR_AIRFLOW_PRIMARY_FLOW, Value::Real(primary)),
        pair(OUTDOOR_AIRFLOW_MAX_FRACTION, Value::Real(fraction)),
        pair(OUTDOOR_AIRFLOW_MEASURED_FLOW, Value::Real(measured)),
    ]
}

pub(crate) fn relief_damper_inputs(t: f64) -> Vec<(String, Value)> {
    let (pressure, fan_status) = match t as u32 {
        0 => (10.0, false),
        1 => (12.0, true),
        2 => (13.0, true),
        3 => (14.0, true),
        4 => (15.0, true),
        _ => (20.0, false),
    };

    vec![
        pair(RELIEF_DAMPER_BUILDING_PRESSURE, Value::Real(pressure)),
        pair(RELIEF_DAMPER_SUPPLY_FAN_STATUS, Value::Boolean(fan_status)),
    ]
}

pub(crate) fn relief_fan_inputs(t: f64) -> Vec<(String, Value)> {
    let pressure = if t < 300.0 {
        12.0
    } else if t <= 1020.0 {
        18.0
    } else {
        12.0
    };
    let fan_status = t >= 300.0;

    vec![
        pair(RELIEF_FAN_BUILDING_PRESSURE, Value::Real(pressure)),
        pair(RELIEF_FAN_SUPPLY_FAN_STATUS, Value::Boolean(fan_status)),
    ]
}

pub(crate) fn return_fan_airflow_inputs(t: f64) -> Vec<(String, Value)> {
    let (supply_airflow, return_airflow, supply_fan_status) = match t as u32 {
        0 => (5.0, 4.0, false),
        1 => (5.25, 4.0, true),
        2 => (5.0, 4.0, true),
        3 => (4.75, 4.0, true),
        4 => (5.5, 4.0, false),
        5 => (5.0, 4.0, true),
        _ => (4.5, 4.0, true),
    };

    vec![
        pair(RETURN_FAN_AIRFLOW_SUPPLY, Value::Real(supply_airflow)),
        pair(RETURN_FAN_AIRFLOW_RETURN, Value::Real(return_airflow)),
        pair(
            RETURN_FAN_AIRFLOW_SUPPLY_FAN,
            Value::Boolean(supply_fan_status),
        ),
    ]
}

pub(crate) fn return_fan_direct_pressure_inputs(t: f64) -> Vec<(String, Value)> {
    let (building_pressure, min_outdoor_air_damper, supply_fan_status) = match t as u32 {
        0 => (12.0, true, false),
        1 => (9.009, false, true),
        2 => (21.006, true, true),
        3 => (-21.012, true, true),
        4 => (135.033, true, true),
        _ => (-264.06, true, true),
    };

    vec![
        pair(
            RETURN_FAN_DIRECT_PRESSURE_BUILDING_PRESSURE,
            Value::Real(building_pressure),
        ),
        pair(
            RETURN_FAN_DIRECT_PRESSURE_MIN_OUTDOOR_AIR_DAMPER,
            Value::Boolean(min_outdoor_air_damper),
        ),
        pair(
            RETURN_FAN_DIRECT_PRESSURE_SUPPLY_FAN,
            Value::Boolean(supply_fan_status),
        ),
    ]
}
