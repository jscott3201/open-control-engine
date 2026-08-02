//! Whole-sequence G36 Tier-2 determinism goldens through the B3 facade driver.
//!
//! These fixtures are engine self-output snapshots, not independent correctness oracles.

use oce_conformance::drive_trace_with_options;

#[path = "g36_determinism/sequence_inputs.rs"]
mod sequence_inputs;
#[path = "g36_determinism/support.rs"]
mod support;

use sequence_inputs::{
    economizer_inputs, outdoor_airflow_inputs, plant_requests_inputs, relief_damper_inputs,
    relief_fan_inputs, return_fan_airflow_inputs, return_fan_direct_pressure_inputs, sat_inputs,
    supply_fan_inputs, supply_signals_inputs, supply_temperature_inputs, vav_inputs,
};
use support::{
    PointSpec, SequenceSpec, assert_exact_comparisons_pass, assert_output_table_shape,
    assert_provenance_matches_outputs, bless_enabled, bless_sequence, captured_output_table,
    config_for, driver_reference_from_output_golden, options_for, read_output_golden,
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
const RETURN_FAN_AIRFLOW: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/multizone_vav_return_fan_airflow_tracking.jsonld"
);
const RETURN_FAN_DIRECT_PRESSURE: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/multizone_vav_return_fan_direct_pressure.jsonld"
);

// The facade exposes flattened runtime connector IDs, while the goldens and provenance preserve
// the fixture-declared output names.
const SAT_ZONE_TEMP: &str = "http://example.org#g36.ahu_supply_air_temp_reset.zone_temp";
const SAT_COOLING_SETPOINT: &str =
    "http://example.org#g36.ahu_supply_air_temp_reset.cooling_setpoint";
const SAT_SETPOINT: &str = "http://example.org#g36.ahu_supply_air_temp_reset.sat_setpoint";
const SAT_COOLING_DEMAND: &str = "http://example.org#g36.ahu_supply_air_temp_reset.cooling_demand";
const SAT_SETPOINT_PATH: &str = "http://example.org#g36.ahu_supply_air_temp_reset.satLine.y";
const SAT_COOLING_DEMAND_PATH: &str =
    "http://example.org#g36.ahu_supply_air_temp_reset.demandLimiter.y";

const ECON_RETURN_AIR_TEMP: &str = "http://example.org#g36.ahu_economizer.return_air_temp";
const ECON_OUTDOOR_AIR_TEMP: &str = "http://example.org#g36.ahu_economizer.outdoor_air_temp";
const ECON_OPERATING_MODE: &str = "http://example.org#g36.ahu_economizer.operating_mode";
const ECON_ENABLED: &str = "http://example.org#g36.ahu_economizer.economizer_enabled";
const ECON_DAMPER_COMMAND: &str = "http://example.org#g36.ahu_economizer.damper_command";
const ECON_OPERATING_MODE_REAL: &str = "http://example.org#g36.ahu_economizer.operating_mode_real";
const ECON_OA_TEMP_DELTA: &str = "http://example.org#g36.ahu_economizer.oa_temperature_delta";
const ECON_ENABLED_PATH: &str = "http://example.org#g36.ahu_economizer.enableLatch.y";
const ECON_DAMPER_COMMAND_PATH: &str = "http://example.org#g36.ahu_economizer.damperSwitch.y";
const ECON_OPERATING_MODE_REAL_PATH: &str = "http://example.org#g36.ahu_economizer.modeToReal.y";
const ECON_OA_TEMP_DELTA_PATH: &str = "http://example.org#g36.ahu_economizer.returnMinusOutdoor.y";

const VAV_ZONE_TEMP: &str = "http://example.org#g36.vav_single_zone.zone_temp";
const VAV_COOLING_SETPOINT: &str = "http://example.org#g36.vav_single_zone.cooling_setpoint";
const VAV_HEATING_SETPOINT: &str = "http://example.org#g36.vav_single_zone.heating_setpoint";
const VAV_DAMPER_COMMAND: &str = "http://example.org#g36.vav_single_zone.damper_command";
const VAV_AIRFLOW_SETPOINT: &str = "http://example.org#g36.vav_single_zone.airflow_setpoint";
const VAV_COOLING_SIGNAL: &str = "http://example.org#g36.vav_single_zone.cooling_signal";
const VAV_HEATING_ENABLED: &str = "http://example.org#g36.vav_single_zone.heating_enabled";
const VAV_DAMPER_COMMAND_PATH: &str = "http://example.org#g36.vav_single_zone.damperLimiter.y";
const VAV_AIRFLOW_SETPOINT_PATH: &str = "http://example.org#g36.vav_single_zone.airflowSwitch.y";
const VAV_COOLING_SIGNAL_PATH: &str = "http://example.org#g36.vav_single_zone.coolingPid.y";
const VAV_HEATING_ENABLED_PATH: &str = "http://example.org#g36.vav_single_zone.heatingNeed.y";
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
const SUPPLY_TEMPERATURE_SETPOINT_PATH: &str =
    "http://example.org#g36.source.multizone_vav_supply_temperature.swi3.y";
