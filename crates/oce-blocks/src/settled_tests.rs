//! Tests for the settled-value rule (issue #800, design note `docs/design/pulse-event-inputs.md`):
//! the formula itself, and every consumer class with every `(w(k-1), v(k), s(k))` combination at
//! a later frame and every `(v, w, s)` at the initial frame. Expectations are hand-derived from
//! section 1.4 of the note: at a later frame an event input fires iff
//! `(v(k) and not w(k-1)) or (not v(k) and s(k))`. At the initial frame the graph passes the cone
//! at initialisation `i` (pulses at their frame value, samples false) in the declared input's
//! place (decision 3 as revised on 2026-11-22): a when-only class fires iff `s and not i`, and
//! `Latch`/`Toggle` run their initial equations on `i`, then their `when` on that rise.

use oce_model::{ParamTable, Value};

use super::settled::{Fire, Settled, fire};
use super::{Block, Ctx, NoopDiagnostics, lookup};

const COMBOS: [(bool, bool, bool); 8] = [
    (false, false, false),
    (false, false, true),
    (false, true, false),
    (false, true, true),
    (true, false, false),
    (true, false, true),
    (true, true, false),
    (true, true, true),
];

#[test]
fn the_formula_rises_from_before_to_during_or_from_during_to_settled() {
    for (prev_w, v, s) in COMBOS {
        let x = Settled { v, w: false, s };
        let got = fire(false, prev_w, x);
        assert_eq!(
            got,
            Fire {
                first: v && !prev_w,
                second: !v && s,
            },
            "w(k-1) {prev_w}, v {v}, s {s}"
        );
        // Never both: three values rise at most once.
        assert!(!(got.first && got.second));
    }
}

#[test]
fn at_the_initial_frame_the_input_goes_from_initialisation_to_w_to_settled() {
    // `v` here is the cone at initialisation `i`, which the graph passes at the initial frame.
    for (i, w, s) in COMBOS {
        for prev_w in [false, true] {
            let got = fire(true, prev_w, Settled { v: i, w, s });
            assert_eq!(
                got,
                Fire {
                    first: w && !i,
                    second: s && !w,
                },
                "i {i}, w {w}, s {s}"
            );
            assert!(!(got.first && got.second), "at most one step rises");
        }
    }
}

#[test]
fn without_settled_values_the_rule_is_the_rising_edge() {
    // No hidden inputs: `w = s = v`, so the formula is `v and not prev`, and the initial frame
    // never fires.
    for (prev, v) in [(false, false), (false, true), (true, false), (true, true)] {
        let x = super::settled::settled_input(&[Value::Boolean(v)], 0, 1, 0);
        assert_eq!(x, Settled { v, w: v, s: v });
        assert_eq!(fire(false, prev, x).any(), v && !prev);
        assert!(!fire(true, prev, x).any());
    }
}

fn make(class: &str, params: &[(&str, Value)]) -> Box<dyn Block> {
    let table = ParamTable {
        values: params
            .iter()
            .map(|(n, v)| (std::sync::Arc::from(*n), v.clone()))
            .collect(),
    };
    (lookup(class).unwrap_or_else(|| panic!("{class}")).make)(&table)
}

/// One frame: emit then update, with `initial` marking the frame driver's first frame.
fn frame(
    block: &dyn Block,
    region: &mut [u64],
    t: f64,
    initial: bool,
    inputs: &[Value],
) -> Vec<Value> {
    let diag = NoopDiagnostics;
    let cx = Ctx::new(t, &diag).with_initial(initial);
    let mut out = Vec::new();
    block.emit_from_state(&cx, inputs, region, &mut |_, v| out.push(v));
    block.update_state(&cx, inputs, region);
    out
}

