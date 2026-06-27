//! Whole-sequence G36 Tier-2 determinism goldens through the B3 facade driver.
//!
//! These fixtures are engine self-output snapshots, not independent correctness oracles.

use oce_api::Value;
use oce_conformance::drive_trace_with_options;

#[path = "g36_determinism/support.rs"]
mod support;

use support::{
    PointSpec, SequenceSpec, assert_exact_comparisons_pass, assert_output_table_shape,
    assert_provenance_matches_outputs, bless_enabled, bless_sequence, captured_output_table,
    config_for, driver_reference_from_output_golden, options_for, pair, read_output_golden,
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
const SUPPLY_SIGNALS: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/multizone_vav_supply_signals.jsonld");

// The facade exposes flattened runtime connector IDs, while the goldens and provenance preserve
// the fixture-declared output names.
const SAT_ZONE_TEMP: &str = "http://example.org#g36.ahu_supply_air_temp_reset.zone_temp";
const SAT_COOLING_SETPOINT: &str =
    "http://example.org#g36.ahu_supply_air_temp_reset.cooling_setpoint";
const SAT_SETPOINT: &str = "http://example.org#g36.ahu_supply_air_temp_reset.sat_setpoint";
const SAT_COOLING_DEMAND: &str = "http://example.org#g36.ahu_supply_air_temp_reset.cooling_demand";
const SAT_SETPOINT_RUNTIME: &str = "conn#14";
const SAT_COOLING_DEMAND_RUNTIME: &str = "conn#4";

const ECON_RETURN_AIR_TEMP: &str = "http://example.org#g36.ahu_economizer.return_air_temp";
const ECON_OUTDOOR_AIR_TEMP: &str = "http://example.org#g36.ahu_economizer.outdoor_air_temp";
const ECON_OPERATING_MODE: &str = "http://example.org#g36.ahu_economizer.operating_mode";
const ECON_ENABLED: &str = "http://example.org#g36.ahu_economizer.economizer_enabled";
const ECON_DAMPER_COMMAND: &str = "http://example.org#g36.ahu_economizer.damper_command";
const ECON_OPERATING_MODE_REAL: &str = "http://example.org#g36.ahu_economizer.operating_mode_real";
const ECON_OA_TEMP_DELTA: &str = "http://example.org#g36.ahu_economizer.oa_temperature_delta";
const ECON_ENABLED_RUNTIME: &str = "conn#20";
const ECON_DAMPER_COMMAND_RUNTIME: &str = "conn#26";
const ECON_OPERATING_MODE_REAL_RUNTIME: &str = "conn#10";
const ECON_OA_TEMP_DELTA_RUNTIME: &str = "conn#2";

const VAV_ZONE_TEMP: &str = "http://example.org#g36.vav_single_zone.zone_temp";
const VAV_COOLING_SETPOINT: &str = "http://example.org#g36.vav_single_zone.cooling_setpoint";
const VAV_HEATING_SETPOINT: &str = "http://example.org#g36.vav_single_zone.heating_setpoint";
const VAV_DAMPER_COMMAND: &str = "http://example.org#g36.vav_single_zone.damper_command";
const VAV_AIRFLOW_SETPOINT: &str = "http://example.org#g36.vav_single_zone.airflow_setpoint";
const VAV_COOLING_SIGNAL: &str = "http://example.org#g36.vav_single_zone.cooling_signal";
const VAV_HEATING_ENABLED: &str = "http://example.org#g36.vav_single_zone.heating_enabled";
const VAV_DAMPER_COMMAND_RUNTIME: &str = "conn#18";
const VAV_AIRFLOW_SETPOINT_RUNTIME: &str = "conn#16";
const VAV_COOLING_SIGNAL_RUNTIME: &str = "conn#4";
const VAV_HEATING_ENABLED_RUNTIME: &str = "conn#11";
const SUPPLY_TEMPERATURE_OUTDOOR_AIR: &str =
    "http://example.org#g36.source.multizone_vav_supply_temperature.TOut";
const SUPPLY_TEMPERATURE_FAN_STATUS: &str =
    "http://example.org#g36.source.multizone_vav_supply_temperature.u1SupFan";
const SUPPLY_TEMPERATURE_OPERATING_MODE: &str =
    "http://example.org#g36.source.multizone_vav_supply_temperature.uOpeMod";
const SUPPLY_TEMPERATURE_REQUESTS: &str =
    "http://example.org#g36.source.multizone_vav_supply_temperature.uZonTemResReq";
const SUPPLY_TEMPERATURE_SETPOINT: &str =
    "http://example.org#g36.source.multizone_vav_supply_temperature.TAirSupSet";
const SUPPLY_TEMPERATURE_SETPOINT_RUNTIME: &str = "conn#123";
const SUPPLY_FAN_OPERATING_MODE: &str =
    "http://example.org#g36.source.multizone_vav_supply_fan.uOpeMod";
const SUPPLY_FAN_DUCT_PRESSURE: &str =
    "http://example.org#g36.source.multizone_vav_supply_fan.dpDuc";
const SUPPLY_FAN_PRESSURE_REQUESTS: &str =
    "http://example.org#g36.source.multizone_vav_supply_fan.uZonPreResReq";
const SUPPLY_FAN_STATUS: &str = "http://example.org#g36.source.multizone_vav_supply_fan.y1SupFan";
const SUPPLY_FAN_SPEED: &str = "http://example.org#g36.source.multizone_vav_supply_fan.ySupFan";
const SUPPLY_FAN_STATUS_RUNTIME: &str = "conn#110";
const SUPPLY_FAN_SPEED_RUNTIME: &str = "conn#107";
const SUPPLY_SIGNALS_MEASURED_TEMP: &str =
    "http://example.org#g36.source.multizone_vav_supply_signals.TAirSup";
const SUPPLY_SIGNALS_SETPOINT: &str =
    "http://example.org#g36.source.multizone_vav_supply_signals.TAirSupSet";
const SUPPLY_SIGNALS_FAN_STATUS: &str =
    "http://example.org#g36.source.multizone_vav_supply_signals.u1SupFan";
const SUPPLY_SIGNALS_U_T_SUP: &str =
    "http://example.org#g36.source.multizone_vav_supply_signals.uTSup";
const SUPPLY_SIGNALS_COOLING: &str =
    "http://example.org#g36.source.multizone_vav_supply_signals.yCooCoi";
const SUPPLY_SIGNALS_HEATING: &str =
    "http://example.org#g36.source.multizone_vav_supply_signals.yHeaCoi";
const SUPPLY_SIGNALS_U_T_SUP_RUNTIME: &str = "conn#7";
const SUPPLY_SIGNALS_COOLING_RUNTIME: &str = "conn#18";
const SUPPLY_SIGNALS_HEATING_RUNTIME: &str = "conn#24";

const SAT_INPUTS: &[PointSpec] = &[
    PointSpec::real(SAT_ZONE_TEMP),
    PointSpec::real(SAT_COOLING_SETPOINT),
];
const SAT_OUTPUTS: &[PointSpec] = &[
    PointSpec::real_alias(SAT_SETPOINT, SAT_SETPOINT_RUNTIME),
    PointSpec::real_alias(SAT_COOLING_DEMAND, SAT_COOLING_DEMAND_RUNTIME),
];

const ECON_INPUTS: &[PointSpec] = &[
    PointSpec::real(ECON_RETURN_AIR_TEMP),
    PointSpec::real(ECON_OUTDOOR_AIR_TEMP),
    PointSpec::integer(ECON_OPERATING_MODE),
];
const ECON_OUTPUTS: &[PointSpec] = &[
    PointSpec::boolean_alias(ECON_ENABLED, ECON_ENABLED_RUNTIME),
    PointSpec::real_alias(ECON_DAMPER_COMMAND, ECON_DAMPER_COMMAND_RUNTIME),
    PointSpec::real_alias(ECON_OPERATING_MODE_REAL, ECON_OPERATING_MODE_REAL_RUNTIME),
    PointSpec::real_alias(ECON_OA_TEMP_DELTA, ECON_OA_TEMP_DELTA_RUNTIME),
];

const VAV_INPUTS: &[PointSpec] = &[
    PointSpec::real(VAV_ZONE_TEMP),
    PointSpec::real(VAV_COOLING_SETPOINT),
    PointSpec::real(VAV_HEATING_SETPOINT),
];
const VAV_OUTPUTS: &[PointSpec] = &[
    PointSpec::real_alias(VAV_DAMPER_COMMAND, VAV_DAMPER_COMMAND_RUNTIME),
    PointSpec::real_alias(VAV_AIRFLOW_SETPOINT, VAV_AIRFLOW_SETPOINT_RUNTIME),
    PointSpec::real_alias(VAV_COOLING_SIGNAL, VAV_COOLING_SIGNAL_RUNTIME),
    PointSpec::boolean_alias(VAV_HEATING_ENABLED, VAV_HEATING_ENABLED_RUNTIME),
];
const SUPPLY_TEMPERATURE_INPUTS: &[PointSpec] = &[
    PointSpec::real(SUPPLY_TEMPERATURE_OUTDOOR_AIR),
    PointSpec::boolean(SUPPLY_TEMPERATURE_FAN_STATUS),
    PointSpec::integer(SUPPLY_TEMPERATURE_OPERATING_MODE),
    PointSpec::integer(SUPPLY_TEMPERATURE_REQUESTS),
];
const SUPPLY_TEMPERATURE_OUTPUTS: &[PointSpec] = &[PointSpec::real_alias(
    SUPPLY_TEMPERATURE_SETPOINT,
    SUPPLY_TEMPERATURE_SETPOINT_RUNTIME,
)];
const SUPPLY_FAN_INPUTS: &[PointSpec] = &[
    PointSpec::integer(SUPPLY_FAN_OPERATING_MODE),
    PointSpec::real(SUPPLY_FAN_DUCT_PRESSURE),
    PointSpec::integer(SUPPLY_FAN_PRESSURE_REQUESTS),
];
const SUPPLY_FAN_OUTPUTS: &[PointSpec] = &[
    PointSpec::boolean_alias(SUPPLY_FAN_STATUS, SUPPLY_FAN_STATUS_RUNTIME),
    PointSpec::real_alias(SUPPLY_FAN_SPEED, SUPPLY_FAN_SPEED_RUNTIME),
];
const SUPPLY_SIGNALS_INPUTS: &[PointSpec] = &[
    PointSpec::real(SUPPLY_SIGNALS_MEASURED_TEMP),
    PointSpec::real(SUPPLY_SIGNALS_SETPOINT),
    PointSpec::boolean(SUPPLY_SIGNALS_FAN_STATUS),
];
const SUPPLY_SIGNALS_OUTPUTS: &[PointSpec] = &[
    PointSpec::real_alias(SUPPLY_SIGNALS_U_T_SUP, SUPPLY_SIGNALS_U_T_SUP_RUNTIME),
    PointSpec::real_alias(SUPPLY_SIGNALS_HEATING, SUPPLY_SIGNALS_HEATING_RUNTIME),
    PointSpec::real_alias(SUPPLY_SIGNALS_COOLING, SUPPLY_SIGNALS_COOLING_RUNTIME),
];

const SEQUENCES: &[SequenceSpec] = &[
    SequenceSpec {
        name: "ahu_supply_air_temp_reset",
        cxf: AHU_SAT_RESET,
        t_stop: 4,
        inputs: SAT_INPUTS,
        outputs: SAT_OUTPUTS,
        input_fn: sat_inputs,
    },
    SequenceSpec {
        name: "ahu_economizer",
        cxf: AHU_ECONOMIZER,
        t_stop: 5,
        inputs: ECON_INPUTS,
        outputs: ECON_OUTPUTS,
        input_fn: economizer_inputs,
    },
    SequenceSpec {
        name: "vav_single_zone",
        cxf: VAV_SINGLE_ZONE,
        t_stop: 5,
        inputs: VAV_INPUTS,
        outputs: VAV_OUTPUTS,
        input_fn: vav_inputs,
    },
    SequenceSpec {
        name: "multizone_vav_supply_temperature",
        cxf: SUPPLY_TEMPERATURE,
        t_stop: 900,
        inputs: SUPPLY_TEMPERATURE_INPUTS,
        outputs: SUPPLY_TEMPERATURE_OUTPUTS,
        input_fn: supply_temperature_inputs,
    },
    SequenceSpec {
        name: "multizone_vav_supply_fan",
        cxf: SUPPLY_FAN,
        t_stop: 900,
        inputs: SUPPLY_FAN_INPUTS,
        outputs: SUPPLY_FAN_OUTPUTS,
        input_fn: supply_fan_inputs,
    },
    SequenceSpec {
        name: "multizone_vav_supply_signals",
        cxf: SUPPLY_SIGNALS,
        t_stop: 9,
        inputs: SUPPLY_SIGNALS_INPUTS,
        outputs: SUPPLY_SIGNALS_OUTPUTS,
        input_fn: supply_signals_inputs,
    },
];

#[test]
fn g36_whole_sequence_outputs_match_determinism_goldens() {
    for spec in SEQUENCES {
        if bless_enabled() {
            bless_sequence(spec);
        }

        let golden = read_output_golden(spec);
        assert_provenance_matches_outputs(spec, &golden);
        let reference = driver_reference_from_output_golden(spec, &golden);
        let run = drive_trace_with_options(
            spec.cxf.as_bytes(),
            &config_for(spec),
            &reference,
            &options_for(spec),
        )
        .unwrap_or_else(|err| panic!("{} driver run failed: {err}", spec.name));

        assert_output_table_shape(spec, &golden);
        assert_eq!(
            captured_output_table(spec, &run),
            golden,
            "{} captured table drifted from committed golden",
            spec.name
        );
        assert_exact_comparisons_pass(spec, golden.n_rows, &run.comparisons);
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

fn supply_signals_inputs(t: f64) -> Vec<(String, Value)> {
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
