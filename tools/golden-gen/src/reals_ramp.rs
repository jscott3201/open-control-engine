//! Reference traces for base `CDL.Reals.Ramp`.
//!
//! These goldens use the project-wide discrete convention for continuous Reals dynamics: implicit
//! one-pole step, then slew-clamp the per-tick increment. They are derived from the source
//! `Ramp.mo` equations and intentionally keep this base block separate from `Sources.Ramp`.

use crate::oracle::{Golden, InputSeries, Sample, ValueKind};

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
    InputSeries::new(name, ValueKind::Boolean, values.into_iter().map(b).collect())
}

/// Build base Ramp goldens.
pub fn goldens() -> Vec<Golden> {
    vec![
        ramp_golden(
            "inactive_passthrough",
            RampOracle {
                raising_slew_rate: 2.0,
                falling_slew_rate: -2.0,
                td: 1.0,
            },
            vec![0.0, 1.0, 2.0, 3.0],
            &[1.0, 3.0, -9.0, -8.0],
            &[true, true, false, false],
            "scenario=inactive_passthrough; raisingSlewRate=2, fallingSlewRate=-2, Td=1; active=[true,true,false,false]",
            "while inactive, Ramp emits current u directly while the internal limited state holds",
        ),
        ramp_golden(
            "active_reinitialize",
            RampOracle {
                raising_slew_rate: 0.5,
                falling_slew_rate: -0.5,
                td: 1.0,
            },
            vec![0.0, 1.0, 2.0, 3.0],
            &[0.0, 10.0, 20.0, 20.0],
            &[false, true, true, true],
            "scenario=active_reinitialize; raisingSlewRate=0.5, fallingSlewRate=-0.5, Td=1; active rises at t=1",
            "active rising reinitializes y_internal to current u on that tick; held active ticks then slew-limit from the reset value",
        ),
        ramp_golden(
            "clamped_edges",
            RampOracle {
                raising_slew_rate: 2.0,
                falling_slew_rate: -3.0,
                td: 0.1,
            },
            vec![0.0, 1.0, 2.0],
            &[0.0, 10.0, -10.0],
            &[true, true, true],
            "scenario=clamped_edges; raisingSlewRate=2, fallingSlewRate=-3, Td=0.1; u=[0,10,-10]",
            "held-active output increments are clamped to raisingSlewRate*dt and fallingSlewRate*dt after the implicit lag step",
        ),
        ramp_golden(
            "implicit_residue",
            RampOracle {
                raising_slew_rate: 100.0,
                falling_slew_rate: -100.0,
                td: 1.0,
            },
            vec![0.0, 0.1, 0.2, 0.4],
            &[0.3, 0.5, 0.5, 0.1],
            &[true, true, true, true],
            "scenario=implicit_residue; raisingSlewRate=100, fallingSlewRate=-100, Td=1; non-dyadic decimal times/inputs",
            "large slew bounds leave the implicit one-pole recurrence visible, pinning f64 residue from non-dyadic dt and inputs",
        ),
    ]
}

#[derive(Clone, Copy)]
struct RampOracle {
    raising_slew_rate: f64,
    falling_slew_rate: f64,
    td: f64,
}

fn ramp_golden(
    scenario: &'static str,
    params: RampOracle,
    time: Vec<f64>,
    u: &[f64],
    active: &[bool],
    input_desc: &'static str,
    rule_desc: &'static str,
) -> Golden {
    let y = ramp_y(params, &time, u, active);
    Golden::new(
        "CDL.Reals.Ramp",
        "y",
        ValueKind::Real,
        time,
        y,
        input_desc,
        rule_desc,
    )
    .with_inputs(vec![
        input_r("u", u.iter().copied()),
        input_b("active", active.iter().copied()),
    ])
    .with_scenario(scenario)
}

fn ramp_y(params: RampOracle, time: &[f64], u: &[f64], active: &[bool]) -> Vec<Sample> {
    assert_eq!(time.len(), u.len());
    assert_eq!(time.len(), active.len());
    let mut initialized = false;
    let mut prev_t = 0.0;
    let mut prev_active = false;
    let mut y_internal = 0.0;
    let mut out = Vec::with_capacity(time.len());

    for ((&t, &u), &active) in time.iter().zip(u).zip(active) {
        let dt = if initialized { t - prev_t } else { 0.0 };
        let active_rising = active && (!initialized || !prev_active);
        if !initialized || active_rising {
            y_internal = u;
        } else if active {
            y_internal = ramp_limited_step(y_internal, u, params, dt);
        }

        out.push(r(if active { y_internal } else { u }));
        initialized = true;
        prev_t = t;
        prev_active = active;
    }

    out
}

fn ramp_limited_step(y: f64, u: f64, params: RampOracle, dt: f64) -> f64 {
    let alpha = dt / params.td;
    let filtered = (y + alpha * u) / (1.0 + alpha);
    y + clamp(
        filtered - y,
        params.falling_slew_rate * dt,
        params.raising_slew_rate * dt,
    )
}

fn clamp(value: f64, lower: f64, upper: f64) -> f64 {
    if value < lower {
        lower
    } else if value > upper {
        upper
    } else {
        value
    }
}
