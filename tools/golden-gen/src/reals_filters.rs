//! Independent Tier-A recurrence oracle for `CDL.Reals.Derivative`.
//!
//! Derives the emit-before-update implicit/backward-Euler recurrence from `_spec/03` R-DYN-1 and
//! the Buildings `Reals/Derivative.mo` equations at the pin: `T_nonZero = max(T, 100*eps)`,
//! `der(x) = (u-x)/T_nonZero`, `y = (k/T_nonZero)*(u-x)`, with the gain `k` and time constant `T`
//! as **RealInput connectors** (declaration order `k, T, u`) and the initial equation
//! `x = if |k| < eps then u else u - T*y_start/k` (raw `T`). It intentionally does not import or
//! call any `oce-*` code.
//!
//! The `time_varying_gain_and_time_constant` scenario is the oracle-diff golden for the
//! 2026-07-06 closeout divergence fix: the engine previously froze `k`/`T` as build-time
//! parameters (arity 1), making dynamically-wired gains unrepresentable.

use crate::oracle::{Golden, InputSeries, Sample, ValueKind};

const EPS: f64 = 1e-15;
const MIN_PARAM: f64 = 100.0 * EPS;

fn r(x: f64) -> Sample {
    Sample::Real(x)
}

fn input_r(name: &'static str, values: impl IntoIterator<Item = f64>) -> InputSeries {
    InputSeries::new(name, ValueKind::Real, values.into_iter().map(r).collect())
}

fn t_non_zero(t: f64) -> f64 {
    // Raw max is oracle-safe only while no golden drives a NaN T: the ENGINE floors a NaN T
    // deterministically via its det_max helper, which raw f64::max does not guarantee for
    // signaling NaN on aarch64. Re-derive that branch independently (NaN drops to MIN_PARAM)
    // before adding any non-finite-T golden.
    t.max(MIN_PARAM)
}

fn x_for_start(u: f64, k: f64, t: f64, y_start: f64) -> f64 {
    if k.abs() < EPS { u } else { u - t * y_start / k }
}

/// Emit-before-update walk: each tick emits `y = (k/T_nonZero)*(u - x)` from the prior state
/// (seeded by the initial equation on the first tick), then advances `x` with the shared
/// implicit filter `x' = (x + (dt/T)*u)/(1 + dt/T)`.
fn derivative_outputs(
    time: &[f64],
    k: &[f64],
    t_const: &[f64],
    u: &[f64],
    y_start: f64,
) -> Vec<Sample> {
    assert_eq!(time.len(), k.len());
    assert_eq!(time.len(), t_const.len());
    assert_eq!(time.len(), u.len());

    let mut x = f64::NAN;
    let mut prev_t: Option<f64> = None;
    let mut out = Vec::with_capacity(time.len());

    for idx in 0..time.len() {
        let x_now = if prev_t.is_none() {
            x_for_start(u[idx], k[idx], t_const[idx], y_start)
        } else {
            x
        };
        let t_nz = t_non_zero(t_const[idx]);
        out.push(r((k[idx] / t_nz) * (u[idx] - x_now)));

        let dt = prev_t.map_or(0.0, |prev| time[idx] - prev);
        let alpha = dt / t_nz;
        x = (x_now + alpha * u[idx]) / (1.0 + alpha);
        prev_t = Some(time[idx]);
    }

    out
}

/// Build the Derivative recurrence goldens.
pub fn goldens() -> Vec<Golden> {
    vec![constant_gain_filter(), time_varying_gain_and_time_constant()]
}

fn constant_gain_filter() -> Golden {
    let time = vec![0.0, 0.5, 1.0, 1.5, 2.0];
    let k = [2.0; 5];
    let t_const = [1.0; 5];
    let u = [1.0, 2.0, 2.0, 0.5, 0.5];
    let y = derivative_outputs(&time, &k, &t_const, &u, 0.25);

    Golden::new(
        "CDL.Reals.Derivative",
        "y",
        ValueKind::Real,
        time,
        y,
        "constant k=2, T=1 wired as inputs; y_start=0.25; dt=0.5 grid; initial x = u - T*y_start/k = 0.875, then the implicit filter tracks the step down to u=0.5",
        "Emit y=(k/T_nonZero)*(u-x) from prior x (first tick: x = u - T*y_start/k, raw T); then x advances by the implicit filter x'=(u-x)/T_nonZero; T_nonZero=max(T,100*eps); Buildings Reals/Derivative.mo (k, T are RealInputs)",
    )
    .with_inputs(vec![
        input_r("k", k),
        input_r("T", t_const),
        input_r("u", u),
    ])
}

fn time_varying_gain_and_time_constant() -> Golden {
    let time = vec![0.0, 0.1, 0.2, 0.3, 0.4, 0.5];
    let k = [1.0, 1.0, 2.5, 2.5, 0.0, 1.0];
    let t_const = [0.5, 0.5, 0.5, 0.25, 0.25, 0.25];
    let u = [0.0, 1.0, 1.0, 1.0, 1.0, -1.0];
    let y = derivative_outputs(&time, &k, &t_const, &u, 0.0);

    Golden::new(
        "CDL.Reals.Derivative",
        "y",
        ValueKind::Real,
        time,
        y,
        "k steps 1 -> 2.5 -> 0 -> 1 and T steps 0.5 -> 0.25 mid-run while u steps 0 -> 1 -> -1; a same-tick k/T change scales y immediately (feedthrough) with no state re-initialization; k=0 row emits exactly 0",
        "Emit y=(k/T_nonZero)*(u-x) with LIVE k/T inputs per tick; state x advances by the implicit filter using the current T_nonZero; Buildings Reals/Derivative.mo (k, T are RealInputs, PIDWithAutotuning wiring)",
    )
    .with_scenario("time_varying_gain_and_time_constant")
    .with_inputs(vec![
        input_r("k", k),
        input_r("T", t_const),
        input_r("u", u),
    ])
}
