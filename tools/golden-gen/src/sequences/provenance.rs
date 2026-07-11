//! Source provenance and fixture-status tables for G36 sequence goldens.

use super::{
    AIR_ECONOMIZER_HIGH_LIMITS_ASHRAE_DIFFERENTIAL, AIR_ECONOMIZER_HIGH_LIMITS_FIXED_18,
    AIR_ECONOMIZER_HIGH_LIMITS_FIXED_21, AIR_ECONOMIZER_HIGH_LIMITS_FIXED_24,
    AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_0,
    AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_1,
    AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_2,
    AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_3,
    AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_21, AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_22,
    AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_23, AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_24,
    COOLING_ONLY_ACTIVE_AIR_FLOW, COOLING_ONLY_ALARMS, COOLING_ONLY_DAMPERS,
    COOLING_ONLY_SYSTEM_REQUESTS, ECON,
    ECONOMIZER_CONTROLLER_SINGLE_DAMPER_RELIEF_DAMPER_FIXED_21, ECONOMIZER_ENABLE,
    ECONOMIZER_LIMITS_COMMON, ECONOMIZER_MODULATIONS_RELIEFS, ECONOMIZER_MODULATIONS_RETURN_FAN,
    ECONOMIZER_MODULATIONS_RETURN_FAN_RELIEF_DAMPER, FREEZE_PROTECTION, OUTDOOR_AIRFLOW_AHU,
    OUTDOOR_AIRFLOW_SUMZONE, OUTDOOR_AIRFLOW_TITLE24_AHU, OUTDOOR_AIRFLOW_TITLE24_SUMZONE,
    PLANT_REQUESTS, REHEAT_OVERRIDES, RELIEF_DAMPER, RELIEF_FAN, RELIEF_FAN_GROUP, RETURN_FAN_AIRFLOW,
    RETURN_FAN_DIRECT_PRESSURE, SAT, SUPPLY_FAN, SUPPLY_SIGNALS, SUPPLY_TEMP,
    TRIM_AND_RESPOND_HAVE_HOL_FALSE, VAV,
};

/// Pinned Buildings source revision used by every G36 Tier-A sequence oracle.
pub(super) const SOURCE_COMMIT: &str = "a131864e4c4df22ebcd52bb8da439de0087ac365";

/// Return the canonical upstream Buildings source paths for a registered sequence.
///
/// # Panics
/// Panics when `sequence` is not one of the registered G36 scenarios.
pub(super) fn source_files(sequence: &str) -> &'static str {
    match sequence {
        SAT => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/SupplyTemperature.mo",
        ECON => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Controller.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Enable.mo; Buildings/Controls/OBC/ASHRAE/G36/Generic/AirEconomizerHighLimits.mo",
        VAV => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/SingleZone/VAV/Controller.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/SingleZone/VAV/SetPoints/Supply.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/SingleZone/VAV/SetPoints/SupplyFan.mo",
        SUPPLY_TEMP => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/SupplyTemperature.mo; Buildings/Controls/OBC/ASHRAE/G36/Generic/TrimAndRespond.mo",
        SUPPLY_FAN => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/SupplyFan.mo; Buildings/Controls/OBC/ASHRAE/G36/Generic/TrimAndRespond.mo",
        SUPPLY_SIGNALS => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/SupplySignals.mo",
        TRIM_AND_RESPOND_HAVE_HOL_FALSE => {
            "Buildings/Controls/OBC/ASHRAE/G36/Generic/TrimAndRespond.mo"
        }
        PLANT_REQUESTS => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/PlantRequests.mo",
        OUTDOOR_AIRFLOW_AHU => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/package.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/package.order; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/ASHRAE62_1/package.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/ASHRAE62_1/package.order; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/ASHRAE62_1/AHU.mo",
        OUTDOOR_AIRFLOW_SUMZONE => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/package.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/package.order; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/ASHRAE62_1/package.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/ASHRAE62_1/package.order; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/ASHRAE62_1/SumZone.mo",
        OUTDOOR_AIRFLOW_TITLE24_AHU => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/package.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/package.order; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/Title24/package.order; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/Title24/AHU.mo",
        OUTDOOR_AIRFLOW_TITLE24_SUMZONE => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/package.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/package.order; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/Title24/package.order; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/Title24/SumZone.mo",
        RELIEF_DAMPER => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/ReliefDamper.mo",
        RELIEF_FAN => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/ReliefFan.mo",
        RELIEF_FAN_GROUP => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/ReliefFanGroup.mo",
        RETURN_FAN_AIRFLOW => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/ReturnFanAirflowTracking.mo",
        RETURN_FAN_DIRECT_PRESSURE => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/ReturnFanDirectPressure.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Controller.mo",
        ECONOMIZER_ENABLE => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Enable.mo",
        ECONOMIZER_LIMITS_COMMON => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Limits/Common.mo",
        ECONOMIZER_CONTROLLER_SINGLE_DAMPER_RELIEF_DAMPER_FIXED_21 => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Controller.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Limits/Common.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Enable.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Modulations/Reliefs.mo; Buildings/Controls/OBC/ASHRAE/G36/Generic/AirEconomizerHighLimits.mo",
        ECONOMIZER_MODULATIONS_RELIEFS => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Modulations/Reliefs.mo",
        ECONOMIZER_MODULATIONS_RETURN_FAN
        | ECONOMIZER_MODULATIONS_RETURN_FAN_RELIEF_DAMPER => {
            "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Modulations/ReturnFan.mo"
        }
        AIR_ECONOMIZER_HIGH_LIMITS_FIXED_24
        | AIR_ECONOMIZER_HIGH_LIMITS_FIXED_21
        | AIR_ECONOMIZER_HIGH_LIMITS_FIXED_18
        | AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_24
        | AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_23
        | AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_22
        | AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_21
        | AIR_ECONOMIZER_HIGH_LIMITS_ASHRAE_DIFFERENTIAL
        | AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_0
        | AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_1
        | AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_2
        | AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_3 => {
            "Buildings/Controls/OBC/ASHRAE/G36/Generic/AirEconomizerHighLimits.mo"
        }
        FREEZE_PROTECTION => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/FreezeProtection.mo",
        COOLING_ONLY_ACTIVE_AIR_FLOW => "Buildings/Controls/OBC/ASHRAE/G36/TerminalUnits/CoolingOnly/Subsequences/ActiveAirFlow.mo",
        COOLING_ONLY_ALARMS => "Buildings/Controls/OBC/ASHRAE/G36/TerminalUnits/CoolingOnly/Subsequences/Alarms.mo",
        COOLING_ONLY_DAMPERS => "Buildings/Controls/OBC/ASHRAE/G36/TerminalUnits/CoolingOnly/Subsequences/Dampers.mo",
        COOLING_ONLY_SYSTEM_REQUESTS => "Buildings/Controls/OBC/ASHRAE/G36/TerminalUnits/CoolingOnly/Subsequences/SystemRequests.mo",
        REHEAT_OVERRIDES => "Buildings/Controls/OBC/ASHRAE/G36/TerminalUnits/Reheat/Subsequences/Overrides.mo",
        _ => unreachable!("unknown G36 sequence {sequence}"),
    }
}

