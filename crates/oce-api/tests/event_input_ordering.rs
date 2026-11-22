//! An event input produced by a block declared AFTER its reader is still read in the frame it is
//! produced (issue #768).
//!
//! Event inputs feed through, so the schedule places their producers first whatever the
//! declaration order. Each case below declares the producers after the block under test and makes
//! the produced value change in the event frame, so a block that read last frame's value (a
//! dropped feedthrough declaration, a schedule in declaration order) emits a different number.
//! Producers are `CivilTime` (y = t) and a threshold or gain on it; frames at t = 0, 1, 2, ... s.

use oce_api::Value;

use crate::event_documents::{Blk, Doc, run};

fn civil_time() -> Blk {
    Blk {
        name: "clock",
        class: "Reals.Sources.CivilTime",
        params: vec![],
        ins: vec![],
        outs: vec![("y", "Real")],
    }
}

fn threshold(name: &'static str, t: &'static str) -> Blk {
    Blk {
        name,
        class: "Reals.GreaterThreshold",
        params: vec![("t", "Real", t)],
        ins: vec![("u", "Real")],
        outs: vec![("y", "Boolean")],
    }
}

fn reals(values: &[f64]) -> Vec<Value> {
    values.iter().map(|&v| Value::Real(v)).collect()
}

fn assert_bits(got: &[Value], want: &[Value]) {
    assert_eq!(got.len(), want.len());
    for (frame, (g, w)) in got.iter().zip(want).enumerate() {
        assert!(
            g.bit_eq(w),
            "frame {frame}: got {g:?}, want {w:?} in {got:?}"
        );
    }
}

fn frames(n: usize) -> Vec<Vec<(&'static str, Value)>> {
    (0..n).map(|_| Vec::new()).collect()
}

#[test]
fn integrator_reset_uses_the_late_declared_y_reset_in_of_its_own_frame() {
    // u = 1 (a constant declared after), y_reset_in = 10 * t, trigger = t > 1.5: the reset at
    // t = 2 must take 20 (this frame's value), not 10 (last frame's). Hand-derived: 0, 0, 20,
    // 20, 21.
    let doc = Doc {
        blocks: vec![
            Blk {
                name: "i",
                class: "Reals.IntegratorWithReset",
                params: vec![("k", "Real", "1"), ("y_start", "Real", "0")],
                ins: vec![
                    ("u", "Real"),
                    ("y_reset_in", "Real"),
                    ("trigger", "Boolean"),
                ],
                outs: vec![("y", "Real")],
            },
            civil_time(),
            Blk {
                name: "gain",
                class: "Reals.MultiplyByParameter",
                params: vec![("k", "Real", "10")],
                ins: vec![("u", "Real")],
                outs: vec![("y", "Real")],
            },
            threshold("thr", "1.5"),
            Blk {
                name: "one",
                class: "Reals.Sources.Constant",
                params: vec![("k", "Real", "1")],
                ins: vec![],
                outs: vec![("y", "Real")],
            },
        ],
        outputs: vec![("y", "Real")],
        wires: vec![
            ("i.y", "y"),
            ("clock.y", "gain.u"),
            ("clock.y", "thr.u"),
            ("gain.y", "i.y_reset_in"),
            ("thr.y", "i.trigger"),
            ("one.y", "i.u"),
        ],
        ..Doc::default()
    };
    assert_bits(
        &run(&doc, &frames(5)),
        &reals(&[0.0, 0.0, 20.0, 20.0, 21.0]),
    );
}

