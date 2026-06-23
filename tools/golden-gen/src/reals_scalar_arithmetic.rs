//! Additional scalar arithmetic `CDL.Reals` goldens for exact, bit-portable math.
//!
//! These blocks are separate from the older `reals` module to keep per-file size under the repo
//! cap while preserving the same Tier-A closed-form oracle discipline.

use crate::oracle::{Golden, InputSeries, Sample, ValueKind};

/// Default HVAC tick cadence ticks (`_spec/01` §8): t = 0, 60, 120, ...
fn ticks(n: usize) -> Vec<f64> {
    (0..n).map(|k| (k as f64) * 60.0).collect()
}

/// Build the scalar arithmetic CDL.Reals goldens added after the original algebraic set.
pub fn goldens() -> Vec<Golden> {
    let mut out = Vec::new();

    // Sqrt: y = sqrt(u), with negative-domain NaN propagation.
    {
        let u = [0.0_f64, 4.0, 2.25, -1.0, f64::INFINITY];
        let y: Vec<Sample> = u.iter().map(|&x| r(x.sqrt())).collect();
        out.push(
            Golden::new(
                "CDL.Reals.Sqrt",
                "y",
                ValueKind::Real,
                ticks(5),
                y,
                "u=[0,4,2.25,-1,+Inf]; exact square roots plus negative-domain NaN",
                "y = sqrt(u) (IEEE-754 correctly-rounded sqrt); Buildings Reals/Sqrt.mo",
            )
            .with_inputs(vec![input_r("u", u)]),
        );
    }

    // Average: y = 0.5 * (u1 + u2). Average is intentionally sourced from the Buildings reference;
    // `_spec/03` lacks this row today.
    {
        let u1 = [2.0_f64, -2.0, f64::MAX, 1.0, -6.0];
        let u2 = [4.0_f64, -4.0, f64::MAX, f64::NAN, 2.0];
        let y: Vec<Sample> = u1
            .iter()
            .zip(u2)
            .map(|(&a, b)| r(0.5 * (a + b)))
            .collect();
        out.push(Golden::new(
            "CDL.Reals.Average",
            "y",
            ValueKind::Real,
            ticks(5),
            y,
            "u1=[2,-2,MAX,1,-6], u2=[4,-4,MAX,NaN,2]; basic, negatives, overflow, NaN",
            "y = 0.5 * (u1 + u2); Buildings Reals/Average.mo",
        )
        .with_inputs(vec![input_r("u1", u1), input_r("u2", u2)]));
    }

    // Modulo: Modelica floored mod, not Rust truncated remainder.
    {
        let u1 = [-5.0_f64, 5.0, -5.0, 0.0, 1.0];
        let u2 = [3.0_f64, -3.0, -3.0, 3.0, 0.0];
        let y: Vec<Sample> = u1
            .iter()
            .zip(u2)
            .map(|(&a, b)| r(a - (a / b).floor() * b))
            .collect();
        out.push(Golden::new(
            "CDL.Reals.Modulo",
            "y",
            ValueKind::Real,
            ticks(5),
            y,
            "u1=[-5,5,-5,0,1], u2=[3,-3,-3,3,0]; pins floored divisor-sign rule and zero divisor",
            "y = u1 - floor(u1/u2) * u2; Modelica mod() / CDL §7.7.2",
        )
        .with_inputs(vec![input_r("u1", u1), input_r("u2", u2)]));
    }

    // Round: Buildings sign-branched half-away-from-zero formula, n = 2.
    {
        let n = 2_i64;
        let u = [1.125_f64, -1.125, 0.0, 1.234, -1.234];
        let y: Vec<Sample> = u.iter().map(|&x| r(round_cdl(x, n))).collect();
        out.push(
            Golden::new(
                "CDL.Reals.Round",
                "y",
                ValueKind::Real,
                ticks(5),
                y,
                "param n=2; u=[1.125,-1.125,0,1.234,-1.234]; half boundaries and non-half decimals",
                "fac built by repeated *10.0; y = floor(u*fac+0.5)/fac if u>0, +0 if u==0, else ceil(u*fac-0.5)/fac; Buildings Reals/Round.mo",
            )
            .with_inputs(vec![input_r("u", u)]),
        );
    }

    out
}

fn r(x: f64) -> Sample {
    Sample::Real(x)
}

fn input_r(name: &'static str, values: impl IntoIterator<Item = f64>) -> InputSeries {
    InputSeries::new(name, ValueKind::Real, values.into_iter().map(r).collect())
}

fn decimal_factor(n: i64) -> f64 {
    let mut factor = 1.0_f64;
    let mut remaining = n.unsigned_abs();
    while remaining > 0 && factor.is_finite() {
        factor *= 10.0;
        remaining -= 1;
    }
    if n >= 0 { factor } else { 1.0 / factor }
}

fn round_cdl(u: f64, n: i64) -> f64 {
    let fac = decimal_factor(n);
    if u == 0.0 {
        0.0
    } else if u > 0.0 {
        (u * fac + 0.5).floor() / fac
    } else {
        (u * fac - 0.5).ceil() / fac
    }
}
