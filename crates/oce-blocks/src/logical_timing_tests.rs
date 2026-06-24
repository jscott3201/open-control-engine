use oce_model::{ParamTable, Value};

use super::{
    Block, BlockKind, Ctx, FallingEdge, IntegerChange, Latch, LogicalChange, NoopDiagnostics,
    OnCounter, Timer, TimerAccumulating, Toggle, TrueDelay, TrueFalseHold, TrueHoldWithReset,
    lookup,
};

fn init_region(block: &dyn Block) -> Vec<u64> {
    let mut region = vec![0u64; block.state_len()];
    block.init_state(&mut region, &ParamTable::default());
    region
}

fn emit_at(block: &dyn Block, region: &[u64], t: f64, inputs: &[Value]) -> Vec<Value> {
    let diag = NoopDiagnostics;
    let cx = Ctx::new(t, &diag);
    let mut out = Vec::new();
    block.emit_from_state(&cx, inputs, region, &mut |idx, val| {
        assert_eq!(idx, out.len(), "outputs must be emitted in port order");
        out.push(val);
    });
    out
}

fn tick_at(block: &dyn Block, region: &mut [u64], t: f64, inputs: Vec<Value>) -> Vec<Value> {
    let diag = NoopDiagnostics;
    let cx = Ctx::new(t, &diag);
    let mut out = Vec::new();
    block.emit_from_state(&cx, &inputs, region, &mut |idx, val| {
        assert_eq!(idx, out.len(), "outputs must be emitted in port order");
        out.push(val);
    });
    block.update_state(&cx, &inputs, region);
    out
}

fn run(block: &dyn Block, steps: &[(f64, Vec<Value>)]) -> (Vec<Vec<Value>>, Vec<u64>) {
    let mut region = init_region(block);
    let trace = steps
        .iter()
        .map(|(t, inputs)| tick_at(block, &mut region, *t, inputs.clone()))
        .collect();
    (trace, region)
}

fn assert_bool(value: &Value, want: bool) {
    assert!(
        value.bit_eq(&Value::Boolean(want)),
        "got {value:?}, want {want}"
    );
}

fn assert_real_bits(value: &Value, want_bits: u64) {
    let Value::Real(got) = value else {
        panic!("expected Real, got {value:?}");
    };
    assert_eq!(got.to_bits(), want_bits, "got {got:?}");
}
#[test]
fn timer_goldens_pin_threshold_and_non_dyadic_dt() {
    let timer = Timer { t: 1.0 };
    let (trace, _) = run(
        &timer,
        &[
            (0.0, vec![Value::Boolean(true)]),
            (0.5, vec![Value::Boolean(true)]),
            (1.0, vec![Value::Boolean(true)]),
            (1.0, vec![Value::Boolean(true)]),
            (1.5, vec![Value::Boolean(false)]),
            (2.0, vec![Value::Boolean(true)]),
            (2.5, vec![Value::Boolean(true)]),
        ],
    );
    let expected = [
        (0x0000_0000_0000_0000, false),
        (0x3fe0_0000_0000_0000, false),
        (0x3ff0_0000_0000_0000, true),
        (0x3ff0_0000_0000_0000, true),
        (0x0000_0000_0000_0000, false),
        (0x0000_0000_0000_0000, false),
        (0x3fe0_0000_0000_0000, false),
    ];
    for (idx, (got, (bits, passed))) in trace.iter().zip(expected).enumerate() {
        assert_real_bits(&got[0], bits);
        assert_bool(&got[1], passed);
        if idx == 2 {
            assert_bool(&got[1], true);
        }
    }

    let timer = Timer { t: 1.0 };
    let (trace, state) = run(
        &timer,
        &[
            (0.2, vec![Value::Boolean(true)]),
            (0.3, vec![Value::Boolean(true)]),
            (0.4, vec![Value::Boolean(true)]),
            (0.5, vec![Value::Boolean(true)]),
            (0.6, vec![Value::Boolean(true)]),
            (0.7, vec![Value::Boolean(true)]),
            (0.8, vec![Value::Boolean(true)]),
            (0.9, vec![Value::Boolean(true)]),
        ],
    );
    let expected = [
        0x0000_0000_0000_0000,
        0x3fb9_9999_9999_9998,
        0x3fc9_9999_9999_999a,
        0x3fd3_3333_3333_3333,
        0x3fd9_9999_9999_9999,
        0x3fdf_ffff_ffff_ffff,
        0x3fe3_3333_3333_3334,
        0x3fe6_6666_6666_6666,
    ];
    for (got, bits) in trace.iter().zip(expected) {
        assert_real_bits(&got[0], bits);
    }
    assert_eq!(
        state[0], 0x3fc9_9999_9999_999a,
        "Timer stores entryTime, not a running accumulator"
    );

    let acc = TimerAccumulating { t: 0.3 };
    let (trace, _) = run(
        &acc,
        &[
            (0.0, vec![Value::Boolean(true), Value::Boolean(false)]),
            (0.1, vec![Value::Boolean(true), Value::Boolean(false)]),
            (0.3, vec![Value::Boolean(true), Value::Boolean(false)]),
            (0.6, vec![Value::Boolean(true), Value::Boolean(false)]),
            (0.6, vec![Value::Boolean(true), Value::Boolean(false)]),
            (0.7, vec![Value::Boolean(false), Value::Boolean(false)]),
            (0.8, vec![Value::Boolean(true), Value::Boolean(true)]),
            (0.9, vec![Value::Boolean(true), Value::Boolean(true)]),
        ],
    );
    let expected = [
        (0x0000_0000_0000_0000, false),
        (0x3fb9_9999_9999_999a, false),
        (0x3fd3_3333_3333_3333, true),
        (0x3fe3_3333_3333_3333, true),
        (0x3fe3_3333_3333_3333, true),
        (0x3fe3_3333_3333_3333, true),
        (0x0000_0000_0000_0000, false),
        // 0.9 - 0.8 rounds below decimal 0.1; this pins real tick_dt FP residue.
        (0x3fb9_9999_9999_9998, false),
    ];
    for (got, (bits, passed)) in trace.iter().zip(expected) {
        assert_real_bits(&got[0], bits);
        assert_bool(&got[1], passed);
    }
}

