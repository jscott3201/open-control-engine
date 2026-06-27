//! Scalar `CDL.Reals` transcendental goldens.
//!
//! These references are derived from the pinned Buildings source equations
//! `y = Modelica.Math.<fn>(...)`, evaluated through the reviewed deterministic `libm` crate. This
//! crate does not depend on `oce-blocks`; it is an independent Tier-A oracle emitter.

use crate::oracle::{Golden, InputSeries, Sample, ValueKind};

/// Default HVAC tick cadence ticks (`_spec/01` §8): t = 0, 60, 120, ...
fn ticks(n: usize) -> Vec<f64> {
    (0..n).map(|k| (k as f64) * 60.0).collect()
}

/// Build scalar transcendental `CDL.Reals` goldens.
pub fn goldens() -> Vec<Golden> {
    let mut out = vec![
        unary(
            "CDL.Reals.Sin",
            &[
                f64::NEG_INFINITY,
                -0.0,
                0.0,
                0.5,
                1.25,
                f64::INFINITY,
                f64::NAN,
            ],
            libm::sin,
            "u=[-Inf,-0.0,0.0,0.5,1.25,+Inf,NaN]; signed zero, finite angles, non-finite edges",
            "y = Modelica.Math.sin(u), evaluated with deterministic libm 0.2.16",
        ),
        unary(
            "CDL.Reals.Cos",
            &[
                0.0,
                0.5,
                1.25,
                std::f64::consts::PI,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NAN,
            ],
            libm::cos,
            "u=[0.0,0.5,1.25,pi,+Inf,-Inf,NaN]; finite angles and non-finite edges",
            "y = Modelica.Math.cos(u), evaluated with deterministic libm 0.2.16",
        ),
        unary(
            "CDL.Reals.Tan",
            &[
                f64::NEG_INFINITY,
                -0.0,
                0.0,
                0.5,
                1.25,
                f64::INFINITY,
                f64::NAN,
            ],
            libm::tan,
            "u=[-Inf,-0.0,0.0,0.5,1.25,+Inf,NaN]; signed zero, finite angles, non-finite edges",
            "y = Modelica.Math.tan(u), evaluated with deterministic libm 0.2.16",
        ),
        unary(
            "CDL.Reals.Asin",
            &[
                f64::NEG_INFINITY,
                -1.0 - f64::EPSILON,
                -1.0,
                -0.0,
                0.0,
                1.0,
                1.0 + f64::EPSILON,
                f64::INFINITY,
                f64::NAN,
            ],
            libm::asin,
            "u=[-Inf,-1-eps,-1,-0.0,0.0,1,1+eps,+Inf,NaN]; domain boundaries, signed zero, violations",
            "y = Modelica.Math.asin(u), evaluated with deterministic libm 0.2.16",
        ),
        unary(
            "CDL.Reals.Acos",
            &[
                f64::NEG_INFINITY,
                -1.0 - f64::EPSILON,
                -1.0,
                0.0,
                1.0,
                1.0 + f64::EPSILON,
                f64::INFINITY,
                f64::NAN,
            ],
            libm::acos,
            "u=[-Inf,-1-eps,-1,0,1,1+eps,+Inf,NaN]; domain boundaries and violations",
            "y = Modelica.Math.acos(u), evaluated with deterministic libm 0.2.16",
        ),
        unary(
            "CDL.Reals.Atan",
            &[
                f64::NEG_INFINITY,
                -1.0,
                -0.0,
                0.0,
                1.0,
                f64::INFINITY,
                f64::NAN,
            ],
            libm::atan,
            "u=[-Inf,-1,-0.0,0.0,1,+Inf,NaN]; signed zero, asymptotes, NaN",
            "y = Modelica.Math.atan(u), evaluated with deterministic libm 0.2.16",
        ),
    ];

    {
        let u1 = [
            1.0_f64,
            1.0,
            -1.0,
            -1.0,
            0.0,
            -0.0,
            1.0,
            -1.0,
            0.0,
            -0.0,
            0.0,
            -0.0,
            0.0,
            -0.0,
            f64::NAN,
            1.0,
            f64::INFINITY,
            f64::NEG_INFINITY,
            1.0,
            1.0,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ];
        let u2 = [
            1.0_f64,
            -1.0,
            -1.0,
            1.0,
            1.0,
            1.0,
            0.0,
            0.0,
            -1.0,
            -1.0,
            0.0,
            0.0,
            -0.0,
            -0.0,
            1.0,
            f64::NAN,
            1.0,
            1.0,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::INFINITY,
        ];
        let y: Vec<Sample> = u1
            .iter()
            .zip(u2)
            .map(|(&a, b)| r(libm::atan2(a, b)))
            .collect();
        out.push(Golden::new(
            "CDL.Reals.Atan2",
            "y",
            ValueKind::Real,
            ticks(u1.len()),
            y,
            "u1=[1,1,-1,-1,0,-0,1,-1,0,-0,0,-0,0,-0,NaN,1,+Inf,-Inf,1,1,+Inf,-Inf], u2=[1,-1,-1,1,1,1,0,0,-1,-1,0,0,-0,-0,1,NaN,1,1,+Inf,-Inf,+Inf,+Inf]; interior quadrants, axes, signed zero, NaN, infinities",
            "y = Modelica.Math.atan2(u1, u2), evaluated with deterministic libm 0.2.16; Buildings docs forbid simultaneous zero, and Open Control Engine emits a runtime warning for those rows before returning the deterministic branch value",
        )
        .with_inputs(vec![input_r("u1", u1), input_r("u2", u2)])
        .with_provenance(
            "math_library",
            "libm 0.2.16, default-features=false, pure Rust deterministic math",
        ));
    }

    out.extend([
        unary(
            "CDL.Reals.Exp",
            &[
                f64::NEG_INFINITY,
                -1.0,
                0.0,
                1.0,
                710.0,
                f64::INFINITY,
                f64::NAN,
            ],
            libm::exp,
            "u=[-Inf,-1,0,1,710,+Inf,NaN]; underflow edge, anchors, overflow edge, non-finite edges",
            "y = Modelica.Math.exp(u), evaluated with deterministic libm 0.2.16",
        ),
        unary(
            "CDL.Reals.Log",
            &[
                f64::NEG_INFINITY,
                0.0,
                -0.0,
                -1.0,
                1.0,
                std::f64::consts::E,
                f64::INFINITY,
                f64::NAN,
            ],
            libm::log,
            "u=[-Inf,0,-0,-1,1,e,+Inf,NaN]; documented invalid domain and positive anchors",
            "y = Modelica.Math.log(u), evaluated with deterministic libm 0.2.16; Buildings docs require u > 0, and Open Control Engine warns for invalid runtime rows",
        ),
        unary(
            "CDL.Reals.Log10",
            &[
                f64::NEG_INFINITY,
                0.0,
                -0.0,
                -1.0,
                1.0,
                10.0,
                1000.0,
                f64::INFINITY,
                f64::NAN,
            ],
            libm::log10,
            "u=[-Inf,0,-0,-1,1,10,1000,+Inf,NaN]; documented invalid domain and decimal anchors",
            "y = Modelica.Math.log10(u), evaluated with deterministic libm 0.2.16; Buildings docs require u > 0, and Open Control Engine warns for invalid runtime rows",
        ),
    ]);

    out
}

fn unary(
    class_path: &'static str,
    u: &'static [f64],
    f: fn(f64) -> f64,
    input_desc: &'static str,
    rule_desc: &'static str,
) -> Golden {
    let y = u.iter().map(|&x| r(f(x))).collect();
    Golden::new(
        class_path,
        "y",
        ValueKind::Real,
        ticks(u.len()),
        y,
        input_desc,
        rule_desc,
    )
    .with_inputs(vec![input_r("u", u.iter().copied())])
    .with_provenance(
        "math_library",
        "libm 0.2.16, default-features=false, pure Rust deterministic math",
    )
}

fn r(x: f64) -> Sample {
    Sample::Real(x)
}

fn input_r(name: &'static str, values: impl IntoIterator<Item = f64>) -> InputSeries {
    InputSeries::new(name, ValueKind::Real, values.into_iter().map(r).collect())
}
