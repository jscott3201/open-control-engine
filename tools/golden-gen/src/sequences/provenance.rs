//! Source provenance and fixture-status tables for G36 sequence goldens.

use super::freeze_protection::FREEZE_PROTECTION_FINE_STEP;
use super::{
    AIR_ECONOMIZER_HIGH_LIMITS_ASHRAE_DIFFERENTIAL, AIR_ECONOMIZER_HIGH_LIMITS_FIXED_18,
    AIR_ECONOMIZER_HIGH_LIMITS_FIXED_21, AIR_ECONOMIZER_HIGH_LIMITS_FIXED_24,
    AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_0,
    AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_1,
    AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_2,
    AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_3,
    AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_21, AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_22,
    AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_23, AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_24,
    ASHRAE62_1_SETPOINTS, COOLING_ONLY_ACTIVE_AIR_FLOW, COOLING_ONLY_ALARMS,
    COOLING_ONLY_CONTROLLER, COOLING_ONLY_DAMPERS, COOLING_ONLY_SYSTEM_REQUESTS, ECON,
    ECONOMIZER_CONTROLLER_SINGLE_DAMPER_RELIEF_DAMPER_FIXED_21, ECONOMIZER_ENABLE,
    ECONOMIZER_LIMITS_COMMON, ECONOMIZER_MODULATIONS_RELIEFS, ECONOMIZER_MODULATIONS_RETURN_FAN,
    ECONOMIZER_MODULATIONS_RETURN_FAN_RELIEF_DAMPER, FREEZE_PROTECTION, OUTDOOR_AIRFLOW_AHU,
    OUTDOOR_AIRFLOW_SUMZONE, OUTDOOR_AIRFLOW_TITLE24_AHU, OUTDOOR_AIRFLOW_TITLE24_SUMZONE,
    PLANT_REQUESTS, REHEAT_OVERRIDES, RELIEF_DAMPER, RELIEF_FAN, RELIEF_FAN_GROUP,
    RETURN_FAN_AIRFLOW, RETURN_FAN_DIRECT_PRESSURE, SAT, SUPPLY_FAN, SUPPLY_SIGNALS, SUPPLY_TEMP,
    THERMAL_ZONES_CONTROL_LOOPS, THERMAL_ZONES_ZONE_STATES, TIME_SUPPRESSION,
    TRIM_AND_RESPOND_HAVE_HOL_FALSE, VAV,
};

/// Pinned Buildings source revision used by every G36 Tier-A sequence reference.
pub(super) const SOURCE_COMMIT: &str = "a131864e4c4df22ebcd52bb8da439de0087ac365";

