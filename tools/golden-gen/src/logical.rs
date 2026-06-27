//! CDL.Logical combinational + edge/latch/timing blocks (`_spec/03` §4.3; CDL §7.6).
//!
//! Stateful blocks follow the two-pass emit-from-prior-state then update contract (`_spec/03`
//! §2.3, `_spec/01` §9). Time-driven dwell timers compare (t_now - entryTime) against the
//! duration parameter (`_spec/01` §8). Derived solely from the spec — never from `oce-blocks`.

use crate::oracle::{Golden, InputSeries, Sample, ValueKind};

fn b(x: bool) -> Sample {
    Sample::Boolean(x)
}
fn r(x: f64) -> Sample {
    Sample::Real(x)
}

fn input_b(name: &'static str, values: impl IntoIterator<Item = bool>) -> InputSeries {
    InputSeries::new(
        name,
        ValueKind::Boolean,
        values.into_iter().map(b).collect(),
    )
}

/// Tick grid at the default 60 s cadence.
fn ticks60(n: usize) -> Vec<f64> {
    (0..n).map(|k| (k as f64) * 60.0).collect()
}

fn timer_accumulating_reference(
    time: &[f64],
    u: &[bool],
    reset: &[bool],
    threshold: f64,
) -> (Vec<Sample>, Vec<Sample>) {
    assert_eq!(time.len(), u.len(), "TimerAccumulating u length");
    assert_eq!(time.len(), reset.len(), "TimerAccumulating reset length");

    let mut entry_time = time[0];
    let mut y_acc = 0.0_f64;
    let mut passed = threshold <= 0.0;
    let mut pre_u = false;
    let mut pre_reset = false;
    let mut pre_y = 0.0_f64;
    let mut y = Vec::with_capacity(time.len());
    let mut passed_samples = Vec::with_capacity(time.len());

    for k in 0..time.len() {
        let reset_rising = reset[k] && !pre_reset;
        let u_rising = u[k] && !pre_u;
        if reset_rising {
            entry_time = time[k];
            passed = threshold <= 0.0;
            y_acc = 0.0;
        } else if u_rising {
            entry_time = time[k];
            passed = threshold <= y_acc;
        } else if u[k] && time[k] >= threshold + entry_time - y_acc {
            passed = true;
        } else if !u[k] {
            y_acc = pre_y;
        }

        let yk = if u[k] {
            y_acc + time[k] - entry_time
        } else {
            y_acc
        };
        y.push(r(yk));
        passed_samples.push(b(passed));
        pre_y = yk;
        pre_u = u[k];
        pre_reset = reset[k];
    }

    (y, passed_samples)
}

/// Build all CDL.Logical goldens.
pub fn goldens() -> Vec<Golden> {
    let mut out = Vec::new();
    out.extend(combinational());
    out.extend(edges_latches());
    out.extend(timing());
    out
}

