//! Checked-in data bindings for the ASHRAE G36 catalog guard tests.

pub(super) const CATALOG_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tools/reference-catalog/Buildings.Controls.OBC.ASHRAE.G36.catalog.json"
));
pub(super) const PROV_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tools/reference-catalog/Buildings.Controls.OBC.ASHRAE.G36.prov.json"
));

pub(super) const AHU_SAT_RESET: &str =
    include_str!("../tests/fixtures/g36/ahu_supply_air_temp_reset.jsonld");
pub(super) const AHU_ECONOMIZER: &str = include_str!("../tests/fixtures/g36/ahu_economizer.jsonld");
pub(super) const VAV_SINGLE_ZONE: &str =
    include_str!("../tests/fixtures/g36/vav_single_zone.jsonld");
pub(super) const G36_TRIM_AND_RESPOND: &str =
    include_str!("../tests/fixtures/g36/trim_and_respond_have_hol_false.jsonld");
pub(super) const G36_GENERIC_TIME_SUPPRESSION: &str =
    include_str!("../tests/fixtures/g36/generic_time_suppression.jsonld");
pub(super) const G36_SUPPLY_TEMPERATURE: &str =
    include_str!("../tests/fixtures/g36/multizone_vav_supply_temperature.jsonld");
pub(super) const G36_SUPPLY_FAN: &str =
    include_str!("../tests/fixtures/g36/multizone_vav_supply_fan.jsonld");
pub(super) const G36_SUPPLY_SIGNALS: &str =
    include_str!("../tests/fixtures/g36/multizone_vav_supply_signals.jsonld");
pub(super) const G36_PLANT_REQUESTS: &str =
    include_str!("../tests/fixtures/g36/multizone_vav_plant_requests.jsonld");
pub(super) const G36_RELIEF_DAMPER: &str =
    include_str!("../tests/fixtures/g36/multizone_vav_relief_damper.jsonld");
pub(super) const G36_RELIEF_FAN: &str =
    include_str!("../tests/fixtures/g36/multizone_vav_relief_fan.jsonld");
pub(super) const G36_RELIEF_FAN_GROUP: &str =
    include_str!("../tests/fixtures/g36/multizone_vav_relief_fan_group.jsonld");
pub(super) const G36_FREEZE_PROTECTION: &str =
    include_str!("../tests/fixtures/g36/multizone_vav_freeze_protection.jsonld");
pub(super) const G36_COOLING_ONLY_ACTIVE_AIR_FLOW: &str =
    include_str!("../tests/fixtures/g36/cooling_only_active_air_flow.jsonld");
pub(super) const G36_COOLING_ONLY_ALARMS: &str =
    include_str!("../tests/fixtures/g36/cooling_only_alarms.jsonld");
pub(super) const G36_COOLING_ONLY_DAMPERS: &str =
    include_str!("../tests/fixtures/g36/cooling_only_dampers.jsonld");
pub(super) const G36_COOLING_ONLY_SYSTEM_REQUESTS: &str =
    include_str!("../tests/fixtures/g36/cooling_only_system_requests.jsonld");
pub(super) const G36_REHEAT_OVERRIDES: &str =
    include_str!("../tests/fixtures/g36/reheat_overrides.jsonld");
pub(super) const G36_RETURN_FAN_AIRFLOW: &str =
    include_str!("../tests/fixtures/g36/multizone_vav_return_fan_airflow_tracking.jsonld");
pub(super) const G36_RETURN_FAN_DIRECT_PRESSURE: &str =
    include_str!("../tests/fixtures/g36/multizone_vav_return_fan_direct_pressure.jsonld");
pub(super) const G36_ECONOMIZER_ENABLE: &str =
    include_str!("../tests/fixtures/g36/multizone_vav_economizer_enable.jsonld");
pub(super) const G36_ECONOMIZER_LIMITS_COMMON: &str =
    include_str!("../tests/fixtures/g36/multizone_vav_economizer_limits_common.jsonld");
pub(super) const G36_ECONOMIZER_MODULATIONS_RELIEFS: &str =
    include_str!("../tests/fixtures/g36/multizone_vav_economizer_modulations_reliefs.jsonld");
pub(super) const G36_ECONOMIZER_MODULATIONS_RETURN_FAN: &str =
    include_str!("../tests/fixtures/g36/multizone_vav_economizer_modulations_return_fan.jsonld");
