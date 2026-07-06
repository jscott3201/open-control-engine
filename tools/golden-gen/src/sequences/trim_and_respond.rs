//! G36 Generic.TrimAndRespond standalone oracle for the `have_hol=false` variant.

use crate::oracle::{Golden, InputSeries, ValueKind};

use super::{
    TRIM_AND_RESPOND_HAVE_HOL_FALSE, clamp, initial_sample_time, input_b, input_i, r, sample_due,
    sample_index, sampler_output, sequence_golden, true_delay_output,
};

pub(super) fn goldens() -> Vec<Golden> {
    let time = (0..=22).map(|tick| tick as f64 * 60.0).collect::<Vec<_>>();
    let request_count = time
        .iter()
        .map(|&t| {
            if t >= 840.0 && t < 1080.0 {
                6
            } else if t >= 720.0 && t < 840.0 {
                3
            } else {
                0
            }
        })
        .collect::<Vec<_>>();
    let device_status = time
        .iter()
        .map(|&t| !(1080.0..1260.0).contains(&t))
        .collect::<Vec<_>>();
    let setpoint = trim_and_respond_trace(&time, &request_count, &device_status);
    let inputs = trim_and_respond_inputs(&request_count, &device_status);

    vec![sequence_golden(
        TRIM_AND_RESPOND_HAVE_HOL_FALSE,
        "setpoint",
        ValueKind::Real,
        time,
        setpoint.into_iter().map(r).collect(),
        "TrimAndRespond have_hol=false: device proven on until 1080s, off 1080..1200s, on again at 1260s; requests step 0 -> 3 at 720s -> 6 at 840s -> 0 at 1080s",
        "Pinned TrimAndRespond.mo have_hol=false variant: TrueDelay(delTim+samplePeriod=720s), Sampler(samplePeriod=120s), UnitDelay(y_start=iniSet=10), triAmo=0.1, resAmo=-0.2, maxRes=-0.6, clamp 0..20; inactive hold path is specialized away",
        inputs,
    )]
}

fn trim_and_respond_trace(
    time: &[f64],
    request_count: &[i64],
    device_status: &[bool],
) -> Vec<f64> {
    const INI_SET: f64 = 10.0;
    const MIN_SET: f64 = 0.0;
    const MAX_SET: f64 = 20.0;
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
    // to y_start = iniSet.
    let mut unit_delay_y = INI_SET;
    let mut unit_delay_u_internal = INI_SET;
    let mut unit_delay_t0 = 0.0;
    let mut unit_delay_last_index = -1;
    let mut unit_delay_initialized = false;

    let mut out = Vec::with_capacity(time.len());
    for ((&t, &requests), &device_on) in time.iter().zip(request_count).zip(device_status) {
        let delay_elapsed = true_delay_output(
            t,
            device_on,
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
        let net_reset = if !delay_elapsed {
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
        let candidate = clamp(unit_delay_out + net_reset, MIN_SET, MAX_SET);
        let trim_respond = if device_on { candidate } else { INI_SET };
        out.push(trim_respond);

        let (next_true_delay, next_timer) = true_delay_output(
            t,
            device_on,
            DEL_TIM + SAMPLE_PERIOD,
            true,
            true_delay_prev_time,
            true_delay_prev_u,
            true_delay_held,
            true_delay_timer,
        );
        true_delay_timer = next_timer;
        true_delay_prev_time = Some(t);
        true_delay_prev_u = device_on;
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

fn trim_and_respond_inputs(request_count: &[i64], device_status: &[bool]) -> Vec<InputSeries> {
    vec![
        input_i("request_count", request_count.iter().copied()),
        input_b("device_status", device_status.iter().copied()),
    ]
}