const SUPPLY_FAN_OPERATING_MODE: &str =
    "http://example.org#g36.source.multizone_vav_supply_fan.uOpeMod";
const SUPPLY_FAN_DUCT_PRESSURE: &str =
    "http://example.org#g36.source.multizone_vav_supply_fan.dpDuc";
const SUPPLY_FAN_PRESSURE_REQUESTS: &str =
    "http://example.org#g36.source.multizone_vav_supply_fan.uZonPreResReq";
const SUPPLY_FAN_STATUS: &str = "http://example.org#g36.source.multizone_vav_supply_fan.y1SupFan";
const SUPPLY_FAN_SPEED: &str = "http://example.org#g36.source.multizone_vav_supply_fan.ySupFan";
const SUPPLY_FAN_STATUS_PATH: &str = "http://example.org#g36.source.multizone_vav_supply_fan.or1.y";
const SUPPLY_FAN_SPEED_PATH: &str = "http://example.org#g36.source.multizone_vav_supply_fan.swi.y";
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
const SUPPLY_SIGNALS_U_T_SUP_PATH: &str =
    "http://example.org#g36.source.multizone_vav_supply_signals.swi.y";
const SUPPLY_SIGNALS_COOLING_PATH: &str =
    "http://example.org#g36.source.multizone_vav_supply_signals.conSigCoo.y";
const SUPPLY_SIGNALS_HEATING_PATH: &str =
    "http://example.org#g36.source.multizone_vav_supply_signals.conSigHea.y";
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
const PLANT_REQUESTS_CHILLED_RESET_PATH: &str =
    "http://example.org#g36.source.multizone_vav_plant_requests.chiWatRes3.y";
const PLANT_REQUESTS_CHILLER_PATH: &str =
    "http://example.org#g36.source.multizone_vav_plant_requests.intSwi3.y";
const PLANT_REQUESTS_HOT_RESET_PATH: &str =
    "http://example.org#g36.source.multizone_vav_plant_requests.hotWatRes3.y";
const PLANT_REQUESTS_HOT_PLANT_PATH: &str =
    "http://example.org#g36.source.multizone_vav_plant_requests.intSwi1.y";
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
const OUTDOOR_AIRFLOW_UNCORRECTED_PATH: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_ahu.min1.y";
const OUTDOOR_AIRFLOW_EFFECTIVE_PATH: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_ahu.min2.y";
const OUTDOOR_AIRFLOW_EFFECTIVE_NORMALIZED_PATH: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_ahu.norVOutMin.y";
const OUTDOOR_AIRFLOW_MEASURED_NORMALIZED_PATH: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_ahu.norVOut.y";
const RELIEF_DAMPER_BUILDING_PRESSURE: &str =
    "http://example.org#g36.source.multizone_vav_relief_damper.dpBui";
const RELIEF_DAMPER_SUPPLY_FAN_STATUS: &str =
    "http://example.org#g36.source.multizone_vav_relief_damper.u1SupFan";
const RELIEF_DAMPER_COMMAND: &str =
    "http://example.org#g36.source.multizone_vav_relief_damper.yRelDam";
const RELIEF_DAMPER_COMMAND_PATH: &str =
    "http://example.org#g36.source.multizone_vav_relief_damper.swi.y";
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
const RELIEF_FAN_AVERAGED_PRESSURE_PATH: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan.movMea.y";
const RELIEF_FAN_DAMPER_STATUS_PATH: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan.relDam.y";
const RELIEF_FAN_FAN_SPEED_PATH: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan.pro1.y";
const RELIEF_FAN_FAN_STATUS_PATH: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan.relFan.y";
const RETURN_FAN_AIRFLOW_SUPPLY: &str =
    "http://example.org#g36.source.multizone_vav_return_fan_airflow_tracking.VAirSup_flow";
const RETURN_FAN_AIRFLOW_RETURN: &str =
    "http://example.org#g36.source.multizone_vav_return_fan_airflow_tracking.VAirRet_flow";
const RETURN_FAN_AIRFLOW_SUPPLY_FAN: &str =
    "http://example.org#g36.source.multizone_vav_return_fan_airflow_tracking.u1SupFan";