pub(super) const G36_ECONOMIZER_MODULATIONS_RETURN_FAN_RELIEF_DAMPER: &str = include_str!(
    "../tests/fixtures/g36/multizone_vav_economizer_modulations_return_fan_relief_damper.jsonld"
);
pub(super) const G36_ECONOMIZER_CONTROLLER_SINGLE_DAMPER_RELIEF_DAMPER_FIXED_21: &str = include_str!(
    "../tests/fixtures/g36/multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.jsonld"
);
pub(super) const G36_AIR_ECONOMIZER_HIGH_LIMITS_FIXED_24: &str =
    include_str!("../tests/fixtures/g36/generic_air_economizer_high_limits_ashrae_fixed_24.jsonld");
pub(super) const G36_AIR_ECONOMIZER_HIGH_LIMITS_FIXED_21: &str =
    include_str!("../tests/fixtures/g36/generic_air_economizer_high_limits_ashrae_fixed_21.jsonld");
pub(super) const G36_AIR_ECONOMIZER_HIGH_LIMITS_FIXED_18: &str =
    include_str!("../tests/fixtures/g36/generic_air_economizer_high_limits_ashrae_fixed_18.jsonld");
pub(super) const G36_AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_24: &str = include_str!(
    "../tests/fixtures/g36/generic_air_economizer_high_limits_title24_fixed_24.jsonld"
);
pub(super) const G36_AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_23: &str = include_str!(
    "../tests/fixtures/g36/generic_air_economizer_high_limits_title24_fixed_23.jsonld"
);
pub(super) const G36_AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_22: &str = include_str!(
    "../tests/fixtures/g36/generic_air_economizer_high_limits_title24_fixed_22.jsonld"
);
pub(super) const G36_AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_21: &str = include_str!(
    "../tests/fixtures/g36/generic_air_economizer_high_limits_title24_fixed_21.jsonld"
);
pub(super) const G36_AIR_ECONOMIZER_HIGH_LIMITS_ASHRAE_DIFFERENTIAL: &str = include_str!(
    "../tests/fixtures/g36/generic_air_economizer_high_limits_ashrae_differential.jsonld"
);
pub(super) const G36_AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_0: &str = include_str!(
    "../tests/fixtures/g36/generic_air_economizer_high_limits_title24_differential_offset_0.jsonld"
);
pub(super) const G36_AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_1: &str = include_str!(
    "../tests/fixtures/g36/generic_air_economizer_high_limits_title24_differential_offset_1.jsonld"
);
pub(super) const G36_AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_2: &str = include_str!(
    "../tests/fixtures/g36/generic_air_economizer_high_limits_title24_differential_offset_2.jsonld"
);
pub(super) const G36_AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_3: &str = include_str!(
    "../tests/fixtures/g36/generic_air_economizer_high_limits_title24_differential_offset_3.jsonld"
);
pub(super) const G36_OUTDOOR_AIRFLOW_AHU: &str =
    include_str!("../tests/fixtures/g36/multizone_vav_outdoor_airflow_ahu.jsonld");
pub(super) const G36_OUTDOOR_AIRFLOW_SUMZONE: &str =
    include_str!("../tests/fixtures/g36/multizone_vav_outdoor_airflow_sumzone.jsonld");
pub(super) const G36_OUTDOOR_AIRFLOW_TITLE24_AHU: &str =
    include_str!("../tests/fixtures/g36/multizone_vav_outdoor_airflow_title24_ahu.jsonld");
pub(super) const G36_OUTDOOR_AIRFLOW_TITLE24_SUMZONE: &str =
    include_str!("../tests/fixtures/g36/multizone_vav_outdoor_airflow_title24_sumzone.jsonld");
pub(super) const PROFILE_SMALL_COMPOSITE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../_spec/oce_g36_gap_specs_v1/reference/fixtures/small-composite.jsonld"
));
pub(super) const PROFILE_PARAMETER_GATED: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../_spec/oce_g36_gap_specs_v1/reference/fixtures/parameter-gated-connector.jsonld"
));

pub(super) const EXPECTED_REFERENCE_COMMIT: &str = "a131864e4c4df22ebcd52bb8da439de0087ac365";
pub(super) const EXPECTED_CATALOG_FINGERPRINT: &str = "d4764d69789fb9e7";

