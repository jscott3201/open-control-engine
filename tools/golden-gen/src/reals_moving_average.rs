//! Tier-A window-integral oracle for `CDL.Reals.MovingAverage`.
//!
//! Buildings `Reals/MovingAverage.mo` (pin `a131864`): `der(mu) = u`, `muDel = delay(mu, delta)`,
//! `y = (mu - muDel)/delta` once `time >= tStart + delta`, else
//! `y = (mu - muDel)/(time - tStart + 1e-3)` with `muDel = mu(tStart) = 0`. The engine's documented
//! host-tick discretization holds each tick's input backward over the gap that ends at that tick
//! (`u_k` on `(t_{k-1}, t_k]`, nothing before `tStart`). This oracle integrates that reconstruction
//! piece by piece over `[t - delta, t]`; it never accumulates or interpolates `mu`, never imports or
//! calls `oce-*` code, and keeps no bounded history.
//!
//! Every scenario has far more ticks inside one window than the former 64-checkpoint ring held.
//! Times, gaps and inputs are chosen so every partial integral is exactly representable (integer
//! or power-of-two gaps, dyadic inputs, small magnitudes). The only rounding is then the final
//! division, which any correct implementation of the same discretization shares, so the engine is
//! compared bit-exactly.

use crate::oracle::{Golden, InputSeries, Sample, ValueKind};

fn input_r(values: &[f64]) -> InputSeries {
    InputSeries::new(
        "u",
        ValueKind::Real,
        values.iter().copied().map(Sample::Real).collect(),
    )
}

/// Exact mean of the held-backward input over the window ending at tick `k`, or the upstream
/// warm-up quotient before the first full window.
fn window_mean(time: &[f64], u: &[f64], delta: f64, k: usize) -> f64 {
    let t0 = time[0];
    let t = time[k];
    let full = t >= t0 + delta;
    let lo = if full { t - delta } else { t0 };
    let mut integral = 0.0;
    for j in 1..=k {
        let start = if time[j - 1] > lo { time[j - 1] } else { lo };
        if time[j] > start {
            integral += u[j] * (time[j] - start);
        }
    }
    if full {
        integral / delta
    } else {
        integral / (t - t0 + 1e-3)
    }
}

fn outputs(time: &[f64], u: &[f64], delta: f64) -> Vec<Sample> {
    assert_eq!(time.len(), u.len());
    (0..time.len())
        .map(|k| Sample::Real(window_mean(time, u, delta, k)))
        .collect()
}

/// Build the MovingAverage window goldens.
pub fn goldens() -> Vec<Golden> {
    vec![
        late_step_in_long_window(),
        one_second_ticks_over_several_windows(),
        irregular_power_of_two_gaps(),
    ]
}

/// delta = 600 s at 1 s ticks, u steps 0 -> 100 at t = 640, run to t = 700.
fn late_step_in_long_window() -> Golden {
    let time: Vec<f64> = (0..=700).map(f64::from).collect();
    let u: Vec<f64> = (0..=700)
        .map(|k| if k >= 640 { 100.0 } else { 0.0 })
        .collect();
    let y = outputs(&time, &u, 600.0);
    Golden::new(
        "CDL.Reals.MovingAverage",
        "y",
        ValueKind::Real,
        time,
        y,
        "delta=600; 1 s ticks t=0..700 (600 ticks per window); u=0 then 100 from t=640",
        "exact held-backward window integral: y(700) = 100*(700-639)/600 = 10.1666...; a window shrunk to 64 checkpoints reports 95.3125",
    )
    .with_scenario("late_step_in_long_window")
    .with_inputs(vec![input_r(&u)])
}