fn combinational() -> Vec<Golden> {
    let mut out = Vec::new();

    // And.
    {
        let u1 = [false, true, true, false];
        let u2 = [false, true, false, true];
        let y: Vec<Sample> = u1.iter().zip(u2).map(|(&a, c)| b(a && c)).collect();
        out.push(
            Golden::new(
                "CDL.Logical.And",
                "y",
                ValueKind::Boolean,
                ticks60(4),
                y,
                "u1=[F,T,T,F], u2=[F,T,F,T]",
                "y = u1 AND u2; _spec/03 §4.3 And",
            )
            .with_inputs(vec![input_b("u1", u1), input_b("u2", u2)]),
        );
    }
    // Or.
    {
        let u1 = [false, true, false, false];
        let u2 = [false, false, true, false];
        let y: Vec<Sample> = u1.iter().zip(u2).map(|(&a, c)| b(a || c)).collect();
        out.push(
            Golden::new(
                "CDL.Logical.Or",
                "y",
                ValueKind::Boolean,
                ticks60(4),
                y,
                "u1=[F,T,F,F], u2=[F,F,T,F]",
                "y = u1 OR u2; _spec/03 §4.3 Or",
            )
            .with_inputs(vec![input_b("u1", u1), input_b("u2", u2)]),
        );
    }
    // MultiAnd.
    {
        let u1 = [false, true, true, true];
        let u2 = [true, true, false, true];
        let u3 = [true, true, true, false];
        let y: Vec<Sample> = (0..u1.len())
            .map(|k| b(u1[k] && u2[k] && u3[k]))
            .collect();
        out.push(
            Golden::new(
                "CDL.Logical.MultiAnd",
                "y",
                ValueKind::Boolean,
                ticks60(4),
                y,
                "nin=3; u1=[F,T,T,T], u2=[T,T,F,T], u3=[T,T,T,F]",
                "y = all_i u[i] in declaration order; empty-vector behavior is unit-tested from Buildings Logical/MultiAnd.mo",
            )
            .with_inputs(vec![input_b("u1", u1), input_b("u2", u2), input_b("u3", u3)]),
        );
    }
    // MultiOr.
    {
        let u1 = [false, true, false, false];
        let u2 = [false, false, true, false];
        let u3 = [false, false, false, true];
        let y: Vec<Sample> = (0..u1.len())
            .map(|k| b(u1[k] || u2[k] || u3[k]))
            .collect();
        out.push(
            Golden::new(
                "CDL.Logical.MultiOr",
                "y",
                ValueKind::Boolean,
                ticks60(4),
                y,
                "nin=3; u1=[F,T,F,F], u2=[F,F,T,F], u3=[F,F,F,T]",
                "y = any_i u[i] in declaration order; empty-vector behavior is unit-tested from Buildings Logical/MultiOr.mo",
            )
            .with_inputs(vec![input_b("u1", u1), input_b("u2", u2), input_b("u3", u3)]),
        );
    }
    // Not.
    {
        let u = [false, true, false];
        let y: Vec<Sample> = u.iter().map(|&x| b(!x)).collect();
        out.push(
            Golden::new(
                "CDL.Logical.Not",
                "y",
                ValueKind::Boolean,
                ticks60(3),
                y,
                "u=[F,T,F]",
                "y = NOT u; _spec/03 §4.3 Not",
            )
            .with_inputs(vec![input_b("u", u)]),
        );
    }
    // Xor.
    {
        let u1 = [false, true, true, false];
        let u2 = [false, false, true, true];
        let y: Vec<Sample> = u1.iter().zip(u2).map(|(&a, c)| b(a ^ c)).collect();
        out.push(
            Golden::new(
                "CDL.Logical.Xor",
                "y",
                ValueKind::Boolean,
                ticks60(4),
                y,
                "u1=[F,T,T,F], u2=[F,F,T,T]",
                "y = u1 XOR u2 (exactly-one); _spec/03 §4.3 Xor",
            )
            .with_inputs(vec![input_b("u1", u1), input_b("u2", u2)]),
        );
    }
    // Switch: y = u1 if u2 else u3 (u2 middle selector).
    {
        let u1 = [true, true, false, false];
        let sel = [true, false, true, false];
        let u3 = [false, false, true, true];
        let y: Vec<Sample> = (0..4)
            .map(|k| if sel[k] { b(u1[k]) } else { b(u3[k]) })
            .collect();
        out.push(
            Golden::new(
                "CDL.Logical.Switch",
                "y",
                ValueKind::Boolean,
                ticks60(4),
                y,
                "u1=[T,T,F,F], sel u2=[T,F,T,F], u3=[F,F,T,T]",
                "y = u1 if u2 else u3; _spec/03 §4.3 Switch",
            )
            .with_inputs(vec![
                input_b("u1", u1),
                input_b("u2", sel),
                input_b("u3", u3),
            ]),
        );
    }

    out
}

