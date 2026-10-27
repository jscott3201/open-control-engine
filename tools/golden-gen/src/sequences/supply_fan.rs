//! G36 MultiZone VAV SupplyFan sequence oracle.

use crate::oracle::{Golden, InputSeries, ValueKind};

use super::trim_and_respond::{TrimAndRespondChain, TrimAndRespondParameters};
use super::{
    SUPPLY_FAN, b, clamp, initial_sample_time, input_i, input_r, r, sample_due, sample_index,
    sequence_golden, unit_ticks,
};

/// Ticks in the scenario: 0..=3000 s at 1 s.
///
/// The fan restarts at 360 s, so with the source-default `delTim = 600 s` and
/// `samplePeriod = 120 s` the TrueDelay (`delTim + samplePeriod = 720 s`) releases the reset at
/// 1080 s. The run then covers eight active sample instants (1080..1920 s), enough for a capped
/// respond, a smaller respond, a trim at exactly `numIgnReq` requests, two trims with no
/// requests, and three uncapped responds. The fan then stops again for 1980..2099 s, a window
/// holding exactly one sample instant (2040 s), so `swi2` stages iniSet into the UnitDelay while
/// its output still holds 161 Pa. The restart at 2100 s lands between sample instants: `swi`
/// publishes iniSet through the re-armed TrueDelay, which releases at 2820 s; the reset then
/// resumes from iniSet with a respond and, after the requests clear at 2940 s, a trim.
const SCENARIO_TICKS: usize = 3001;

pub(super) fn goldens() -> Vec<Golden> {
    let time = unit_ticks(SCENARIO_TICKS);
    let operating_mode: Vec<i64> = time
        .iter()
        .map(|&t| match t as u32 {
            300..=359 | 1980..=2099 => 4,
            _ => 1,
        })
        .collect();
    let duct_static_pressure = time
        .iter()
        .map(|&t| {
            if t >= 1500.0 {
                140.0
            } else if t >= 720.0 {
                80.0
            } else {
                120.0
            }
        })
        .collect::<Vec<_>>();
    let zone_pressure_reset_requests: Vec<i64> = time
        .iter()
        .map(|&t| match t as u32 {
            0..=719 => 0,
            720..=839 => 3,
            840..=1139 => 5,
            1140..=1259 => 3,
            1260..=1379 => 2,
            1380..=1619 => 0,
            1620..=2939 => 4,
            _ => 0,
        })
        .collect();
    let trace = supply_fan_trace(
        &time,
        &operating_mode,
        &duct_static_pressure,
        &zone_pressure_reset_requests,
    );
    let inputs = supply_fan_inputs(
        &operating_mode,
        &duct_static_pressure,
        &zone_pressure_reset_requests,
    );

    vec![
        sequence_golden(
            SUPPLY_FAN,
            "supply_fan_status",
            ValueKind::Boolean,
            time.clone(),
            trace.fan_status.iter().copied().map(b).collect(),
            "SupplyFan: default have_perZonRehBox=false; operating_mode=1 except warm-up mode 4 at t=300..359 and t=1980..2099",
            "Pinned SupplyFan.mo default no-perimeter branch: y1SupFan is true only for operation modes 1, 2, or 3; mode 4 forces false",
            inputs.clone(),
        ),
        sequence_golden(
            SUPPLY_FAN,
            "supply_fan_speed",
            ValueKind::Real,
            time.clone(),
            trace.fan_speed.into_iter().map(r).collect(),
            "SupplyFan over 0..3000s: operating_mode 4 (fan off) at 300..359s and 1980..2099s; duct_static_pressure=120 Pa until 720s, 80 Pa until 1500s, then 140 Pa; pressure requests 0 -> 3 at 720s -> 5 at 840s -> 3 at 1140s -> 2 at 1260s -> 0 at 1380s -> 4 at 1620s -> 0 at 2940s",
            "Pinned SupplyFan.mo default variant: nested TrimAndRespond pressure reset -> FirstOrderHold(samplePeriod=120s) -> PI PIDWithReset(k=0.1,Ti=60,yMin=0.1,yMax=1,y_reset=0.1); Reals.Switch emits zero when y1SupFan=false",
            inputs.clone(),
        ),
        sequence_golden(
            SUPPLY_FAN,
            "duct_static_pressure_setpoint",
            ValueKind::Real,
            time,
            trace.setpoint.into_iter().map(r).collect(),
            "SupplyFan pressure reset with source-default delTim=600s, samplePeriod=120s: the 360s fan restart releases the TrueDelay at 1080s; requests sampled at 1080..1920s are 5, 3, 2, 0, 0, 4, 4, 4, so the setpoint responds +20 (maxRes-capped) to 140 Pa, +3 to 143, trims at exactly numIgnReq to 131, trims to 119 and 107, then responds +18 to 125, 143 and 161; the fan stop at 1980..2099s spans the 2040s sample instant, so the setpoint is iniSet=120 Pa from 1980s through the re-armed TrueDelay (held 161 Pa at the 2100s restart) and the reset resumes from iniSet at 2820s: +18 to 138 (2820s and 2880s), then a trim to 126 at 3000s",
            "Pinned SupplyFan.mo default variant, nested Generic/TrimAndRespond.mo have_hol=false staPreSetRes.y: iniSet=120 Pa until TrueDelay(delTim+samplePeriod) elapses, then UnitDelay(y_start=iniSet) + net reset (triAmo=-12, resAmo=15, maxRes=32, numIgnReq=2) clamped to minSet=25..maxSet=410 Pa every samplePeriod; swi2 feeds iniSet into the UnitDelay while the fan is off",
            inputs,
        ),
    ]
}

