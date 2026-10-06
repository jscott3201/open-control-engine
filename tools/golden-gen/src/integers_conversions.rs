//! CDL.Integers (arithmetic / comparators / counters) + CDL.Conversions.
//!
//! Integer inputs and results stay in the portable signed-32-bit domain recommended by OBC CDL
//! §7.4.1.2 / Modelica 3.6 §4.8.2. The oracle calculates in i64; implementation-only wide/wrap
//! probes belong in block unit tests, not these CSV conformance vectors. IntegerToReal is exact
//! throughout this domain. Derived solely from the spec — never from `oce-blocks`.

use crate::oracle::{Golden, InputSeries, Sample, ValueKind};

fn i(x: i64) -> Sample {
    Sample::Integer(x)
}
fn b(x: bool) -> Sample {
    Sample::Boolean(x)
}
fn r(x: f64) -> Sample {
    Sample::Real(x)
}

fn input_i(name: &'static str, values: impl IntoIterator<Item = i64>) -> InputSeries {
    InputSeries::new(
        name,
        ValueKind::Integer,
        values.into_iter().map(i).collect(),
    )
}

fn input_b(name: &'static str, values: impl IntoIterator<Item = bool>) -> InputSeries {
    InputSeries::new(
        name,
        ValueKind::Boolean,
        values.into_iter().map(b).collect(),
    )
}

fn input_r(name: &'static str, values: impl IntoIterator<Item = f64>) -> InputSeries {
    InputSeries::new(name, ValueKind::Real, values.into_iter().map(r).collect())
}

/// Integer-index tick grid t = 0, 1, 2, ... (the oracle traces use unit indices here).
fn ticks(n: usize) -> Vec<f64> {
    (0..n).map(|k| k as f64).collect()
}

/// Build all CDL.Integers + CDL.Conversions goldens.
pub fn goldens() -> Vec<Golden> {
    let mut out = Vec::new();
    out.extend(integer_arithmetic());
    out.extend(integer_comparators());
    out.extend(integer_stateful());
    out.extend(conversions());
    out
}