#[test]
fn reset_pid_resets_at_the_frame_its_late_declared_trigger_rises() {
    // PI k = Ti = 1, y_reset = 5, u_s = 1 and u_m = 0 from constants declared after, trigger
    // t > 1.5 declared after: 1, 1, 5, 5, 6. A reader of last frame's trigger gives 1, 1, 2, 5, 6.
    let constant = |name: &'static str, k: &'static str| Blk {
        name,
        class: "Reals.Sources.Constant",
        params: vec![("k", "Real", k)],
        ins: vec![],
        outs: vec![("y", "Real")],
    };
    for mode in ["PI", "PID"] {
        let doc = Doc {
            blocks: vec![
                Blk {
                    name: "pid",
                    class: "Reals.PIDWithReset",
                    params: vec![
                        ("controllerType", "SimpleController", mode),
                        ("k", "Real", "1"),
                        ("Ti", "Real", "1"),
                        ("Td", "Real", "0.5"),
                        ("y_reset", "Real", "5"),
                        ("yMax", "Real", "100"),
                        ("yMin", "Real", "-100"),
                    ],
                    ins: vec![("u_s", "Real"), ("u_m", "Real"), ("trigger", "Boolean")],
                    outs: vec![("y", "Real")],
                },
                civil_time(),
                threshold("thr", "1.5"),
                constant("set", "1"),
                constant("meas", "0"),
            ],
            outputs: vec![("y", "Real")],
            wires: vec![
                ("pid.y", "y"),
                ("clock.y", "thr.u"),
                ("thr.y", "pid.trigger"),
                ("set.y", "pid.u_s"),
                ("meas.y", "pid.u_m"),
            ],
            ..Doc::default()
        };
        assert_bits(&run(&doc, &frames(5)), &reals(&[1.0, 1.0, 5.0, 5.0, 6.0]));
    }
}

#[test]
fn counter_counts_and_resets_at_the_frames_its_late_declared_inputs_rise() {
    // trigger = t > 0.5 (rises at 1), reset = t > 2.5 (rises at 3): 0, 1, 1, 0, 0. A reader of
    // last frame's inputs gives 0, 0, 1, 1, 0.
    let doc = Doc {
        blocks: vec![
            Blk {
                name: "c",
                class: "Integers.OnCounter",
                params: vec![("y_start", "Integer", "0")],
                ins: vec![("trigger", "Boolean"), ("reset", "Boolean")],
                outs: vec![("y", "Integer")],
            },
            civil_time(),
            threshold("trg", "0.5"),
            threshold("rst", "2.5"),
        ],
        outputs: vec![("y", "Integer")],
        wires: vec![
            ("c.y", "y"),
            ("clock.y", "trg.u"),
            ("clock.y", "rst.u"),
            ("trg.y", "c.trigger"),
            ("rst.y", "c.reset"),
        ],
        ..Doc::default()
    };
    let want: Vec<Value> = [0, 1, 1, 0, 0].into_iter().map(Value::Integer).collect();
    assert_bits(&run(&doc, &frames(5)), &want);
}

#[test]
fn triggered_sampler_and_max_sample_their_late_declared_inputs_in_the_rising_frame() {
    // u = 10 * t and trigger = t > 1.5, both declared after: the sampler emits y_start = 7 until
    // the rise at t = 2, then 20 (this frame's u); TriggeredMax emits u at frame 0 (0), holds it,
    // and takes max(0, |20|) = 20 at t = 2.
    for (class, params, want) in [
        (
            "Discrete.TriggeredSampler",
            vec![("y_start", "Real", "7")],
            [7.0, 7.0, 20.0, 20.0],
        ),
        ("Discrete.TriggeredMax", vec![], [0.0, 0.0, 20.0, 20.0]),
    ] {
        let doc = Doc {
            blocks: vec![
                Blk {
                    name: "s",
                    class,
                    params,
                    ins: vec![("u", "Real"), ("trigger", "Boolean")],
                    outs: vec![("y", "Real")],
                },
                civil_time(),
                Blk {
                    name: "gain",
                    class: "Reals.MultiplyByParameter",
                    params: vec![("k", "Real", "10")],
                    ins: vec![("u", "Real")],
                    outs: vec![("y", "Real")],
                },
                threshold("thr", "1.5"),
            ],
            outputs: vec![("y", "Real")],
            wires: vec![
                ("s.y", "y"),
                ("clock.y", "gain.u"),
                ("clock.y", "thr.u"),
                ("gain.y", "s.u"),
                ("thr.y", "s.trigger"),
            ],
            ..Doc::default()
        };
        assert_bits(&run(&doc, &frames(4)), &reals(&want));
    }
}
