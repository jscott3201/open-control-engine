//! Shared G36 Tier-A sequence table and neutral path/kind helpers.
//!
//! Extracted from `g36_funnel.rs` so both the exact-oracle suite (`g36_funnel.rs`) and the
//! funnel-band routing suite (`g36_funnel_band_routing.rs`) key on a single source of truth for every
//! connector id, fixture, and golden path — no duplicated (and driftable) signal identity. Included by
//! each via `#[path = "g36_funnel_band/sequences.rs"] mod sequences;`, and kept in a subdirectory so
//! Cargo does not compile it as its own integration-test binary. Each consumer uses a different subset,
//! so `#![allow(dead_code)]` covers items unused in any one binary.
//!
//! `include_str!` paths carry one extra `../` versus `g36_funnel.rs` because this file sits one level
//! deeper (`tests/g36_funnel_band/`); the macro resolves relative to this file, not the includer.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use oce_conformance::ValueKind;

const AHU_SAT_RESET: &str =
    include_str!("../../../oce-cxf/tests/fixtures/g36/ahu_supply_air_temp_reset.jsonld");
const AHU_ECONOMIZER: &str =
    include_str!("../../../oce-cxf/tests/fixtures/g36/ahu_economizer.jsonld");
const VAV_SINGLE_ZONE: &str =
    include_str!("../../../oce-cxf/tests/fixtures/g36/vav_single_zone.jsonld");
const SUPPLY_TEMPERATURE: &str =
    include_str!("../../../oce-cxf/tests/fixtures/g36/multizone_vav_supply_temperature.jsonld");
const SUPPLY_FAN: &str =
    include_str!("../../../oce-cxf/tests/fixtures/g36/multizone_vav_supply_fan.jsonld");
const SUPPLY_SIGNALS: &str =
    include_str!("../../../oce-cxf/tests/fixtures/g36/multizone_vav_supply_signals.jsonld");
const TRIM_AND_RESPOND: &str =
    include_str!("../../../oce-cxf/tests/fixtures/g36/trim_and_respond_have_hol_false.jsonld");
const PLANT_REQUESTS: &str =
    include_str!("../../../oce-cxf/tests/fixtures/g36/multizone_vav_plant_requests.jsonld");
const OUTDOOR_AIRFLOW_AHU: &str =
    include_str!("../../../oce-cxf/tests/fixtures/g36/multizone_vav_outdoor_airflow_ahu.jsonld");

const GOLDEN_DIR: &str = "../../tools/golden-gen/goldens/G36";

const SAT_ZONE_TEMP: &str = "http://example.org#g36.ahu_supply_air_temp_reset.zone_temp";
const SAT_COOLING_SETPOINT: &str =
    "http://example.org#g36.ahu_supply_air_temp_reset.cooling_setpoint";
const SAT_SETPOINT_PATH: &str = "http://example.org#g36.ahu_supply_air_temp_reset.satLine.y";
const SAT_COOLING_DEMAND_PATH: &str =
    "http://example.org#g36.ahu_supply_air_temp_reset.demandLimiter.y";

const ECON_RETURN_AIR_TEMP: &str = "http://example.org#g36.ahu_economizer.return_air_temp";
const ECON_OUTDOOR_AIR_TEMP: &str = "http://example.org#g36.ahu_economizer.outdoor_air_temp";
const ECON_OPERATING_MODE: &str = "http://example.org#g36.ahu_economizer.operating_mode";
const ECON_ENABLED_PATH: &str = "http://example.org#g36.ahu_economizer.enableLatch.y";
const ECON_DAMPER_COMMAND_PATH: &str = "http://example.org#g36.ahu_economizer.damperSwitch.y";
const ECON_OPERATING_MODE_REAL_PATH: &str = "http://example.org#g36.ahu_economizer.modeToReal.y";
const ECON_OA_TEMP_DELTA_PATH: &str = "http://example.org#g36.ahu_economizer.returnMinusOutdoor.y";

