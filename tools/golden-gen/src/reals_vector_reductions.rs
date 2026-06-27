//! CDL.Reals vector-reduction goldens.
//!
//! The reference formulas are re-derived from the CDL source equations and Modelica array
//! reduction identities, independent of `oce-blocks`.

use crate::oracle::{Golden, InputSeries, Sample, ValueKind};

fn ticks(n: usize) -> Vec<f64> {
    (0..n).map(|k| (k as f64) * 60.0).collect()
}

fn r(x: f64) -> Sample {
    Sample::Real(x)
}

fn input_r(name: &'static str, values: impl IntoIterator<Item = f64>) -> InputSeries {
    InputSeries::new(name, ValueKind::Real, values.into_iter().map(r).collect())
}

/// Build CDL.Reals vector-reduction goldens.
pub fn goldens() -> Vec<Golden> {
    let mut out = Vec::new();

    // MultiSum: y = k * u in declaration order. The first row is intentionally non-associative:
    // ((0 + 0.1*1e17) + 1) + (-0.1*1e17) loses the middle unit in IEEE f64.
    {
        let k = [0.1_f64, 1.0, -0.1];
        let u1 = [1.0e17_f64, 1.25, -2.0, -0.0];
        let u2 = [1.0_f64, 4.0, 8.0, 5.0];
        let u3 = [1.0e17_f64, -2.0, -3.0, 0.0];
        let y: Vec<Sample> = u1
            .iter()
            .zip(u2)
            .zip(u3)
            .map(|((&a, b), c)| r(((0.0 + k[0] * a) + k[1] * b) + k[2] * c))
            .collect();
        out.push(
            Golden::new(
                "CDL.Reals.MultiSum",
                "y",
                ValueKind::Real,
                ticks(4),
                y,
                "params nin=3, k=[0.1,1,-0.1]; u rows include a non-associative cancellation probe",
                "y = sum_i k[i]*u[i] in declaration order; empty reduction identity is 0",
            )
            .with_inputs(vec![input_r("u1", u1), input_r("u2", u2), input_r("u3", u3)]),
        );
    }

    // MultiSum empty vector: source branches around k*u and emits 0.
    {
        out.push(
            Golden::new(
                "CDL.Reals.MultiSum",
                "y",
                ValueKind::Real,
                ticks(2),
                vec![r(0.0), r(0.0)],
                "param nin=0; no input connectors",
                "if size(u,1)==0 then y = 0",
            )
            .with_scenario("empty"),
        );
    }

    // MultiMin: y = min(u), using the Modelica reduction identity for the empty scenario below.
    {
        let u1 = [3.0_f64, -0.0, 6.5, -8.0];
        let u2 = [2.0_f64, 0.0, -4.25, -8.0];
        let u3 = [5.0_f64, 1.0, 9.0, -7.5];
        let y: Vec<Sample> = u1
            .iter()
            .zip(u2)
            .zip(u3)
            .map(|((&a, b), c)| r(a.min(b).min(c)))
            .collect();
        out.push(
            Golden::new(
                "CDL.Reals.MultiMin",
                "y",
                ValueKind::Real,
                ticks(4),
                y,
                "param nin=3; finite Real rows cover first/middle/tie selections and signed zero",
                "y = min(u) over u[1:nin]; empty reduction identity is +Inf",
            )
            .with_inputs(vec![input_r("u1", u1), input_r("u2", u2), input_r("u3", u3)]),
        );
    }

    {
        out.push(
            Golden::new(
                "CDL.Reals.MultiMin",
                "y",
                ValueKind::Real,
                ticks(2),
                vec![r(f64::INFINITY), r(f64::INFINITY)],
                "param nin=0; no input connectors",
                "Modelica min(empty Real vector) reduction identity is +Inf",
            )
            .with_scenario("empty"),
        );
    }

    // MultiMax: y = max(u), using the Modelica reduction identity for the empty scenario below.
    {
        let u1 = [3.0_f64, -0.0, 6.5, -8.0];
        let u2 = [2.0_f64, 0.0, -4.25, -8.0];
        let u3 = [5.0_f64, 1.0, 9.0, -7.5];
        let y: Vec<Sample> = u1
            .iter()
            .zip(u2)
            .zip(u3)
            .map(|((&a, b), c)| r(a.max(b).max(c)))
            .collect();
        out.push(
            Golden::new(
                "CDL.Reals.MultiMax",
                "y",
                ValueKind::Real,
                ticks(4),
                y,
                "param nin=3; finite Real rows cover last/middle/tie selections and signed zero",
                "y = max(u) over u[1:nin]; empty reduction identity is -Inf",
            )
            .with_inputs(vec![input_r("u1", u1), input_r("u2", u2), input_r("u3", u3)]),
        );
    }

    {
        out.push(
            Golden::new(
                "CDL.Reals.MultiMax",
                "y",
                ValueKind::Real,
                ticks(2),
                vec![r(f64::NEG_INFINITY), r(f64::NEG_INFINITY)],
                "param nin=0; no input connectors",
                "Modelica max(empty Real vector) reduction identity is -Inf",
            )
            .with_scenario("empty"),
        );
    }

    out
}
