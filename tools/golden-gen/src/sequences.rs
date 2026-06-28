//! G36 whole-sequence Tier-A references for the derivable sequence outputs.
//!
//! These references are hand-derived from the G36 fixture topology plus CDL / Buildings block
//! semantics. They intentionally do not depend on, import, or inspect any `oce-*` crate.

use crate::oracle::{Golden, InputSeries, Sample, ValueKind};

mod plant_requests;
mod economizer_enable;
mod outdoor_airflow_ahu;
mod outdoor_airflow_sumzone;
mod outdoor_airflow_title24_ahu;
mod outdoor_airflow_title24_sumzone;
mod relief_damper;
mod relief_fan;
mod return_fan_airflow_tracking;
mod return_fan_direct_pressure;
mod supply_fan;
mod supply_signals;
mod vav_single_zone;

const SAT: &str = "ahu_supply_air_temp_reset";
const ECON: &str = "ahu_economizer";
const VAV: &str = "vav_single_zone";
const SUPPLY_TEMP: &str = "multizone_vav_supply_temperature";
const SUPPLY_FAN: &str = "multizone_vav_supply_fan";
const SUPPLY_SIGNALS: &str = "multizone_vav_supply_signals";
const PLANT_REQUESTS: &str = "multizone_vav_plant_requests";
const OUTDOOR_AIRFLOW_AHU: &str = "multizone_vav_outdoor_airflow_ahu";
const OUTDOOR_AIRFLOW_SUMZONE: &str = "multizone_vav_outdoor_airflow_sumzone";
const OUTDOOR_AIRFLOW_TITLE24_AHU: &str = "multizone_vav_outdoor_airflow_title24_ahu";
const OUTDOOR_AIRFLOW_TITLE24_SUMZONE: &str =
    "multizone_vav_outdoor_airflow_title24_sumzone";
const RELIEF_DAMPER: &str = "multizone_vav_relief_damper";
const RELIEF_FAN: &str = "multizone_vav_relief_fan";
const RETURN_FAN_AIRFLOW: &str = "multizone_vav_return_fan_airflow_tracking";
const RETURN_FAN_DIRECT_PRESSURE: &str = "multizone_vav_return_fan_direct_pressure";
const ECONOMIZER_ENABLE: &str = "multizone_vav_economizer_enable";
const SOURCE_COMMIT: &str = "a131864e4c4df22ebcd52bb8da439de0087ac365";

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
    out.extend(plant_requests::goldens());
    out.extend(outdoor_airflow_ahu::goldens());
    out.extend(outdoor_airflow_sumzone::goldens());
    out.extend(outdoor_airflow_title24_ahu::goldens());
    out.extend(outdoor_airflow_title24_sumzone::goldens());
    out.extend(relief_damper::goldens());
    out.extend(relief_fan::goldens());
    out.extend(return_fan_airflow_tracking::goldens());
    out.extend(return_fan_direct_pressure::goldens());
    out.extend(economizer_enable::goldens());
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

fn source_files(sequence: &str) -> &'static str {
    match sequence {
        SAT => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/SupplyTemperature.mo",
        ECON => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Controller.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Enable.mo; Buildings/Controls/OBC/ASHRAE/G36/Generic/AirEconomizerHighLimits.mo",
        VAV => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/SingleZone/VAV/Controller.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/SingleZone/VAV/SetPoints/Supply.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/SingleZone/VAV/SetPoints/SupplyFan.mo",
        SUPPLY_TEMP => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/SupplyTemperature.mo; Buildings/Controls/OBC/ASHRAE/G36/Generic/TrimAndRespond.mo",
        SUPPLY_FAN => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/SupplyFan.mo; Buildings/Controls/OBC/ASHRAE/G36/Generic/TrimAndRespond.mo",
        SUPPLY_SIGNALS => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/SupplySignals.mo",
        PLANT_REQUESTS => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/PlantRequests.mo",
        OUTDOOR_AIRFLOW_AHU => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/package.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/package.order; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/ASHRAE62_1/package.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/ASHRAE62_1/package.order; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/ASHRAE62_1/AHU.mo",
        OUTDOOR_AIRFLOW_SUMZONE => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/package.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/package.order; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/ASHRAE62_1/package.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/ASHRAE62_1/package.order; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/ASHRAE62_1/SumZone.mo",
        OUTDOOR_AIRFLOW_TITLE24_AHU => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/package.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/package.order; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/Title24/package.order; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/Title24/AHU.mo",
        OUTDOOR_AIRFLOW_TITLE24_SUMZONE => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/package.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/package.order; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/Title24/package.order; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/Title24/SumZone.mo",
        RELIEF_DAMPER => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/ReliefDamper.mo",
        RELIEF_FAN => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/ReliefFan.mo",
        RETURN_FAN_AIRFLOW => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/ReturnFanAirflowTracking.mo",
        RETURN_FAN_DIRECT_PRESSURE => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/ReturnFanDirectPressure.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Controller.mo",
        ECONOMIZER_ENABLE => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Enable.mo",
        _ => unreachable!("unknown G36 sequence {sequence}"),
    }
}