pub(super) const EXPECTED_PACKAGE_ORDER_FILES: &[&str] = &[
    "Buildings/Controls/OBC/ASHRAE/G36/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/ASHRAE62_1/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/Title24/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/SingleZone/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/SingleZone/VAV/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/SingleZone/VAV/SetPoints/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/TerminalUnits/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/TerminalUnits/CoolingOnly/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/TerminalUnits/CoolingOnly/Subsequences/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/TerminalUnits/Reheat/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/TerminalUnits/Reheat/Subsequences/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/Generic/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/DemandLimitLevels/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/FreezeProtectionStages/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/OperationModes/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/ZoneStates/package.order",
];
pub(super) const EXPECTED_SEQUENCE_SOURCE_FILES: &[&str] = &[
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Controller.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Controller.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Enable.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Limits/Common.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Modulations/Reliefs.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Modulations/ReturnFan.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/package.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/ASHRAE62_1/package.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/ASHRAE62_1/AHU.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/ASHRAE62_1/SumZone.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/Title24/AHU.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/Title24/SumZone.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/FreezeProtection.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/PlantRequests.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/ReliefDamper.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/ReliefFan.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/ReliefFanGroup.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/ReturnFanAirflowTracking.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/ReturnFanDirectPressure.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/SupplyFan.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/SupplySignals.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/SupplyTemperature.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/SingleZone/VAV/Controller.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/SingleZone/VAV/SetPoints/Supply.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/SingleZone/VAV/SetPoints/SupplyFan.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/TerminalUnits/CoolingOnly/Subsequences/ActiveAirFlow.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/TerminalUnits/CoolingOnly/Subsequences/Alarms.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/TerminalUnits/CoolingOnly/Subsequences/Dampers.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/TerminalUnits/CoolingOnly/Subsequences/SystemRequests.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/TerminalUnits/Reheat/Subsequences/Overrides.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/Generic/TrimAndRespond.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/Generic/AirEconomizerHighLimits.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/Generic/TimeSuppression.mo",
];
pub(super) const EXPECTED_TYPE_SOURCE_FILES: &[&str] = &[
    "Buildings/Controls/OBC/ASHRAE/G36/Types/ASHRAEClimateZone.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/ControlEconomizer.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/CoolingCoil.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/EnergyStandard.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/FreezeStat.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/HeatingCoil.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/OutdoorAirSection.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/PressureControl.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/Title24ClimateZone.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/VentilationStandard.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/DemandLimitLevels/package.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/FreezeProtectionStages/package.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/OperationModes/package.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/ZoneStates/package.mo",
];

#[derive(Clone, Copy)]
pub(super) struct FixtureSource {
    pub(super) name: &'static str,
    pub(super) path: &'static str,
    pub(super) text: &'static str,
}

pub(super) const RUNTIME_FIXTURES: &[FixtureSource] = &[
    FixtureSource {
        name: "ahu_supply_air_temp_reset",
        path: "crates/oce-cxf/tests/fixtures/g36/ahu_supply_air_temp_reset.jsonld",
        text: AHU_SAT_RESET,
    },
    FixtureSource {
        name: "ahu_economizer",
        path: "crates/oce-cxf/tests/fixtures/g36/ahu_economizer.jsonld",
        text: AHU_ECONOMIZER,
    },
    FixtureSource {
        name: "vav_single_zone",
        path: "crates/oce-cxf/tests/fixtures/g36/vav_single_zone.jsonld",
        text: VAV_SINGLE_ZONE,
    },
];

