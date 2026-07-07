//! CDL.Reals exact-math algebraic [A] blocks (`_spec/03` §4.1; CDL §7.6/§7.7.2) plus the
//! Reals comparator family (Greater, Less, *Threshold, Hysteresis).
//!
//! Each rule is the closed-form spec math applied as a single IEEE-754 f64 operation; the f64
//! operation itself is the reference (round-to-nearest-even). Derived solely from the spec and
//! CDL semantics — never from `oce-blocks`.

use crate::oracle::{Golden, InputSeries, Sample, ValueKind};

/// Default HVAC tick cadence ticks (`_spec/01` §8): t = 0, 60, 120, ...
fn ticks(n: usize) -> Vec<f64> {
    (0..n).map(|k| (k as f64) * 60.0).collect()
}

/// Build all CDL.Reals goldens.
pub fn goldens() -> Vec<Golden> {
    let mut v = Vec::new();
    v.extend(algebraic());
    v.extend(comparators());
    v
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

fn algebraic() -> Vec<Golden> {
    let mut out = Vec::new();

    // Add: y = u1 + u2 (single IEEE add).
    {
        let u1 = [
            0.1,
            1.1,
            100000.1,
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ];
        let u2 = [0.2, 2.2, 0.3, 1.0, f64::NEG_INFINITY, f64::NEG_INFINITY];
        let y: Vec<Sample> = u1.iter().zip(u2).map(|(a, b)| r(a + b)).collect();
        out.push(Golden::new(
            "CDL.Reals.Add",
            "y",
            ValueKind::Real,
            ticks(6),
            y,
            "u1=[0.1,1.1,100000.1,+Inf,+Inf,-Inf], u2=[0.2,2.2,0.3,1,-Inf,-Inf]; finite round-off plus IEEE Inf/NaN rows",
            "y = u1 + u2 (IEEE-754 f64 add, round-to-nearest-even); _spec/03 §4.1 Add",
        )
        .with_inputs(vec![input_r("u1", u1), input_r("u2", u2)]));
    }

    // Subtract: y = u1 - u2.
    {
        let u1 = [
            0.3,
            1.0,
            2.2,
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ];
        let u2 = [0.1, 0.9, 1.1, 1.0, f64::INFINITY, f64::INFINITY];
        let y: Vec<Sample> = u1.iter().zip(u2).map(|(a, b)| r(a - b)).collect();
        out.push(
            Golden::new(
                "CDL.Reals.Subtract",
                "y",
                ValueKind::Real,
                ticks(6),
                y,
                "u1=[0.3,1.0,2.2,+Inf,+Inf,-Inf], u2=[0.1,0.9,1.1,1,+Inf,+Inf]; finite cancellation plus IEEE Inf/NaN rows",
                "y = u1 - u2 (IEEE-754 f64 subtract); _spec/03 §4.1 Subtract",
            )
            .with_inputs(vec![input_r("u1", u1), input_r("u2", u2)]),
        );
    }

    // Multiply: y = u1 * u2.
    {
        let u1 = [
            0.1,
            1.1,
            3.3,
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ];
        let u2 = [0.1, 1.1, 0.7, 2.0, 0.0, 2.0];
        let y: Vec<Sample> = u1.iter().zip(u2).map(|(a, b)| r(a * b)).collect();
        out.push(
            Golden::new(
                "CDL.Reals.Multiply",
                "y",
                ValueKind::Real,
                ticks(6),
                y,
                "u1=[0.1,1.1,3.3,+Inf,+Inf,-Inf], u2=[0.1,1.1,0.7,2,0,2]; finite products plus IEEE Inf/NaN rows",
                "y = u1 * u2 (IEEE-754 f64 multiply); _spec/03 §4.1 Multiply",
            )
            .with_inputs(vec![input_r("u1", u1), input_r("u2", u2)]),
        );
    }

    // Divide: y = u1 / u2, IEEE divide-by-zero edges, NO guard mutation.
    {
        let u1 = [1.0, 0.1, 10.0, -10.0, 0.0];
        let u2 = [3.0, 0.3, 0.0, 0.0, 0.0];
        let y: Vec<Sample> = u1.iter().zip(u2).map(|(a, b)| r(a / b)).collect();
        out.push(Golden::new(
            "CDL.Reals.Divide",
            "y",
            ValueKind::Real,
            ticks(5),
            y,
            "u1=[1,0.1,10,-10,0], u2=[3,0.3,0,0,0]; IEEE divide-by-zero edges (+Inf,-Inf,NaN)",
            "y = u1 / u2 (IEEE-754 f64 divide, no guard); _spec/03 §4.1 Divide. Inf/NaN by IEEE class",
        )
        .with_inputs(vec![input_r("u1", u1), input_r("u2", u2)]));
    }

    // MultiplyByParameter: y = k * u, k = 0.1.
    {
        let k = 0.1;
        let u = [7.0, 2.5, 11.0];
        let y: Vec<Sample> = u.iter().map(|&x| r(k * x)).collect();
        out.push(Golden::new(
            "CDL.Reals.MultiplyByParameter",
            "y",
            ValueKind::Real,
            ticks(3),
            y,
            "param k=0.1; u=[7.0,2.5,11.0]",
            "y = k * u (IEEE-754 f64 multiply); _spec/03 §4.1 MultiplyByParameter (canonical; Gain alias)",
        )
        .with_inputs(vec![input_r("u", u)]));
    }

    // AddParameter: y = u + p, p = 0.2.
    {
        let p = 0.2;
        let u = [0.1, 1.1, -2.2];
        let y: Vec<Sample> = u.iter().map(|&x| r(x + p)).collect();
        out.push(
            Golden::new(
                "CDL.Reals.AddParameter",
                "y",
                ValueKind::Real,
                ticks(3),
                y,
                "param p=0.2; u=[0.1,1.1,-2.2]",
                "y = u + p (IEEE-754 f64 add); _spec/03 §4.1 AddParameter",
            )
            .with_inputs(vec![input_r("u", u)]),
        );
    }

    // Abs: y = |u| (sign-bit clear; -0.0 -> +0.0).
    {
        let u = [-3.7_f64, 0.1, -0.0, -1.5e-300];
        let y: Vec<Sample> = u.iter().map(|&x| r(x.abs())).collect();
        out.push(
            Golden::new(
                "CDL.Reals.Abs",
                "y",
                ValueKind::Real,
                ticks(4),
                y,
                "u=[-3.7,0.1,-0.0,-1.5e-300]; negative, positive, signed zero, tiny",
                "y = |u| (IEEE magnitude, sign bit cleared); _spec/03 §4.1 Abs",
            )
            .with_inputs(vec![input_r("u", u)]),
        );
    }

    // Min: y = min(u1,u2) (verbatim operand).
    {
        let u1 = [0.1_f64, 3.3, -1.1, 2.7];
        let u2 = [0.2_f64, 3.3, 2.7, -1.1];
        let y: Vec<Sample> = u1.iter().zip(u2).map(|(&a, b)| r(a.min(b))).collect();
        out.push(
            Golden::new(
                "CDL.Reals.Min",
                "y",
                ValueKind::Real,
                ticks(4),
                y,
                "u1=[0.1,3.3,-1.1,2.7], u2=[0.2,3.3,2.7,-1.1]; covers both operand selections and tie",
                "y = min(u1,u2) (lesser operand verbatim); _spec/03 §4.1 Min / CDL §7.7.2",
            )
            .with_inputs(vec![input_r("u1", u1), input_r("u2", u2)]),
        );
    }

    // Max: y = max(u1,u2).
    {
        let u1 = [0.1_f64, 3.3, -1.1, 2.7];
        let u2 = [0.2_f64, 3.3, 2.7, -1.1];
        let y: Vec<Sample> = u1.iter().zip(u2).map(|(&a, b)| r(a.max(b))).collect();
        out.push(
            Golden::new(
                "CDL.Reals.Max",
                "y",
                ValueKind::Real,
                ticks(4),
                y,
                "u1=[0.1,3.3,-1.1,2.7], u2=[0.2,3.3,2.7,-1.1]; covers both operand selections and tie",
                "y = max(u1,u2) (greater operand verbatim); _spec/03 §4.1 Max / CDL §7.7.2",
            )
            .with_inputs(vec![input_r("u1", u1), input_r("u2", u2)]),
        );
    }

    // Limiter: the upstream Buildings `Reals/Limiter.mo` (pin a131864) equation is the
    // comparison chain `if u > uMax then uMax elseif u < uMin then uMin else u`, NOT a min/max
    // pair — the two disagree for NaN u (the chain passes NaN through; min/max absorb it into
    // a bound) and for signed-zero boundary equality.
    {
        fn limiter_chain(u: f64, u_min: f64, u_max: f64) -> f64 {
            if u > u_max {
                u_max
            } else if u < u_min {
                u_min
            } else {
                u
            }
        }

        let (u_min, u_max) = (0.0_f64, 5.5_f64);
        let u = [-1.3_f64, 2.2, 9.9];
        let y: Vec<Sample> = u.iter().map(|&x| r(limiter_chain(x, u_min, u_max))).collect();
        out.push(
            Golden::new(
                "CDL.Reals.Limiter",
                "y",
                ValueKind::Real,
                ticks(3),
                y,
                "params uMin=0.0, uMax=5.5; u=[-1.3,2.2,9.9]; below/in/above band",
                "y = if u > uMax then uMax elseif u < uMin then uMin else u; Buildings Reals/Limiter.mo comparison chain; _spec/03 §4.1 Limiter (the only legal saturation)",
            )
            .with_inputs(vec![input_r("u", u)]),
        );

        // Non-finite and boundary cells — the oracle-diff golden for the 2026-07-06 closeout
        // fix: the engine previously absorbed a NaN u into uMin (fault-hiding); upstream passes
        // it through. Also pins ±inf clamping and boundary-equal passthrough.
        let u = [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 5.5, 0.0, 2.2];
        let y: Vec<Sample> = u.iter().map(|&x| r(limiter_chain(x, u_min, u_max))).collect();
        out.push(
            Golden::new(
                "CDL.Reals.Limiter",
                "y",
                ValueKind::Real,
                ticks(6),
                y,
                "params uMin=0.0, uMax=5.5; u=[NaN,+inf,-inf,5.5,0.0,2.2]: NaN passes through fail-visible, infinities clamp to the bounds, boundary-equal u returns verbatim",
                "y = if u > uMax then uMax elseif u < uMin then uMin else u; NaN fails both comparisons and reaches y (upstream Reals/Limiter.mo); oracle-diff for the 2026-07-06 NaN-absorption divergence fix",
            )
            .with_scenario("non_finite_and_boundary")
            .with_inputs(vec![input_r("u", u)]),
        );
    }

    // Line: clamp u into [x1,x2], then linear interp through (x1,f1),(x2,f2).
    {
        // (x1,f1,x2,f2,u) per tick.
        let segs = [
            (1.0_f64, 10.0_f64, 4.0_f64, 20.0_f64, 2.5_f64),
            (1.0, 10.0, 4.0, 20.0, 0.5),
            (1.0, 10.0, 4.0, 20.0, 5.0),
            (0.3, 0.5, 2.7, 3.3, 1.1),
        ];
        // Pin to the Buildings Reals/Line.mo arrangement (b=slope; a=f2-b*x2; y=a+b*xLim) so newly
        // added samples cannot silently FP-diverge from the engine; bit-identical to the one-line
        // interp form for the samples below.
        let y: Vec<Sample> = segs
            .iter()
            .map(|&(x1, f1, x2, f2, u)| {
                let x_lim = u.max(x1).min(x2);
                let b = (f2 - f1) / (x2 - x1);
                let a = f2 - b * x2;
                r(a + b * x_lim)
            })
            .collect();
        out.push(Golden::new(
            "CDL.Reals.Line",
            "y",
            ValueKind::Real,
            ticks(4),
            y,
            "(x1,f1,x2,f2,u): in-range, clamp-low, clamp-high, non-dyadic segment",
            "xLim=min(max(u,x1),x2); b=(f2-f1)/(x2-x1); a=f2-b*x2; y=a+b*xLim; Buildings Reals/Line.mo",
        )
        .with_inputs(vec![
            input_r("x1", segs.iter().map(|row| row.0)),
            input_r("f1", segs.iter().map(|row| row.1)),
            input_r("x2", segs.iter().map(|row| row.2)),
            input_r("f2", segs.iter().map(|row| row.3)),
            input_r("u", segs.iter().map(|row| row.4)),
        ]));
    }

    // Line scenario variants: structural limit gates from Buildings Reals/Line.mo.
    {
        let segs = [
            (0.0_f64, 0.0_f64, 10.0_f64, 10.0_f64, -5.0_f64),
            (0.0, 0.0, 10.0, 10.0, 5.0),
            (0.0, 0.0, 10.0, 10.0, 15.0),
        ];
        out.push(line_golden(
            "limit_below_only",
            &segs,
            true,
            false,
            "limitBelow=true, limitAbove=false; (x1,f1,x2,f2)=(0,0,10,10), u below/in/above range",
            "xLim=max(x1,u); above x2 extrapolates; b=(f2-f1)/(x2-x1); a=f2-b*x2; y=a+b*xLim; Buildings Reals/Line.mo",
        ));
        out.push(line_golden(
            "limit_above_only",
            &segs,
            false,
            true,
            "limitBelow=false, limitAbove=true; (x1,f1,x2,f2)=(0,0,10,10), u below/in/above range",
            "xLim=min(x2,u); below x1 extrapolates; b=(f2-f1)/(x2-x1); a=f2-b*x2; y=a+b*xLim; Buildings Reals/Line.mo",
        ));
        out.push(line_golden(
            "unlimited",
            &segs,
            false,
            false,
            "limitBelow=false, limitAbove=false; (x1,f1,x2,f2)=(0,0,10,10), u below/in/above range",
            "xLim=u; both sides extrapolate; b=(f2-f1)/(x2-x1); a=f2-b*x2; y=a+b*xLim; Buildings Reals/Line.mo",
        ));
    }

    // Switch (Reals): y = u1 if u2 else u3 (u2 Boolean selector). Output is Real.
    {
        // (u1, sel u2, u3) per tick.
        let rows = [
            (0.1, true, 9.9),
            (0.1, false, 9.9),
            (-3.3, true, 2.7),
            (-3.3, false, 2.7),
        ];
        let y: Vec<Sample> = rows
            .iter()
            .map(|&(u1, sel, u3)| if sel { r(u1) } else { r(u3) })
            .collect();
        out.push(
            Golden::new(
                "CDL.Reals.Switch",
                "y",
                ValueKind::Real,
                ticks(4),
                y,
                "(u1,sel u2:Bool,u3): toggle selector over non-dyadic data",
                "y = u1 if u2 else u3 (selected Real verbatim); _spec/03 §4.1 Switch",
            )
            .with_inputs(vec![
                input_r("u1", rows.iter().map(|row| row.0)),
                input_b("u2", rows.iter().map(|row| row.1)),
                input_r("u3", rows.iter().map(|row| row.2)),
            ]),
        );
    }

    out
}

fn line_golden(
    scenario: &'static str,
    segs: &[(f64, f64, f64, f64, f64)],
    limit_below: bool,
    limit_above: bool,
    input_desc: &'static str,
    rule_desc: &'static str,
) -> Golden {
    let y: Vec<Sample> = segs
        .iter()
        .map(|&(x1, f1, x2, f2, u)| {
            let x_lim = match (limit_below, limit_above) {
                (true, true) => u.max(x1).min(x2),
                (true, false) => u.max(x1),
                (false, true) => u.min(x2),
                (false, false) => u,
            };
            let b = (f2 - f1) / (x2 - x1);
            let a = f2 - b * x2;
            r(a + b * x_lim)
        })
        .collect();
    Golden::new(
        "CDL.Reals.Line",
        "y",
        ValueKind::Real,
        ticks(segs.len()),
        y,
        input_desc,
        rule_desc,
    )
    .with_scenario(scenario)
    .with_inputs(vec![
        input_r("x1", segs.iter().map(|row| row.0)),
        input_r("f1", segs.iter().map(|row| row.1)),
        input_r("x2", segs.iter().map(|row| row.2)),
        input_r("f2", segs.iter().map(|row| row.3)),
        input_r("u", segs.iter().map(|row| row.4)),
    ])
}

fn comparators() -> Vec<Golden> {
    let mut out = Vec::new();

    // Greater (h=0): y = u1 > u2 (strict).
    {
        let u1 = [3.2, 5.7, 5.7, 2.1, 5.70000000001];
        let u2 = [5.7, 5.7, 3.2, 5.7, 5.7];
        let y: Vec<Sample> = u1.iter().zip(u2).map(|(&a, c)| b(a > c)).collect();
        out.push(
            Golden::new(
                "CDL.Reals.Greater",
                "y",
                ValueKind::Boolean,
                ticks(5),
                y,
                "h=0; u1=[3.2,5.7,5.7,2.1,5.70000000001], u2=[5.7,5.7,3.2,5.7,5.7]",
                "y = (u1 > u2), strict, h=0; _spec/03 §4.1 Greater",
            )
            .with_inputs(vec![input_r("u1", u1), input_r("u2", u2)]),
        );
    }

    // Greater (h>0): asymmetric Buildings band, set at u1>u2 and reset at u1<=u2-h.
    {
        let h = 1.0;
        let u1 = [3.0, 5.0, 4.0, 3.5, 4.0, 5.0, 3.6, 3.4];
        let u2 = [4.5; 8];
        let mut prev = false; // pre_y_start
        let mut y = Vec::new();
        for (&a, &c) in u1.iter().zip(&u2) {
            let next = (!prev && a > c) || (prev && a > c - h);
            y.push(b(next));
            prev = next;
        }
        out.push(
            Golden::new(
                "CDL.Reals.Greater",
                "y",
                ValueKind::Boolean,
                ticks(8),
                y,
                "scenario=hysteretic, h=1.0, pre_y_start=false; u2=4.5; u1=[3.0,5.0,4.0,3.5,4.0,5.0,3.6,3.4]",
                "y_h = (!pre_y && u1>u2) || (pre_y && u1>u2-h); asymmetric Buildings Reals/Greater.mo band; _spec/03 §4.1 Greater",
            )
            .with_scenario("hysteretic")
            .with_inputs(vec![input_r("u1", u1), input_r("u2", u2)]),
        );
    }

    // Greater preset (h>0, pre_y_start=true): first sample starts inside the hold band.
    {
        let h = 1.0;
        let u1 = [4.0, 5.0, 3.5, 3.0, 4.0, 5.0, 4.2, 3.4];
        let u2 = [4.5; 8];
        let mut prev = true; // pre_y_start
        let mut y = Vec::new();
        for (&a, &c) in u1.iter().zip(&u2) {
            let next = (!prev && a > c) || (prev && a > c - h);
            y.push(b(next));
            prev = next;
        }
        out.push(
            Golden::new(
                "CDL.Reals.Greater",
                "y",
                ValueKind::Boolean,
                ticks(8),
                y,
                "scenario=hysteretic_preset, h=1.0, pre_y_start=true; u2=4.5; u1=[4.0,5.0,3.5,3.0,4.0,5.0,4.2,3.4]",
                "y_h = (!pre_y && u1>u2) || (pre_y && u1>u2-h); first row is in the hold band so pre_y_start=true is observable; _spec/03 §4.1 Greater",
            )
            .with_scenario("hysteretic_preset")
            .with_inputs(vec![input_r("u1", u1), input_r("u2", u2)]),
        );
    }

    // Less (h=0): y = u1 < u2.
    {
        let u1 = [2.1, 5.7, 7.9, 5.7, 5.69999999999];
        let u2 = [5.7, 5.7, 3.2, 5.7, 5.7];
        let y: Vec<Sample> = u1.iter().zip(u2).map(|(&a, c)| b(a < c)).collect();
        out.push(
            Golden::new(
                "CDL.Reals.Less",
                "y",
                ValueKind::Boolean,
                ticks(5),
                y,
                "h=0; u1=[2.1,5.7,7.9,5.7,5.69999999999], u2=[5.7,5.7,3.2,5.7,5.7]",
                "y = (u1 < u2), strict, h=0; _spec/03 §4.1 Less",
            )
            .with_inputs(vec![input_r("u1", u1), input_r("u2", u2)]),
        );
    }

    // Less (h>0): asymmetric Buildings band, set at u1<u2 and reset at u1>=u2+h.
    {
        let h = 1.0;
        let u1 = [6.0, 4.0, 5.0, 5.5, 5.0, 4.0, 5.4, 5.6];
        let u2 = [4.5; 8];
        let mut prev = false; // pre_y_start
        let mut y = Vec::new();
        for (&a, &c) in u1.iter().zip(&u2) {
            let next = (!prev && a < c) || (prev && a < c + h);
            y.push(b(next));
            prev = next;
        }
        out.push(
            Golden::new(
                "CDL.Reals.Less",
                "y",
                ValueKind::Boolean,
                ticks(8),
                y,
                "scenario=hysteretic, h=1.0, pre_y_start=false; u2=4.5; u1=[6.0,4.0,5.0,5.5,5.0,4.0,5.4,5.6]",
                "y_h = (!pre_y && u1<u2) || (pre_y && u1<u2+h); asymmetric Buildings Reals/Less.mo band; _spec/03 §4.1 Less",
            )
            .with_scenario("hysteretic")
            .with_inputs(vec![input_r("u1", u1), input_r("u2", u2)]),
        );
    }

    // Less preset (h>0, pre_y_start=true): first sample starts inside the hold band.
    {
        let h = 1.0;
        let u1 = [5.0, 4.0, 5.5, 6.0, 5.0, 4.0, 5.2, 5.6];
        let u2 = [4.5; 8];
        let mut prev = true; // pre_y_start
        let mut y = Vec::new();
        for (&a, &c) in u1.iter().zip(&u2) {
            let next = (!prev && a < c) || (prev && a < c + h);
            y.push(b(next));
            prev = next;
        }
        out.push(
            Golden::new(
                "CDL.Reals.Less",
                "y",
                ValueKind::Boolean,
                ticks(8),
                y,
                "scenario=hysteretic_preset, h=1.0, pre_y_start=true; u2=4.5; u1=[5.0,4.0,5.5,6.0,5.0,4.0,5.2,5.6]",
                "y_h = (!pre_y && u1<u2) || (pre_y && u1<u2+h); first row is in the hold band so pre_y_start=true is observable; _spec/03 §4.1 Less",
            )
            .with_scenario("hysteretic_preset")
            .with_inputs(vec![input_r("u1", u1), input_r("u2", u2)]),
        );
    }

    // GreaterThreshold (h=0, t=4.5): y = u > t.
    {
        let t = 4.5;
        let u = [4.5, 4.50000000001, 9.3, 1.0, 4.49999999999];
        let y: Vec<Sample> = u.iter().map(|&x| b(x > t)).collect();
        out.push(
            Golden::new(
                "CDL.Reals.GreaterThreshold",
                "y",
                ValueKind::Boolean,
                ticks(5),
                y,
                "h=0, t=4.5; u=[4.5,4.50000000001,9.3,1.0,4.49999999999]",
                "y = (u > t), strict, h=0; _spec/03 §4.1 GreaterThreshold",
            )
            .with_inputs(vec![input_r("u", u)]),
        );
    }

    // GreaterThreshold (h>0): set at u>t, reset at u<=t-h, and hold in band.
    {
        let (t, h) = (4.5, 1.0);
        let u = [3.0, 5.0, 4.0, 3.5, 4.0, 5.0, 3.6, 3.4];
        let mut prev = false; // pre_y_start
        let mut y = Vec::new();
        for &x in &u {
            let next = (!prev && x > t) || (prev && x > t - h);
            y.push(b(next));
            prev = next;
        }
        out.push(
            Golden::new(
                "CDL.Reals.GreaterThreshold",
                "y",
                ValueKind::Boolean,
                ticks(8),
                y,
                "scenario=hysteretic, t=4.5, h=1.0, pre_y_start=false; u=[3.0,5.0,4.0,3.5,4.0,5.0,3.6,3.4]",
                "y_h = (!pre_y && u>t) || (pre_y && u>t-h); asymmetric Buildings Reals/GreaterThreshold.mo band; _spec/03 §4.1 GreaterThreshold",
            )
            .with_scenario("hysteretic")
            .with_inputs(vec![input_r("u", u)]),
        );
    }

    // GreaterThreshold preset (h>0, pre_y_start=true): first sample starts inside the hold band.
    {
        let (t, h) = (4.5, 1.0);
        let u = [4.0, 5.0, 3.5, 3.0, 4.0, 5.0, 4.2, 3.4];
        let mut prev = true; // pre_y_start
        let mut y = Vec::new();
        for &x in &u {
            let next = (!prev && x > t) || (prev && x > t - h);
            y.push(b(next));
            prev = next;
        }
        out.push(
            Golden::new(
                "CDL.Reals.GreaterThreshold",
                "y",
                ValueKind::Boolean,
                ticks(8),
                y,
                "scenario=hysteretic_preset, t=4.5, h=1.0, pre_y_start=true; u=[4.0,5.0,3.5,3.0,4.0,5.0,4.2,3.4]",
                "y_h = (!pre_y && u>t) || (pre_y && u>t-h); first row is in the hold band so pre_y_start=true is observable; _spec/03 §4.1 GreaterThreshold",
            )
            .with_scenario("hysteretic_preset")
            .with_inputs(vec![input_r("u", u)]),
        );
    }

    // LessThreshold (h=0, t=4.5): y = u < t.
    {
        let t = 4.5;
        let u = [4.5, 4.49999999999, 1.0, 9.3, 4.50000000001];
        let y: Vec<Sample> = u.iter().map(|&x| b(x < t)).collect();
        out.push(
            Golden::new(
                "CDL.Reals.LessThreshold",
                "y",
                ValueKind::Boolean,
                ticks(5),
                y,
                "h=0, t=4.5; u=[4.5,4.49999999999,1.0,9.3,4.50000000001]",
                "y = (u < t), strict, h=0; _spec/03 §4.1 LessThreshold",
            )
            .with_inputs(vec![input_r("u", u)]),
        );
    }

    // LessThreshold (h>0): set at u<t, reset at u>=t+h, and hold in band.
    {
        let (t, h) = (4.5, 1.0);
        let u = [6.0, 4.0, 5.0, 5.5, 5.0, 4.0, 5.4, 5.6];
        let mut prev = false; // pre_y_start
        let mut y = Vec::new();
        for &x in &u {
            let next = (!prev && x < t) || (prev && x < t + h);
            y.push(b(next));
            prev = next;
        }
        out.push(
            Golden::new(
                "CDL.Reals.LessThreshold",
                "y",
                ValueKind::Boolean,
                ticks(8),
                y,
                "scenario=hysteretic, t=4.5, h=1.0, pre_y_start=false; u=[6.0,4.0,5.0,5.5,5.0,4.0,5.4,5.6]",
                "y_h = (!pre_y && u<t) || (pre_y && u<t+h); asymmetric Buildings Reals/LessThreshold.mo band; _spec/03 §4.1 LessThreshold",
            )
            .with_scenario("hysteretic")
            .with_inputs(vec![input_r("u", u)]),
        );
    }

    // LessThreshold preset (h>0, pre_y_start=true): first sample starts inside the hold band.
    {
        let (t, h) = (4.5, 1.0);
        let u = [5.0, 4.0, 5.5, 6.0, 5.0, 4.0, 5.2, 5.6];
        let mut prev = true; // pre_y_start
        let mut y = Vec::new();
        for &x in &u {
            let next = (!prev && x < t) || (prev && x < t + h);
            y.push(b(next));
            prev = next;
        }
        out.push(
            Golden::new(
                "CDL.Reals.LessThreshold",
                "y",
                ValueKind::Boolean,
                ticks(8),
                y,
                "scenario=hysteretic_preset, t=4.5, h=1.0, pre_y_start=true; u=[5.0,4.0,5.5,6.0,5.0,4.0,5.2,5.6]",
                "y_h = (!pre_y && u<t) || (pre_y && u<t+h); first row is in the hold band so pre_y_start=true is observable; _spec/03 §4.1 LessThreshold",
            )
            .with_scenario("hysteretic_preset")
            .with_inputs(vec![input_r("u", u)]),
        );
    }

    // Hysteresis: latch with dead-band [uLow,uHigh], strict crossings, pre_y_start=false.
    {
        let (u_low, u_high) = (1.5_f64, 3.5_f64);
        let u = [2.5, 3.6, 2.5, 3.5, 1.5, 1.4, 2.5, 3.5];
        let mut prev = false; // pre_y_start
        let mut y = Vec::new();
        for &x in &u {
            // emit-from-state would emit prior latch; but Hysteresis is feedthrough on u: the CDL
            // state machine recomputes the latch from current u and only HOLDS when in-band.
            let next = if x > u_high {
                true
            } else if x < u_low {
                false
            } else {
                prev
            };
            y.push(b(next));
            prev = next;
        }
        out.push(Golden::new(
            "CDL.Reals.Hysteresis",
            "y",
            ValueKind::Boolean,
            ticks(8),
            y,
            "uLow=1.5, uHigh=3.5, pre_y_start=false; u=[2.5,3.6,2.5,3.5,1.5,1.4,2.5,3.5]",
            "y=true if u>uHigh; y=false if u<uLow; else hold pre(y); dead-band boundaries u==uLow and u==uHigh hold the prior state; pre_y_start=false; Buildings Reals/Hysteresis.mo",
        )
        .with_inputs(vec![input_r("u", u)]));
    }

    out
}