fn integer_arithmetic() -> Vec<Golden> {
    let mut out = Vec::new();

    // Add.
    {
        let u1 = [2147483000_i64, -5, 100000, -2147483647];
        let u2 = [647_i64, -2147483640, 100000, -1];
        let y: Vec<Sample> = u1.iter().zip(u2).map(|(&a, c)| i(a + c)).collect();
        out.push(
            Golden::new(
                "CDL.Integers.Add",
                "y",
                ValueKind::Integer,
                ticks(4),
                y,
                "u1=[2147483000,-5,100000,-2147483647], u2=[647,-2147483640,100000,-1]",
                "y = u1 + u2 (exact i64 add); _spec/03 §4.2 Add",
            )
            .with_inputs(vec![input_i("u1", u1), input_i("u2", u2)]),
        );
    }
    // Subtract.
    {
        let u1 = [5_i64, -2147483648, 0, 2147483646];
        let u2 = [12_i64, -2147483647, -7, -1];
        let y: Vec<Sample> = u1.iter().zip(u2).map(|(&a, c)| i(a - c)).collect();
        out.push(
            Golden::new(
                "CDL.Integers.Subtract",
                "y",
                ValueKind::Integer,
                ticks(4),
                y,
                "u1=[5,-2147483648,0,2147483646], u2=[12,-2147483647,-7,-1]; final result is i32::MAX",
                "y = u1 - u2; operands and results stay in the portable i32 domain; CDL.Integers.Subtract",
            )
            .with_inputs(vec![input_i("u1", u1), input_i("u2", u2)]),
        );
    }
    // Multiply.
    {
        let u1 = [7_i64, -6, 46340, 0];
        let u2 = [-8_i64, -9, 46340, 2147483647];
        let y: Vec<Sample> = u1.iter().zip(u2).map(|(&a, c)| i(a * c)).collect();
        out.push(
            Golden::new(
                "CDL.Integers.Multiply",
                "y",
                ValueKind::Integer,
                ticks(4),
                y,
                "u1=[7,-6,46340,0], u2=[-8,-9,46340,2147483647]; 46340 is the largest integer whose square fits i32",
                "y = u1 * u2; operands and results stay in the portable i32 domain; CDL.Integers.Multiply",
            )
            .with_inputs(vec![input_i("u1", u1), input_i("u2", u2)]),
        );
    }
    // AddParameter.
    {
        let p = 1000_i64;
        let u = [-1000_i64, 0, 2147482647];
        let y: Vec<Sample> = u.iter().map(|&x| i(x + p)).collect();
        out.push(
            Golden::new(
                "CDL.Integers.AddParameter",
                "y",
                ValueKind::Integer,
                ticks(3),
                y,
                "p=1000; u=[-1000,0,2147482647]",
                "y = u + p (exact i64 add); _spec/03 §4.2 AddParameter",
            )
            .with_inputs(vec![input_i("u", u)]),
        );
    }
    // Abs.
    {
        let u = [
            -2147483647_i64,
            -2147483646,
            -2,
            -1,
            0,
            1,
            2,
            2147483646,
            2147483647,
        ];
        let y: Vec<Sample> = u.iter().map(|&x| i(x.abs())).collect();
        out.push(
            Golden::new(
                "CDL.Integers.Abs",
                "y",
                ValueKind::Integer,
                ticks(9),
                y,
                "u=[-2147483647,-2147483646,-2,-1,0,1,2,2147483646,2147483647]; signs, zero, and representable magnitude boundaries",
                "y = abs(u); exclude i32::MIN because its positive magnitude is outside portable i32; CDL.Integers.Abs",
            )
            .with_inputs(vec![input_i("u", u)]),
        );
    }
    // Max.
    {
        let u1 = [5_i64, -10, 7, 2147483647];
        let u2 = [5_i64, -3, -7, -2147483648];
        let y: Vec<Sample> = u1.iter().zip(u2).map(|(&a, c)| i(a.max(c))).collect();
        out.push(
            Golden::new(
                "CDL.Integers.Max",
                "y",
                ValueKind::Integer,
                ticks(4),
                y,
                "u1=[5,-10,7,2147483647], u2=[5,-3,-7,-2147483648]",
                "y = max(u1,u2) (integer ordering); _spec/03 §4.2 Max",
            )
            .with_inputs(vec![input_i("u1", u1), input_i("u2", u2)]),
        );
    }
    // Min.
    {
        let u1 = [5_i64, -10, 7, 2147483647];
        let u2 = [5_i64, -3, -7, -2147483648];
        let y: Vec<Sample> = u1.iter().zip(u2).map(|(&a, c)| i(a.min(c))).collect();
        out.push(
            Golden::new(
                "CDL.Integers.Min",
                "y",
                ValueKind::Integer,
                ticks(4),
                y,
                "u1=[5,-10,7,2147483647], u2=[5,-3,-7,-2147483648]",
                "y = min(u1,u2) (integer ordering); _spec/03 §4.2 Min",
            )
            .with_inputs(vec![input_i("u1", u1), input_i("u2", u2)]),
        );
    }
    // MultiSum: y = sum k[i]*u[i] in declared order. nin=3, k=[2,-1,3].
    {
        let k = [2_i64, -1, 3];
        let rows: [[i64; 3]; 2] = [[10, 4, -2], [100, 100, 100]];
        let y: Vec<Sample> = rows
            .iter()
            .map(|row| i(row.iter().zip(k).map(|(&u, kk)| kk * u).sum::<i64>()))
            .collect();
        out.push(Golden::new(
            "CDL.Integers.MultiSum",
            "y",
            ValueKind::Integer,
            ticks(2),
            y,
            "nin=3, k=[2,-1,3]; u(t0)=[10,4,-2], u(t1)=[100,100,100]",
            "y = sum_i k[i]*u[i] in declared order (exact i64); _spec/03 §4.2 MultiSum / R-IMPL-6",
        )
        .with_inputs(vec![
            input_i("u1", rows.iter().map(|row| row[0])),
            input_i("u2", rows.iter().map(|row| row[1])),
            input_i("u3", rows.iter().map(|row| row[2])),
        ]));
    }
    // Switch: y = u1 if u2 else u3.
    {
        let u1 = [100_i64, 100, -7, -7];
        let sel = [true, false, true, false];
        let u3 = [200_i64, 200, 9, 9];
        let y: Vec<Sample> = (0..4)
            .map(|k| if sel[k] { i(u1[k]) } else { i(u3[k]) })
            .collect();
        out.push(
            Golden::new(
                "CDL.Integers.Switch",
                "y",
                ValueKind::Integer,
                ticks(4),
                y,
                "u1=[100,100,-7,-7], sel u2=[T,F,T,F], u3=[200,200,9,9]",
                "y = u1 if u2 else u3; _spec/03 §4.2 Switch",
            )
            .with_inputs(vec![
                input_i("u1", u1),
                input_b("u2", sel),
                input_i("u3", u3),
            ]),
        );
    }

    // Sources.Constant.
    {
        let k = -12345_i64;
        out.push(Golden::new(
            "CDL.Integers.Sources.Constant",
            "y",
            ValueKind::Integer,
            vec![0.0, 10.0, 20.0],
            vec![i(k); 3],
            "k=-12345; ticks t=[0,10,20] (zero-input source)",
            "y = k (stateless source, t-invariant, parameter only); _spec/03 §4.2 Sources.Constant",
        ));
    }

    out
}