const RETURN_FAN_AIRFLOW_SPEED: &str =
    "http://example.org#g36.source.multizone_vav_return_fan_airflow_tracking.yRetFan";
const RETURN_FAN_AIRFLOW_STATUS: &str =
    "http://example.org#g36.source.multizone_vav_return_fan_airflow_tracking.y1RetFan";
const RETURN_FAN_AIRFLOW_SPEED_PATH: &str =
    "http://example.org#g36.source.multizone_vav_return_fan_airflow_tracking.swi.y";
const RETURN_FAN_AIRFLOW_STATUS_PATH: &str =
    "http://example.org#g36.source.multizone_vav_return_fan_airflow_tracking.y1RetFan";
const RETURN_FAN_DIRECT_PRESSURE_BUILDING_PRESSURE: &str =
    "http://example.org#g36.source.multizone_vav_return_fan_direct_pressure.dpBui";
const RETURN_FAN_DIRECT_PRESSURE_MIN_OUTDOOR_AIR_DAMPER: &str =
    "http://example.org#g36.source.multizone_vav_return_fan_direct_pressure.u1MinOutAirDam";
const RETURN_FAN_DIRECT_PRESSURE_SUPPLY_FAN: &str =
    "http://example.org#g36.source.multizone_vav_return_fan_direct_pressure.u1SupFan";
const RETURN_FAN_DIRECT_PRESSURE_AVERAGED_PRESSURE: &str =
    "http://example.org#g36.source.multizone_vav_return_fan_direct_pressure.yDpBui";
const RETURN_FAN_DIRECT_PRESSURE_RELIEF_DAMPER: &str =
    "http://example.org#g36.source.multizone_vav_return_fan_direct_pressure.yRelDam";
const RETURN_FAN_DIRECT_PRESSURE_DISCHARGE_PRESSURE: &str =
    "http://example.org#g36.source.multizone_vav_return_fan_direct_pressure.dpDisSet";
const RETURN_FAN_DIRECT_PRESSURE_SPEED: &str =
    "http://example.org#g36.source.multizone_vav_return_fan_direct_pressure.yRetFan";
const RETURN_FAN_DIRECT_PRESSURE_STATUS: &str =
    "http://example.org#g36.source.multizone_vav_return_fan_direct_pressure.y1RetFan";
const RETURN_FAN_DIRECT_PRESSURE_AVERAGED_PRESSURE_PATH: &str =
    "http://example.org#g36.source.multizone_vav_return_fan_direct_pressure.movMea.y";
const RETURN_FAN_DIRECT_PRESSURE_RELIEF_DAMPER_PATH: &str =
    "http://example.org#g36.source.multizone_vav_return_fan_direct_pressure.swi1.y";
const RETURN_FAN_DIRECT_PRESSURE_DISCHARGE_PRESSURE_PATH: &str =
    "http://example.org#g36.source.multizone_vav_return_fan_direct_pressure.swi.y";
const RETURN_FAN_DIRECT_PRESSURE_SPEED_PATH: &str =
    "http://example.org#g36.source.multizone_vav_return_fan_direct_pressure.swi2.y";
const RETURN_FAN_DIRECT_PRESSURE_STATUS_PATH: &str =
    "http://example.org#g36.source.multizone_vav_return_fan_direct_pressure.y1RetFan";

const SAT_INPUTS: &[PointSpec] = &[
    PointSpec::real(SAT_ZONE_TEMP),
    PointSpec::real(SAT_COOLING_SETPOINT),
];
const SAT_OUTPUTS: &[PointSpec] = &[
    PointSpec::real_alias(SAT_SETPOINT, SAT_SETPOINT_PATH),
    PointSpec::real_alias(SAT_COOLING_DEMAND, SAT_COOLING_DEMAND_PATH),
];

const ECON_INPUTS: &[PointSpec] = &[
    PointSpec::real(ECON_RETURN_AIR_TEMP),
    PointSpec::real(ECON_OUTDOOR_AIR_TEMP),
    PointSpec::integer(ECON_OPERATING_MODE),
];
const ECON_OUTPUTS: &[PointSpec] = &[
    PointSpec::boolean_alias(ECON_ENABLED, ECON_ENABLED_PATH),
    PointSpec::real_alias(ECON_DAMPER_COMMAND, ECON_DAMPER_COMMAND_PATH),
    PointSpec::real_alias(ECON_OPERATING_MODE_REAL, ECON_OPERATING_MODE_REAL_PATH),
    PointSpec::real_alias(ECON_OA_TEMP_DELTA, ECON_OA_TEMP_DELTA_PATH),
];