fn edges_latches() -> Vec<Golden> {
    let mut out = Vec::new();

    // Edge: y = u AND NOT pre(u); pre_u_start=false.
    {
        let u = [false, true, true, false, true];
        let mut pre = false;
        let mut y = Vec::new();
        for &x in &u {
            y.push(b(x && !pre));
            pre = x;
        }
        out.push(
            Golden::new(
                "CDL.Logical.Edge",
                "y",
                ValueKind::Boolean,
                ticks60(5),
                y,
                "pre_u_start=false; u=[F,T,T,F,T]",
                "y = u AND NOT pre(u) (rising edge); _spec/03 §4.3 Edge",
            )
            .with_inputs(vec![input_b("u", u)]),
        );
    }
    // FallingEdge: y = NOT u AND pre(u).
    {
        let u = [false, true, false, false, true];
        let mut pre = false;
        let mut y = Vec::new();
        for &x in &u {
            y.push(b(!x && pre));
            pre = x;
        }
        out.push(
            Golden::new(
                "CDL.Logical.FallingEdge",
                "y",
                ValueKind::Boolean,
                ticks60(5),
                y,
                "pre_u_start=false; u=[F,T,F,F,T]",
                "y = (NOT u) AND pre(u) (falling edge); _spec/03 §4.3 FallingEdge",
            )
            .with_inputs(vec![input_b("u", u)]),
        );
    }
    // Change: y = u != pre(u).
    {
        let u = [false, false, true, true, false];
        let mut pre = false;
        let mut y = Vec::new();
        for &x in &u {
            y.push(b(x != pre));
            pre = x;
        }
        out.push(
            Golden::new(
                "CDL.Logical.Change",
                "y",
                ValueKind::Boolean,
                ticks60(5),
                y,
                "pre_u_start=false; u=[F,F,T,T,F]",
                "y = (u != pre(u)); _spec/03 §4.3 Change",
            )
            .with_inputs(vec![input_b("u", u)]),
        );
    }
    // Latch: clear-dominant SR. y = if clr then false else (rising(u) ? true : pre(y)).
    {
        let u = [false, true, false, true, true, false];
        let clr = [false, false, false, false, true, false];
        let mut pre_u = false;
        let mut pre_y = false;
        let mut y = Vec::new();
        for k in 0..u.len() {
            let rising = u[k] && !pre_u;
            let next = if clr[k] {
                false
            } else if rising {
                true
            } else {
                pre_y
            };
            y.push(b(next));
            pre_y = next;
            pre_u = u[k];
        }
        out.push(Golden::new(
            "CDL.Logical.Latch",
            "y",
            ValueKind::Boolean,
            ticks60(6),
            y,
            "u=[F,T,F,T,T,F], clr=[F,F,F,F,T,F]; init pre(y)=false, pre(u)=false",
            "clear-dominant SR: clr->false; else rising(u)->true; else hold pre(y); _spec/03 §4.3 Latch",
        )
        .with_inputs(vec![input_b("u", u), input_b("clr", clr)]));
    }
    // Toggle: clear-dominant T-flip-flop. rising(u) inverts pre(y); clr forces false.
    {
        let u = [false, true, true, false, false, false];
        let clr = [false, false, true, false, false, false];
        let mut pre_u = false;
        let mut pre_y = false;
        let mut y = Vec::new();
        for k in 0..u.len() {
            let rising = u[k] && !pre_u;
            let next = if clr[k] {
                false
            } else if rising {
                !pre_y
            } else {
                pre_y
            };
            y.push(b(next));
            pre_y = next;
            pre_u = u[k];
        }
        out.push(Golden::new(
            "CDL.Logical.Toggle",
            "y",
            ValueKind::Boolean,
            ticks60(6),
            y,
            "u=[F,T,T,F,F,F], clr=[F,F,T,F,F,F]; init pre(y)=false, pre(u)=false",
            "clear-dominant T-flip-flop: rising(u) inverts pre(y); clr->false; exercises clear path at row 2; _spec/03 §4.3 Toggle",
        )
        .with_inputs(vec![input_b("u", u), input_b("clr", clr)]));
    }

    out
}