/// delta = 300 s (the G36 relief / return-fan window) at 1 s ticks over more than three windows.
fn one_second_ticks_over_several_windows() -> Golden {
    let n = 1000;
    let time: Vec<f64> = (0..n).map(f64::from).collect();
    let u: Vec<f64> = (0..n)
        .map(|k| f64::from((k * 37) % 23) - 11.0 + if k >= 450 { 40.0 } else { 0.0 })
        .collect();
    let y = outputs(&time, &u, 300.0);
    Golden::new(
        "CDL.Reals.MovingAverage",
        "y",
        ValueKind::Real,
        time,
        y,
        "delta=300; 1 s ticks t=0..999 (300 ticks per window); u=((37k mod 23) - 11) plus a +40 step from t=450",
        "exact held-backward window integral; integer inputs at integer times keep every partial sum exact, one rounding at the final division",
    )
    .with_scenario("one_second_ticks_over_several_windows")
    .with_inputs(vec![input_r(&u)])
}

/// delta = 200 s with gaps cycling through 0.25, 2, 0.5, 1 s, so window starts fall strictly
/// inside tick gaps and the engine must interpolate its accumulated integral there.
fn irregular_power_of_two_gaps() -> Golden {
    let gaps = [0.25, 2.0, 0.5, 1.0];
    let n = 700;
    let mut time = Vec::with_capacity(n);
    let mut t = 0.0;
    for k in 0..n {
        if k > 0 {
            t += gaps[k % gaps.len()];
        }
        time.push(t);
    }
    let u: Vec<f64> = (0..n)
        .map(|k| f64::from(i32::try_from((k * 53) % 41).unwrap() - 20) * 0.25)
        .collect();
    let y = outputs(&time, &u, 200.0);
    Golden::new(
        "CDL.Reals.MovingAverage",
        "y",
        ValueKind::Real,
        time,
        y,
        "delta=200; 700 ticks with gaps cycling 0.25/2/0.5/1 s (~213 ticks per window); u=0.25*(((53k mod 41)) - 20)",
        "exact held-backward window integral with window starts inside tick gaps; power-of-two gaps and quarter-step inputs keep every partial sum exact, one rounding at the final division",
    )
    .with_scenario("irregular_power_of_two_gaps")
    .with_inputs(vec![input_r(&u)])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn real(sample: &Sample) -> f64 {
        match sample {
            Sample::Real(x) => *x,
            other => panic!("expected Real, got {other:?}"),
        }
    }

    #[test]
    fn late_step_reports_the_full_window_mean() {
        let golden = late_step_in_long_window();
        assert_eq!(
            real(&golden.samples[700]).to_bits(),
            (6100.0f64 / 600.0).to_bits()
        );
        assert_eq!(real(&golden.samples[639]).to_bits(), 0.0f64.to_bits());
    }

    #[test]
    fn partial_integrals_are_exact_in_every_scenario() {
        // Re-sum each window in reverse order: an inexact partial sum would differ by order.
        for golden in goldens() {
            let u: Vec<f64> = golden.inputs[0].samples.iter().map(real).collect();
            let delta = if golden.scenario == Some("late_step_in_long_window") {
                600.0
            } else if golden.scenario == Some("one_second_ticks_over_several_windows") {
                300.0
            } else {
                200.0
            };
            for k in 0..golden.time.len() {
                let t = golden.time[k];
                let full = t >= golden.time[0] + delta;
                let lo = if full { t - delta } else { golden.time[0] };
                let mut reversed = 0.0;
                for j in (1..=k).rev() {
                    let start = golden.time[j - 1].max(lo);
                    if golden.time[j] > start {
                        reversed += u[j] * (golden.time[j] - start);
                    }
                }
                let denom = if full {
                    delta
                } else {
                    t - golden.time[0] + 1e-3
                };
                assert_eq!(
                    (reversed / denom).to_bits(),
                    real(&golden.samples[k]).to_bits(),
                    "{:?} k={k}",
                    golden.scenario
                );
            }
        }
    }

    #[test]
    fn every_scenario_holds_far_more_ticks_per_window_than_sixty_four() {
        for golden in goldens() {
            let last = *golden.time.last().unwrap();
            let window_ticks = golden.time.iter().filter(|&&t| t > last - 200.0).count();
            assert!(window_ticks > 150, "{:?}", golden.scenario);
        }
    }
}