pub(super) const COMPOSITE_IMPORT_FIXTURES: &[FixtureSource] = &[
    FixtureSource {
        name: "trim_and_respond_have_hol_false",
        path: "crates/oce-cxf/tests/fixtures/g36/trim_and_respond_have_hol_false.jsonld",
        text: G36_TRIM_AND_RESPOND,
    },
    FixtureSource {
        name: "multizone_vav_supply_temperature",
        path: "crates/oce-cxf/tests/fixtures/g36/multizone_vav_supply_temperature.jsonld",
        text: G36_SUPPLY_TEMPERATURE,
    },
    FixtureSource {
        name: "multizone_vav_supply_fan",
        path: "crates/oce-cxf/tests/fixtures/g36/multizone_vav_supply_fan.jsonld",
        text: G36_SUPPLY_FAN,
    },
    FixtureSource {
        name: "multizone_vav_supply_signals",
        path: "crates/oce-cxf/tests/fixtures/g36/multizone_vav_supply_signals.jsonld",
        text: G36_SUPPLY_SIGNALS,
    },
    FixtureSource {
        name: "multizone_vav_plant_requests",
        path: "crates/oce-cxf/tests/fixtures/g36/multizone_vav_plant_requests.jsonld",
        text: G36_PLANT_REQUESTS,
    },
    FixtureSource {
        name: "multizone_vav_relief_damper",
        path: "crates/oce-cxf/tests/fixtures/g36/multizone_vav_relief_damper.jsonld",
        text: G36_RELIEF_DAMPER,
    },
    FixtureSource {
        name: "multizone_vav_relief_fan",
        path: "crates/oce-cxf/tests/fixtures/g36/multizone_vav_relief_fan.jsonld",
        text: G36_RELIEF_FAN,
    },
    FixtureSource {
        name: "multizone_vav_relief_fan_group",
        path: "crates/oce-cxf/tests/fixtures/g36/multizone_vav_relief_fan_group.jsonld",
        text: G36_RELIEF_FAN_GROUP,
    },
    FixtureSource {
        name: "multizone_vav_freeze_protection",
        path: "crates/oce-cxf/tests/fixtures/g36/multizone_vav_freeze_protection.jsonld",
        text: G36_FREEZE_PROTECTION,
    },
    FixtureSource {
        name: "cooling_only_active_air_flow",
        path: "crates/oce-cxf/tests/fixtures/g36/cooling_only_active_air_flow.jsonld",
        text: G36_COOLING_ONLY_ACTIVE_AIR_FLOW,
    },
    FixtureSource {
        name: "cooling_only_alarms",
        path: "crates/oce-cxf/tests/fixtures/g36/cooling_only_alarms.jsonld",
        text: G36_COOLING_ONLY_ALARMS,
    },
    FixtureSource {
        name: "generic_time_suppression",
        path: "crates/oce-cxf/tests/fixtures/g36/generic_time_suppression.jsonld",
        text: G36_GENERIC_TIME_SUPPRESSION,
    },
    FixtureSource {
        name: "cooling_only_dampers",
        path: "crates/oce-cxf/tests/fixtures/g36/cooling_only_dampers.jsonld",
        text: G36_COOLING_ONLY_DAMPERS,
    },
    FixtureSource {
        name: "cooling_only_system_requests",
        path: "crates/oce-cxf/tests/fixtures/g36/cooling_only_system_requests.jsonld",
        text: G36_COOLING_ONLY_SYSTEM_REQUESTS,
    },
    FixtureSource {
        name: "reheat_overrides",
        path: "crates/oce-cxf/tests/fixtures/g36/reheat_overrides.jsonld",
        text: G36_REHEAT_OVERRIDES,
    },
    FixtureSource {
        name: "multizone_vav_return_fan_airflow_tracking",
        path: "crates/oce-cxf/tests/fixtures/g36/multizone_vav_return_fan_airflow_tracking.jsonld",
        text: G36_RETURN_FAN_AIRFLOW,
    },
    FixtureSource {
        name: "multizone_vav_return_fan_direct_pressure",
        path: "crates/oce-cxf/tests/fixtures/g36/multizone_vav_return_fan_direct_pressure.jsonld",
        text: G36_RETURN_FAN_DIRECT_PRESSURE,
    },
    FixtureSource {
        name: "multizone_vav_economizer_enable",
        path: "crates/oce-cxf/tests/fixtures/g36/multizone_vav_economizer_enable.jsonld",
        text: G36_ECONOMIZER_ENABLE,
    },
    FixtureSource {
        name: "multizone_vav_economizer_limits_common",
        path: "crates/oce-cxf/tests/fixtures/g36/multizone_vav_economizer_limits_common.jsonld",
        text: G36_ECONOMIZER_LIMITS_COMMON,
    },
    FixtureSource {
        name: "multizone_vav_economizer_modulations_reliefs",
        path: "crates/oce-cxf/tests/fixtures/g36/multizone_vav_economizer_modulations_reliefs.jsonld",
        text: G36_ECONOMIZER_MODULATIONS_RELIEFS,
    },
    FixtureSource {
        name: "multizone_vav_economizer_modulations_return_fan",
        path: "crates/oce-cxf/tests/fixtures/g36/multizone_vav_economizer_modulations_return_fan.jsonld",
        text: G36_ECONOMIZER_MODULATIONS_RETURN_FAN,
    },
    FixtureSource {
        name: "multizone_vav_economizer_modulations_return_fan_relief_damper",
        path: "crates/oce-cxf/tests/fixtures/g36/multizone_vav_economizer_modulations_return_fan_relief_damper.jsonld",
        text: G36_ECONOMIZER_MODULATIONS_RETURN_FAN_RELIEF_DAMPER,
    },
    FixtureSource {
        name: "multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21",
        path: "crates/oce-cxf/tests/fixtures/g36/multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.jsonld",
        text: G36_ECONOMIZER_CONTROLLER_SINGLE_DAMPER_RELIEF_DAMPER_FIXED_21,
    },
    FixtureSource {
        name: "generic_air_economizer_high_limits_ashrae_fixed_24",
        path: "crates/oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_ashrae_fixed_24.jsonld",
        text: G36_AIR_ECONOMIZER_HIGH_LIMITS_FIXED_24,
    },
    FixtureSource {
        name: "generic_air_economizer_high_limits_ashrae_fixed_21",
        path: "crates/oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_ashrae_fixed_21.jsonld",
        text: G36_AIR_ECONOMIZER_HIGH_LIMITS_FIXED_21,
    },
    FixtureSource {
        name: "generic_air_economizer_high_limits_ashrae_fixed_18",
        path: "crates/oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_ashrae_fixed_18.jsonld",
        text: G36_AIR_ECONOMIZER_HIGH_LIMITS_FIXED_18,
    },
    FixtureSource {
        name: "generic_air_economizer_high_limits_title24_fixed_24",
        path: "crates/oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_title24_fixed_24.jsonld",
        text: G36_AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_24,
    },
    FixtureSource {
        name: "generic_air_economizer_high_limits_title24_fixed_23",
        path: "crates/oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_title24_fixed_23.jsonld",
        text: G36_AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_23,
    },
    FixtureSource {
        name: "generic_air_economizer_high_limits_title24_fixed_22",
        path: "crates/oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_title24_fixed_22.jsonld",
        text: G36_AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_22,
    },
    FixtureSource {
        name: "generic_air_economizer_high_limits_title24_fixed_21",
        path: "crates/oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_title24_fixed_21.jsonld",
        text: G36_AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_21,
    },
    FixtureSource {
        name: "generic_air_economizer_high_limits_ashrae_differential",
        path: "crates/oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_ashrae_differential.jsonld",
        text: G36_AIR_ECONOMIZER_HIGH_LIMITS_ASHRAE_DIFFERENTIAL,
    },
    FixtureSource {
        name: "generic_air_economizer_high_limits_title24_differential_offset_0",
        path: "crates/oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_title24_differential_offset_0.jsonld",
        text: G36_AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_0,
    },
    FixtureSource {
        name: "generic_air_economizer_high_limits_title24_differential_offset_1",
        path: "crates/oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_title24_differential_offset_1.jsonld",
        text: G36_AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_1,
    },
    FixtureSource {
        name: "generic_air_economizer_high_limits_title24_differential_offset_2",
        path: "crates/oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_title24_differential_offset_2.jsonld",
        text: G36_AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_2,
    },
    FixtureSource {
        name: "generic_air_economizer_high_limits_title24_differential_offset_3",
        path: "crates/oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_title24_differential_offset_3.jsonld",
        text: G36_AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_3,
    },
    FixtureSource {
        name: "multizone_vav_outdoor_airflow_ahu",
        path: "crates/oce-cxf/tests/fixtures/g36/multizone_vav_outdoor_airflow_ahu.jsonld",
        text: G36_OUTDOOR_AIRFLOW_AHU,
    },
    FixtureSource {
        name: "multizone_vav_outdoor_airflow_sumzone",
        path: "crates/oce-cxf/tests/fixtures/g36/multizone_vav_outdoor_airflow_sumzone.jsonld",
        text: G36_OUTDOOR_AIRFLOW_SUMZONE,
    },
    FixtureSource {
        name: "multizone_vav_outdoor_airflow_title24_ahu",
        path: "crates/oce-cxf/tests/fixtures/g36/multizone_vav_outdoor_airflow_title24_ahu.jsonld",
        text: G36_OUTDOOR_AIRFLOW_TITLE24_AHU,
    },
    FixtureSource {
        name: "multizone_vav_outdoor_airflow_title24_sumzone",
        path: "crates/oce-cxf/tests/fixtures/g36/multizone_vav_outdoor_airflow_title24_sumzone.jsonld",
        text: G36_OUTDOOR_AIRFLOW_TITLE24_SUMZONE,
    },
];

pub(super) const PROFILE_FIXTURES: &[FixtureSource] = &[
    FixtureSource {
        name: "small_composite",
        path: "_spec/oce_g36_gap_specs_v1/reference/fixtures/small-composite.jsonld",
        text: PROFILE_SMALL_COMPOSITE,
    },
    FixtureSource {
        name: "parameter_gated_connector",
        path: "_spec/oce_g36_gap_specs_v1/reference/fixtures/parameter-gated-connector.jsonld",
        text: PROFILE_PARAMETER_GATED,
    },
];
