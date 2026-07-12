//! G36 whole-sequence Tier-A references for the derivable sequence outputs.
//!
//! These references are hand-derived from the G36 fixture topology plus CDL / Buildings block
//! semantics. They intentionally do not depend on, import, or inspect any `oce-*` crate.

use crate::oracle::{Golden, InputSeries, Sample, ValueKind};

mod cdl_recurrences;
mod cooling_only_active_air_flow;
mod cooling_only_alarms;
mod cooling_only_dampers;
mod cooling_only_system_requests;
mod control_loops;
mod reheat_overrides;
mod freeze_protection;
mod air_economizer_high_limits;
mod economizer_limits_common;
mod economizer_controller;
mod economizer_modulations_reliefs;
mod economizer_modulations_return_fan;
mod plant_requests;
mod provenance;
mod economizer_enable;
mod outdoor_airflow_ahu;
mod outdoor_airflow_sumzone;
mod outdoor_airflow_title24_ahu;
mod outdoor_airflow_title24_sumzone;
mod relief_damper;
mod relief_fan;
mod relief_fan_group;
mod return_fan_airflow_tracking;
mod return_fan_direct_pressure;
mod supply_fan;
mod supply_signals;
mod time_suppression;
mod trim_and_respond;
mod vav_single_zone;
mod zone_states;

#[allow(unused_imports)]
pub(crate) use cdl_recurrences::{
    buildings_line, buildings_round_six, clamp, edge, greater_hysteretic, hysteresis,
    initial_sample_time, latch, less_hysteretic, pre, sample_due, sample_index, sampler_output,
    timer, triggered_sampler, true_delay, true_delay_on_init, true_delay_output, unit_delay,
};
use provenance::{SOURCE_COMMIT, fixture_status, source_files};

const SAT: &str = "ahu_supply_air_temp_reset";
const ECON: &str = "ahu_economizer";
const VAV: &str = "vav_single_zone";
const SUPPLY_TEMP: &str = "multizone_vav_supply_temperature";
const SUPPLY_FAN: &str = "multizone_vav_supply_fan";
const SUPPLY_SIGNALS: &str = "multizone_vav_supply_signals";
const TRIM_AND_RESPOND_HAVE_HOL_FALSE: &str = "trim_and_respond_have_hol_false";
const PLANT_REQUESTS: &str = "multizone_vav_plant_requests";
const OUTDOOR_AIRFLOW_AHU: &str = "multizone_vav_outdoor_airflow_ahu";
const OUTDOOR_AIRFLOW_SUMZONE: &str = "multizone_vav_outdoor_airflow_sumzone";
const OUTDOOR_AIRFLOW_TITLE24_AHU: &str = "multizone_vav_outdoor_airflow_title24_ahu";
const OUTDOOR_AIRFLOW_TITLE24_SUMZONE: &str =
    "multizone_vav_outdoor_airflow_title24_sumzone";
const RELIEF_DAMPER: &str = "multizone_vav_relief_damper";
const RELIEF_FAN: &str = "multizone_vav_relief_fan";
const RELIEF_FAN_GROUP: &str = "multizone_vav_relief_fan_group";
const RETURN_FAN_AIRFLOW: &str = "multizone_vav_return_fan_airflow_tracking";
const RETURN_FAN_DIRECT_PRESSURE: &str = "multizone_vav_return_fan_direct_pressure";
const ECONOMIZER_ENABLE: &str = "multizone_vav_economizer_enable";
const ECONOMIZER_LIMITS_COMMON: &str = "multizone_vav_economizer_limits_common";
const ECONOMIZER_CONTROLLER_SINGLE_DAMPER_RELIEF_DAMPER_FIXED_21: &str =
    "multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21";
const ECONOMIZER_MODULATIONS_RELIEFS: &str = "multizone_vav_economizer_modulations_reliefs";
const ECONOMIZER_MODULATIONS_RETURN_FAN: &str =
    "multizone_vav_economizer_modulations_return_fan";
