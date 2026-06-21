//! Independent Tier-A recurrence oracle for the default-PI `CDL.Reals.PID` controllers.
//!
//! This module derives the limited PI recurrence from `_spec/03` R-REALS-2 and the Buildings
//! `PID.mo` / `PIDWithReset.mo` block wiring. It intentionally does not import or call any
//! `oce-*` code. The emitted traces exercise non-dyadic forward-Euler integral accumulation,
//! limiter back-calculation anti-windup, and the local `PIDWithReset` trigger + `y_reset_in`
//! interface.

use crate::oracle::{Golden, InputSeries, Sample, ValueKind};

const K: f64 = 0.37;
const TI: f64 = 0.3;
const NI: f64 = 0.9;

/// Build the PID-family recurrence goldens.
pub fn goldens() -> Vec<Golden> {
    vec![pid_pi_recurrence(), pid_with_reset_pi_recurrence()]
}

fn r(x: f64) -> Sample {
    Sample::Real(x)
}

fn b(x: bool) -> Sample {
    Sample::Boolean(x)
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

fn decimal_ticks(n: usize) -> Vec<f64> {
    (0..n).map(|idx| (idx as f64) * 0.1).collect()
}

#[derive(Clone, Copy)]
struct PiConfig {
    k: f64,
    ti: f64,
    ni: f64,
    xi_start: f64,
    y_min: f64,
    y_max: f64,
}

struct ResetInputs<'a> {
    trigger: &'a [bool],
    y_reset_in: &'a [f64],
}

fn pi_outputs(
    time: &[f64],
    u_s: &[f64],
    u_m: &[f64],
    config: PiConfig,
    reset: Option<ResetInputs<'_>>,
) -> Vec<Sample> {
    assert_eq!(time.len(), u_s.len());
    assert_eq!(time.len(), u_m.len());
    if let Some(reset) = &reset {
        assert_eq!(time.len(), reset.trigger.len());
        assert_eq!(time.len(), reset.y_reset_in.len());
    }

    let mut y_i = config.xi_start;
    let mut prev_t: Option<f64> = None;
    let mut prev_trigger = false;
    let mut out = Vec::with_capacity(time.len());

    for idx in 0..time.len() {
        let e = (u_s[idx] - u_m[idx]) / 1.0;
        let y_p = config.k * e;
        let y_u = y_p + y_i;
        let y = y_u.max(config.y_min).min(config.y_max);
        out.push(r(y));

        let dt = prev_t.map_or(0.0, |t_prev| time[idx] - t_prev);
        let rising_reset = reset
            .as_ref()
            .is_some_and(|reset| reset.trigger[idx] && !prev_trigger);

        y_i = if rising_reset {
            let reset = reset.as_ref().expect("rising reset has reset inputs");
            reset.y_reset_in[idx] - y_p
        } else {
            let delta_y = (y_u - y) / (config.k * config.ni);
            let err_i = e - delta_y;
            y_i + ((config.k / config.ti) * err_i) * dt
        };

        prev_t = Some(time[idx]);
        if let Some(reset) = &reset {
            prev_trigger = reset.trigger[idx];
        }
    }

    out
}

fn pid_pi_recurrence() -> Golden {
    let time = decimal_ticks(10);
    let u_s = [0.14, 0.16, 0.61, 0.65, -0.58, -0.63, -1.30, 1.20, -0.019, 0.021];
    let u_m = [0.0; 10];
    let config = PiConfig {
        k: K,
        ti: TI,
        ni: NI,
        xi_start: 0.19,
        y_min: -0.36,
        y_max: 0.42,
    };
    let y = pi_outputs(&time, &u_s, &u_m, config, None);

    Golden::new(
        "CDL.Reals.PID",
        "y",
        ValueKind::Real,
        time,
        y,
        "default controllerType=PI; k=0.37, Ti=0.3, Ni=default 0.9, xi_start=0.19, yMin=-0.36, yMax=0.42; dt=0.1 decimal grid; u_m=0; u_s drives high-limit, interior, and cancellation rows",
        "At each event: e=(u_s-u_m), yP=k*e, yU=yP+xI, y=max(yMin,min(yU,yMax)); then xI := xI + (k/Ti)*(e-(yU-y)/(k*Ni))*dt with first dt=0. This is the forward-Euler PI branch plus Buildings limiter anti-windup.",
    )
    .with_scenario("pi_recurrence")
    .with_inputs(vec![input_r("u_s", u_s), input_r("u_m", u_m)])
    .with_provenance("fp_residue_note", fp_residue_note("PID row 6 uses e=-1.30 so the xI increment is about -0.1603333333333335 while xI is about 0.17323927754915414 before the add; the subtraction crosses near zero."))
}

fn pid_with_reset_pi_recurrence() -> Golden {
    let time = decimal_ticks(10);
    let u_s = [0.20, 0.22, 0.95, 0.18, 0.18, -1.38, 1.10, -0.40, -0.40, 0.025];
    let u_m = [0.0; 10];
    let trigger = [false, false, false, true, true, false, false, true, true, false];
    let y_reset_in = [0.0, 0.0, 0.0, -0.123, -0.123, 0.0, 0.0, 0.275, 0.275, 0.0];
    let config = PiConfig {
        k: K,
        ti: TI,
        ni: NI,
        xi_start: 0.11,
        y_min: -0.35,
        y_max: 0.45,
    };
    let y = pi_outputs(
        &time,
        &u_s,
        &u_m,
        config,
        Some(ResetInputs {
            trigger: &trigger,
            y_reset_in: &y_reset_in,
        }),
    );

    Golden::new(
        "CDL.Reals.PIDWithReset",
        "y",
        ValueKind::Real,
        time,
        y,
        "default controllerType=PI; k=0.37, Ti=0.3, Ni=default 0.9, xi_start=0.11, yMin=-0.35, yMax=0.45; trigger rises at t=0.3 and t=0.7; y_reset_in=-0.123 then 0.275; dt=0.1 decimal grid",
        "Same limited PI recurrence as PID, with a rising trigger applying the local reset interface during state update: xI := y_reset_in - yP. Rows t=0.4 and t=0.8 hold the same current P term as the reset row, so emitted y pins the reset target on the next tick.",
    )
    .with_scenario("pi_reset_recurrence")
    .with_inputs(vec![
        input_r("u_s", u_s),
        input_r("u_m", u_m),
        input_b("trigger", trigger),
        input_r("y_reset_in", y_reset_in),
    ])
    .with_provenance("fp_residue_note", fp_residue_note("PIDWithReset row 5 drives lower saturation after a non-dyadic reset state, so anti-windup contributes a non-dyadic correction before later reset-target rows."))
}

fn fp_residue_note(case_note: &str) -> String {
    format!(
        "The stimulus avoids the G36 dyadic PID cone: k=0.37, Ti=0.3, Ni=0.9, and dt=0.1 are all non-dyadic decimal literals in binary64, so k/Ti and the accumulated xI values carry rounding residue. {case_note}"
    )
}