/// Return the review status of the fixture backing a registered sequence oracle.
///
/// # Panics
/// Panics when `sequence` is not one of the registered G36 scenarios.
pub(super) fn fixture_status(sequence: &str) -> &'static str {
    match sequence {
        SUPPLY_TEMP
        | SUPPLY_FAN
        | SUPPLY_SIGNALS
        | TRIM_AND_RESPOND_HAVE_HOL_FALSE
        | PLANT_REQUESTS
        | OUTDOOR_AIRFLOW_AHU
        | OUTDOOR_AIRFLOW_SUMZONE
        | OUTDOOR_AIRFLOW_TITLE24_AHU
        | OUTDOOR_AIRFLOW_TITLE24_SUMZONE
        | RELIEF_DAMPER
        | RELIEF_FAN
        | RELIEF_FAN_GROUP
        | RETURN_FAN_AIRFLOW
        | RETURN_FAN_DIRECT_PRESSURE
        | ECONOMIZER_ENABLE
        | ECONOMIZER_LIMITS_COMMON
        | ECONOMIZER_CONTROLLER_SINGLE_DAMPER_RELIEF_DAMPER_FIXED_21
        | ECONOMIZER_MODULATIONS_RELIEFS
        | ECONOMIZER_MODULATIONS_RETURN_FAN
        | ECONOMIZER_MODULATIONS_RETURN_FAN_RELIEF_DAMPER
        | AIR_ECONOMIZER_HIGH_LIMITS_FIXED_24
        | AIR_ECONOMIZER_HIGH_LIMITS_FIXED_21
        | AIR_ECONOMIZER_HIGH_LIMITS_FIXED_18
        | AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_24
        | AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_23
        | AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_22
        | AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_21
        | AIR_ECONOMIZER_HIGH_LIMITS_ASHRAE_DIFFERENTIAL
        | AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_0
        | AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_1
        | AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_2
        | AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_3
        | FREEZE_PROTECTION
        | COOLING_ONLY_ACTIVE_AIR_FLOW
        | COOLING_ONLY_ALARMS
        | COOLING_ONLY_DAMPERS
        | COOLING_ONLY_SYSTEM_REQUESTS
        | REHEAT_OVERRIDES => {
            "supported-runtime-sequence source-verified composite"
        }
        SAT | ECON | VAV => "supported-fixture-only source-reviewed fragment",
        _ => unreachable!("unknown G36 sequence {sequence}"),
    }
}