/// Return the canonical upstream Buildings source paths for a registered sequence.
///
/// # Panics
/// Panics when `sequence` is not one of the registered G36 scenarios.
pub(super) fn source_files(sequence: &str) -> &'static str {
    match sequence {
        // Fixture-local compositions cite only the CDL elementary blocks whose semantics the
        // reference applies; they are not derived from any G36 sequence `.mo`.
        SAT => {
            "Buildings/Controls/OBC/CDL/Reals/Subtract.mo; Buildings/Controls/OBC/CDL/Reals/Limiter.mo; Buildings/Controls/OBC/CDL/Reals/Sources/Constant.mo; Buildings/Controls/OBC/CDL/Reals/Line.mo"
        }
        ECON => {
            "Buildings/Controls/OBC/CDL/Reals/Subtract.mo; Buildings/Controls/OBC/CDL/Reals/Hysteresis.mo; Buildings/Controls/OBC/CDL/Integers/AddParameter.mo; Buildings/Controls/OBC/CDL/Integers/GreaterThreshold.mo; Buildings/Controls/OBC/CDL/Conversions/IntegerToReal.mo; Buildings/Controls/OBC/CDL/Logical/And.mo; Buildings/Controls/OBC/CDL/Logical/Not.mo; Buildings/Controls/OBC/CDL/Logical/TrueDelay.mo; Buildings/Controls/OBC/CDL/Logical/Latch.mo; Buildings/Controls/OBC/CDL/Reals/Sources/Constant.mo; Buildings/Controls/OBC/CDL/Reals/Switch.mo"
        }
        VAV => {
            "Buildings/Controls/OBC/CDL/Reals/PID.mo; Buildings/Controls/OBC/CDL/Reals/Less.mo; Buildings/Controls/OBC/CDL/Reals/AddParameter.mo; Buildings/Controls/OBC/CDL/Reals/Max.mo; Buildings/Controls/OBC/CDL/Reals/Switch.mo; Buildings/Controls/OBC/CDL/Reals/Limiter.mo; Buildings/Controls/OBC/CDL/Reals/Sources/Constant.mo"
        }
        SUPPLY_TEMP => {
            "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/SupplyTemperature.mo; Buildings/Controls/OBC/ASHRAE/G36/Generic/TrimAndRespond.mo"
        }
        SUPPLY_FAN => {
            "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/SupplyFan.mo; Buildings/Controls/OBC/ASHRAE/G36/Generic/TrimAndRespond.mo"
        }
        SUPPLY_SIGNALS => {
            "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/SupplySignals.mo"
        }
        TRIM_AND_RESPOND_HAVE_HOL_FALSE => {
            "Buildings/Controls/OBC/ASHRAE/G36/Generic/TrimAndRespond.mo"
        }
        PLANT_REQUESTS => {
            "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/PlantRequests.mo"
        }
        OUTDOOR_AIRFLOW_AHU => {
            "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/package.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/package.order; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/ASHRAE62_1/package.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/ASHRAE62_1/package.order; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/ASHRAE62_1/AHU.mo"
        }
        OUTDOOR_AIRFLOW_SUMZONE => {
            "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/package.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/package.order; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/ASHRAE62_1/package.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/ASHRAE62_1/package.order; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/ASHRAE62_1/SumZone.mo"
        }
        OUTDOOR_AIRFLOW_TITLE24_AHU => {
            "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/package.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/package.order; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/Title24/package.order; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/Title24/AHU.mo"
        }
        OUTDOOR_AIRFLOW_TITLE24_SUMZONE => {
            "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/package.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/package.order; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/Title24/package.order; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/Title24/SumZone.mo"
        }
        RELIEF_DAMPER => {
            "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/ReliefDamper.mo"
        }
        RELIEF_FAN => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/ReliefFan.mo",
        RELIEF_FAN_GROUP => {
            "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/ReliefFanGroup.mo"
        }
        RETURN_FAN_AIRFLOW => {
            "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/ReturnFanAirflowTracking.mo"
        }
        RETURN_FAN_DIRECT_PRESSURE => {
            "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/ReturnFanDirectPressure.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Controller.mo"
        }
        ECONOMIZER_ENABLE => {
            "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Enable.mo"
        }
        ECONOMIZER_LIMITS_COMMON => {
            "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Limits/Common.mo"
        }
        ECONOMIZER_CONTROLLER_SINGLE_DAMPER_RELIEF_DAMPER_FIXED_21 => {
            "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Controller.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Limits/Common.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Enable.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Modulations/Reliefs.mo; Buildings/Controls/OBC/ASHRAE/G36/Generic/AirEconomizerHighLimits.mo"
        }
        ECONOMIZER_MODULATIONS_RELIEFS => {
            "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Modulations/Reliefs.mo"
        }
        ECONOMIZER_MODULATIONS_RETURN_FAN | ECONOMIZER_MODULATIONS_RETURN_FAN_RELIEF_DAMPER => {
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
        FREEZE_PROTECTION | FREEZE_PROTECTION_FINE_STEP => {
            "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/FreezeProtection.mo"
        }
        TIME_SUPPRESSION => "Buildings/Controls/OBC/ASHRAE/G36/Generic/TimeSuppression.mo",
        THERMAL_ZONES_ZONE_STATES => "Buildings/Controls/OBC/ASHRAE/G36/ThermalZones/ZoneStates.mo",
        THERMAL_ZONES_CONTROL_LOOPS => {
            "Buildings/Controls/OBC/ASHRAE/G36/ThermalZones/ControlLoops.mo"
        }
        ASHRAE62_1_SETPOINTS => {
            "Buildings/Controls/OBC/ASHRAE/G36/VentilationZones/ASHRAE62_1/Setpoints.mo"
        }
        COOLING_ONLY_ACTIVE_AIR_FLOW => {
            "Buildings/Controls/OBC/ASHRAE/G36/TerminalUnits/CoolingOnly/Subsequences/ActiveAirFlow.mo"
        }
        COOLING_ONLY_ALARMS => {
            "Buildings/Controls/OBC/ASHRAE/G36/TerminalUnits/CoolingOnly/Subsequences/Alarms.mo"
        }
        COOLING_ONLY_CONTROLLER => {
            "Buildings/Controls/OBC/ASHRAE/G36/TerminalUnits/CoolingOnly/Controller.mo"
        }
        COOLING_ONLY_DAMPERS => {
            "Buildings/Controls/OBC/ASHRAE/G36/TerminalUnits/CoolingOnly/Subsequences/Dampers.mo"
        }
        COOLING_ONLY_SYSTEM_REQUESTS => {
            "Buildings/Controls/OBC/ASHRAE/G36/TerminalUnits/CoolingOnly/Subsequences/SystemRequests.mo"
        }
        REHEAT_OVERRIDES => {
            "Buildings/Controls/OBC/ASHRAE/G36/TerminalUnits/Reheat/Subsequences/Overrides.mo"
        }
        _ => unreachable!("unknown G36 sequence {sequence}"),
    }
}

