//! G36 MultiZone VAV SupplyFan sequence oracle.

use crate::oracle::{Golden, InputSeries, ValueKind};

use super::{
    b, clamp, initial_sample_time, input_i, input_r, r, sample_due, sample_index, sampler_output,
    sequence_golden, true_delay_output, unit_ticks, SUPPLY_FAN,
};

pub(super) fn goldens() -> Vec<Golden> {
    let time = unit_ticks(901);
    let operating_mode: Vec<i64> = time
        .iter()
        .map(|&t| match t as u32 {
            300..=359 => 4,
            _ => 1,
        })
        .collect();
    let duct_static_pressure = time
        .iter()
        .map(|&t| if t >= 720.0 { 80.0 } else { 120.0 })
        .collect::<Vec<_>>();
    let zone_pressure_reset_requests: Vec<i64> = time
        .iter()
        .map(|&t| {
            if t >= 840.0 {
                5
            } else if t >= 720.0 {
                3
            } else {
                0
            }
        })
        .collect();
    let (supply_fan_status, supply_fan_speed) = supply_fan_trace(
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
            supply_fan_status.iter().copied().map(b).collect(),
            "SupplyFan: default have_perZonRehBox=false; operating_mode=1 except warm-up mode 4 at t=300..359",
            "Pinned SupplyFan.mo default no-perimeter branch: y1SupFan is true only for operation modes 1, 2, or 3; mode 4 forces false",
            inputs.clone(),
        ),
        sequence_golden(
            SUPPLY_FAN,
            "supply_fan_speed",
            ValueKind::Real,
            time,
            supply_fan_speed.into_iter().map(r).collect(),
            "SupplyFan: duct_static_pressure=120 Pa until 720s then 80 Pa; pressure requests step 0 -> 3 at 720s -> 5 at 840s",
            "Pinned SupplyFan.mo default variant: nested TrimAndRespond pressure reset -> FirstOrderHold(samplePeriod=120s) -> PI PIDWithReset(k=0.1,Ti=60,yMin=0.1,yMax=1,y_reset=0.1); Reals.Switch emits zero when y1SupFan=false",
            inputs,
        ),
    ]
}

fn supply_fan_trace(
    time: &[f64],
    operating_mode: &[i64],
    duct_static_pressure: &[f64],
    zone_pressure_reset_requests: &[i64],
) -> (Vec<bool>, Vec<f64>) {
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

    let mut true_delay_timer = 0.0;
    let mut true_delay_prev_time: Option<f64> = None;
    let mut true_delay_prev_u = false;
    let mut true_delay_held = false;

    let mut sampler_held = 0.0;
    let mut sampler_t0 = 0.0;
    let mut sampler_last_index = -1;
    let mut sampler_initialized = false;

    let mut unit_delay_held = INI_SET;
    let mut unit_delay_t0 = 0.0;
    let mut unit_delay_last_index = -1;
    let mut unit_delay_initialized = false;

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

    for (((&t, &mode), &pressure), &requests) in time
        .iter()
        .zip(operating_mode)
        .zip(duct_static_pressure)
        .zip(zone_pressure_reset_requests)
    {
        let fan_on = matches!(mode, 1..=3);
        fan_status.push(fan_on);

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
        let response = RES_AMO.signum() * (RES_AMO.abs() * request_delta).min(MAX_RES.abs());
        let net_reset = if !tim {
            0.0
        } else if request_delta > 0.0 {
            TRI_AMO + response
        } else {
            TRI_AMO
        };
        let candidate = clamp(unit_delay_held + net_reset, MIN_SET, MAX_SET);
        let trim_respond = if fan_on { candidate } else { MAX_SET };
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

    (fan_status, fan_speed)
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

fn first_order_hold_output(
    t: f64,
    input: f64,
    period: f64,
    state: FirstOrderHoldState,
) -> f64 {
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
