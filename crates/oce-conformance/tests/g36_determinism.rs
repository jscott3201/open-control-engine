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

const ECON_RETURN_AIR_TEMP: &str = "http://example.org#g36.ahu_economizer.return_air_temp";
const ECON_OUTDOOR_AIR_TEMP: &str = "http://example.org#g36.ahu_economizer.outdoor_air_temp";
const ECON_OPERATING_MODE: &str = "http://example.org#g36.ahu_economizer.operating_mode";
const ECON_ENABLED: &str = "http://example.org#g36.ahu_economizer.economizer_enabled";
const ECON_DAMPER_COMMAND: &str = "http://example.org#g36.ahu_economizer.damper_command";
const ECON_OPERATING_MODE_REAL: &str = "http://example.org#g36.ahu_economizer.operating_mode_real";
const ECON_OA_TEMP_DELTA: &str = "http://example.org#g36.ahu_economizer.oa_temperature_delta";

const VAV_ZONE_TEMP: &str = "http://example.org#g36.vav_single_zone.zone_temp";
const VAV_COOLING_SETPOINT: &str = "http://example.org#g36.vav_single_zone.cooling_setpoint";
const VAV_HEATING_SETPOINT: &str = "http://example.org#g36.vav_single_zone.heating_setpoint";
const VAV_DAMPER_COMMAND: &str = "http://example.org#g36.vav_single_zone.damper_command";
const VAV_AIRFLOW_SETPOINT: &str = "http://example.org#g36.vav_single_zone.airflow_setpoint";
const VAV_COOLING_SIGNAL: &str = "http://example.org#g36.vav_single_zone.cooling_signal";
const VAV_HEATING_ENABLED: &str = "http://example.org#g36.vav_single_zone.heating_enabled";
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
const SUPPLY_FAN_OPERATING_MODE: &str =
    "http://example.org#g36.source.multizone_vav_supply_fan.uOpeMod";
const SUPPLY_FAN_DUCT_PRESSURE: &str =
    "http://example.org#g36.source.multizone_vav_supply_fan.dpDuc";
const SUPPLY_FAN_PRESSURE_REQUESTS: &str =
    "http://example.org#g36.source.multizone_vav_supply_fan.uZonPreResReq";
const SUPPLY_FAN_STATUS: &str = "http://example.org#g36.source.multizone_vav_supply_fan.y1SupFan";
const SUPPLY_FAN_SPEED: &str = "http://example.org#g36.source.multizone_vav_supply_fan.ySupFan";
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
const RELIEF_DAMPER_BUILDING_PRESSURE: &str =
    "http://example.org#g36.source.multizone_vav_relief_damper.dpBui";
const RELIEF_DAMPER_SUPPLY_FAN_STATUS: &str =
    "http://example.org#g36.source.multizone_vav_relief_damper.u1SupFan";
const RELIEF_DAMPER_COMMAND: &str =
    "http://example.org#g36.source.multizone_vav_relief_damper.yRelDam";
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

const SAT_INPUTS: &[PointSpec] = &[
    PointSpec::real(SAT_ZONE_TEMP),
    PointSpec::real(SAT_COOLING_SETPOINT),
];
const SAT_OUTPUTS: &[PointSpec] = &[
    PointSpec::real(SAT_SETPOINT),
    PointSpec::real(SAT_COOLING_DEMAND),
];

const ECON_INPUTS: &[PointSpec] = &[
    PointSpec::real(ECON_RETURN_AIR_TEMP),
    PointSpec::real(ECON_OUTDOOR_AIR_TEMP),
    PointSpec::integer(ECON_OPERATING_MODE),
];
const ECON_OUTPUTS: &[PointSpec] = &[
    PointSpec::boolean(ECON_ENABLED),
    PointSpec::real(ECON_DAMPER_COMMAND),
    PointSpec::real(ECON_OPERATING_MODE_REAL),
    PointSpec::real(ECON_OA_TEMP_DELTA),
];