const ECONOMIZER_MODULATIONS_RETURN_FAN_RELIEF_DAMPER: &str =
    "multizone_vav_economizer_modulations_return_fan_relief_damper";
const AIR_ECONOMIZER_HIGH_LIMITS_FIXED_24: &str =
    "generic_air_economizer_high_limits_ashrae_fixed_24";
const AIR_ECONOMIZER_HIGH_LIMITS_FIXED_21: &str =
    "generic_air_economizer_high_limits_ashrae_fixed_21";
const AIR_ECONOMIZER_HIGH_LIMITS_FIXED_18: &str =
    "generic_air_economizer_high_limits_ashrae_fixed_18";
const AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_24: &str =
    "generic_air_economizer_high_limits_title24_fixed_24";
const AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_23: &str =
    "generic_air_economizer_high_limits_title24_fixed_23";
const AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_22: &str =
    "generic_air_economizer_high_limits_title24_fixed_22";
const AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_21: &str =
    "generic_air_economizer_high_limits_title24_fixed_21";
const AIR_ECONOMIZER_HIGH_LIMITS_ASHRAE_DIFFERENTIAL: &str =
    "generic_air_economizer_high_limits_ashrae_differential";
const AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_0: &str =
    "generic_air_economizer_high_limits_title24_differential_offset_0";
const AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_1: &str =
    "generic_air_economizer_high_limits_title24_differential_offset_1";
const AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_2: &str =
    "generic_air_economizer_high_limits_title24_differential_offset_2";
const AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_3: &str =
    "generic_air_economizer_high_limits_title24_differential_offset_3";
const TIME_SUPPRESSION: &str = "generic_time_suppression";
const THERMAL_ZONES_ZONE_STATES: &str = "thermal_zones_zone_states";
const THERMAL_ZONES_CONTROL_LOOPS: &str = "thermal_zones_control_loops";
const FREEZE_PROTECTION: &str = "multizone_vav_freeze_protection";
const COOLING_ONLY_ACTIVE_AIR_FLOW: &str = "cooling_only_active_air_flow";
const COOLING_ONLY_ALARMS: &str = "cooling_only_alarms";
const COOLING_ONLY_DAMPERS: &str = "cooling_only_dampers";
const COOLING_ONLY_SYSTEM_REQUESTS: &str = "cooling_only_system_requests";
const REHEAT_OVERRIDES: &str = "reheat_overrides";

/// A generated provenance-only marker for deferred correctness-oracle coverage.
pub struct DeferredProvenance {
    /// Path under `tools/golden-gen/goldens`.
    pub relative_path: &'static str,
    /// JSON payload.
    pub contents: String,
    /// Manifest line describing the payload.
    pub manifest_line: &'static str,
}

/// Build all sequence-level G36 Tier-A goldens.
pub fn goldens() -> Vec<Golden> {
    let mut out = Vec::new();
    out.extend(sat_reset());
    out.extend(economizer());
    out.extend(vav_single_zone::goldens());
    out.extend(multizone_vav_supply_temperature());
    out.extend(supply_fan::goldens());
    out.extend(supply_signals::goldens());
    out.extend(trim_and_respond::goldens());
    out.extend(plant_requests::goldens());
    out.extend(outdoor_airflow_ahu::goldens());
    out.extend(outdoor_airflow_sumzone::goldens());
    out.extend(outdoor_airflow_title24_ahu::goldens());
    out.extend(outdoor_airflow_title24_sumzone::goldens());
    out.extend(relief_damper::goldens());
    out.extend(relief_fan::goldens());
    out.extend(relief_fan_group::goldens());
    out.extend(return_fan_airflow_tracking::goldens());
    out.extend(return_fan_direct_pressure::goldens());
    out.extend(economizer_enable::goldens());
    out.extend(economizer_limits_common::goldens());
    out.extend(economizer_controller::goldens());
    out.extend(economizer_modulations_reliefs::goldens());
    out.extend(economizer_modulations_return_fan::goldens());
    out.extend(air_economizer_high_limits::goldens());
    out.extend(time_suppression::goldens());
    out.extend(zone_states::goldens());
    out.extend(control_loops::goldens());
    out.extend(freeze_protection::goldens());
    out.extend(cooling_only_active_air_flow::goldens());
    out.extend(cooling_only_alarms::goldens());
    out.extend(cooling_only_dampers::goldens());
    out.extend(cooling_only_system_requests::goldens());
    out.extend(reheat_overrides::goldens());
    out
}

