//! Per-tick recurrence helpers mirroring CDL elementary-block semantics, shared by the sequence oracles.

pub(crate) fn buildings_line(x1: f64, f1: f64, x2: f64, f2: f64, u: f64) -> f64 {
    let x_lim = clamp(u, x1, x2);
    let slope = (f2 - f1) / (x2 - x1);
    let intercept = f2 - slope * x2;
    intercept + slope * x_lim
}

pub(crate) fn clamp(value: f64, min: f64, max: f64) -> f64 {
    min.max(value.min(max))
}

pub(crate) fn hysteresis(u: &[f64], u_low: f64, u_high: f64, pre_y_start: bool) -> Vec<bool> {
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

pub(crate) fn true_delay(time: &[f64], u: &[bool], delay_time: f64) -> Vec<bool> {
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

pub(crate) fn latch(u: &[bool], clear: &[bool]) -> Vec<bool> {
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

pub(crate) fn less_hysteretic(u1: &[f64], u2: &[f64], h: f64, pre_y_start: bool) -> Vec<bool> {
    let mut previous = pre_y_start;
    let mut out = Vec::with_capacity(u1.len());
    for (&left, &right) in u1.iter().zip(u2) {
        let next = (!previous && left < right) || (previous && left < right + h);
        out.push(next);
        previous = next;
    }
    out
}

pub(crate) fn greater_hysteretic(u1: &[f64], u2: &[f64], h: f64, pre_y_start: bool) -> Vec<bool> {
    let mut previous = pre_y_start;
    let mut out = Vec::with_capacity(u1.len());
    for (&left, &right) in u1.iter().zip(u2) {
        let next = (!previous && left > right) || (previous && left > right - h);
        out.push(next);
        previous = next;
    }
    out
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn true_delay_output(
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

pub(crate) fn sampler_output(
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

pub(crate) fn initial_sample_time(t_start: f64, period: f64) -> f64 {
    buildings_round_six((t_start / period).floor() * period)
}

pub(crate) fn buildings_round_six(x: f64) -> f64 {
    const FACTOR: f64 = 1_000_000.0;
    if x > 0.0 {
        (x * FACTOR + 0.5).floor() / FACTOR
    } else {
        (x * FACTOR - 0.5).ceil() / FACTOR
    }
}

pub(crate) fn sample_index(t_now: f64, t0: f64, period: f64) -> i64 {
    ((t_now - t0) / period + 1e-9).floor() as i64
}

pub(crate) fn sample_due(t_now: f64, t0: f64, period: f64, last_index: i64) -> (bool, i64) {
    let index = sample_index(t_now, t0, period);
    (index > last_index, index)
}