const VAV_INPUTS: &[PointSpec] = &[
    PointSpec::real(VAV_ZONE_TEMP),
    PointSpec::real(VAV_COOLING_SETPOINT),
    PointSpec::real(VAV_HEATING_SETPOINT),
];
const VAV_OUTPUTS: &[PointSpec] = &[
    PointSpec::real_alias(VAV_DAMPER_COMMAND, VAV_DAMPER_COMMAND_PATH),
    PointSpec::real_alias(VAV_AIRFLOW_SETPOINT, VAV_AIRFLOW_SETPOINT_PATH),
    PointSpec::real_alias(VAV_COOLING_SIGNAL, VAV_COOLING_SIGNAL_PATH),
    PointSpec::boolean_alias(VAV_HEATING_ENABLED, VAV_HEATING_ENABLED_PATH),
];
const SUPPLY_TEMPERATURE_INPUTS: &[PointSpec] = &[
    PointSpec::real(SUPPLY_TEMPERATURE_OUTDOOR_AIR),
    PointSpec::boolean(SUPPLY_TEMPERATURE_FAN_STATUS),
    PointSpec::integer(SUPPLY_TEMPERATURE_OPERATING_MODE),
    PointSpec::integer(SUPPLY_TEMPERATURE_REQUESTS),
];
const SUPPLY_TEMPERATURE_OUTPUTS: &[PointSpec] = &[PointSpec::real_alias(
    SUPPLY_TEMPERATURE_SETPOINT,
    SUPPLY_TEMPERATURE_SETPOINT_PATH,
)];
const SUPPLY_FAN_INPUTS: &[PointSpec] = &[
    PointSpec::integer(SUPPLY_FAN_OPERATING_MODE),
    PointSpec::real(SUPPLY_FAN_DUCT_PRESSURE),
    PointSpec::integer(SUPPLY_FAN_PRESSURE_REQUESTS),
];
const SUPPLY_FAN_OUTPUTS: &[PointSpec] = &[
    PointSpec::boolean_alias(SUPPLY_FAN_STATUS, SUPPLY_FAN_STATUS_PATH),
    PointSpec::real_alias(SUPPLY_FAN_SPEED, SUPPLY_FAN_SPEED_PATH),
];
const SUPPLY_SIGNALS_INPUTS: &[PointSpec] = &[
    PointSpec::real(SUPPLY_SIGNALS_MEASURED_TEMP),
    PointSpec::real(SUPPLY_SIGNALS_SETPOINT),
    PointSpec::boolean(SUPPLY_SIGNALS_FAN_STATUS),
];
const SUPPLY_SIGNALS_OUTPUTS: &[PointSpec] = &[
    PointSpec::real_alias(SUPPLY_SIGNALS_U_T_SUP, SUPPLY_SIGNALS_U_T_SUP_PATH),
    PointSpec::real_alias(SUPPLY_SIGNALS_HEATING, SUPPLY_SIGNALS_HEATING_PATH),
    PointSpec::real_alias(SUPPLY_SIGNALS_COOLING, SUPPLY_SIGNALS_COOLING_PATH),
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
        PLANT_REQUESTS_CHILLED_RESET_PATH,
    ),
    PointSpec::integer_alias(PLANT_REQUESTS_CHILLER, PLANT_REQUESTS_CHILLER_PATH),
    PointSpec::integer_alias(PLANT_REQUESTS_HOT_RESET, PLANT_REQUESTS_HOT_RESET_PATH),
    PointSpec::integer_alias(PLANT_REQUESTS_HOT_PLANT, PLANT_REQUESTS_HOT_PLANT_PATH),
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
        OUTDOOR_AIRFLOW_UNCORRECTED_PATH,
    ),
    PointSpec::real_alias(OUTDOOR_AIRFLOW_EFFECTIVE, OUTDOOR_AIRFLOW_EFFECTIVE_PATH),
    PointSpec::real_alias(
        OUTDOOR_AIRFLOW_EFFECTIVE_NORMALIZED,
        OUTDOOR_AIRFLOW_EFFECTIVE_NORMALIZED_PATH,
    ),
    PointSpec::real_alias(
        OUTDOOR_AIRFLOW_MEASURED_NORMALIZED,
        OUTDOOR_AIRFLOW_MEASURED_NORMALIZED_PATH,
    ),
];
const RELIEF_DAMPER_INPUTS: &[PointSpec] = &[
    PointSpec::real(RELIEF_DAMPER_BUILDING_PRESSURE),
    PointSpec::boolean(RELIEF_DAMPER_SUPPLY_FAN_STATUS),
];
const RELIEF_DAMPER_OUTPUTS: &[PointSpec] = &[PointSpec::real_alias(
    RELIEF_DAMPER_COMMAND,
    RELIEF_DAMPER_COMMAND_PATH,
)];
const RELIEF_FAN_INPUTS: &[PointSpec] = &[
    PointSpec::real(RELIEF_FAN_BUILDING_PRESSURE),
    PointSpec::boolean(RELIEF_FAN_SUPPLY_FAN_STATUS),
];
const RELIEF_FAN_OUTPUTS: &[PointSpec] = &[
    PointSpec::real_alias(
        RELIEF_FAN_AVERAGED_PRESSURE,
        RELIEF_FAN_AVERAGED_PRESSURE_PATH,
    ),
    PointSpec::boolean_alias(RELIEF_FAN_DAMPER_STATUS, RELIEF_FAN_DAMPER_STATUS_PATH),
    PointSpec::boolean_alias(RELIEF_FAN_FAN_STATUS, RELIEF_FAN_FAN_STATUS_PATH),
    PointSpec::real_alias(RELIEF_FAN_FAN_SPEED, RELIEF_FAN_FAN_SPEED_PATH),
];
const RETURN_FAN_AIRFLOW_INPUTS: &[PointSpec] = &[
    PointSpec::real(RETURN_FAN_AIRFLOW_SUPPLY),
    PointSpec::real(RETURN_FAN_AIRFLOW_RETURN),
    PointSpec::boolean(RETURN_FAN_AIRFLOW_SUPPLY_FAN),
];
const RETURN_FAN_AIRFLOW_OUTPUTS: &[PointSpec] = &[
    PointSpec::real_alias(RETURN_FAN_AIRFLOW_SPEED, RETURN_FAN_AIRFLOW_SPEED_PATH),
    PointSpec::boolean_alias(RETURN_FAN_AIRFLOW_STATUS, RETURN_FAN_AIRFLOW_STATUS_PATH),
];
const RETURN_FAN_DIRECT_PRESSURE_INPUTS: &[PointSpec] = &[
    PointSpec::real(RETURN_FAN_DIRECT_PRESSURE_BUILDING_PRESSURE),
    PointSpec::boolean(RETURN_FAN_DIRECT_PRESSURE_MIN_OUTDOOR_AIR_DAMPER),
    PointSpec::boolean(RETURN_FAN_DIRECT_PRESSURE_SUPPLY_FAN),
];
const RETURN_FAN_DIRECT_PRESSURE_OUTPUTS: &[PointSpec] = &[
    PointSpec::real_alias(
        RETURN_FAN_DIRECT_PRESSURE_AVERAGED_PRESSURE,
        RETURN_FAN_DIRECT_PRESSURE_AVERAGED_PRESSURE_PATH,
    ),
    PointSpec::real_alias(
        RETURN_FAN_DIRECT_PRESSURE_RELIEF_DAMPER,
        RETURN_FAN_DIRECT_PRESSURE_RELIEF_DAMPER_PATH,
    ),
    PointSpec::real_alias(
        RETURN_FAN_DIRECT_PRESSURE_DISCHARGE_PRESSURE,
        RETURN_FAN_DIRECT_PRESSURE_DISCHARGE_PRESSURE_PATH,
    ),
    PointSpec::real_alias(
        RETURN_FAN_DIRECT_PRESSURE_SPEED,
        RETURN_FAN_DIRECT_PRESSURE_SPEED_PATH,
    ),
    PointSpec::boolean_alias(
        RETURN_FAN_DIRECT_PRESSURE_STATUS,
        RETURN_FAN_DIRECT_PRESSURE_STATUS_PATH,
    ),
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
    SequenceSpec {
        name: "multizone_vav_return_fan_airflow_tracking",
        cxf: RETURN_FAN_AIRFLOW,
        t_stop: 7,
        sample_step: 1.0,
        inputs: RETURN_FAN_AIRFLOW_INPUTS,
        outputs: RETURN_FAN_AIRFLOW_OUTPUTS,
        input_fn: return_fan_airflow_inputs,
    },
    SequenceSpec {
        name: "multizone_vav_return_fan_direct_pressure",
        cxf: RETURN_FAN_DIRECT_PRESSURE,
        t_stop: 5,
        sample_step: 1.0,
        inputs: RETURN_FAN_DIRECT_PRESSURE_INPUTS,
        outputs: RETURN_FAN_DIRECT_PRESSURE_OUTPUTS,
        input_fn: return_fan_direct_pressure_inputs,
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