/// Provenance-only records for intentionally deferred sequence correctness coverage.
pub fn deferred_provenance(_generator_version: &str) -> Vec<DeferredProvenance> {
    Vec::new()
}

fn sat_reset() -> Vec<Golden> {
    let time = unit_ticks(5);
    let zone_temp = [22.0, 24.0, 24.5, 25.5, 25.5];
    let cooling_setpoint = [24.0; 5];

    let cooling_demand: Vec<f64> = zone_temp
        .iter()
        .zip(cooling_setpoint)
        .map(|(&zone, setpoint)| clamp(zone - setpoint, 0.0, 1.0))
        .collect();
    let sat_setpoint: Vec<f64> = cooling_demand
        .iter()
        .map(|&demand| buildings_line(0.0, 16.0, 1.0, 12.0, demand))
        .collect();
    let inputs = sat_inputs(&zone_temp, &cooling_setpoint);

    vec![
        sequence_golden(
            SAT,
            "sat_setpoint",
            ValueKind::Real,
            time.clone(),
            sat_setpoint.into_iter().map(r).collect(),
            "SAT reset: zone_temp=[22,24,24.5,25.5,25.5], cooling_setpoint=24; dyadic trace",
            "zoneMinusSet=zone_temp-cooling_setpoint; demandLimiter=max(0,min(diff,1)); \
             satLine uses Buildings Reals/Line.mo xLim, b=(f2-f1)/(x2-x1), a=f2-b*x2, y=a+b*xLim",
            inputs.clone(),
        ),
        sequence_golden(
            SAT,
            "cooling_demand",
            ValueKind::Real,
            time,
            cooling_demand.into_iter().map(r).collect(),
            "SAT reset: same inputs; output is demandLimiter.y",
            "cooling_demand=max(0,min(zone_temp-cooling_setpoint,1)); dyadic operands, no add-rounding coverage",
            inputs,
        ),
    ]
}

fn economizer() -> Vec<Golden> {
    let time = unit_ticks(6);
    let return_air_temp = [24.0; 6];
    let outdoor_air_temp = [23.0, 19.0, 19.0, 19.0, 24.0, 19.0];
    let operating_mode = [1, 1, 1, 1, 1, 0];

    let oa_temperature_delta: Vec<f64> = return_air_temp
        .iter()
        .zip(outdoor_air_temp)
        .map(|(&ret, outdoor)| ret - outdoor)
        .collect();
    let favorable = hysteresis(&oa_temperature_delta, 1.0, 3.0, false);
    let mode_allowed: Vec<bool> = operating_mode.iter().map(|&mode| mode > 0).collect();
    let candidate: Vec<bool> = favorable
        .iter()
        .zip(&mode_allowed)
        .map(|(&fav, &allowed)| fav && allowed)
        .collect();
    let delayed = true_delay(&time, &candidate, 1.0);
    let not_candidate: Vec<bool> = candidate.iter().map(|&value| !value).collect();
    let economizer_enabled = latch(&delayed, &not_candidate);
    let damper_command: Vec<f64> = economizer_enabled
        .iter()
        .map(|&enabled| if enabled { 1.0 } else { 0.2 })
        .collect();
    let operating_mode_real: Vec<f64> = operating_mode.iter().map(|&mode| mode as f64).collect();
    let inputs = economizer_inputs(&return_air_temp, &outdoor_air_temp, &operating_mode);

    vec![
        sequence_golden(
            ECON,
            "oa_temperature_delta",
            ValueKind::Real,
            time.clone(),
            oa_temperature_delta.into_iter().map(r).collect(),
            "Economizer: return_air_temp=24; outdoor_air_temp=[23,19,19,19,24,19]",
            "returnMinusOutdoor.y = return_air_temp - outdoor_air_temp; dyadic operands, no add-rounding coverage",
            inputs.clone(),
        ),
        sequence_golden(
            ECON,
            "operating_mode_real",
            ValueKind::Real,
            time.clone(),
            operating_mode_real.into_iter().map(r).collect(),
            "Economizer: operating_mode=[1,1,1,1,1,0]",
            "modeIdentity adds integer parameter p=0; modeToReal converts the integer value to Real",
            inputs.clone(),
        ),
        sequence_golden(
            ECON,
            "economizer_enabled",
            ValueKind::Boolean,
            time.clone(),
            economizer_enabled.iter().copied().map(b).collect(),
            "Economizer FSM: Hysteresis(uLow=1,uHigh=3) -> And(mode>0) -> TrueDelay(1s) -> Latch(clear=!candidate)",
            "Buildings semantics: Hysteresis holds inside [uLow,uHigh]; TrueDelay emits true after continuous true duration >= delayTime; Latch is clear-dominant and set by rising delayed input",
            inputs.clone(),
        ),
        sequence_golden(
            ECON,
            "damper_command",
            ValueKind::Real,
            time,
            damper_command.into_iter().map(r).collect(),
            "Economizer damperSwitch: u1=1.0, u2=economizer_enabled, u3=0.2",
            "Reals.Switch selects u1 when enabled else u3; selected constants are compared exactly",
            inputs,
        ),
    ]
}