/// Per-tick outputs of the SupplyFan oracle.
struct SupplyFanTrace {
    /// `y1SupFan`: supply fan commanded on.
    fan_status: Vec<bool>,
    /// `ySupFan`: commanded speed, normalized 0..1.
    fan_speed: Vec<f64>,
    /// `staPreSetRes.y`: trim-and-respond duct static pressure setpoint, Pa, before the
    /// FirstOrderHold.
    setpoint: Vec<f64>,
}

fn supply_fan_trace(
    time: &[f64],
    operating_mode: &[i64],
    duct_static_pressure: &[f64],
    zone_pressure_reset_requests: &[i64],
) -> SupplyFanTrace {
    const INI_SET: f64 = 120.0;
    const MIN_SET: f64 = 25.0;
    const MAX_SET: f64 = 410.0;
    const DEL_TIM: f64 = 600.0;
    const SAMPLE_PERIOD: f64 = 120.0;
    const NUM_IGN_REQ: f64 = 2.0;
    const TRI_AMO: f64 = -12.0;
    const RES_AMO: f64 = 15.0;
    const MAX_RES: f64 = 32.0;
    const K: f64 = 0.1;
    const TI: f64 = 60.0;
    const MIN_SPE: f64 = 0.1;
    const MAX_SPE: f64 = 1.0;
    const INI_SPE: f64 = 0.1;

    let mut pressure_reset = TrimAndRespondChain::new(TrimAndRespondParameters {
        ini_set: INI_SET,
        min_set: MIN_SET,
        max_set: MAX_SET,
        del_tim: DEL_TIM,
        sample_period: SAMPLE_PERIOD,
        num_ign_req: NUM_IGN_REQ,
        tri_amo: TRI_AMO,
        res_amo: RES_AMO,
        max_res: MAX_RES,
    });

    let mut hold_initialized = false;
    let mut hold_t0 = 0.0;
    let mut hold_last_index = -1;
    let mut hold_t_sample = 0.0;
    let mut hold_u_sample = 0.0;
    let mut hold_pre_u_sample = 0.0;
    let mut hold_slope = 0.0;

    let mut pid_i = 0.0;
    let mut pid_prev_time: Option<f64> = None;
    let mut pid_prev_trigger = false;

    let mut fan_status = Vec::with_capacity(time.len());
    let mut fan_speed = Vec::with_capacity(time.len());
    let mut setpoint = Vec::with_capacity(time.len());

    for (((&t, &mode), &pressure), &requests) in time
        .iter()
        .zip(operating_mode)
        .zip(duct_static_pressure)
        .zip(zone_pressure_reset_requests)
    {
        let fan_on = matches!(mode, 1..=3);
        fan_status.push(fan_on);

        // `staPreSetRes`: the shared TrimAndRespond chain (`swi` and `swi2` both modeled).
        let trim_respond = pressure_reset.step(t, fan_on, requests);
        setpoint.push(trim_respond);
        let held_setpoint = first_order_hold_output(
            t,
            trim_respond,
            SAMPLE_PERIOD,
            FirstOrderHoldState {
                initialized: hold_initialized,
                t0: hold_t0,
                last_index: hold_last_index,
                t_sample: hold_t_sample,
                u_sample: hold_u_sample,
                pre_u_sample: hold_pre_u_sample,
                slope: hold_slope,
            },
        );
        let normalized_setpoint = held_setpoint / MAX_SET;
        let normalized_measurement = pressure / MAX_SET;
        let pid_output = pid_with_reset_pi_output(
            normalized_setpoint,
            normalized_measurement,
            K,
            MIN_SPE,
            MAX_SPE,
            pid_i,
        );
        fan_speed.push(if fan_on { pid_output } else { 0.0 });

        let next_hold = first_order_hold_update(
            t,
            trim_respond,
            SAMPLE_PERIOD,
            FirstOrderHoldState {
                initialized: hold_initialized,
                t0: hold_t0,
                last_index: hold_last_index,
                t_sample: hold_t_sample,
                u_sample: hold_u_sample,
                pre_u_sample: hold_pre_u_sample,
                slope: hold_slope,
            },
        );
        hold_initialized = next_hold.initialized;
        hold_t0 = next_hold.t0;
        hold_last_index = next_hold.last_index;
        hold_t_sample = next_hold.t_sample;
        hold_u_sample = next_hold.u_sample;
        hold_pre_u_sample = next_hold.pre_u_sample;
        hold_slope = next_hold.slope;

        let dt = pid_prev_time.map_or(0.0, |previous| (t - previous).max(0.0));
        let rising_reset = fan_on && !pid_prev_trigger;
        let error = normalized_setpoint - normalized_measurement;
        let proportional = K * error;
        pid_i = if rising_reset {
            INI_SPE - proportional
        } else {
            let unlimited = proportional + pid_i;
            let limited = clamp(unlimited, MIN_SPE, MAX_SPE);
            let anti_windup_gain = (unlimited - limited) / (K * 0.9);
            let corrected_error = error - anti_windup_gain;
            pid_i + (K / TI) * corrected_error * dt
        };
        pid_prev_time = Some(t);
        pid_prev_trigger = fan_on;
    }

    SupplyFanTrace {
        fan_status,
        fan_speed,
        setpoint,
    }
}