fn integer_comparators() -> Vec<Golden> {
    let mut out = Vec::new();

    // Greater.
    {
        let u1 = [5_i64, 5, 6, -2147483648];
        let u2 = [4_i64, 5, 5, 2147483647];
        let y: Vec<Sample> = u1.iter().zip(u2).map(|(&a, c)| b(a > c)).collect();
        out.push(
            Golden::new(
                "CDL.Integers.Greater",
                "y",
                ValueKind::Boolean,
                ticks(4),
                y,
                "u1=[5,5,6,-2147483648], u2=[4,5,5,2147483647]",
                "y = (u1 > u2) strict; _spec/03 §4.2 Greater",
            )
            .with_inputs(vec![input_i("u1", u1), input_i("u2", u2)]),
        );
    }
    // Equal.
    {
        let u1 = [
            0_i64,
            -42,
            -42,
            2_147_483_647,
            -2_147_483_648,
            2_147_483_647,
            2_147_483_646,
            2_147_483_646,
            -2_147_483_647,
            -2_147_483_647,
        ];
        let u2 = [
            0_i64,
            -42,
            42,
            2_147_483_647,
            -2_147_483_648,
            -2_147_483_648,
            2_147_483_646,
            2_147_483_645,
            -2_147_483_647,
            -2_147_483_646,
        ];
        let y: Vec<Sample> = u1.iter().zip(u2).map(|(&a, c)| b(a == c)).collect();
        out.push(
            Golden::new(
                "CDL.Integers.Equal",
                "y",
                ValueKind::Boolean,
                ticks(10),
                y,
                "u1=[0,-42,-42,2147483647,-2147483648,2147483647,2147483646,2147483646,-2147483647,-2147483647], u2=[0,-42,42,2147483647,-2147483648,-2147483648,2147483646,2147483645,-2147483647,-2147483646]; i32 endpoints and unequal neighbors",
                "y = (u1 == u2) exact integer equality; Buildings Controls/OBC/CDL/Integers/Equal.mo",
            )
            .with_inputs(vec![input_i("u1", u1), input_i("u2", u2)]),
        );
    }
    // GreaterEqual.
    {
        let u1 = [5_i64, 5, 4];
        let u2 = [4_i64, 5, 5];
        let y: Vec<Sample> = u1.iter().zip(u2).map(|(&a, c)| b(a >= c)).collect();
        out.push(
            Golden::new(
                "CDL.Integers.GreaterEqual",
                "y",
                ValueKind::Boolean,
                ticks(3),
                y,
                "u1=[5,5,4], u2=[4,5,5]",
                "y = (u1 >= u2); _spec/03 §4.2 GreaterEqual",
            )
            .with_inputs(vec![input_i("u1", u1), input_i("u2", u2)]),
        );
    }
    // GreaterThreshold (t=10).
    {
        let t = 10_i64;
        let u = [11_i64, 10, 9];
        let y: Vec<Sample> = u.iter().map(|&x| b(x > t)).collect();
        out.push(
            Golden::new(
                "CDL.Integers.GreaterThreshold",
                "y",
                ValueKind::Boolean,
                ticks(3),
                y,
                "t=10; u=[11,10,9]",
                "y = (u > t) strict; _spec/03 §4.2 GreaterThreshold",
            )
            .with_inputs(vec![input_i("u", u)]),
        );
    }
    // GreaterEqualThreshold (t=10).
    {
        let t = 10_i64;
        let u = [11_i64, 10, 9];
        let y: Vec<Sample> = u.iter().map(|&x| b(x >= t)).collect();
        out.push(
            Golden::new(
                "CDL.Integers.GreaterEqualThreshold",
                "y",
                ValueKind::Boolean,
                ticks(3),
                y,
                "t=10; u=[11,10,9]",
                "y = (u >= t); _spec/03 §4.2 GreaterEqualThreshold",
            )
            .with_inputs(vec![input_i("u", u)]),
        );
    }
    // Less.
    {
        let u1 = [3_i64, 5, 6, 2147483647];
        let u2 = [4_i64, 5, 5, -2147483648];
        let y: Vec<Sample> = u1.iter().zip(u2).map(|(&a, c)| b(a < c)).collect();
        out.push(
            Golden::new(
                "CDL.Integers.Less",
                "y",
                ValueKind::Boolean,
                ticks(4),
                y,
                "u1=[3,5,6,2147483647], u2=[4,5,5,-2147483648]",
                "y = (u1 < u2) strict; _spec/03 §4.2 Less",
            )
            .with_inputs(vec![input_i("u1", u1), input_i("u2", u2)]),
        );
    }
    // LessEqual.
    {
        let u1 = [3_i64, 5, 6];
        let u2 = [4_i64, 5, 5];
        let y: Vec<Sample> = u1.iter().zip(u2).map(|(&a, c)| b(a <= c)).collect();
        out.push(
            Golden::new(
                "CDL.Integers.LessEqual",
                "y",
                ValueKind::Boolean,
                ticks(3),
                y,
                "u1=[3,5,6], u2=[4,5,5]",
                "y = (u1 <= u2); _spec/03 §4.2 LessEqual",
            )
            .with_inputs(vec![input_i("u1", u1), input_i("u2", u2)]),
        );
    }
    // LessThreshold (t=10).
    {
        let t = 10_i64;
        let u = [9_i64, 10, 11];
        let y: Vec<Sample> = u.iter().map(|&x| b(x < t)).collect();
        out.push(
            Golden::new(
                "CDL.Integers.LessThreshold",
                "y",
                ValueKind::Boolean,
                ticks(3),
                y,
                "t=10; u=[9,10,11]",
                "y = (u < t) strict; _spec/03 §4.2 LessThreshold",
            )
            .with_inputs(vec![input_i("u", u)]),
        );
    }
    // LessEqualThreshold (t=10).
    {
        let t = 10_i64;
        let u = [9_i64, 10, 11];
        let y: Vec<Sample> = u.iter().map(|&x| b(x <= t)).collect();
        out.push(
            Golden::new(
                "CDL.Integers.LessEqualThreshold",
                "y",
                ValueKind::Boolean,
                ticks(3),
                y,
                "t=10; u=[9,10,11]",
                "y = (u <= t); _spec/03 §4.2 LessEqualThreshold",
            )
            .with_inputs(vec![input_i("u", u)]),
        );
    }

    out
}