fn multizone_vav_supply_temperature() -> Vec<Golden> {
    let time = unit_ticks(901);
    let outdoor_air_temperature = vec![289.15; time.len()];
    let supply_fan_status = vec![true; time.len()];
    let operating_mode = vec![1; time.len()];
    let zone_temperature_reset_requests: Vec<i64> = time
        .iter()
        .map(|&t| {
            if t >= 840.0 {
                6
            } else if t >= 720.0 {
                3
            } else {
                0
            }
        })
        .collect();
    let supply_air_temperature_setpoint = supply_temperature_setpoint_trace(
        &time,
        &outdoor_air_temperature,
        &supply_fan_status,
        &operating_mode,
        &zone_temperature_reset_requests,
    );
    let inputs = supply_temperature_inputs(
        &outdoor_air_temperature,
        &supply_fan_status,
        &operating_mode,
        &zone_temperature_reset_requests,
    );

    vec![sequence_golden(
        SUPPLY_TEMP,
        "supply_air_temperature_setpoint",
        ValueKind::Real,
        time,
        supply_air_temperature_setpoint.into_iter().map(r).collect(),
        "SupplyTemperature: TOut=TOut_min, fan proven on, literal operating mode 1; requests step 0 -> 3 at 720s -> 6 at 840s",
        "Pinned SupplyTemperature.mo mode/fan switches plus nested TrimAndRespond.mo have_hol=false default variant; UnitDelay uses samplePeriod=120s and TrueDelay uses delTim+samplePeriod=720s",
        inputs,
    )]
}

#[allow(clippy::too_many_arguments)]
fn sequence_golden(
    sequence: &'static str,
    signal: &'static str,
    kind: ValueKind,
    time: Vec<f64>,
    samples: Vec<Sample>,
    input_desc: &'static str,
    rule_desc: &'static str,
    inputs: Vec<InputSeries>,
) -> Golden {
    Golden::new("G36", signal, kind, time, samples, input_desc, rule_desc)
        .with_scenario(sequence)
        .with_inputs(inputs)
        .with_provenance("source_commit", SOURCE_COMMIT)
        .with_provenance("source_files", source_files(sequence))
        .with_provenance("fixture_status", fixture_status(sequence))
}

fn unit_ticks(n: usize) -> Vec<f64> {
    (0..n).map(|tick| tick as f64).collect()
}