#[derive(Clone, Copy)]
struct FirstOrderHoldState {
    initialized: bool,
    t0: f64,
    last_index: i64,
    t_sample: f64,
    u_sample: f64,
    pre_u_sample: f64,
    slope: f64,
}

fn first_order_hold_output(t: f64, input: f64, period: f64, state: FirstOrderHoldState) -> f64 {
    if !state.initialized {
        return input;
    }
    if sample_due(t, state.t0, period, state.last_index).0 {
        state.u_sample
    } else {
        state.pre_u_sample + state.slope * (t - state.t_sample)
    }
}

fn first_order_hold_update(
    t: f64,
    input: f64,
    period: f64,
    state: FirstOrderHoldState,
) -> FirstOrderHoldState {
    if !state.initialized {
        let t0 = initial_sample_time(t, period);
        return FirstOrderHoldState {
            initialized: true,
            t0,
            last_index: sample_index(t, t0, period),
            t_sample: t0,
            u_sample: input,
            pre_u_sample: input,
            slope: 0.0,
        };
    }
    let (due, index) = sample_due(t, state.t0, period, state.last_index);
    if !due {
        return state;
    }
    let first_trigger = t <= state.t0 + period / 2.0;
    FirstOrderHoldState {
        initialized: true,
        t0: state.t0,
        last_index: index,
        t_sample: t,
        u_sample: input,
        pre_u_sample: state.u_sample,
        slope: if first_trigger {
            0.0
        } else {
            (input - state.u_sample) / period
        },
    }
}

fn pid_with_reset_pi_output(
    setpoint: f64,
    measurement: f64,
    k: f64,
    y_min: f64,
    y_max: f64,
    integrator: f64,
) -> f64 {
    let error = setpoint - measurement;
    clamp(k * error + integrator, y_min, y_max)
}

fn supply_fan_inputs(
    operating_mode: &[i64],
    duct_static_pressure: &[f64],
    zone_pressure_reset_requests: &[i64],
) -> Vec<InputSeries> {
    vec![
        input_i("operating_mode", operating_mode.iter().copied()),
        input_r("duct_static_pressure", duct_static_pressure.iter().copied()),
        input_i(
            "zone_pressure_reset_requests",
            zone_pressure_reset_requests.iter().copied(),
        ),
    ]
}