fn integer_stateful() -> Vec<Golden> {
    let mut out = Vec::new();

    // OnCounter: emit start-of-tick count; after the first tick, either input's rising edge fires
    // the Modelica `when {trigger,reset}`. The branch uses the current reset level, not reset's
    // edge status.
    {
        let trigger = [false, true, true, false, true, true, false];
        let reset = [false, false, false, false, true, true, false];
        let y_start = 0_i64;
        out.push(on_counter_golden(
            None,
            &trigger,
            &reset,
            y_start,
            "y_start=0; trigger=[F,T,T,F,T,T,F], reset=[F,F,F,F,T,T,F]; includes simultaneous trigger/reset rise",
            "emit start-of-tick count; no when fires at t0; on trigger-or-reset rising edge, y := if reset then y_start else pre(y)+1; reset level has priority; Buildings Integers/OnCounter.mo",
        ));
    }

    // Held reset suppresses trigger rising edges because the when-branch tests reset's level.
    {
        let trigger = [false, false, true, true, false, true, true];
        let reset = [true, true, true, true, true, true, true];
        out.push(on_counter_golden(
            Some("held_reset"),
            &trigger,
            &reset,
            3,
            "scenario=held_reset, y_start=3; reset is high for the full trace while trigger rises twice",
            "Modelica when fires on trigger rising edges, but the current reset level is true, so y := y_start and increments are suppressed; Buildings Integers/OnCounter.mo",
        ));
    }

    // A condition that is already true at the initial tick does not fire a Modelica `when`.
    {
        let trigger = [true, true, false, true, true];
        let reset = [false, false, false, false, false];
        out.push(on_counter_golden(
            Some("trigger_initially_true"),
            &trigger,
            &reset,
            2,
            "scenario=trigger_initially_true, y_start=2; trigger starts true at t0, then falls and rises again",
            "No when event fires for trigger already true at the initial tick; the later false->true transition increments by one; Buildings Integers/OnCounter.mo",
        ));
    }

    // Change (Integer): y = u != prev, up = u > prev, down = u < prev; prev seeded to first sample.
    {
        let u = [5_i64, 5, 8, 8, 2];
        // Buildings Integers/Change.mo: parameter pre_u_start=0, u(start=pre_u_start),
        // so pre(u) = 0 at t0 (NOT the first sample). y0 = (5 != 0) = true.
        let mut prev = 0_i64;
        let mut yy = Vec::new();
        let mut up = Vec::new();
        let mut down = Vec::new();
        for &x in &u {
            yy.push(b(x != prev));
            up.push(b(x > prev));
            down.push(b(x < prev));
            prev = x;
        }
        out.push(
            Golden::new(
                "CDL.Integers.Change",
                "y",
                ValueKind::Boolean,
                ticks(5),
                yy,
                "u=[5,5,8,8,2]; pre(u) seeded to pre_u_start=0 (Buildings default)",
                "y = (u != pre(u)), pre(u) start = pre_u_start = 0; Buildings Integers/Change.mo",
            )
            .with_inputs(vec![input_i("u", u)]),
        );
        out.push(
            Golden::new(
                "CDL.Integers.Change",
                "up",
                ValueKind::Boolean,
                ticks(5),
                up,
                "u=[5,5,8,8,2]; pre(u) seeded to pre_u_start=0 (Buildings default)",
                "up = (u > pre(u)), pre(u) start = pre_u_start = 0; Buildings Integers/Change.mo",
            )
            .with_inputs(vec![input_i("u", u)]),
        );
        out.push(
            Golden::new(
                "CDL.Integers.Change",
                "down",
                ValueKind::Boolean,
                ticks(5),
                down,
                "u=[5,5,8,8,2]; pre(u) seeded to pre_u_start=0 (Buildings default)",
                "down = (u < pre(u)), pre(u) start = pre_u_start = 0; Buildings Integers/Change.mo",
            )
            .with_inputs(vec![input_i("u", u)]),
        );
    }

    out
}

