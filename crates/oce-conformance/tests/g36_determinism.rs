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
const PLANT_REQUESTS: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/multizone_vav_plant_requests.jsonld");
const OUTDOOR_AIRFLOW_AHU: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/multizone_vav_outdoor_airflow_ahu.jsonld");
const RELIEF_DAMPER: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/multizone_vav_relief_damper.jsonld");
const RELIEF_FAN: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/multizone_vav_relief_fan.jsonld");

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
const PLANT_REQUESTS_SUPPLY_AIR: &str =
    "http://example.org#g36.source.multizone_vav_plant_requests.TAirSup";
const PLANT_REQUESTS_SETPOINT: &str =
    "http://example.org#g36.source.multizone_vav_plant_requests.TAirSupSet";
const PLANT_REQUESTS_COOLING_VALVE: &str =
    "http://example.org#g36.source.multizone_vav_plant_requests.uCooCoiSet";
const PLANT_REQUESTS_HEATING_VALVE: &str =
    "http://example.org#g36.source.multizone_vav_plant_requests.uHeaCoiSet";
const PLANT_REQUESTS_CHILLED_RESET: &str =
    "http://example.org#g36.source.multizone_vav_plant_requests.yChiWatResReq";
const PLANT_REQUESTS_CHILLER: &str =
    "http://example.org#g36.source.multizone_vav_plant_requests.yChiPlaReq";
const PLANT_REQUESTS_HOT_RESET: &str =
    "http://example.org#g36.source.multizone_vav_plant_requests.yHotWatResReq";
const PLANT_REQUESTS_HOT_PLANT: &str =
    "http://example.org#g36.source.multizone_vav_plant_requests.yHotWatPlaReq";
const PLANT_REQUESTS_CHILLED_RESET_RUNTIME: &str = "conn#17";
const PLANT_REQUESTS_CHILLER_RUNTIME: &str = "conn#42";
const PLANT_REQUESTS_HOT_RESET_RUNTIME: &str = "conn#57";
const PLANT_REQUESTS_HOT_PLANT_RUNTIME: &str = "conn#81";
const OUTDOOR_AIRFLOW_POPULATION_FLOW: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_ahu.VSumAdjPopBreZon_flow";
const OUTDOOR_AIRFLOW_AREA_FLOW: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_ahu.VSumAdjAreBreZon_flow";
const OUTDOOR_AIRFLOW_PRIMARY_FLOW: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_ahu.VSumZonPri_flow";
const OUTDOOR_AIRFLOW_MAX_FRACTION: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_ahu.uOutAirFra_max";
const OUTDOOR_AIRFLOW_MEASURED_FLOW: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_ahu.VAirOut_flow";
const OUTDOOR_AIRFLOW_UNCORRECTED: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_ahu.VUncOutAir_flow";
const OUTDOOR_AIRFLOW_EFFECTIVE: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_ahu.VEffAirOut_flow_min";
const OUTDOOR_AIRFLOW_EFFECTIVE_NORMALIZED: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_ahu.effOutAir_normalized";
const OUTDOOR_AIRFLOW_MEASURED_NORMALIZED: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_ahu.outAir_normalized";
const OUTDOOR_AIRFLOW_UNCORRECTED_RUNTIME: &str = "conn#6";
const OUTDOOR_AIRFLOW_EFFECTIVE_RUNTIME: &str = "conn#24";
const OUTDOOR_AIRFLOW_EFFECTIVE_NORMALIZED_RUNTIME: &str = "conn#29";
const OUTDOOR_AIRFLOW_MEASURED_NORMALIZED_RUNTIME: &str = "conn#32";
const RELIEF_DAMPER_BUILDING_PRESSURE: &str =
    "http://example.org#g36.source.multizone_vav_relief_damper.dpBui";
const RELIEF_DAMPER_SUPPLY_FAN_STATUS: &str =
    "http://example.org#g36.source.multizone_vav_relief_damper.u1SupFan";
const RELIEF_DAMPER_COMMAND: &str =
    "http://example.org#g36.source.multizone_vav_relief_damper.yRelDam";