fn timing() -> Vec<Golden> {
    let mut out = Vec::new();

    // Timer: Buildings Logical/Timer.mo — `when u then entryTime = time` (rising edge re-arms the
    // clock), then `y = if u then time - entryTime else 0.0`. Elapsed is a SINGLE (time - entryTime)
    // subtraction since the most recent rising edge, NOT an accumulation of per-tick dt (which would
    // both mis-bill the re-arm tick and FP-drift in the running case). pre(u) initial = false.
    {
        let t = [0.0, 0.1, 0.3, 0.6, 0.7];
        let u = [true, true, true, false, true];
        let threshold = 0.25;
        let mut entry_time = 0.0_f64;
        let mut pre_u = false;
        let mut y = Vec::new();
        let mut passed = Vec::new();
        for k in 0..t.len() {
            if u[k] && !pre_u {
                entry_time = t[k]; // rising edge: re-arm the clock to now
            }
            let yk = if u[k] { t[k] - entry_time } else { 0.0 };
            y.push(r(yk));
            passed.push(b(u[k] && yk >= threshold));
            pre_u = u[k];
        }
        out.push(Golden::new(
            "CDL.Logical.Timer",
            "y",
            ValueKind::Real,
            t.to_vec(),
            y,
            "threshold t=0.25; t=[0,0.1,0.3,0.6,0.7] (non-dyadic), u=[T,T,T,F,T]; re-arm rising edge at t=0.7 yields y=0",
            "y = if u then time - entryTime else 0; entryTime set on rising edge of u; Buildings Logical/Timer.mo",
        )
        .with_inputs(vec![input_b("u", u)]));
        out.push(
            Golden::new(
                "CDL.Logical.Timer",
                "passed",
                ValueKind::Boolean,
                t.to_vec(),
                passed,
                "threshold t=0.25; same trace as Timer.y",
                "passed = (y >= t) AND running; _spec/03 §4.3 Timer (canonical Buildings >=)",
            )
            .with_inputs(vec![input_b("u", u)]),
        );
    }

    // Timer scenario variant: default threshold t=0, proving idle ticks gate passed false.
    {
        let t = [0.0, 0.1, 0.2, 0.3, 0.4];
        let u = [true, false, false, true, false];
        let threshold = 0.0;
        let mut entry_time = 0.0_f64;
        let mut pre_u = false;
        let mut y = Vec::new();
        let mut passed = Vec::new();
        for k in 0..t.len() {
            if u[k] && !pre_u {
                entry_time = t[k];
            }
            let yk = if u[k] { t[k] - entry_time } else { 0.0 };
            y.push(r(yk));
            passed.push(b(u[k] && yk >= threshold));
            pre_u = u[k];
        }
        out.push(
            Golden::new(
                "CDL.Logical.Timer",
                "y",
                ValueKind::Real,
                t.to_vec(),
                y,
                "threshold t=0; t=[0,0.1,0.2,0.3,0.4], u=[T,F,F,T,F]; idle ticks remain y=0",
                "y = if u then time - entryTime else 0; entryTime set on rising edge of u; Buildings Logical/Timer.mo",
            )
            .with_scenario("threshold_zero")
            .with_inputs(vec![input_b("u", u)]),
        );
        out.push(
            Golden::new(
                "CDL.Logical.Timer",
                "passed",
                ValueKind::Boolean,
                t.to_vec(),
                passed,
                "threshold t=0; same trace as Timer.y; idle ticks must keep passed=false",
                "passed is gated by running input: u AND (y >= t); Buildings Logical/Timer.mo elsewhen not u",
            )
            .with_scenario("threshold_zero")
            .with_inputs(vec![input_b("u", u)]),
        );
    }

    // TimerAccumulating scenario variant: t=0 initializes and resets passed to true, then holds it
    // across u-false idle ticks. This distinguishes it from the non-accumulating Timer gate.
    {
        let t = [0.0, 0.1, 0.2, 0.3, 0.4];
        let u = [false, true, false, false, false];
        let reset = [false, false, false, true, false];
        let threshold = 0.0;
        let (y, passed) = timer_accumulating_reference(&t, &u, &reset, threshold);
        out.push(
            Golden::new(
                "CDL.Logical.TimerAccumulating",
                "y",
                ValueKind::Real,
                t.to_vec(),
                y,
                "threshold t=0; t=[0,0.1,0.2,0.3,0.4], u=[F,T,F,F,F], reset=[F,F,F,T,F]",
                "y = if u then yAcc + time - entryTime else yAcc; reset rising clears yAcc; Buildings Logical/TimerAccumulating.mo",
            )
            .with_scenario("threshold_zero")
            .with_inputs(vec![input_b("u", u), input_b("reset", reset)]),
        );
        out.push(
            Golden::new(
                "CDL.Logical.TimerAccumulating",
                "passed",
                ValueKind::Boolean,
                t.to_vec(),
                passed,
                "threshold t=0; same trace as TimerAccumulating.y; reset sets passed back to t<=0",
                "passed is a discrete latch: reset -> (t<=0), u rising samples yAcc, u-threshold crossing sets true, not u holds pre(passed); Buildings Logical/TimerAccumulating.mo",
            )
            .with_scenario("threshold_zero")
            .with_inputs(vec![input_b("u", u), input_b("reset", reset)]),
        );
    }

    // TimerAccumulating scenario variant: positive threshold proves passed latches across u=false
    // after crossing and clears to false on reset.
    {
        let t = [0.0, 0.25, 0.75, 1.0, 1.25, 1.5, 1.75];
        let u = [false, true, true, false, false, false, false];
        let reset = [true, false, false, false, false, true, false];
        let threshold = 0.5;
        let (y, passed) = timer_accumulating_reference(&t, &u, &reset, threshold);
        out.push(
            Golden::new(
                "CDL.Logical.TimerAccumulating",
                "y",
                ValueKind::Real,
                t.to_vec(),
                y,
                "threshold t=0.5; t=[0,0.25,0.75,1,1.25,1.5,1.75], u=[F,T,T,F,F,F,F], reset=[T,F,F,F,F,T,F]",
                "y accumulates while u is true, holds while u is false, and reset rising clears yAcc; Buildings Logical/TimerAccumulating.mo",
            )
            .with_scenario("latch_reset")
            .with_inputs(vec![input_b("u", u), input_b("reset", reset)]),
        );
        out.push(
            Golden::new(
                "CDL.Logical.TimerAccumulating",
                "passed",
                ValueKind::Boolean,
                t.to_vec(),
                passed,
                "threshold t=0.5; same trace as TimerAccumulating.y; passed holds true across u-false and reset clears to false",
                "passed follows Modelica event priority reset > u-rising > threshold crossing > hold pre(passed); Buildings Logical/TimerAccumulating.mo",
            )
            .with_scenario("latch_reset")
            .with_inputs(vec![input_b("u", u), input_b("reset", reset)]),
        );
    }

    // TrueDelay: assert true only after u continuously true >= delayTime; falling passes immediately.
    {
        let t = [0.0, 30.0, 100.0, 130.0, 160.0];
        let u = [false, true, true, true, false];
        let delay = 75.0;
        let mut entry: Option<f64> = None; // time of most recent rising edge
        let mut pre_u = false;
        let mut y = Vec::new();
        for k in 0..t.len() {
            if u[k] && !pre_u {
                entry = Some(t[k]);
            }
            let yk = if u[k] {
                match entry {
                    Some(e) => (t[k] - e) >= delay,
                    None => false,
                }
            } else {
                false
            };
            y.push(b(yk));
            pre_u = u[k];
        }
        out.push(
            Golden::new(
                "CDL.Logical.TrueDelay",
                "y",
                ValueKind::Boolean,
                t.to_vec(),
                y,
                "delayTime=75; t=[0,30,100,130,160], u=[F,T,T,T,F]; rising at t=30, elapses at 105",
                "y = u AND (t - entryTime >= delayTime); falling passes; _spec/03 §4.3 TrueDelay",
            )
            .with_inputs(vec![input_b("u", u)]),
        );
    }

    // TrueFalseHold: minimum-dwell debounce. Initial output follows u at t0.
    {
        let t = [0.0, 100.0, 250.0, 400.0, 700.0];
        let u = [true, false, true, false, true];
        let true_hold = 300.0;
        let false_hold = 300.0;
        // Reference: y starts following u at t0; tHold = time of last output change.
        // While (t - tHold) < dwell(current y), output is frozen; once expired, follow u.
        let mut y_state = u[0];
        let mut t_hold = t[0];
        let mut y = Vec::new();
        for k in 0..t.len() {
            if k == 0 {
                y.push(b(y_state));
                continue;
            }
            let dwell = if y_state { true_hold } else { false_hold };
            if (t[k] - t_hold) >= dwell {
                // dwell expired: output may follow u
                if u[k] != y_state {
                    y_state = u[k];
                    t_hold = t[k];
                }
            }
            y.push(b(y_state));
        }
        out.push(Golden::new(
            "CDL.Logical.TrueFalseHold",
            "y",
            ValueKind::Boolean,
            t.to_vec(),
            y,
            "trueHold=300, falseHold=300; t=[0,100,250,400,700], u=[T,F,T,F,T]",
            "min-dwell debounce: hold until dwell expires then follow u; _spec/03 §4.3 TrueFalseHold",
        )
        .with_inputs(vec![input_b("u", u)]));
    }

    // TrueHoldWithReset: hold true >= duration after u rises; clr rising forces false.
    {
        let t = [0.0, 50.0, 150.0, 300.0, 360.0];
        let u = [false, true, false, false, true];
        let clr = [false, false, false, true, false];
        let duration = 200.0;
        let mut pre_u = false;
        let mut t_hold: Option<f64> = None; // start of current hold window
        let mut y = Vec::new();
        for k in 0..t.len() {
            let rising = u[k] && !pre_u;
            if rising {
                t_hold = Some(t[k]);
            }
            let yk = if clr[k] {
                t_hold = None;
                false
            } else {
                let within = match t_hold {
                    Some(h) => (t[k] - h) < duration,
                    None => false,
                };
                within || u[k]
            };
            y.push(b(yk));
            pre_u = u[k];
        }
        out.push(Golden::new(
            "CDL.Logical.TrueHoldWithReset",
            "y",
            ValueKind::Boolean,
            t.to_vec(),
            y,
            "duration=200; t=[0,50,150,300,360], u=[F,T,F,F,T], clr=[F,F,F,T,F]; rising at 50",
            "rising(u) holds true for >=duration even if u falls; clr forces false; _spec/03 §4.3 TrueHoldWithReset",
        )
        .with_inputs(vec![input_b("u", u), input_b("clr", clr)]));
    }

    out
}