/// Return the review status of the fixture backing a registered sequence reference.
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
        | FREEZE_PROTECTION_FINE_STEP
        | THERMAL_ZONES_CONTROL_LOOPS
        | THERMAL_ZONES_ZONE_STATES
        | ASHRAE62_1_SETPOINTS
        | COOLING_ONLY_ACTIVE_AIR_FLOW
        | COOLING_ONLY_ALARMS
        | COOLING_ONLY_DAMPERS
        | COOLING_ONLY_SYSTEM_REQUESTS
        | REHEAT_OVERRIDES => "supported-runtime-sequence source-verified composite",
        TIME_SUPPRESSION | COOLING_ONLY_CONTROLLER | RELIEF_FAN_GROUP => {
            "supported-runtime-sequence HostTick-v1-profile composite"
        }
        SAT | ECON | VAV => "supported-fixture-only fixture-local composition",
        _ => unreachable!("unknown G36 sequence {sequence}"),
    }
}

/// `sequence_derivation` value for a reference re-derived from the pinned G36 sequence sources.
pub(crate) const DERIVATION_G36_SOURCE: &str = "g36-source-rederivation";

/// `sequence_derivation` value for a fixture-local composition of CDL elementary blocks.
///
/// Such a fixture borrows a pattern from a G36 sequence but substitutes fixture-local inputs and
/// logic, so its reference checks CDL block semantics over the fixture topology only. It is not
/// an answer key for any G36 sequence.
pub(crate) const DERIVATION_FIXTURE_LOCAL: &str = "fixture-local-composition";

/// Return how a registered sequence reference relates to the G36 sources.
///
/// The three fixture-only sequences use fixture-local inputs (for example `zone_temp` and
/// `cooling_setpoint` for the supply-air-temperature reset) that the G36 sequence `.mo` files do
/// not have; every other registered sequence re-derives the pinned G36 source graph.
///
/// # Panics
/// Panics when `sequence` is not one of the registered G36 scenarios.
pub(super) fn sequence_derivation(sequence: &str) -> &'static str {
    match sequence {
        SAT | ECON | VAV => DERIVATION_FIXTURE_LOCAL,
        _ => {
            // Exhaustiveness: every registered sequence has a source table entry.
            let _ = source_files(sequence);
            DERIVATION_G36_SOURCE
        }
    }
}

/// Divergence note shared by every reference built on `Generic/TrimAndRespond.mo` (GitHub issue
/// #354). A macro, not a `const`, so `concat!` can append it to a sequence-specific note.
macro_rules! trim_and_respond_short_trip_note {
    () => {
        "faithful to the pinned Generic/TrimAndRespond.mo graph, not its documentation: the \
         documentation says the setpoint is iniSet while the device is off and the reset starts \
         delTim after device start, but iniSet reaches the reset only through swi2 -> uniDel.u, \
         and the UnitDelay captures it only at a samplePeriod instant. A device-off window that \
         holds no sample instant never reseeds the UnitDelay, so after the re-armed TrueDelay the \
         reset resumes from the stale pre-trip value, not from iniSet. Not a G36-intent answer \
         key for short device trips"
    };
}