const VAV_ZONE_TEMP: &str = "http://example.org#g36.vav_single_zone.zone_temp";
const VAV_COOLING_SETPOINT: &str = "http://example.org#g36.vav_single_zone.cooling_setpoint";
const VAV_HEATING_SETPOINT: &str = "http://example.org#g36.vav_single_zone.heating_setpoint";
pub const VAV_DAMPER_COMMAND_PATH: &str = "http://example.org#g36.vav_single_zone.damperLimiter.y";
pub const VAV_AIRFLOW_SETPOINT_PATH: &str =
    "http://example.org#g36.vav_single_zone.airflowSwitch.y";
const VAV_COOLING_SIGNAL_PATH: &str = "http://example.org#g36.vav_single_zone.coolingPid.y";
pub const VAV_HEATING_ENABLED_PATH: &str = "http://example.org#g36.vav_single_zone.heatingNeed.y";
const SUPPLY_TEMPERATURE_OUTDOOR_AIR: &str =
    "http://example.org#g36.source.multizone_vav_supply_temperature.TOut";
const SUPPLY_TEMPERATURE_FAN_STATUS: &str =
    "http://example.org#g36.source.multizone_vav_supply_temperature.u1SupFan";
const SUPPLY_TEMPERATURE_OPERATING_MODE: &str =
    "http://example.org#g36.source.multizone_vav_supply_temperature.uOpeMod";
const SUPPLY_TEMPERATURE_REQUESTS: &str =
    "http://example.org#g36.source.multizone_vav_supply_temperature.uZonTemResReq";
const SUPPLY_TEMPERATURE_SETPOINT_PATH: &str =
    "http://example.org#g36.source.multizone_vav_supply_temperature.swi3.y";
const SUPPLY_FAN_OPERATING_MODE: &str =
    "http://example.org#g36.source.multizone_vav_supply_fan.uOpeMod";
const SUPPLY_FAN_DUCT_PRESSURE: &str =
    "http://example.org#g36.source.multizone_vav_supply_fan.dpDuc";
const SUPPLY_FAN_PRESSURE_REQUESTS: &str =
    "http://example.org#g36.source.multizone_vav_supply_fan.uZonPreResReq";
const SUPPLY_FAN_STATUS_PATH: &str = "http://example.org#g36.source.multizone_vav_supply_fan.or1.y";
const SUPPLY_FAN_SPEED_PATH: &str = "http://example.org#g36.source.multizone_vav_supply_fan.swi.y";
const SUPPLY_SIGNALS_MEASURED_TEMP: &str =
    "http://example.org#g36.source.multizone_vav_supply_signals.TAirSup";
const SUPPLY_SIGNALS_SETPOINT: &str =
    "http://example.org#g36.source.multizone_vav_supply_signals.TAirSupSet";
const SUPPLY_SIGNALS_FAN_STATUS: &str =
    "http://example.org#g36.source.multizone_vav_supply_signals.u1SupFan";
const SUPPLY_SIGNALS_U_T_SUP_PATH: &str =
    "http://example.org#g36.source.multizone_vav_supply_signals.swi.y";
const SUPPLY_SIGNALS_COOLING_PATH: &str =
    "http://example.org#g36.source.multizone_vav_supply_signals.conSigCoo.y";
const SUPPLY_SIGNALS_HEATING_PATH: &str =
    "http://example.org#g36.source.multizone_vav_supply_signals.conSigHea.y";
const TRIM_AND_RESPOND_REQUESTS: &str =
    "http://example.org#g36.source.trim_and_respond_have_hol_false.numOfReq";
const TRIM_AND_RESPOND_DEVICE_STATUS: &str =
    "http://example.org#g36.source.trim_and_respond_have_hol_false.uDevSta";
const TRIM_AND_RESPOND_SETPOINT_PATH: &str =
    "http://example.org#g36.source.trim_and_respond_have_hol_false.swi.y";
const PLANT_REQUESTS_SUPPLY_AIR: &str =
    "http://example.org#g36.source.multizone_vav_plant_requests.TAirSup";
const PLANT_REQUESTS_SETPOINT: &str =
    "http://example.org#g36.source.multizone_vav_plant_requests.TAirSupSet";
const PLANT_REQUESTS_COOLING_VALVE: &str =
    "http://example.org#g36.source.multizone_vav_plant_requests.uCooCoiSet";