/// A consumer class under test: how to build it, its declared inputs at a frame given the
/// settled input's frame value `v` (every other input fixed), and whether its frame-1 outputs
/// show an event.
struct Consumer {
    class: &'static str,
    params: Vec<(&'static str, Value)>,
    inputs: fn(bool, usize) -> Vec<Value>,
    fired: fn(&[Value]) -> bool,
    /// Settled inputs other than the one under test (their hidden pair is `(v, v)`).
    others: usize,
    /// The slot of the input under test among the block's settled inputs.
    slot: usize,
}

fn real(v: &Value) -> f64 {
    match v {
        Value::Real(x) => *x,
        Value::Integer(n) => *n as f64,
        Value::Boolean(b) => f64::from(u8::from(*b)),
        other => panic!("{other:?}"),
    }
}

fn consumers() -> Vec<Consumer> {
    use Value::{Boolean as B, Integer as I, Real as R};
    vec![
        Consumer {
            class: "CDL.Integers.OnCounter",
            params: vec![("y_start", I(0))],
            inputs: |v, _k| vec![B(v), B(false)],
            fired: |o| real(&o[0]) == 1.0,
            others: 1,
            slot: 0,
        },
        Consumer {
            class: "CDL.Integers.OnCounter",
            params: vec![("y_start", I(5))],
            inputs: |v, _k| vec![B(false), B(v)],
            // A reset event sets the count to `y_start`; without one it stays at 5 too, so read
            // the reset through a count that has moved: see `on_counter_reset_*` below.
            fired: |_| false,
            others: 1,
            slot: 1,
        },
        Consumer {
            class: "CDL.Discrete.TriggeredSampler",
            params: vec![("y_start", R(7.0))],
            inputs: |v, _k| vec![R(3.0), B(v)],
            fired: |o| real(&o[0]) == 3.0,
            others: 0,
            slot: 0,
        },
        Consumer {
            class: "CDL.Discrete.TriggeredMax",
            params: vec![],
            // `u = -5` at frame 0 (so `y = -5`): an event at frame 1 gives `max(-5, |u|) = 5`.
            inputs: |v, _k| vec![R(if v { 5.0 } else { -5.0 }), B(v)],
            fired: |o| real(&o[0]) == 5.0,
            others: 0,
            slot: 0,
        },
        Consumer {
            class: "CDL.Discrete.TriggeredMovingMean",
            params: vec![("n", I(2))],
            // `u = 1` at frame 0, 3 at frame 1: an event there gives the mean (1 + 3) / 2.
            inputs: |v, k| vec![R(if k == 0 { 1.0 } else { 3.0 }), B(v)],
            fired: |o| real(&o[0]) == 2.0,
            others: 0,
            slot: 0,
        },
        Consumer {
            class: "CDL.Reals.IntegratorWithReset",
            params: vec![("k", R(1.0)), ("y_start", R(0.0))],
            inputs: |v, _k| vec![R(0.0), R(9.0), B(v)],
            fired: |o| real(&o[0]) == 9.0,
            others: 0,
            slot: 0,
        },
        Consumer {
            class: "CDL.Reals.PIDWithReset",
            params: vec![
                (
                    "controllerType",
                    // PI (ordinal 2), as `state_contract.rs` builds it.
                    Value::Enum {
                        class: oce_model::EnumClassId::SIMPLE_CONTROLLER,
                        ordinal: 2,
                    },
                ),
                ("k", R(1.0)),
                ("Ti", R(1e20)),
                ("xi_start", R(0.0)),
                ("y_reset", R(0.5)),
            ],
            inputs: |v, _k| vec![R(1.0), R(0.0), B(v)],
            fired: |o| real(&o[0]) == 0.5,
            others: 0,
            slot: 0,
        },
        Consumer {
            class: "CDL.Logical.Latch",
            params: vec![],
            inputs: |v, _k| vec![B(v), B(false)],
            fired: |o| real(&o[0]) == 1.0,
            others: 1,
            slot: 0,
        },
        Consumer {
            class: "CDL.Logical.Toggle",
            params: vec![],
            inputs: |v, _k| vec![B(v), B(false)],
            fired: |o| real(&o[0]) == 1.0,
            others: 1,
            slot: 0,
        },
        Consumer {
            class: "CDL.Logical.TimerAccumulating",
            params: vec![("t", R(100.0))],
            inputs: |v, _k| vec![B(true), B(v)],
            fired: |o| real(&o[0]) == 0.0,
            others: 0,
            slot: 0,
        },
    ]
}

/// The block's inputs with the hidden `w`, `s` pairs: the input under test gets `(w, s)`, the
/// other settled inputs `(v, v)` of their declared values.
fn with_hidden(c: &Consumer, declared: Vec<Value>, w: bool, s: bool) -> Vec<Value> {
    let block = make(c.class, &c.params);
    let settled = block.settled_event_inputs().to_vec();
    assert_eq!(settled.len(), c.others + 1, "{}", c.class);
    let mut inputs = declared.clone();
    for (slot, &port) in settled.iter().enumerate() {
        if slot == c.slot {
            inputs.push(Value::Boolean(w));
            inputs.push(Value::Boolean(s));
        } else {
            inputs.push(declared[port].clone());
            inputs.push(declared[port].clone());
        }
    }
    inputs
}

#[test]
fn every_consumer_fires_by_the_formula_at_a_later_frame_for_all_eight_combinations() {
    for c in consumers() {
        if c.class == "CDL.Integers.OnCounter" && c.slot == 1 {
            continue;
        }
        for (prev_w, v, s) in COMBOS {
            let block = make(c.class, &c.params);
            let mut region = vec![0u64; block.state_len()];
            block.init_state(&mut region, &ParamTable::default());
            // Frame 0: everything false, nothing fires. Frame 1: `v = s = false`, `w = prev_w`,
            // nothing fires and `prev_w` becomes the stored word. Frame 2 is the case.
            let zero = with_hidden(&c, (c.inputs)(false, 0), false, false);
            frame(block.as_ref(), &mut region, 0.0, true, &zero);
            let one = with_hidden(&c, (c.inputs)(false, 1), prev_w, false);
            frame(block.as_ref(), &mut region, 1.0, false, &one);
            let two = with_hidden(&c, (c.inputs)(v, 2), false, s);
            let out = frame(block.as_ref(), &mut region, 2.0, false, &two);
            let want = (v && !prev_w) || (!v && s);
            assert_eq!(
                (c.fired)(&out),
                want,
                "{}: w(k-1) {prev_w}, v {v}, s {s}: {out:?}",
                c.class
            );
        }
    }
}

#[test]
fn at_the_initial_frame_classes_fire_on_i_to_w_or_w_to_s() {
    for c in consumers() {
        if c.class == "CDL.Integers.OnCounter" && c.slot == 1 {
            continue;
        }
        for (i, w, s) in COMBOS {
            let block = make(c.class, &c.params);
            let mut region = vec![0u64; block.state_len()];
            block.init_state(&mut region, &ParamTable::default());
            let inputs = with_hidden(&c, (c.inputs)(i, 0), w, s);
            let out = frame(block.as_ref(), &mut region, 0.0, true, &inputs);
            match c.class {
                // `when {initial(), trigger}` samples at the initial frame whatever the trigger.
                "CDL.Discrete.TriggeredMovingMean" => assert_eq!(real(&out[0]), 1.0),
                // `initial equation y = u`, then `max(pre(y), abs(u))` on a sample instant.
                "CDL.Discrete.TriggeredMax" => {
                    let u: f64 = if i { 5.0 } else { -5.0 };
                    let fires = (w && !i) || (s && !w);
                    let want = if fires { u.abs() } else { u };
                    assert_eq!(real(&out[0]), want, "i {i}, w {w}, s {s}");
                }
                // The reset's assignments equal the initial values (`y = 0`, `passed = t <= 0`):
                // an event at the initial frame cannot show in the outputs, in Modelica either.
                "CDL.Logical.TimerAccumulating" => assert_eq!(real(&out[0]), 0.0),
                // `Latch` (clr false): `y = u_i`, then `y = u` at each rise: `i or w or s`.
                // `Toggle`: `y = u_i`, then toggled at each rise.
                _ => {
                    let want = match c.class {
                        "CDL.Logical.Latch" => i || w || s,
                        "CDL.Logical.Toggle" => i ^ (w && !i) ^ (s && !w),
                        _ => (w && !i) || (s && !w),
                    };
                    assert_eq!((c.fired)(&out), want, "{}: i {i}, w {w}, s {s}", c.class);
                }
            }
        }
    }
}

#[test]
fn the_stored_word_is_w_whatever_the_frame_value() {
    // A direct pulse: `w = s = false` while `v` is true on two consecutive frames. Both fire.
    for c in consumers() {
        if (c.class == "CDL.Integers.OnCounter" && c.slot == 1) || c.class.contains("Moving") {
            continue;
        }
        let block = make(c.class, &c.params);
        let mut region = vec![0u64; block.state_len()];
        block.init_state(&mut region, &ParamTable::default());
        frame(
            block.as_ref(),
            &mut region,
            0.0,
            true,
            &with_hidden(&c, (c.inputs)(false, 0), false, false),
        );
        let pulse = with_hidden(&c, (c.inputs)(true, 1), false, false);
        let first = frame(block.as_ref(), &mut region, 1.0, false, &pulse);
        assert!((c.fired)(&first), "{}: the first pulse", c.class);
        // A second consecutive pulse fires again (Toggle back, counter 2, ...): read it through
        // the class's own effect where `fired` cannot tell.
        let second = frame(block.as_ref(), &mut region, 2.0, false, &pulse);
        match c.class {
            "CDL.Integers.OnCounter" => assert_eq!(real(&second[0]), 2.0),
            "CDL.Logical.Toggle" => assert_eq!(real(&second[0]), 0.0),
            _ => assert!((c.fired)(&second), "{}: the second pulse", c.class),
        }
    }
}

#[test]
fn on_counter_assigns_with_reset_s_value_in_the_iteration_that_fired() {
    use Value::{Boolean as B, Integer as I};
    let block = make("CDL.Integers.OnCounter", &[("y_start", I(5))]);
    let mut region = vec![0u64; block.state_len()];
    block.init_state(&mut region, &ParamTable::default());
    // Frame 0: nothing. Frame 1: trigger fires in the first iteration (count 6).
    frame(
        block.as_ref(),
        &mut region,
        0.0,
        true,
        &[B(false), B(false)],
    );
    let out = frame(
        block.as_ref(),
        &mut region,
        1.0,
        false,
        &[B(true), B(false), B(false), B(false), B(false), B(false)],
    );
    assert_eq!(real(&out[0]), 6.0);
    // Frame 2: the trigger pulses in the first iteration (`pre(y) + 1` with reset false there),
    // then `reset = Not(pulse)` rises in the second (reset true): 7, then `y_start`.
    let out = frame(
        block.as_ref(),
        &mut region,
        2.0,
        false,
        &[B(true), B(false), B(false), B(false), B(true), B(true)],
    );
    assert_eq!(real(&out[0]), 5.0);
    // Frame 3: the reset level held (`w = s = v = true`): no event, the count stays.
    let out = frame(
        block.as_ref(),
        &mut region,
        3.0,
        false,
        &[B(false), B(true), B(false), B(false), B(true), B(true)],
    );
    assert_eq!(real(&out[0]), 5.0);
}

#[test]
fn moving_mean_samples_twice_at_the_initial_frame_on_a_sample_instant_only() {
    use Value::{Boolean as B, Integer as I, Real as R};
    let block = make("CDL.Discrete.TriggeredMovingMean", &[("n", I(3))]);
    // A sample instant at the start (`i = w = false`, `s = true`): `initial()` and the rise, then
    // one sample of 3.5 at frame 1: (2.5 + 2.5 + 3.5) / 3, as OpenModelica gives.
    let mut region = vec![0u64; block.state_len()];
    block.init_state(&mut region, &ParamTable::default());
    frame(
        block.as_ref(),
        &mut region,
        0.0,
        true,
        &[R(2.5), B(false), B(false), B(true)],
    );
    let out = frame(
        block.as_ref(),
        &mut region,
        1.0,
        false,
        &[R(3.5), B(true), B(false), B(true)],
    );
    assert_eq!(real(&out[0]), (2.5 + 2.5 + 3.5) / 3.0);
    // A `Change` true from the start (its initialisation pulse: `i = true`, `w = s = false`) and
    // an off-grid start (the sample leaf false at frame 0) sample once: (2.5 + 3.5) / 2.
    for frame0 in [
        [R(2.5), B(true), B(false), B(false)],
        [R(2.5), B(false), B(false), B(false)],
    ] {
        let mut region = vec![0u64; block.state_len()];
        block.init_state(&mut region, &ParamTable::default());
        frame(block.as_ref(), &mut region, 0.0, true, &frame0);
        let out = frame(
            block.as_ref(),
            &mut region,
            1.0,
            false,
            &[R(3.5), B(true), B(false), B(true)],
        );
        assert_eq!(real(&out[0]), (2.5 + 3.5) / 2.0, "{frame0:?}");
    }
}

#[test]
fn on_counter_stores_reset_s_w_and_a_second_consecutive_reset_pulse_fires() {
    use Value::{Boolean as B, Integer as I};
    let block = make("CDL.Integers.OnCounter", &[("y_start", I(5))]);
    let mut region = vec![0u64; block.state_len()];
    block.init_state(&mut region, &ParamTable::default());
    // Inputs: trigger, reset, then trigger's (w, s) and reset's (w, s).
    frame(
        block.as_ref(),
        &mut region,
        0.0,
        true,
        &[B(false), B(false)],
    );
    // Frame 1: a reset pulse fires in the first iteration (5), and `trigger = Not(pulse)` rises
    // in the second (reset settled false): 6.
    let out = frame(
        block.as_ref(),
        &mut region,
        1.0,
        false,
        &[B(false), B(true), B(true), B(true), B(false), B(false)],
    );
    assert_eq!(real(&out[0]), 6.0);
    // Frame 2: the reset pulses again on the next frame (its stored `w` is false): back to 5.
    let out = frame(
        block.as_ref(),
        &mut region,
        2.0,
        false,
        &[B(true), B(true), B(true), B(true), B(false), B(false)],
    );
    assert_eq!(real(&out[0]), 5.0);
}

/// `Latch.mo` and `Toggle.mo` at a later frame, written from their text: the inputs go
/// `before -> v -> s` (`u`'s before is `prev_u`, `clr`'s is `prev_clr`), and in each step the
/// `when {clr, u}` of `Latch` (`y = not clr and u` when either rises) or the `when`/`elsewhen`
/// chain of `Toggle` (`u` rising with `clr` false toggles; else `clr` rising clears; a falling
/// `u` keeps `pre(y)`) runs on that step's values.
fn modelica_latch_toggle(toggle: bool, u: [bool; 3], clr: [bool; 3], held: bool) -> bool {
    let mut y = held;
    for k in 1..3 {
        let (u_rose, clr_rose) = (u[k] && !u[k - 1], clr[k] && !clr[k - 1]);
        if toggle {
            if !clr[k] && u_rose {
                y = !y;
            } else if clr_rose {
                y = false;
            }
        } else if u_rose || clr_rose {
            y = !clr[k] && u[k];
        }
    }
    y
}

#[test]
fn latch_and_toggle_follow_their_mo_in_every_row_but_the_registered_negated_sample_clear() {
    use Value::Boolean as B;
    let bits = |n: u32, i: u32| n >> i & 1 == 1;
    for class in ["CDL.Logical.Latch", "CDL.Logical.Toggle"] {
        let toggle = class.ends_with("Toggle");
        let block = make(class, &[]);
        let (mut differ, mut rows) = (0, 0);
        // (u: prev, v, s) x (clr: prev, v, s) x held: 128 rows.
        for n in 0..128u32 {
            let (u, clr, held) = (
                [bits(n, 0), bits(n, 1), bits(n, 2)],
                [bits(n, 3), bits(n, 4), bits(n, 5)],
                bits(n, 6),
            );
            let mut region = vec![u64::from(held), u64::from(u[0])];
            let inputs = [B(u[1]), B(clr[1]), B(false), B(u[2]), B(false), B(clr[2])];
            let out = frame(block.as_ref(), &mut region, 1.0, false, &inputs);
            let got = real(&out[0]) == 1.0;
            let want = modelica_latch_toggle(toggle, u, clr, held);
            rows += 1;
            // The engine stores no previous `clr`: it reads `clr` true at the frame as a rise.
            // That differs only where the output is set while `clr` was already true before the
            // frame, which needs a negated sample in `clr`'s cone (DVG-000008).
            if clr[0] && held && clr[1] {
                let approx = modelica_latch_toggle(toggle, u, [false, clr[1], clr[2]], held);
                assert_eq!(got, approx, "{class} row {n}");
                differ += usize::from(got != want);
            } else {
                assert_eq!(got, want, "{class}: u {u:?}, clr {clr:?}, held {held}");
            }
        }
        assert_eq!(rows, 128);
        // Of the 16 rows where the two can differ (the output set, `clr` already true before
        // the frame and true at it), `Toggle` differs in all; `Latch` in the 8 where `u` does
        // not rise in the first step and nothing rises in the second (any such rise evaluates
        // `not clr and u` again, the same in both).
        assert_eq!(differ, if toggle { 16 } else { 8 }, "{class}");
    }
}