const RELIEF_DAMPER_COMMAND_RUNTIME: &str = "conn#3";
const RELIEF_FAN_BUILDING_PRESSURE: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan.dpBui";
const RELIEF_FAN_SUPPLY_FAN_STATUS: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan.u1SupFan";
const RELIEF_FAN_AVERAGED_PRESSURE: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan.yDpBui";
const RELIEF_FAN_DAMPER_STATUS: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan.y1RelDam";
const RELIEF_FAN_FAN_SPEED: &str = "http://example.org#g36.source.multizone_vav_relief_fan.yRelFan";
const RELIEF_FAN_FAN_STATUS: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan.y1RelFan";
const RELIEF_FAN_AVERAGED_PRESSURE_RUNTIME: &str = "conn#1";
const RELIEF_FAN_DAMPER_STATUS_RUNTIME: &str = "conn#38";
const RELIEF_FAN_FAN_SPEED_RUNTIME: &str = "conn#46";
const RELIEF_FAN_FAN_STATUS_RUNTIME: &str = "conn#41";

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
const PLANT_REQUESTS_INPUTS: &[PointSpec] = &[
    PointSpec::real(PLANT_REQUESTS_SUPPLY_AIR),
    PointSpec::real(PLANT_REQUESTS_SETPOINT),
    PointSpec::real(PLANT_REQUESTS_COOLING_VALVE),
    PointSpec::real(PLANT_REQUESTS_HEATING_VALVE),
];
const PLANT_REQUESTS_OUTPUTS: &[PointSpec] = &[
    PointSpec::integer_alias(
        PLANT_REQUESTS_CHILLED_RESET,
        PLANT_REQUESTS_CHILLED_RESET_RUNTIME,
    ),
    PointSpec::integer_alias(PLANT_REQUESTS_CHILLER, PLANT_REQUESTS_CHILLER_RUNTIME),
    PointSpec::integer_alias(PLANT_REQUESTS_HOT_RESET, PLANT_REQUESTS_HOT_RESET_RUNTIME),
    PointSpec::integer_alias(PLANT_REQUESTS_HOT_PLANT, PLANT_REQUESTS_HOT_PLANT_RUNTIME),
];
const OUTDOOR_AIRFLOW_INPUTS: &[PointSpec] = &[
    PointSpec::real(OUTDOOR_AIRFLOW_POPULATION_FLOW),
    PointSpec::real(OUTDOOR_AIRFLOW_AREA_FLOW),
    PointSpec::real(OUTDOOR_AIRFLOW_PRIMARY_FLOW),
    PointSpec::real(OUTDOOR_AIRFLOW_MAX_FRACTION),
    PointSpec::real(OUTDOOR_AIRFLOW_MEASURED_FLOW),
];
const OUTDOOR_AIRFLOW_OUTPUTS: &[PointSpec] = &[
    PointSpec::real_alias(
        OUTDOOR_AIRFLOW_UNCORRECTED,
        OUTDOOR_AIRFLOW_UNCORRECTED_RUNTIME,
    ),
    PointSpec::real_alias(OUTDOOR_AIRFLOW_EFFECTIVE, OUTDOOR_AIRFLOW_EFFECTIVE_RUNTIME),
    PointSpec::real_alias(
        OUTDOOR_AIRFLOW_EFFECTIVE_NORMALIZED,
        OUTDOOR_AIRFLOW_EFFECTIVE_NORMALIZED_RUNTIME,
    ),
    PointSpec::real_alias(
        OUTDOOR_AIRFLOW_MEASURED_NORMALIZED,
        OUTDOOR_AIRFLOW_MEASURED_NORMALIZED_RUNTIME,
    ),
];
const RELIEF_DAMPER_INPUTS: &[PointSpec] = &[
    PointSpec::real(RELIEF_DAMPER_BUILDING_PRESSURE),
    PointSpec::boolean(RELIEF_DAMPER_SUPPLY_FAN_STATUS),
];
const RELIEF_DAMPER_OUTPUTS: &[PointSpec] = &[PointSpec::real_alias(
    RELIEF_DAMPER_COMMAND,
    RELIEF_DAMPER_COMMAND_RUNTIME,
)];
const RELIEF_FAN_INPUTS: &[PointSpec] = &[
    PointSpec::real(RELIEF_FAN_BUILDING_PRESSURE),
    PointSpec::boolean(RELIEF_FAN_SUPPLY_FAN_STATUS),
];
const RELIEF_FAN_OUTPUTS: &[PointSpec] = &[
    PointSpec::real_alias(
        RELIEF_FAN_AVERAGED_PRESSURE,
        RELIEF_FAN_AVERAGED_PRESSURE_RUNTIME,
    ),
    PointSpec::boolean_alias(RELIEF_FAN_DAMPER_STATUS, RELIEF_FAN_DAMPER_STATUS_RUNTIME),
    PointSpec::boolean_alias(RELIEF_FAN_FAN_STATUS, RELIEF_FAN_FAN_STATUS_RUNTIME),
    PointSpec::real_alias(RELIEF_FAN_FAN_SPEED, RELIEF_FAN_FAN_SPEED_RUNTIME),
];