#[test]
fn timer_threshold_zero_passed_is_false_while_idle() {
    let timer = Timer { t: 0.0 };
    let (trace, _) = run(
        &timer,
        &[
            (0.0, vec![Value::Boolean(true)]),
            (0.1, vec![Value::Boolean(false)]),
            (0.2, vec![Value::Boolean(false)]),
            (0.3, vec![Value::Boolean(true)]),
            (0.4, vec![Value::Boolean(false)]),
        ],
    );
    let expected = [
        (0x0000_0000_0000_0000, true),
        (0x0000_0000_0000_0000, false),
        (0x0000_0000_0000_0000, false),
        (0x0000_0000_0000_0000, true),
        (0x0000_0000_0000_0000, false),
    ];
    for (got, (bits, passed)) in trace.iter().zip(expected) {
        assert_real_bits(&got[0], bits);
        assert_bool(&got[1], passed);
    }
}

#[test]
fn timer_accumulating_passed_latch_holds_and_reset_sets_threshold_value() {
    let acc = TimerAccumulating { t: 0.0 };
    let (trace, _) = run(
        &acc,
        &[
            (0.0, vec![Value::Boolean(false), Value::Boolean(false)]),
            (0.1, vec![Value::Boolean(true), Value::Boolean(false)]),
            (0.2, vec![Value::Boolean(false), Value::Boolean(false)]),
            (0.3, vec![Value::Boolean(false), Value::Boolean(true)]),
            (0.4, vec![Value::Boolean(false), Value::Boolean(false)]),
        ],
    );
    for got in &trace {
        assert_real_bits(&got[0], 0x0000_0000_0000_0000);
        assert_bool(&got[1], true);
    }

    let acc = TimerAccumulating { t: 0.5 };
    let (trace, _) = run(
        &acc,
        &[
            (0.0, vec![Value::Boolean(false), Value::Boolean(true)]),
            (0.25, vec![Value::Boolean(true), Value::Boolean(false)]),
            (0.75, vec![Value::Boolean(true), Value::Boolean(false)]),
            (1.0, vec![Value::Boolean(false), Value::Boolean(false)]),
            (1.25, vec![Value::Boolean(false), Value::Boolean(false)]),
            (1.5, vec![Value::Boolean(false), Value::Boolean(true)]),
            (1.75, vec![Value::Boolean(false), Value::Boolean(false)]),
        ],
    );
    let expected = [
        (0x0000_0000_0000_0000, false),
        (0x0000_0000_0000_0000, false),
        (0x3fe0_0000_0000_0000, true),
        (0x3fe0_0000_0000_0000, true),
        (0x3fe0_0000_0000_0000, true),
        (0x0000_0000_0000_0000, false),
        (0x0000_0000_0000_0000, false),
    ];
    for (got, (bits, passed)) in trace.iter().zip(expected) {
        assert_real_bits(&got[0], bits);
        assert_bool(&got[1], passed);
    }
}