fn on_counter_golden(
    scenario: Option<&'static str>,
    trigger: &[bool],
    reset: &[bool],
    y_start: i64,
    input_desc: &'static str,
    rule_desc: &'static str,
) -> Golden {
    let y = on_counter_y(trigger, reset, y_start);
    let mut golden = Golden::new(
        "CDL.Integers.OnCounter",
        "y",
        ValueKind::Integer,
        ticks(trigger.len()),
        y,
        input_desc,
        rule_desc,
    )
    .with_inputs(vec![
        input_b("trigger", trigger.iter().copied()),
        input_b("reset", reset.iter().copied()),
    ]);
    if let Some(scenario) = scenario {
        golden = golden.with_scenario(scenario);
    }
    golden
}

fn on_counter_y(trigger: &[bool], reset: &[bool], y_start: i64) -> Vec<Sample> {
    assert_eq!(trigger.len(), reset.len());
    let mut count = y_start;
    let mut pre_trigger = false;
    let mut pre_reset = false;
    let mut has_history = false;
    let mut y = Vec::with_capacity(trigger.len());
    for (&trigger, &reset) in trigger.iter().zip(reset) {
        y.push(i(count));
        if has_history {
            let trigger_rising = trigger && !pre_trigger;
            let reset_rising = reset && !pre_reset;
            let event = trigger_rising || reset_rising;
            if event {
                count = if reset {
                    y_start
                } else {
                    count.wrapping_add(1)
                };
            }
        }
        pre_trigger = trigger;
        pre_reset = reset;
        has_history = true;
    }
    y
}