const PLANT_REQUESTS_HEATING_VALVE: &str =
    "http://example.org#g36.source.multizone_vav_plant_requests.uHeaCoiSet";
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
const OUTDOOR_AIRFLOW_UNCORRECTED_PATH: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_ahu.min1.y";
const OUTDOOR_AIRFLOW_EFFECTIVE_PATH: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_ahu.min2.y";
const OUTDOOR_AIRFLOW_EFFECTIVE_NORMALIZED_PATH: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_ahu.norVOutMin.y";
const OUTDOOR_AIRFLOW_MEASURED_NORMALIZED_PATH: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_ahu.norVOut.y";

const SAT_INPUTS: &[PointSpec] = &[
    PointSpec::real("zone_temp", SAT_ZONE_TEMP),
    PointSpec::real("cooling_setpoint", SAT_COOLING_SETPOINT),
];
const SAT_EXACT_OUTPUTS: &[PointSpec] = &[
    PointSpec::real("sat_setpoint", SAT_SETPOINT_PATH),
    PointSpec::real("cooling_demand", SAT_COOLING_DEMAND_PATH),
];

const ECON_INPUTS: &[PointSpec] = &[
    PointSpec::real("return_air_temp", ECON_RETURN_AIR_TEMP),
    PointSpec::real("outdoor_air_temp", ECON_OUTDOOR_AIR_TEMP),
    PointSpec::integer("operating_mode", ECON_OPERATING_MODE),
];
const ECON_EXACT_OUTPUTS: &[PointSpec] = &[
    PointSpec::real("oa_temperature_delta", ECON_OA_TEMP_DELTA_PATH),
    PointSpec::real("operating_mode_real", ECON_OPERATING_MODE_REAL_PATH),
    PointSpec::boolean("economizer_enabled", ECON_ENABLED_PATH),
    PointSpec::real("damper_command", ECON_DAMPER_COMMAND_PATH),
];

const VAV_INPUTS: &[PointSpec] = &[
    PointSpec::real("zone_temp", VAV_ZONE_TEMP),
    PointSpec::real("cooling_setpoint", VAV_COOLING_SETPOINT),
    PointSpec::real("heating_setpoint", VAV_HEATING_SETPOINT),
];
const VAV_EXACT_OUTPUTS: &[PointSpec] = &[
    PointSpec::real("damper_command", VAV_DAMPER_COMMAND_PATH),
    PointSpec::real("airflow_setpoint", VAV_AIRFLOW_SETPOINT_PATH),
    PointSpec::real("cooling_signal", VAV_COOLING_SIGNAL_PATH),
    PointSpec::boolean("heating_enabled", VAV_HEATING_ENABLED_PATH),
];
const SUPPLY_TEMPERATURE_INPUTS: &[PointSpec] = &[
    PointSpec::real("outdoor_air_temperature", SUPPLY_TEMPERATURE_OUTDOOR_AIR),
    PointSpec::boolean("supply_fan_status", SUPPLY_TEMPERATURE_FAN_STATUS),
    PointSpec::integer("operating_mode", SUPPLY_TEMPERATURE_OPERATING_MODE),
    PointSpec::integer(
        "zone_temperature_reset_requests",
        SUPPLY_TEMPERATURE_REQUESTS,
    ),
];
const SUPPLY_TEMPERATURE_EXACT_OUTPUTS: &[PointSpec] = &[PointSpec::real(
    "supply_air_temperature_setpoint",
    SUPPLY_TEMPERATURE_SETPOINT_PATH,
)];
const SUPPLY_FAN_INPUTS: &[PointSpec] = &[
    PointSpec::integer("operating_mode", SUPPLY_FAN_OPERATING_MODE),
    PointSpec::real("duct_static_pressure", SUPPLY_FAN_DUCT_PRESSURE),
    PointSpec::integer("zone_pressure_reset_requests", SUPPLY_FAN_PRESSURE_REQUESTS),
];
const SUPPLY_FAN_EXACT_OUTPUTS: &[PointSpec] = &[
    PointSpec::boolean("supply_fan_status", SUPPLY_FAN_STATUS_PATH),
    PointSpec::real("supply_fan_speed", SUPPLY_FAN_SPEED_PATH),
];
const SUPPLY_SIGNALS_INPUTS: &[PointSpec] = &[
    PointSpec::real("supply_air_temperature", SUPPLY_SIGNALS_MEASURED_TEMP),
    PointSpec::real("supply_air_temperature_setpoint", SUPPLY_SIGNALS_SETPOINT),
    PointSpec::boolean("supply_fan_status", SUPPLY_SIGNALS_FAN_STATUS),
];
const SUPPLY_SIGNALS_EXACT_OUTPUTS: &[PointSpec] = &[
    PointSpec::real("uTSup", SUPPLY_SIGNALS_U_T_SUP_PATH),
    PointSpec::real("yHeaCoi", SUPPLY_SIGNALS_HEATING_PATH),
    PointSpec::real("yCooCoi", SUPPLY_SIGNALS_COOLING_PATH),
];
const TRIM_AND_RESPOND_INPUTS: &[PointSpec] = &[
    PointSpec::integer("request_count", TRIM_AND_RESPOND_REQUESTS),
    PointSpec::boolean("device_status", TRIM_AND_RESPOND_DEVICE_STATUS),
];
const TRIM_AND_RESPOND_EXACT_OUTPUTS: &[PointSpec] =
    &[PointSpec::real("setpoint", TRIM_AND_RESPOND_SETPOINT_PATH)];