#[test]
fn timer_accumulating_passed_hold_is_stateful_not_running_gated() {
    let acc = TimerAccumulating { t: 1.0 };
    let held_passed_region = [0x3fe0_0000_0000_0000, 0.0f64.to_bits(), 0, 0, 1];
    let got = emit_at(
        &acc,
        &held_passed_region,
        1.0,
        &[Value::Boolean(false), Value::Boolean(false)],
    );

    assert_real_bits(&got[0], 0x3fe0_0000_0000_0000);
    assert_bool(&got[1], true);
}

#[test]
fn hold_delay_goldens_pin_boundaries_and_clear_priority() {
    let delay = TrueDelay {
        delay_time: 1.0,
        delay_on_init: true,
    };
    let (trace, _) = run(
        &delay,
        &[
            (0.0, vec![Value::Boolean(true)]),
            (0.5, vec![Value::Boolean(true)]),
            (1.0, vec![Value::Boolean(true)]),
            (1.5, vec![Value::Boolean(false)]),
            (1.5, vec![Value::Boolean(true)]),
        ],
    );
    for (got, want) in trace.iter().zip([false, false, true, false, false]) {
        assert_bool(&got[0], want);
    }

    let no_init_delay = TrueDelay {
        delay_time: 1.0,
        delay_on_init: false,
    };
    let (trace, _) = run(&no_init_delay, &[(0.0, vec![Value::Boolean(true)])]);
    assert_bool(&trace[0][0], true);

    let hold = TrueFalseHold {
        true_hold_duration: 1.0,
        false_hold_duration: 0.5,
    };
    let (trace, _) = run(
        &hold,
        &[
            (0.0, vec![Value::Boolean(true)]),
            (0.2, vec![Value::Boolean(false)]),
            (1.0, vec![Value::Boolean(false)]),
            (1.2, vec![Value::Boolean(true)]),
            (1.5, vec![Value::Boolean(true)]),
            (1.5, vec![Value::Boolean(false)]),
        ],
    );
    for (got, want) in trace.iter().zip([true, true, false, false, true, true]) {
        assert_bool(&got[0], want);
    }

    let reset_hold = TrueHoldWithReset { duration: 1.0 };
    let (trace, _) = run(
        &reset_hold,
        &[
            (0.0, vec![Value::Boolean(true), Value::Boolean(false)]),
            (0.2, vec![Value::Boolean(false), Value::Boolean(false)]),
            (1.0, vec![Value::Boolean(false), Value::Boolean(false)]),
            (1.1, vec![Value::Boolean(true), Value::Boolean(true)]),
            (1.2, vec![Value::Boolean(true), Value::Boolean(false)]),
        ],
    );
    for (got, want) in trace.iter().zip([true, true, false, false, true]) {
        assert_bool(&got[0], want);
    }
}

