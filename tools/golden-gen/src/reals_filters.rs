//! Tier-A recurrence oracles for `CDL.Reals.Derivative` and `CDL.Reals.LimitSlewRate`.
//!
//! The Derivative oracle derives the emit-before-update implicit/backward-Euler recurrence from
//! `_spec/03` R-DYN-1 and
//! the Buildings `Reals/Derivative.mo` equations at the pin: `T_nonZero = max(T, 100*eps)`,
//! `der(x) = (u-x)/T_nonZero`, `y = (k/T_nonZero)*(u-x)`, with the gain `k` and time constant `T`
//! as **RealInput connectors** (declaration order `k, T, u`) and the initial equation
//! `x = if |k| < eps then u else u - T*y_start/k` (raw `T`). It intentionally does not import or
//! call any `oce-*` code. The LimitSlewRate oracle independently derives the project's documented
//! discrete convention from R-DYN-1 plus Buildings `Reals/LimitSlewRate.mo`: implicit lag first,
//! then a post-filter increment clamp. Exact IEEE-754 comparison makes that expression
//! operation-for-operation coincident with the engine's documented recurrence; no engine source
//! is imported or called.
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
    if k.abs() < EPS {
        u
    } else {
        u - t * y_start / k
    }
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

/// Build the Derivative and LimitSlewRate recurrence goldens.
pub fn goldens() -> Vec<Golden> {
    vec![
        constant_gain_filter(),
        time_varying_gain_and_time_constant(),
        rising_saturation(),
        falling_saturation(),
        lag_non_uniform_grid(),
        disabled_passthrough(),
    ]
}

/// Derive LimitSlewRate outputs using implicit lag followed by the finite-domain increment clamp.
fn limit_slew_rate_outputs(
    time: &[f64],
    u: &[f64],
    rising: f64,
    falling: f64,
    td: f64,
    enable: bool,
) -> Vec<Sample> {
    assert_eq!(time.len(), u.len());
    let mut y = u[0];
    let mut previous_time: Option<f64> = None;
    let mut out = Vec::with_capacity(time.len());

    for idx in 0..time.len() {
        if let (Some(previous), true) = (previous_time, enable) {
            let dt = time[idx] - previous;
            let alpha = dt / td;
            let filtered = (y + alpha * u[idx]) / (1.0 + alpha);
            // All pinned stimuli are finite and cannot produce signed-zero or NaN clamp bounds.
            let dy = (filtered - y).max(falling * dt).min(rising * dt);
            y += dy;
        } else {
            y = u[idx];
        }
        out.push(r(y));
        previous_time = Some(time[idx]);
    }

    out
}

/// Rising clamp followed by release into the implicit lag.
fn rising_saturation() -> Golden {
    let time = vec![0.0, 1.0, 2.0, 3.0, 4.0];
    let u = [0.0, 10.0, 10.0, 3.0, 3.0];
    let y = limit_slew_rate_outputs(&time, &u, 1.0, -0.5, 0.2, true);
    Golden::new(
        "CDL.Reals.LimitSlewRate",
        "y",
        ValueKind::Real,
        time,
        y,
        "raisingSlewRate=1.0, fallingSlewRate=-0.5, Td=0.2, enable=true; u=[0,10,10,3,3]",
        "Buildings CDL.Reals.LimitSlewRate.mo plus the project-wide implicit Reals dynamics convention (implicit lag, then post-filter increment clamp); t=0 first-tick y=u; t=1,2 rising-clamped at +1.0*dt; t=3,4 unsaturated lag",
    )
    .with_scenario("rising_saturation")
    .with_inputs(vec![input_r("u", u)])
}

/// Falling clamp under asymmetric slew-rate bounds.
fn falling_saturation() -> Golden {
    let time = vec![0.0, 1.0, 2.0, 3.0, 4.0];
    let u = [5.0, -5.0, -5.0, -5.0, -5.0];
    let y = limit_slew_rate_outputs(&time, &u, 0.5, -1.0, 0.2, true);
    Golden::new(
        "CDL.Reals.LimitSlewRate",
        "y",
        ValueKind::Real,
        time,
        y,
        "raisingSlewRate=0.5, fallingSlewRate=-1.0, Td=0.2, enable=true; u=[5,-5,-5,-5,-5]",
        "Buildings CDL.Reals.LimitSlewRate.mo plus the project-wide implicit Reals dynamics convention (implicit lag, then post-filter increment clamp); t=0 first-tick y=u; t=1..4 falling-clamped at -1.0*dt",
    )
    .with_scenario("falling_saturation")
    .with_inputs(vec![input_r("u", u)])
}

/// Pure implicit lag on a non-uniform, non-dyadic time grid.
fn lag_non_uniform_grid() -> Golden {
    let time = vec![0.0, 0.1, 0.3, 0.7];
    let u = [0.2, 0.9, -0.4, 0.6];
    let y = limit_slew_rate_outputs(&time, &u, 100.0, -100.0, 0.5, true);
    Golden::new(
        "CDL.Reals.LimitSlewRate",
        "y",
        ValueKind::Real,
        time,
        y,
        "raisingSlewRate=100.0, fallingSlewRate=-100.0, Td=0.5, enable=true; non-uniform t=[0,0.1,0.3,0.7]; u=[0.2,0.9,-0.4,0.6]",
        "Buildings CDL.Reals.LimitSlewRate.mo plus the project-wide implicit Reals dynamics convention (implicit lag, then post-filter increment clamp); never clamped, so the varying-dt implicit lag determines every update",
    )
    .with_scenario("lag_non_uniform_grid")
    .with_inputs(vec![input_r("u", u)])
}

/// Disabled mode passthrough at every tick.
fn disabled_passthrough() -> Golden {
    let time = vec![0.0, 1.0, 2.0, 3.0];
    let u = [0.0, 5.0, -3.0, 7.0];
    let y = limit_slew_rate_outputs(&time, &u, 1.0, -1.0, 0.5, false);
    Golden::new(
        "CDL.Reals.LimitSlewRate",
        "y",
        ValueKind::Real,
        time,
        y,
        "raisingSlewRate=1.0, fallingSlewRate=-1.0, Td=0.5, enable=false; u=[0,5,-3,7]",
        "Buildings CDL.Reals.LimitSlewRate.mo plus the project-wide implicit Reals dynamics convention (implicit lag, then post-filter increment clamp); disabled mode gives y=u every tick",
    )
    .with_scenario("disabled_passthrough")
    .with_inputs(vec![input_r("u", u)])
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
