//! Independent Tier-A recurrence oracle for `CDL.Reals.IntegratorWithReset`.
//!
//! Derives the emit-before-update forward-Euler recurrence from `_spec/03` and the Buildings
//! `Reals/IntegratorWithReset.mo` equations: `der(y) = k * u` (gain `k` defaults to `1`) with
//! `when trigger then reinit(y, y_reset_in)` on a rising edge — the reset value is assigned
//! directly, never scaled by `k`. It intentionally does not import or call any `oce-*` code.
//!
//! The `gain` scenario is the oracle-diff golden for the 2026-07-06 closeout divergence fix:
//! the engine previously dropped `k` entirely, so any `k != 1` model mis-integrated silently.

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
    InputSeries::new(
        name,
        ValueKind::Boolean,
        values.into_iter().map(b).collect(),
    )
}

fn decimal_ticks(n: usize) -> Vec<f64> {
    (0..n).map(|idx| (idx as f64) * 0.1).collect()
}

/// Emit-before-update forward Euler: each tick emits the prior accumulator, then advances it by
/// `k * u * dt` (first `dt = 0`), with a rising `trigger` storing `y_reset_in` instead — visible
/// at the next emit; a held-high trigger integrates, it does not re-reset.
fn integrator_outputs(
    time: &[f64],
    u: &[f64],
    y_reset_in: &[f64],
    trigger: &[bool],
    k: f64,
    y_start: f64,
) -> Vec<Sample> {
    assert_eq!(time.len(), u.len());
    assert_eq!(time.len(), y_reset_in.len());
    assert_eq!(time.len(), trigger.len());

    let mut x = y_start;
    let mut prev_t: Option<f64> = None;
    let mut prev_trigger = false;
    let mut out = Vec::with_capacity(time.len());

    for idx in 0..time.len() {
        out.push(r(x));
        let dt = prev_t.map_or(0.0, |prev| time[idx] - prev);
        x = if trigger[idx] && !prev_trigger {
            y_reset_in[idx]
        } else {
            x + k * u[idx] * dt
        };
        prev_t = Some(time[idx]);
        prev_trigger = trigger[idx];
    }

    out
}

/// Build the IntegratorWithReset recurrence goldens.
pub fn goldens() -> Vec<Golden> {
    vec![default_gain_reset_edges(), gain_scales_integrand()]
}

fn default_gain_reset_edges() -> Golden {
    let time = decimal_ticks(8);
    let u = [2.0, 2.0, -1.0, 4.0, 0.5, 10.0, 0.0, 3.0];
    let y_reset_in = [0.0, 0.0, 0.0, 7.0, 11.0, 0.0, 0.0, -5.0];
    let trigger = [false, false, false, true, true, false, false, true];
    let y = integrator_outputs(&time, &u, &y_reset_in, &trigger, 1.0, 1.0);

    Golden::new(
        "CDL.Reals.IntegratorWithReset",
        "y",
        ValueKind::Real,
        time,
        y,
        "default k=1, y_start=1.0; dt=0.1 decimal grid (non-dyadic residue); rising trigger at row 3 stores 7.0 for the next emit, held-high row 4 integrates instead of re-resetting to 11.0, second rising trigger at row 7 stores -5.0",
        "Emit prior x; then x := y_reset_in on a rising trigger else x + k*u*dt (first dt=0); reinit is direct assignment, never scaled by k; Buildings Reals/IntegratorWithReset.mo der(y)=k*u + when trigger reinit(y, y_reset_in)",
    )
    .with_inputs(vec![
        input_r("u", u),
        input_r("y_reset_in", y_reset_in),
        input_b("trigger", trigger),
    ])
}

fn gain_scales_integrand() -> Golden {
    let time = decimal_ticks(6);
    let u = [2.0, 2.0, -1.0, -1.0, 4.0, 4.0];
    let y_reset_in = [7.0; 6];
    let trigger = [false, false, false, false, true, false];
    let y = integrator_outputs(&time, &u, &y_reset_in, &trigger, 2.5, 1.0);

    Golden::new(
        "CDL.Reals.IntegratorWithReset",
        "y",
        ValueKind::Real,
        time,
        y,
        "k=2.5, y_start=1.0; dt=0.1 decimal grid; the accumulator advances by k*u*dt (x0=1, +2.5*2*0.1 per early row), and the rising trigger at row 4 stores exactly 7.0 — NOT 2.5*7.0 — proving reinit is unscaled",
        "Emit prior x; then x := y_reset_in on a rising trigger else x + k*u*dt (first dt=0); oracle-diff for the 2026-07-06 closeout fix (engine previously dropped the upstream gain k entirely); Buildings Reals/IntegratorWithReset.mo der(y)=k*u",
    )
    .with_scenario("gain")
    .with_inputs(vec![
        input_r("u", u),
        input_r("y_reset_in", y_reset_in),
        input_b("trigger", trigger),
    ])
}