#[test]
fn timing_duration_floor_canonicalizes_negative_zero_to_positive_zero() {
    let delay = TrueDelay {
        delay_time: -0.0,
        delay_on_init: true,
    };
    let mut region = init_region(&delay);
    let trace = tick_at(&delay, &mut region, 0.0, vec![Value::Boolean(true)]);

    assert_bool(&trace[0], true);
    assert_eq!(region[0], 0.0f64.to_bits());
}
#[test]
fn timing_latch_feedthrough_perturbations_pin_current_input_surface() {
    let one = 1.0f64.to_bits();
    let half = 0.5f64.to_bits();
    let zero = 0.0f64.to_bits();

    assert_bool(
        &emit_at(&FallingEdge::default(), &[1], 0.0, &[Value::Boolean(false)])[0],
        true,
    );
    assert_bool(
        &emit_at(
            &LogicalChange::default(),
            &[0],
            0.0,
            &[Value::Boolean(true)],
        )[0],
        true,
    );
    assert_bool(
        &emit_at(
            &Latch,
            &[0, 0],
            0.0,
            &[Value::Boolean(true), Value::Boolean(false)],
        )[0],
        true,
    );
    assert_bool(
        &emit_at(
            &Toggle,
            &[0, 0],
            0.0,
            &[Value::Boolean(true), Value::Boolean(false)],
        )[0],
        true,
    );

    let timer = Timer { t: 1.0 };
    assert_real_bits(
        &emit_at(&timer, &[half, zero, 1], 1.0, &[Value::Boolean(true)])[0],
        0x3fe0_0000_0000_0000,
    );
    assert_real_bits(
        &emit_at(&timer, &[half, zero, 1], 1.0, &[Value::Boolean(false)])[0],
        zero,
    );

    let acc = TimerAccumulating { t: 1.0 };
    assert_real_bits(
        &emit_at(
            &acc,
            &[half, zero, 1, 0, 0],
            1.0,
            &[Value::Boolean(true), Value::Boolean(false)],
        )[0],
        0x3ff8_0000_0000_0000,
    );
    assert_real_bits(
        &emit_at(
            &acc,
            &[half, zero, 1, 0, 0],
            1.0,
            &[Value::Boolean(true), Value::Boolean(true)],
        )[0],
        zero,
    );

    let delay = TrueDelay {
        delay_time: 1.0,
        delay_on_init: true,
    };
    assert_bool(
        &emit_at(&delay, &[half, zero, 1, 0], 1.0, &[Value::Boolean(true)])[0],
        true,
    );
    assert_bool(
        &emit_at(&delay, &[half, zero, 1, 0], 1.0, &[Value::Boolean(false)])[0],
        false,
    );

    let hold = TrueFalseHold {
        true_hold_duration: 1.0,
        false_hold_duration: 1.0,
    };
    assert_bool(
        &emit_at(&hold, &[0, one, zero], 1.0, &[Value::Boolean(true)])[0],
        true,
    );
    assert_bool(
        &emit_at(&hold, &[0, one, zero], 1.0, &[Value::Boolean(false)])[0],
        false,
    );

    let reset_hold = TrueHoldWithReset { duration: 1.0 };
    assert_bool(
        &emit_at(
            &reset_hold,
            &[1, half, zero],
            0.25,
            &[Value::Boolean(false), Value::Boolean(false)],
        )[0],
        true,
    );
    assert_bool(
        &emit_at(
            &reset_hold,
            &[1, half, zero],
            0.25,
            &[Value::Boolean(false), Value::Boolean(true)],
        )[0],
        false,
    );

    let int_change = IntegerChange::default();
    let changed = emit_at(
        &int_change,
        &[1i64.cast_unsigned()],
        0.0,
        &[Value::Integer(2)],
    );
    assert_bool(&changed[0], true);
    assert_bool(&changed[1], true);
    assert_bool(&changed[2], false);

    let counter = OnCounter { y_start: 5 };
    let no_trigger = emit_at(
        &counter,
        &[5i64.cast_unsigned(), 0, 0, 1],
        0.0,
        &[Value::Boolean(false), Value::Boolean(false)],
    );
    let trigger = emit_at(
        &counter,
        &[5i64.cast_unsigned(), 0, 0, 1],
        0.0,
        &[Value::Boolean(true), Value::Boolean(false)],
    );
    assert!(
        no_trigger[0].bit_eq(&trigger[0]),
        "OnCounter trigger must not feed through to same-tick y"
    );
}