const SEQUENCES: &[SequenceSpec] = &[
    SequenceSpec {
        name: "ahu_supply_air_temp_reset",
        cxf: AHU_SAT_RESET,
        t_stop: 4,
        sample_step: 1.0,
        inputs: SAT_INPUTS,
        outputs: SAT_OUTPUTS,
        input_fn: sat_inputs,
    },
    SequenceSpec {
        name: "ahu_economizer",
        cxf: AHU_ECONOMIZER,
        t_stop: 5,
        sample_step: 1.0,
        inputs: ECON_INPUTS,
        outputs: ECON_OUTPUTS,
        input_fn: economizer_inputs,
    },
    SequenceSpec {
        name: "vav_single_zone",
        cxf: VAV_SINGLE_ZONE,
        t_stop: 5,
        sample_step: 1.0,
        inputs: VAV_INPUTS,
        outputs: VAV_OUTPUTS,
        input_fn: vav_inputs,
    },
    SequenceSpec {
        name: "multizone_vav_supply_temperature",
        cxf: SUPPLY_TEMPERATURE,
        t_stop: 900,
        sample_step: 1.0,
        inputs: SUPPLY_TEMPERATURE_INPUTS,
        outputs: SUPPLY_TEMPERATURE_OUTPUTS,
        input_fn: supply_temperature_inputs,
    },
    SequenceSpec {
        name: "multizone_vav_supply_fan",
        cxf: SUPPLY_FAN,
        t_stop: 900,
        sample_step: 1.0,
        inputs: SUPPLY_FAN_INPUTS,
        outputs: SUPPLY_FAN_OUTPUTS,
        input_fn: supply_fan_inputs,
    },
    SequenceSpec {
        name: "multizone_vav_supply_signals",
        cxf: SUPPLY_SIGNALS,
        t_stop: 9,
        sample_step: 1.0,
        inputs: SUPPLY_SIGNALS_INPUTS,
        outputs: SUPPLY_SIGNALS_OUTPUTS,
        input_fn: supply_signals_inputs,
    },
    SequenceSpec {
        name: "multizone_vav_plant_requests",
        cxf: PLANT_REQUESTS,
        t_stop: 19,
        sample_step: 60.0,
        inputs: PLANT_REQUESTS_INPUTS,
        outputs: PLANT_REQUESTS_OUTPUTS,
        input_fn: plant_requests_inputs,
    },
    SequenceSpec {
        name: "multizone_vav_outdoor_airflow_ahu",
        cxf: OUTDOOR_AIRFLOW_AHU,
        t_stop: 4,
        sample_step: 1.0,
        inputs: OUTDOOR_AIRFLOW_INPUTS,
        outputs: OUTDOOR_AIRFLOW_OUTPUTS,
        input_fn: outdoor_airflow_inputs,
    },
    SequenceSpec {
        name: "multizone_vav_relief_damper",
        cxf: RELIEF_DAMPER,
        t_stop: 5,
        sample_step: 1.0,
        inputs: RELIEF_DAMPER_INPUTS,
        outputs: RELIEF_DAMPER_OUTPUTS,
        input_fn: relief_damper_inputs,
    },
    SequenceSpec {
        name: "multizone_vav_relief_fan",
        cxf: RELIEF_FAN,
        t_stop: 29,
        sample_step: 60.0,
        inputs: RELIEF_FAN_INPUTS,
        outputs: RELIEF_FAN_OUTPUTS,
        input_fn: relief_fan_inputs,
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

fn plant_requests_inputs(t: f64) -> Vec<(String, Value)> {
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

fn outdoor_airflow_inputs(t: f64) -> Vec<(String, Value)> {
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

fn relief_damper_inputs(t: f64) -> Vec<(String, Value)> {
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

fn relief_fan_inputs(t: f64) -> Vec<(String, Value)> {
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