fn conversions() -> Vec<Golden> {
    let mut out = Vec::new();

    // BooleanToReal: y = realTrue if u else realFalse. realTrue=0.1, realFalse=-0.3.
    {
        let real_true = 0.1;
        let real_false = -0.3;
        let u = [true, false, true, false];
        let y: Vec<Sample> = u
            .iter()
            .map(|&x| r(if x { real_true } else { real_false }))
            .collect();
        out.push(
            Golden::new(
                "CDL.Conversions.BooleanToReal",
                "y",
                ValueKind::Real,
                ticks(4),
                y,
                "realTrue=0.1, realFalse=-0.3; u=[T,F,T,F]; non-dyadic constants pin f64 bits",
                "y = realTrue if u else realFalse; _spec/03 §4.4 BooleanToReal (R-CONV-1)",
            )
            .with_inputs(vec![input_b("u", u)]),
        );
    }
    // BooleanToInteger: y = integerTrue if u else integerFalse. 7 / -2.
    {
        let int_true = 7_i64;
        let int_false = -2_i64;
        let u = [true, false, true];
        let y: Vec<Sample> = u
            .iter()
            .map(|&x| i(if x { int_true } else { int_false }))
            .collect();
        out.push(
            Golden::new(
                "CDL.Conversions.BooleanToInteger",
                "y",
                ValueKind::Integer,
                ticks(3),
                y,
                "integerTrue=7, integerFalse=-2; u=[T,F,T]",
                "y = integerTrue if u else integerFalse; _spec/03 §4.4 BooleanToInteger",
            )
            .with_inputs(vec![input_b("u", u)]),
        );
    }
    // IntegerToReal: y = (Real) u. Every portable i32 input is exactly representable in binary64.
    {
        let u = [0_i64, -7, 2147483647, -2147483648];
        let y: Vec<Sample> = u.iter().map(|&x| r(x as f64)).collect();
        out.push(Golden::new(
            "CDL.Conversions.IntegerToReal",
            "y",
            ValueKind::Real,
            ticks(4),
            y,
            "u=[0,-7,2147483647,-2147483648]; zero, negative, and both portable i32 endpoints",
            "y = (Real) u; exact binary64 conversion over portable i32 inputs; CDL.Conversions.IntegerToReal",
        )
        .with_inputs(vec![input_i("u", u)]));
    }
    // RealToInteger: Buildings Conversions/RealToInteger.mo is SIGN-BRANCHED (round half away from
    // zero): y = if u > 0 then integer(floor(u + 0.5)) else integer(ceil(u - 0.5)).
    {
        let u = [2.5_f64, -2.5, 2.4, 2.6, -2.4, -2.6, 0.5, -0.5];
        let y: Vec<Sample> = u
            .iter()
            .map(|&x| {
                let rounded = if x > 0.0 {
                    (x + 0.5).floor()
                } else {
                    (x - 0.5).ceil()
                };
                i(rounded as i64)
            })
            .collect();
        out.push(Golden::new(
            "CDL.Conversions.RealToInteger",
            "y",
            ValueKind::Integer,
            ticks(8),
            y,
            "u=[2.5,-2.5,2.4,2.6,-2.4,-2.6,0.5,-0.5]; half-boundary cases pin round-half-away-from-zero",
            "y = if u>0 then integer(floor(u+0.5)) else integer(ceil(u-0.5)) (round half away from zero); Buildings Conversions/RealToInteger.mo",
        )
        .with_inputs(vec![input_r("u", u)]));
    }
    out
}