/// Return the note for a reference that faithfully reproduces pinned Buildings behaviour which
/// diverges from the stated G36 intent, or `None` when no divergence is recorded.
///
/// `None` is not a review verdict that the sequence matches G36; it means no divergence is
/// recorded here. The goldens stay faithful to the pinned `.mo` on purpose; the note warns
/// consumers not to treat them as G36-intent answer keys for the named behaviour.
///
/// # Panics
/// Panics when `sequence` is not one of the registered G36 scenarios.
pub(super) fn g36_intent_divergence(sequence: &str) -> Option<&'static str> {
    match sequence {
        ECONOMIZER_ENABLE => Some(
            "faithful to pinned Enable.mo, which combines the outdoor temperature and enthalpy \
             high-limit hysteresis outputs with Logical.Or (or2) into truFalHol -> and1 -> \
             andEnaDis, so the economizer is enabled only while outdoor air is above the \
             high-limit cutoff; the block documentation and G36 section 5.16.7 disable it above \
             the high limit. Not a G36-intent answer key for enable polarity",
        ),
        ECONOMIZER_CONTROLLER_SINGLE_DAMPER_RELIEF_DAMPER_FIXED_21 => Some(
            "embeds pinned Enable.mo (enaDis), whose Logical.Or high-limit wiring enables the \
             economizer only while outdoor air is above the cutoff, the reverse of the block \
             documentation and G36 section 5.16.7; outputs that depend on the enable state \
             follow the pinned wiring, not G36 intent",
        ),
        TRIM_AND_RESPOND_HAVE_HOL_FALSE => Some(trim_and_respond_short_trip_note!()),
        SUPPLY_FAN => Some(concat!(
            "embeds Generic/TrimAndRespond.mo as staPreSetRes (duct static pressure reset); the \
             pressure setpoint and the fan speed it drives are ",
            trim_and_respond_short_trip_note!(),
            ". supply_fan_status does not depend on the reset; it carries this note because the \
             label is per scenario"
        )),
        SUPPLY_TEMP => Some(concat!(
            "faithful to pinned SupplyTemperature.mo: cooDowMod k=3 and the 0<uOpeMod<3 reset \
             test, with OperationModes coolDown=2 and setUp=3, give cool-down the reset branch \
             and setup the minimum setpoint, the reverse of the .mo component comments and G36 \
             section 5.16.2. This golden drives operating mode 1 only, so it does not exercise \
             the inverted modes. Separately, and not from the .mo: SupplyTemperature.mo binds \
             the nested maxSupTemRes trim-and-respond parameters to the outer ones (final \
             delTim=delTim, ...), but the CXF fixture sets them as literal values; the literals \
             equal the outer values today, so the trace is unaffected. Separately, the nested \
             maxSupTemRes reset is ",
            trim_and_respond_short_trip_note!()
        )),
        _ => {
            let _ = source_files(sequence);
            None
        }
    }
}