const PLANT_REQUESTS_INPUTS: &[PointSpec] = &[
    PointSpec::real("supply_air_temperature", PLANT_REQUESTS_SUPPLY_AIR),
    PointSpec::real("supply_air_temperature_setpoint", PLANT_REQUESTS_SETPOINT),
    PointSpec::real("cooling_coil_valve", PLANT_REQUESTS_COOLING_VALVE),
    PointSpec::real("heating_coil_valve", PLANT_REQUESTS_HEATING_VALVE),
];
const PLANT_REQUESTS_EXACT_OUTPUTS: &[PointSpec] = &[
    PointSpec::integer(
        "chilled_water_reset_request",
        PLANT_REQUESTS_CHILLED_RESET_PATH,
    ),
    PointSpec::integer("chiller_plant_request", PLANT_REQUESTS_CHILLER_PATH),
    PointSpec::integer("hot_water_reset_request", PLANT_REQUESTS_HOT_RESET_PATH),
    PointSpec::integer("hot_water_plant_request", PLANT_REQUESTS_HOT_PLANT_PATH),
];
const OUTDOOR_AIRFLOW_INPUTS: &[PointSpec] = &[
    PointSpec::real("population_flow", OUTDOOR_AIRFLOW_POPULATION_FLOW),
    PointSpec::real("area_flow", OUTDOOR_AIRFLOW_AREA_FLOW),
    PointSpec::real("primary_flow", OUTDOOR_AIRFLOW_PRIMARY_FLOW),
    PointSpec::real("max_outdoor_air_fraction", OUTDOOR_AIRFLOW_MAX_FRACTION),
    PointSpec::real("measured_outdoor_air", OUTDOOR_AIRFLOW_MEASURED_FLOW),
];
const OUTDOOR_AIRFLOW_EXACT_OUTPUTS: &[PointSpec] = &[
    PointSpec::real(
        "uncorrected_outdoor_airflow",
        OUTDOOR_AIRFLOW_UNCORRECTED_PATH,
    ),
    PointSpec::real(
        "effective_minimum_outdoor_airflow",
        OUTDOOR_AIRFLOW_EFFECTIVE_PATH,
    ),
    PointSpec::real(
        "effective_outdoor_airflow_normalized",
        OUTDOOR_AIRFLOW_EFFECTIVE_NORMALIZED_PATH,
    ),
    PointSpec::real(
        "measured_outdoor_airflow_normalized",
        OUTDOOR_AIRFLOW_MEASURED_NORMALIZED_PATH,
    ),
];