fn supply_temperature_setpoint_trace(
    time: &[f64],
    outdoor_air_temperature: &[f64],
    supply_fan_status: &[bool],
    operating_mode: &[i64],
    zone_temperature_reset_requests: &[i64],
) -> Vec<f64> {
    const T_SUP_COO_MIN: f64 = 285.15;
    const T_SUP_COO_MAX: f64 = 291.15;
    const T_OUT_MIN: f64 = 289.15;
    const T_OUT_MAX: f64 = 294.15;
    const T_SUP_WAR_UP_SET_BAC: f64 = 308.15;
    const T_DEA_BAN: f64 = 299.15;
    const DEL_TIM: f64 = 600.0;
    const SAMPLE_PERIOD: f64 = 120.0;
    const NUM_IGN_REQ: f64 = 2.0;
    const TRI_AMO: f64 = 0.1;
    const RES_AMO: f64 = -0.2;
    const MAX_RES: f64 = -0.6;

    let mut true_delay_timer = 0.0;
    let mut true_delay_prev_time: Option<f64> = None;
    let mut true_delay_prev_u = false;
    let mut true_delay_held = false;

    let mut sampler_held = 0.0;
    let mut sampler_t0 = 0.0;
    let mut sampler_last_index = -1;
    let mut sampler_initialized = false;

    // Upstream Buildings `Discrete/UnitDelay.mo` pair: `y = pre(u_internal)` (held across the
    // interval) and the staged `u_internal` (sampled at the most recent instant). Both initialize
    // to y_start = T_SUP_COO_MAX (maxSupTemRes.uniDel iniSet).
    let mut unit_delay_y = T_SUP_COO_MAX;
    let mut unit_delay_u_internal = T_SUP_COO_MAX;
    let mut unit_delay_t0 = 0.0;
    let mut unit_delay_last_index = -1;
    let mut unit_delay_initialized = false;

    let mut out = Vec::with_capacity(time.len());
    for ((((&t, &t_out), &fan_on), &mode), &requests) in time
        .iter()
        .zip(outdoor_air_temperature)
        .zip(supply_fan_status)
        .zip(operating_mode)
        .zip(zone_temperature_reset_requests)
    {
        let tim = true_delay_output(
            t,
            fan_on,
            DEL_TIM + SAMPLE_PERIOD,
            true,
            true_delay_prev_time,
            true_delay_prev_u,
            true_delay_held,
            true_delay_timer,
        )
        .0;
        let sampled_requests = sampler_output(
            t,
            requests as f64,
            SAMPLE_PERIOD,
            sampler_initialized,
            sampler_t0,
            sampler_last_index,
            sampler_held,
        );
        let request_delta = sampled_requests - NUM_IGN_REQ;
        let response = -((RES_AMO.abs() * request_delta).min(MAX_RES.abs()));
        let net_reset = if !tim {
            0.0
        } else if request_delta > 0.0 {
            TRI_AMO + response
        } else {
            TRI_AMO
        };
        // UnitDelay output at this tick: at a sample instant the `when` fires before the output
        // is read (y becomes the previously staged sample); between instants y holds.
        let unit_delay_out = if !unit_delay_initialized {
            unit_delay_y
        } else {
            let (due, _) = sample_due(t, unit_delay_t0, SAMPLE_PERIOD, unit_delay_last_index);
            if due { unit_delay_u_internal } else { unit_delay_y }
        };
        let candidate = clamp(unit_delay_out + net_reset, T_SUP_COO_MIN, T_SUP_COO_MAX);
        let trim_respond = if fan_on { candidate } else { T_SUP_COO_MAX };
        let reset_branch = buildings_line(
            T_OUT_MIN,
            trim_respond,
            T_OUT_MAX,
            T_SUP_COO_MIN,
            t_out,
        );
        let selected = if !fan_on {
            T_DEA_BAN
        } else if mode > 0 && mode < 3 {
            reset_branch
        } else if mode == 3 {
            T_SUP_COO_MIN
        } else if mode > 3 && mode < 6 {
            T_SUP_WAR_UP_SET_BAC
        } else {
            T_DEA_BAN
        };
        out.push(selected);

        let (next_true_delay, next_timer) = true_delay_output(
            t,
            fan_on,
            DEL_TIM + SAMPLE_PERIOD,
            true,
            true_delay_prev_time,
            true_delay_prev_u,
            true_delay_held,
            true_delay_timer,
        );
        true_delay_timer = next_timer;
        true_delay_prev_time = Some(t);
        true_delay_prev_u = fan_on;
        true_delay_held = next_true_delay;

        if !sampler_initialized {
            sampler_t0 = initial_sample_time(t, SAMPLE_PERIOD);
            sampler_last_index = sample_index(t, sampler_t0, SAMPLE_PERIOD);
            sampler_held = requests as f64;
            sampler_initialized = true;
        } else {
            let (due, index) = sample_due(t, sampler_t0, SAMPLE_PERIOD, sampler_last_index);
            if due {
                sampler_last_index = index;
                sampler_held = requests as f64;
            }
        }

        if !unit_delay_initialized {
            unit_delay_t0 = initial_sample_time(t, SAMPLE_PERIOD);
            unit_delay_last_index = sample_index(t, unit_delay_t0, SAMPLE_PERIOD);
            // First instant: `y = pre(u_internal)` keeps y_start; the current input is staged.
            unit_delay_u_internal = trim_respond;
            unit_delay_initialized = true;
        } else {
            let (due, index) = sample_due(t, unit_delay_t0, SAMPLE_PERIOD, unit_delay_last_index);
            if due {
                unit_delay_last_index = index;
                unit_delay_y = unit_delay_u_internal; // y = pre(u_internal)
                unit_delay_u_internal = trim_respond; // u_internal = u
            }
        }
    }
    out
}