#[test]
fn timing_latch_and_counter_blocks_are_deterministic_over_output_and_full_state_region() {
    type Step = (f64, Vec<Value>);
    type Case = (&'static str, Box<dyn Block>, Vec<Step>);

    let cases: Vec<Case> = vec![
        (
            "FallingEdge",
            Box::new(FallingEdge::default()),
            vec![
                (0.0, vec![Value::Boolean(false)]),
                (0.0, vec![Value::Boolean(true)]),
                (0.0, vec![Value::Boolean(false)]),
            ],
        ),
        (
            "LogicalChange",
            Box::new(LogicalChange::default()),
            vec![
                (0.0, vec![Value::Boolean(false)]),
                (0.0, vec![Value::Boolean(true)]),
            ],
        ),
        (
            "Latch",
            Box::new(Latch),
            vec![
                (0.0, vec![Value::Boolean(true), Value::Boolean(false)]),
                (0.0, vec![Value::Boolean(false), Value::Boolean(false)]),
            ],
        ),
        (
            "Toggle",
            Box::new(Toggle),
            vec![
                (0.0, vec![Value::Boolean(true), Value::Boolean(false)]),
                (0.0, vec![Value::Boolean(false), Value::Boolean(false)]),
                (0.0, vec![Value::Boolean(true), Value::Boolean(false)]),
            ],
        ),
        (
            "Timer",
            Box::new(Timer { t: 0.3 }),
            vec![
                (0.0, vec![Value::Boolean(true)]),
                (0.1, vec![Value::Boolean(true)]),
                (0.3, vec![Value::Boolean(true)]),
            ],
        ),
        (
            "TimerAccumulating",
            Box::new(TimerAccumulating { t: 0.3 }),
            vec![
                (0.0, vec![Value::Boolean(true), Value::Boolean(false)]),
                (0.1, vec![Value::Boolean(true), Value::Boolean(false)]),
                (0.3, vec![Value::Boolean(false), Value::Boolean(false)]),
            ],
        ),
        (
            "TrueDelay",
            Box::new(TrueDelay {
                delay_time: 0.5,
                delay_on_init: true,
            }),
            vec![
                (0.0, vec![Value::Boolean(true)]),
                (0.25, vec![Value::Boolean(true)]),
                (0.5, vec![Value::Boolean(true)]),
            ],
        ),
        (
            "TrueFalseHold",
            Box::new(TrueFalseHold {
                true_hold_duration: 0.3,
                false_hold_duration: 0.2,
            }),
            vec![
                (0.0, vec![Value::Boolean(true)]),
                (0.1, vec![Value::Boolean(false)]),
                (0.3, vec![Value::Boolean(false)]),
                (0.5, vec![Value::Boolean(true)]),
            ],
        ),
        (
            "TrueHoldWithReset",
            Box::new(TrueHoldWithReset { duration: 0.3 }),
            vec![
                (0.0, vec![Value::Boolean(true), Value::Boolean(false)]),
                (0.1, vec![Value::Boolean(false), Value::Boolean(false)]),
                (0.3, vec![Value::Boolean(false), Value::Boolean(false)]),
            ],
        ),
        (
            "OnCounter",
            Box::new(OnCounter { y_start: 2 }),
            vec![
                (0.0, vec![Value::Boolean(true), Value::Boolean(false)]),
                (1.0, vec![Value::Boolean(false), Value::Boolean(false)]),
                (2.0, vec![Value::Boolean(true), Value::Boolean(true)]),
            ],
        ),
        (
            "IntegerChange",
            Box::new(IntegerChange { pre_u_start: 4 }),
            vec![
                (0.0, vec![Value::Integer(4)]),
                (0.0, vec![Value::Integer(6)]),
                (0.0, vec![Value::Integer(3)]),
            ],
        ),
    ];

    for (name, block, steps) in cases {
        let (trace_a, state_a) = run(block.as_ref(), &steps);
        let (trace_b, state_b) = run(block.as_ref(), &steps);
        assert_eq!(state_a, state_b, "{name} full state region drifted");
        for (tick, (outs_a, outs_b)) in trace_a.iter().zip(&trace_b).enumerate() {
            assert_eq!(outs_a.len(), outs_b.len(), "{name} output arity drifted");
            for (port, (a, b)) in outs_a.iter().zip(outs_b).enumerate() {
                assert!(
                    a.bit_eq(b),
                    "{name} tick {tick} port {port}: {a:?} vs {b:?}"
                );
            }
        }
    }
}

#[test]
fn registry_paths_and_feedthrough_classification_are_complete() {
    let paths = [
        "CDL.Logical.FallingEdge",
        "CDL.Logical.Change",
        "CDL.Logical.Latch",
        "CDL.Logical.Toggle",
        "CDL.Logical.Timer",
        "CDL.Logical.TimerAccumulating",
        "CDL.Logical.TrueDelay",
        "CDL.Logical.TrueFalseHold",
        "CDL.Logical.TrueHoldWithReset",
        "CDL.Integers.OnCounter",
        "CDL.Integers.Change",
    ];
    for path in paths {
        let block = (lookup(path)
            .unwrap_or_else(|| panic!("missing {path}"))
            .make)(&ParamTable::default());
        assert_eq!(block.signature().class_path, path);
        assert_eq!(block.kind(), BlockKind::Stateful, "{path}");
    }

    assert!(FallingEdge::default().feeds_through(0, 0));
    assert!(LogicalChange::default().feeds_through(0, 0));
    assert!(Latch.feeds_through(0, 0) && Latch.feeds_through(1, 0));
    assert!(Toggle.feeds_through(0, 0) && Toggle.feeds_through(1, 0));
    assert!(Timer::default().feeds_through(0, 0) && Timer::default().feeds_through(0, 1));
    assert!(
        TimerAccumulating::default().feeds_through(0, 0)
            && TimerAccumulating::default().feeds_through(1, 0)
            && TimerAccumulating::default().feeds_through(0, 1)
            && TimerAccumulating::default().feeds_through(1, 1)
    );
    assert!(TrueDelay::default().feeds_through(0, 0));
    assert!(TrueFalseHold::default().feeds_through(0, 0));
    assert!(
        TrueHoldWithReset::default().feeds_through(0, 0)
            && TrueHoldWithReset::default().feeds_through(1, 0)
    );
    assert!(
        IntegerChange::default().feeds_through(0, 0)
            && IntegerChange::default().feeds_through(0, 1)
            && IntegerChange::default().feeds_through(0, 2)
    );
    assert!(!OnCounter::default().feeds_through(0, 0));
    assert!(!OnCounter::default().feeds_through(1, 0));
}