pub const SEQUENCES: &[SequenceSpec] = &[
    SequenceSpec {
        name: "ahu_supply_air_temp_reset",
        cxf: AHU_SAT_RESET,
        t_stop: 4,
        sample_step: 1.0,
        inputs: SAT_INPUTS,
        exact_outputs: SAT_EXACT_OUTPUTS,
        masked_outputs: &[],
    },
    SequenceSpec {
        name: "ahu_economizer",
        cxf: AHU_ECONOMIZER,
        t_stop: 5,
        sample_step: 1.0,
        inputs: ECON_INPUTS,
        exact_outputs: ECON_EXACT_OUTPUTS,
        masked_outputs: &[],
    },
    SequenceSpec {
        name: "vav_single_zone",
        cxf: VAV_SINGLE_ZONE,
        t_stop: 5,
        sample_step: 1.0,
        inputs: VAV_INPUTS,
        exact_outputs: VAV_EXACT_OUTPUTS,
        masked_outputs: &[],
    },
    SequenceSpec {
        name: "multizone_vav_supply_temperature",
        cxf: SUPPLY_TEMPERATURE,
        t_stop: 900,
        sample_step: 1.0,
        inputs: SUPPLY_TEMPERATURE_INPUTS,
        exact_outputs: SUPPLY_TEMPERATURE_EXACT_OUTPUTS,
        masked_outputs: &[],
    },
    SequenceSpec {
        name: "multizone_vav_supply_fan",
        cxf: SUPPLY_FAN,
        t_stop: 900,
        sample_step: 1.0,
        inputs: SUPPLY_FAN_INPUTS,
        exact_outputs: SUPPLY_FAN_EXACT_OUTPUTS,
        masked_outputs: &[],
    },
    SequenceSpec {
        name: "multizone_vav_supply_signals",
        cxf: SUPPLY_SIGNALS,
        t_stop: 9,
        sample_step: 1.0,
        inputs: SUPPLY_SIGNALS_INPUTS,
        exact_outputs: SUPPLY_SIGNALS_EXACT_OUTPUTS,
        masked_outputs: &[],
    },
    SequenceSpec {
        name: "trim_and_respond_have_hol_false",
        cxf: TRIM_AND_RESPOND,
        t_stop: 22,
        sample_step: 60.0,
        inputs: TRIM_AND_RESPOND_INPUTS,
        exact_outputs: TRIM_AND_RESPOND_EXACT_OUTPUTS,
        masked_outputs: &[],
    },
    SequenceSpec {
        name: "multizone_vav_plant_requests",
        cxf: PLANT_REQUESTS,
        t_stop: 19,
        sample_step: 60.0,
        inputs: PLANT_REQUESTS_INPUTS,
        exact_outputs: PLANT_REQUESTS_EXACT_OUTPUTS,
        masked_outputs: &[],
    },
    SequenceSpec {
        name: "multizone_vav_outdoor_airflow_ahu",
        cxf: OUTDOOR_AIRFLOW_AHU,
        t_stop: 4,
        sample_step: 1.0,
        inputs: OUTDOOR_AIRFLOW_INPUTS,
        exact_outputs: OUTDOOR_AIRFLOW_EXACT_OUTPUTS,
        masked_outputs: &[],
    },
];

#[derive(Clone, Copy)]
pub struct PointSpec {
    pub reference_name: &'static str,
    pub cdl_name: &'static str,
    pub kind: ValueKind,
}

impl PointSpec {
    const fn real(reference_name: &'static str, cdl_name: &'static str) -> Self {
        Self {
            reference_name,
            cdl_name,
            kind: ValueKind::Real,
        }
    }

    const fn integer(reference_name: &'static str, cdl_name: &'static str) -> Self {
        Self {
            reference_name,
            cdl_name,
            kind: ValueKind::Integer,
        }
    }

    const fn boolean(reference_name: &'static str, cdl_name: &'static str) -> Self {
        Self {
            reference_name,
            cdl_name,
            kind: ValueKind::Boolean,
        }
    }
}

pub struct SequenceSpec {
    pub name: &'static str,
    pub cxf: &'static str,
    pub t_stop: u32,
    pub sample_step: f64,
    pub inputs: &'static [PointSpec],
    pub exact_outputs: &'static [PointSpec],
    pub masked_outputs: &'static [PointSpec],
}

pub fn reference_path(spec: &SequenceSpec) -> PathBuf {
    reference_dir(spec).join("reference.csv")
}

pub fn reference_dir(spec: &SequenceSpec) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(GOLDEN_DIR)
        .join(spec.name)
}

pub fn kind_name(kind: ValueKind) -> &'static str {
    match kind {
        ValueKind::Real => "Real",
        ValueKind::Integer => "Integer",
        ValueKind::Boolean => "Boolean",
    }
}