fn sat_inputs(zone_temp: &[f64], cooling_setpoint: &[f64]) -> Vec<InputSeries> {
    vec![
        input_r("zone_temp", zone_temp.iter().copied()),
        input_r("cooling_setpoint", cooling_setpoint.iter().copied()),
    ]
}

fn economizer_inputs(
    return_air_temp: &[f64],
    outdoor_air_temp: &[f64],
    operating_mode: &[i64],
) -> Vec<InputSeries> {
    vec![
        input_r("return_air_temp", return_air_temp.iter().copied()),
        input_r("outdoor_air_temp", outdoor_air_temp.iter().copied()),
        input_i("operating_mode", operating_mode.iter().copied()),
    ]
}

fn supply_temperature_inputs(
    outdoor_air_temperature: &[f64],
    supply_fan_status: &[bool],
    operating_mode: &[i64],
    zone_temperature_reset_requests: &[i64],
) -> Vec<InputSeries> {
    vec![
        input_r("outdoor_air_temperature", outdoor_air_temperature.iter().copied()),
        input_b("supply_fan_status", supply_fan_status.iter().copied()),
        input_i("operating_mode", operating_mode.iter().copied()),
        input_i(
            "zone_temperature_reset_requests",
            zone_temperature_reset_requests.iter().copied(),
        ),
    ]
}

fn input_r(name: &'static str, values: impl IntoIterator<Item = f64>) -> InputSeries {
    InputSeries::new(name, ValueKind::Real, values.into_iter().map(r).collect())
}

fn input_b(name: &'static str, values: impl IntoIterator<Item = bool>) -> InputSeries {
    InputSeries::new(
        name,
        ValueKind::Boolean,
        values.into_iter().map(b).collect(),
    )
}

fn input_i(name: &'static str, values: impl IntoIterator<Item = i64>) -> InputSeries {
    InputSeries::new(
        name,
        ValueKind::Integer,
        values.into_iter().map(i).collect(),
    )
}

fn r(x: f64) -> Sample {
    Sample::Real(x)
}

fn i(x: i64) -> Sample {
    Sample::Integer(x)
}

fn b(x: bool) -> Sample {
    Sample::Boolean(x)
}