const VAV_INPUTS: &[PointSpec] = &[
    PointSpec::real(VAV_ZONE_TEMP),
    PointSpec::real(VAV_COOLING_SETPOINT),
    PointSpec::real(VAV_HEATING_SETPOINT),
];
const VAV_OUTPUTS: &[PointSpec] = &[
    PointSpec::real(VAV_DAMPER_COMMAND),
    PointSpec::real(VAV_AIRFLOW_SETPOINT),
    PointSpec::real(VAV_COOLING_SIGNAL),
    PointSpec::boolean(VAV_HEATING_ENABLED),
];
const SUPPLY_TEMPERATURE_INPUTS: &[PointSpec] = &[
    PointSpec::real(SUPPLY_TEMPERATURE_OUTDOOR_AIR),
    PointSpec::boolean(SUPPLY_TEMPERATURE_FAN_STATUS),
    PointSpec::integer(SUPPLY_TEMPERATURE_OPERATING_MODE),
    PointSpec::integer(SUPPLY_TEMPERATURE_REQUESTS),
];
const SUPPLY_TEMPERATURE_OUTPUTS: &[PointSpec] = &[PointSpec::real(SUPPLY_TEMPERATURE_SETPOINT)];
const SUPPLY_FAN_INPUTS: &[PointSpec] = &[
    PointSpec::integer(SUPPLY_FAN_OPERATING_MODE),
    PointSpec::real(SUPPLY_FAN_DUCT_PRESSURE),
    PointSpec::integer(SUPPLY_FAN_PRESSURE_REQUESTS),
];
const SUPPLY_FAN_OUTPUTS: &[PointSpec] = &[
    PointSpec::boolean(SUPPLY_FAN_STATUS),
    PointSpec::real(SUPPLY_FAN_SPEED),
];
const SUPPLY_SIGNALS_INPUTS: &[PointSpec] = &[
    PointSpec::real(SUPPLY_SIGNALS_MEASURED_TEMP),
    PointSpec::real(SUPPLY_SIGNALS_SETPOINT),
    PointSpec::boolean(SUPPLY_SIGNALS_FAN_STATUS),
];
const SUPPLY_SIGNALS_OUTPUTS: &[PointSpec] = &[
    PointSpec::real(SUPPLY_SIGNALS_U_T_SUP),
    PointSpec::real(SUPPLY_SIGNALS_HEATING),
    PointSpec::real(SUPPLY_SIGNALS_COOLING),
];
const PLANT_REQUESTS_INPUTS: &[PointSpec] = &[
    PointSpec::real(PLANT_REQUESTS_SUPPLY_AIR),
    PointSpec::real(PLANT_REQUESTS_SETPOINT),
    PointSpec::real(PLANT_REQUESTS_COOLING_VALVE),
    PointSpec::real(PLANT_REQUESTS_HEATING_VALVE),
];
const PLANT_REQUESTS_OUTPUTS: &[PointSpec] = &[
    PointSpec::integer(PLANT_REQUESTS_CHILLED_RESET),
    PointSpec::integer(PLANT_REQUESTS_CHILLER),
    PointSpec::integer(PLANT_REQUESTS_HOT_RESET),
    PointSpec::integer(PLANT_REQUESTS_HOT_PLANT),
];
const OUTDOOR_AIRFLOW_INPUTS: &[PointSpec] = &[
    PointSpec::real(OUTDOOR_AIRFLOW_POPULATION_FLOW),
    PointSpec::real(OUTDOOR_AIRFLOW_AREA_FLOW),
    PointSpec::real(OUTDOOR_AIRFLOW_PRIMARY_FLOW),
    PointSpec::real(OUTDOOR_AIRFLOW_MAX_FRACTION),
    PointSpec::real(OUTDOOR_AIRFLOW_MEASURED_FLOW),
];
const OUTDOOR_AIRFLOW_OUTPUTS: &[PointSpec] = &[
    PointSpec::real(OUTDOOR_AIRFLOW_UNCORRECTED),
    PointSpec::real(OUTDOOR_AIRFLOW_EFFECTIVE),
    PointSpec::real(OUTDOOR_AIRFLOW_EFFECTIVE_NORMALIZED),
    PointSpec::real(OUTDOOR_AIRFLOW_MEASURED_NORMALIZED),
];
const RELIEF_DAMPER_INPUTS: &[PointSpec] = &[
    PointSpec::real(RELIEF_DAMPER_BUILDING_PRESSURE),
    PointSpec::boolean(RELIEF_DAMPER_SUPPLY_FAN_STATUS),
];
const RELIEF_DAMPER_OUTPUTS: &[PointSpec] = &[PointSpec::real(RELIEF_DAMPER_COMMAND)];
const RELIEF_FAN_INPUTS: &[PointSpec] = &[
    PointSpec::real(RELIEF_FAN_BUILDING_PRESSURE),
    PointSpec::boolean(RELIEF_FAN_SUPPLY_FAN_STATUS),
];
const RELIEF_FAN_OUTPUTS: &[PointSpec] = &[
    PointSpec::real(RELIEF_FAN_AVERAGED_PRESSURE),
    PointSpec::boolean(RELIEF_FAN_DAMPER_STATUS),
    PointSpec::boolean(RELIEF_FAN_FAN_STATUS),
    PointSpec::real(RELIEF_FAN_FAN_SPEED),
];
const RETURN_FAN_AIRFLOW_INPUTS: &[PointSpec] = &[
    PointSpec::real(RETURN_FAN_AIRFLOW_SUPPLY),
    PointSpec::real(RETURN_FAN_AIRFLOW_RETURN),
    PointSpec::boolean(RETURN_FAN_AIRFLOW_SUPPLY_FAN),
];
const RETURN_FAN_AIRFLOW_OUTPUTS: &[PointSpec] = &[
    PointSpec::real(RETURN_FAN_AIRFLOW_SPEED),
    PointSpec::boolean(RETURN_FAN_AIRFLOW_STATUS),
];
const RETURN_FAN_DIRECT_PRESSURE_INPUTS: &[PointSpec] = &[
    PointSpec::real(RETURN_FAN_DIRECT_PRESSURE_BUILDING_PRESSURE),
    PointSpec::boolean(RETURN_FAN_DIRECT_PRESSURE_MIN_OUTDOOR_AIR_DAMPER),
    PointSpec::boolean(RETURN_FAN_DIRECT_PRESSURE_SUPPLY_FAN),
];
const RETURN_FAN_DIRECT_PRESSURE_OUTPUTS: &[PointSpec] = &[
    PointSpec::real(RETURN_FAN_DIRECT_PRESSURE_AVERAGED_PRESSURE),
    PointSpec::real(RETURN_FAN_DIRECT_PRESSURE_RELIEF_DAMPER),
    PointSpec::real(RETURN_FAN_DIRECT_PRESSURE_DISCHARGE_PRESSURE),
    PointSpec::real(RETURN_FAN_DIRECT_PRESSURE_SPEED),
    PointSpec::boolean(RETURN_FAN_DIRECT_PRESSURE_STATUS),
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