fn fixture_status(sequence: &str) -> &'static str {
    match sequence {
        SUPPLY_TEMP
        | SUPPLY_FAN
        | SUPPLY_SIGNALS
        | PLANT_REQUESTS
        | OUTDOOR_AIRFLOW_AHU
        | OUTDOOR_AIRFLOW_SUMZONE
        | OUTDOOR_AIRFLOW_TITLE24_AHU
        | OUTDOOR_AIRFLOW_TITLE24_SUMZONE
        | RELIEF_DAMPER
        | RELIEF_FAN
        | RETURN_FAN_AIRFLOW
        | RETURN_FAN_DIRECT_PRESSURE
        | ECONOMIZER_ENABLE => {
            "supported-runtime-sequence source-verified composite"
        }
        SAT | ECON | VAV => "supported-fixture-only source-reviewed fragment",
        _ => unreachable!("unknown G36 sequence {sequence}"),
    }
}

fn unit_ticks(n: usize) -> Vec<f64> {
    (0..n).map(|tick| tick as f64).collect()
}

fn buildings_line(x1: f64, f1: f64, x2: f64, f2: f64, u: f64) -> f64 {
    let x_lim = clamp(u, x1, x2);
    let slope = (f2 - f1) / (x2 - x1);
    let intercept = f2 - slope * x2;
    intercept + slope * x_lim
}

fn clamp(value: f64, min: f64, max: f64) -> f64 {
    min.max(value.min(max))
}

fn hysteresis(u: &[f64], u_low: f64, u_high: f64, pre_y_start: bool) -> Vec<bool> {
    let mut previous = pre_y_start;
    let mut out = Vec::with_capacity(u.len());
    for &value in u {
        let next = if value > u_high {
            true
        } else if value < u_low {
            false
        } else {
            previous
        };
        out.push(next);
        previous = next;
    }
    out
}

fn true_delay(time: &[f64], u: &[bool], delay_time: f64) -> Vec<bool> {
    let mut entry_time = None;
    let mut previous_u = false;
    let mut out = Vec::with_capacity(time.len());
    for (&t, &input) in time.iter().zip(u) {
        if input && !previous_u {
            entry_time = Some(t);
        }
        out.push(input && entry_time.is_some_and(|entry| t - entry >= delay_time));
        if !input {
            entry_time = None;
        }
        previous_u = input;
    }
    out
}

fn latch(u: &[bool], clear: &[bool]) -> Vec<bool> {
    let mut previous_u = false;
    let mut previous_y = false;
    let mut out = Vec::with_capacity(u.len());
    for (&input, &clr) in u.iter().zip(clear) {
        let rising = input && !previous_u;
        let next = if clr {
            false
        } else if rising {
            true
        } else {
            previous_y
        };
        out.push(next);
        previous_y = next;
        previous_u = input;
    }
    out
}

fn less_hysteretic(u1: &[f64], u2: &[f64], h: f64, pre_y_start: bool) -> Vec<bool> {
    let mut previous = pre_y_start;
    let mut out = Vec::with_capacity(u1.len());
    for (&left, &right) in u1.iter().zip(u2) {
        let next = (!previous && left < right) || (previous && left < right + h);
        out.push(next);
        previous = next;
    }
    out
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

    let mut unit_delay_held = T_SUP_COO_MAX;
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
        let candidate = clamp(unit_delay_held + net_reset, T_SUP_COO_MIN, T_SUP_COO_MAX);
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
            unit_delay_held = trim_respond;
            unit_delay_initialized = true;
        } else {
            let (due, index) = sample_due(t, unit_delay_t0, SAMPLE_PERIOD, unit_delay_last_index);
            if due {
                unit_delay_last_index = index;
                unit_delay_held = trim_respond;
            }
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn true_delay_output(
    t: f64,
    u: bool,
    delay_time: f64,
    delay_on_init: bool,
    prev_time: Option<f64>,
    prev_u: bool,
    held: bool,
    timer: f64,
) -> (bool, f64) {
    if !u {
        return (false, 0.0);
    }
    let delay = delay_time.max(0.0);
    let Some(previous_time) = prev_time else {
        return if delay_on_init && delay > 0.0 {
            (false, 0.0)
        } else {
            (true, delay)
        };
    };
    if held {
        (true, delay)
    } else if !prev_u {
        (delay <= 0.0, 0.0)
    } else {
        let next_timer = timer + (t - previous_time).max(0.0);
        (next_timer >= delay, next_timer)
    }
}

fn sampler_output(
    t: f64,
    input: f64,
    period: f64,
    initialized: bool,
    t0: f64,
    last_index: i64,
    held: f64,
) -> f64 {
    if !initialized || sample_due(t, t0, period, last_index).0 {
        input
    } else {
        held
    }
}

fn initial_sample_time(t_start: f64, period: f64) -> f64 {
    buildings_round_six((t_start / period).floor() * period)
}

fn buildings_round_six(x: f64) -> f64 {
    const FACTOR: f64 = 1_000_000.0;
    if x > 0.0 {
        (x * FACTOR + 0.5).floor() / FACTOR
    } else {
        (x * FACTOR - 0.5).ceil() / FACTOR
    }
}

fn sample_index(t_now: f64, t0: f64, period: f64) -> i64 {
    ((t_now - t0) / period + 1e-9).floor() as i64
}

fn sample_due(t_now: f64, t0: f64, period: f64, last_index: i64) -> (bool, i64) {
    let index = sample_index(t_now, t0, period);
    (index > last_index, index)
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