/// Return the note for a reference signal whose values depend on the frame step, or `None` when
/// no such dependence is recorded.
///
/// A frame-rate-dependent signal is still computed without `oce-blocks`, but it follows a
/// fixed-step discretization the engine shares, so agreement checks the transcription of that
/// discretization rather than continuous-time `.mo` behaviour.
///
/// # Criterion
/// Every PI/PID integrator in these references uses forward Euler at the golden's frame step:
/// `xI += (k/Ti)*(e-(yU-y)/(k*Ni))*dt`. Applied to the integrator's own time constant `Ti/k`,
/// forward Euler has amplification factor `1 - k*dt/Ti`, which reaches -1 at `k*dt/Ti = 2`: from
/// there on the discrete recurrence no longer tracks the continuous PI (it overshoots by at least
/// the full error each frame, typically saturating between its limits every frame), so the
/// values depend on the frame step. A signal is labelled when an integrating PI with
/// `k*dt/Ti >= 2` reaches it. The labelled cases are 120 (freeze protection heating coil, the
/// single-damper economizer controller minimum-OA loop) and 2 (both return-fan scenarios).
/// Sub-threshold PI references still use the same recurrence and are treated as
/// frame-step-insensitive at their step: cooling_only_dampers 0.2, cooling_only_controller 0.1,
/// vav_single_zone 0.0125, thermal_zones_control_loops ~0.0067, multizone_vav_supply_fan
/// ~0.0017, multizone_vav_economizer_limits_common ~4.2e-4, multizone_vav_supply_signals
/// ~8.3e-5, and multizone_vav_freeze_protection_fine_step 0.125 (the FreezeProtection heating-coil
/// PI at a 1/16 s frame, the continuous-time counterpart of the labelled 60 s column, which
/// `oce-conformance` checks against the closed-form PI response). No scenario lies in `[0.2, 2)`. The conformance suite
/// `g36_reference_frame_rate_labels` recomputes these ratios from the fixtures and fails when the
/// labels and the criterion disagree.
///
/// # Panics
/// Panics when `sequence` is not one of the registered G36 scenarios.
pub(super) fn frame_rate_dependence(sequence: &str, signal: &str) -> Option<&'static str> {
    match (sequence, signal) {
        (FREEZE_PROTECTION, "heating_coil_command") => Some(
            "frame-rate dependent: the FreezeProtection.mo heating-coil PI controllers (k=1, \
             Ti=0.5 s) are integrated by forward Euler at the 60 s frame step, applied after \
             each frame's output: xI += (k/Ti)*(e-(yU-y)/(k*Ni))*dt with dt=60 s. Because \
             k*dt/Ti=120 the integrator saturates every frame and the stage-1 and stage-3 \
             outputs alternate between yMax=1 and yMin=0. The engine uses the same forward-Euler \
             recurrence, so this column checks that shared discretization, not an independent \
             continuous-time derivation of FreezeProtection.mo. The \
             multizone_vav_freeze_protection_fine_step scenario runs the same graph at a 1/16 s \
             frame (k*dt/Ti=0.125), where oce-conformance checks this column against the \
             closed-form continuous-time PI response",
        ),
        (
            ECONOMIZER_CONTROLLER_SINGLE_DAMPER_RELIEF_DAMPER_FIXED_21,
            "outdoor_damper_min_limit" | "outdoor_damper_command" | "return_damper_command",
        ) => Some(
            "frame-rate dependent: this signal is driven by the Controller.mo damLim minimum-OA \
             PI loop (k=1, Ti=TiMinOA=0.5 s), integrated by forward Euler at the 60 s frame \
             step, applied after each frame's output: xI += (k/Ti)*(e-(yU-y)/(k*Ni))*dt with \
             dt=60 s, reset to y_reset-k*e (y_reset=0) on a rising supply-fan status. Because k*dt/Ti=120 \
             the integrator saturates every frame and the loop output alternates between its \
             limits, which outdoor_damper_min_limit carries into the outdoor and return damper \
             commands. The engine uses the same forward-Euler recurrence, so this column checks \
             that shared discretization, not an independent continuous-time derivation of \
             Controller.mo",
        ),
        (RETURN_FAN_AIRFLOW, "return_fan_speed") => Some(
            "frame-rate dependent: the ReturnFanAirflowTracking.mo conP PI loop (k=1, Ti=0.5 s) \
             is integrated by forward Euler at the 1 s frame step, applied after each frame's \
             output: xI += (k/Ti)*(e-(yU-y)/(k*Ni))*dt with dt=1 s, so k*dt/Ti=2 and each \
             frame's integral step is twice the error. The values change with the frame step. \
             The engine uses the same forward-Euler recurrence, so this column checks that \
             shared discretization, not an independent continuous-time derivation of \
             ReturnFanAirflowTracking.mo",
        ),
        (
            RETURN_FAN_DIRECT_PRESSURE,
            "relief_damper_command" | "discharge_pressure_setpoint" | "return_fan_speed",
        ) => Some(
            "frame-rate dependent: this signal is a Line of the ReturnFanDirectPressure.mo conP \
             PI loop (k=1, Ti=0.5 s), integrated by forward Euler at the 1 s frame step, \
             applied after each frame's output: xI += (k/Ti)*(e-(yU-y)/(k*Ni))*dt with dt=1 s, \
             so k*dt/Ti=2 and each frame's integral step is twice the error. The values change \
             with the frame step. The engine uses the same forward-Euler recurrence, so this \
             column checks that shared discretization, not an independent continuous-time \
             derivation of ReturnFanDirectPressure.mo",
        ),
        _ => {
            let _ = source_files(sequence);
            None
        }
    }
}
