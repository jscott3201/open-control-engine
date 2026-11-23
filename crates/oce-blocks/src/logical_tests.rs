//! Exact algebraic tests for `CDL.Logical` combinational blocks.
//! Expected values are derived directly from `_spec/03` §4.2–§4.4 and compared bit-exactly.

use std::sync::Arc;

use oce_model::{ParamTable, Value};

use super::{
    Block, BlockKind, BooleanToInteger, BooleanToReal, Ctx, IntegerAbs, IntegerAdd,
    IntegerAddParameter, IntegerConstant, IntegerEqual, IntegerGreater, IntegerGreaterEqual,
    IntegerGreaterEqualThreshold, IntegerGreaterThreshold, IntegerLess, IntegerLessEqual,
    IntegerLessEqualThreshold, IntegerLessThreshold, IntegerMax, IntegerMin, IntegerMultiply,
    IntegerSubtract, IntegerSwitch, IntegerToReal, LogicalConstant, LogicalSwitch, MultiAnd,
    MultiOr, Nand, NoopDiagnostics, Nor, Or, RealToInteger, Xor, lookup,
};

fn out(block: &dyn Block, inputs: &[Value]) -> Value {
    let diag = NoopDiagnostics;
    let cx = Ctx::new(0.0, &diag);
    let mut out = None;
    block.step_algebraic(&cx, inputs, &mut |idx, val| {
        assert_eq!(idx, 0, "algebraic blocks emit one output");
        out = Some(val);
    });
    out.expect("algebraic block must emit one output")
}

fn bool_out(block: &dyn Block, inputs: &[Value]) -> bool {
    match out(block, inputs) {
        Value::Boolean(y) => y,
        other => panic!("expected Boolean output, got {other:?}"),
    }
}

fn b(v: bool) -> Value {
    Value::Boolean(v)
}

fn i(v: i64) -> Value {
    Value::Integer(v)
}

fn r(v: f64) -> Value {
    Value::Real(v)
}

fn assert_one_out(block: &dyn Block, inputs: &[Value], want: Value) {
    let got = out(block, inputs);
    assert!(got.bit_eq(&want), "got {got:?}, want {want:?}");
}

fn assert_perturb_moves(block: &dyn Block, base: &[Value], variants: &[(usize, Value)]) {
    let base_y = out(block, base);
    for (idx, replacement) in variants {
        assert!(
            block.feeds_through(*idx, 0),
            "input {idx} must feed output 0"
        );
        let mut changed = base.to_vec();
        changed[*idx] = replacement.clone();
        let y = out(block, &changed);
        assert!(
            !y.bit_eq(&base_y),
            "perturbing input {idx} must move output; base={base_y:?}, y={y:?}"
        );
    }
}

fn assert_unselected_does_not_leak(
    block: &dyn Block,
    base: &[Value],
    idx: usize,
    replacement: Value,
) {
    let base_y = out(block, base);
    let mut changed = base.to_vec();
    changed[idx] = replacement;
    let y = out(block, &changed);
    assert!(
        y.bit_eq(&base_y),
        "unselected switch input {idx} leaked into output; base={base_y:?}, y={y:?}"
    );
}

#[test]
fn logical_truth_table_and_source_constant_are_bit_goldens() {
    let cases = [
        (false, false, false, true, true, false),
        (false, true, true, true, false, true),
        (true, false, true, true, false, true),
        (true, true, true, false, false, false),
    ];
    for (u1, u2, or, nand, nor, xor) in cases {
        let inputs = [b(u1), b(u2)];
        assert_eq!(bool_out(&Or, &inputs), or);
        assert_eq!(bool_out(&Nand, &inputs), nand);
        assert_eq!(bool_out(&Nor, &inputs), nor);
        assert_eq!(bool_out(&Xor, &inputs), xor);
    }

    assert_one_out(&LogicalConstant { k: true }, &[], b(true));
    assert_one_out(&LogicalConstant { k: false }, &[], b(false));
    assert_eq!(LogicalConstant { k: true }.kind(), BlockKind::Algebraic);
    assert!(!LogicalConstant { k: true }.feeds_through(0, 0));
}

#[test]
fn logical_switch_selector_order_and_non_leakage_are_pinned() {
    let sw = LogicalSwitch;
    assert_one_out(&sw, &[b(false), b(true), b(true)], b(false));
    assert_one_out(&sw, &[b(false), b(false), b(true)], b(true));
    assert_perturb_moves(
        &sw,
        &[b(false), b(true), b(true)],
        &[(0, b(true)), (1, b(false))],
    );
    assert_perturb_moves(
        &sw,
        &[b(true), b(false), b(false)],
        &[(2, b(true)), (1, b(true))],
    );
    assert_unselected_does_not_leak(&sw, &[b(false), b(true), b(true)], 2, b(false));
    assert_unselected_does_not_leak(&sw, &[b(true), b(false), b(false)], 0, b(false));
}

#[test]
fn logical_multi_reducers_handle_empty_single_and_ordered_vectors() {
    let empty_and = (lookup("CDL.Logical.MultiAnd").unwrap().make)(&ParamTable::default());
    let empty_or = (lookup("CDL.Logical.MultiOr").unwrap().make)(&ParamTable::default());
    assert!(!bool_out(empty_and.as_ref(), &[]));
    assert!(!bool_out(empty_or.as_ref(), &[]));

    assert!(bool_out(&MultiAnd::new(1), &[b(true)]));
    assert!(!bool_out(&MultiAnd::new(1), &[b(false)]));
    assert!(bool_out(&MultiOr::new(1), &[b(true)]));
    assert!(!bool_out(&MultiOr::new(1), &[b(false)]));

    let multi_and = MultiAnd::new(4);
    assert!(bool_out(&multi_and, &[b(true), b(true), b(true), b(true)]));
    assert!(!bool_out(
        &multi_and,
        &[b(true), b(true), b(false), b(true)]
    ));
    assert_perturb_moves(
        &multi_and,
        &[b(true), b(true), b(true), b(true)],
        &[(0, b(false)), (2, b(false)), (3, b(false))],
    );

    let multi_or = MultiOr::new(4);
    assert!(!bool_out(
        &multi_or,
        &[b(false), b(false), b(false), b(false)]
    ));
    assert!(bool_out(
        &multi_or,
        &[b(false), b(false), b(true), b(false)]
    ));
    assert_perturb_moves(
        &multi_or,
        &[b(false), b(false), b(false), b(false)],
        &[(0, b(true)), (2, b(true)), (3, b(true))],
    );
}

#[test]
fn algebraic_feedthrough_perturbation_matches_declared_contracts() {
    assert_perturb_moves(&Or, &[b(false), b(false)], &[(0, b(true)), (1, b(true))]);
    assert_perturb_moves(&Nand, &[b(true), b(true)], &[(0, b(false)), (1, b(false))]);
    assert_perturb_moves(&Nor, &[b(false), b(false)], &[(0, b(true)), (1, b(true))]);
    assert_perturb_moves(&Xor, &[b(false), b(false)], &[(0, b(true)), (1, b(true))]);
    assert_perturb_moves(&BooleanToInteger::default(), &[b(false)], &[(0, b(true))]);
    assert_perturb_moves(&BooleanToReal::default(), &[b(false)], &[(0, b(true))]);
    assert_perturb_moves(&IntegerToReal, &[i(1)], &[(0, i(2))]);
    assert_perturb_moves(&RealToInteger, &[r(0.4)], &[(0, r(0.5))]);

    assert_perturb_moves(&IntegerAbs, &[i(-2)], &[(0, i(3))]);
    assert_perturb_moves(&IntegerAdd, &[i(1), i(2)], &[(0, i(3)), (1, i(4))]);
    assert_perturb_moves(&IntegerSubtract, &[i(5), i(2)], &[(0, i(6)), (1, i(3))]);
    assert_perturb_moves(&IntegerMultiply, &[i(2), i(3)], &[(0, i(4)), (1, i(5))]);
    assert_perturb_moves(&IntegerAddParameter { p: 1 }, &[i(2)], &[(0, i(3))]);
    assert_perturb_moves(&IntegerMax, &[i(3), i(1)], &[(0, i(0)), (1, i(4))]);
    assert_perturb_moves(&IntegerMin, &[i(3), i(1)], &[(0, i(0)), (1, i(4))]);
    assert_perturb_moves(&IntegerEqual, &[i(2), i(2)], &[(0, i(1)), (1, i(3))]);
    assert_perturb_moves(&IntegerGreater, &[i(2), i(1)], &[(0, i(0)), (1, i(3))]);
    assert_perturb_moves(&IntegerGreaterThreshold { t: 1 }, &[i(2)], &[(0, i(1))]);
    assert_perturb_moves(&IntegerGreaterEqual, &[i(2), i(1)], &[(0, i(0)), (1, i(3))]);
    assert_perturb_moves(
        &IntegerGreaterEqualThreshold { t: 1 },
        &[i(1)],
        &[(0, i(0))],
    );
    assert_perturb_moves(&IntegerLess, &[i(1), i(2)], &[(0, i(3)), (1, i(0))]);
    assert_perturb_moves(&IntegerLessThreshold { t: 2 }, &[i(1)], &[(0, i(2))]);
    assert_perturb_moves(&IntegerLessEqual, &[i(1), i(2)], &[(0, i(3)), (1, i(0))]);
    assert_perturb_moves(&IntegerLessEqualThreshold { t: 2 }, &[i(2)], &[(0, i(3))]);

    for source in [
        &LogicalConstant { k: true } as &dyn Block,
        &IntegerConstant { k: 1 },
    ] {
        assert!(!source.feeds_through(0, 0));
    }
}

#[test]
fn algebraic_outputs_are_bit_deterministic_across_reruns() {
    let cases: &[(&dyn Block, &[Value])] = &[
        (&Or, &[b(false), b(true)]),
        (&Nand, &[b(true), b(true)]),
        (&Nor, &[b(false), b(false)]),
        (&Xor, &[b(true), b(false)]),
        (&LogicalSwitch, &[b(false), b(false), b(true)]),
        (&BooleanToInteger::default(), &[b(true)]),
        (&BooleanToReal::default(), &[b(true)]),
        (&IntegerToReal, &[i(9007199254740992)]),
        (&RealToInteger, &[r(-2.5)]),
        (&IntegerAbs, &[i(i64::MIN)]),
        (&IntegerAdd, &[i(i64::MAX), i(1)]),
        (&IntegerSubtract, &[i(i64::MIN), i(1)]),
        (&IntegerMultiply, &[i(i64::MAX), i(2)]),
        (&IntegerAddParameter { p: 1 }, &[i(i64::MAX)]),
        (&IntegerMax, &[i(i64::MIN), i(0)]),
        (&IntegerMin, &[i(i64::MIN), i(0)]),
        (&IntegerSwitch, &[i(1), b(false), i(9)]),
        (
            &IntegerEqual,
            &[i(9_007_199_254_740_993), i(9_007_199_254_740_993)],
        ),
        (&IntegerGreater, &[i(3), i(2)]),
        (&IntegerGreaterThreshold { t: 2 }, &[i(3)]),
        (&IntegerGreaterEqual, &[i(2), i(2)]),
        (&IntegerGreaterEqualThreshold { t: 2 }, &[i(2)]),
        (&IntegerLess, &[i(1), i(2)]),
        (&IntegerLessThreshold { t: 2 }, &[i(1)]),
        (&IntegerLessEqual, &[i(2), i(2)]),
        (&IntegerLessEqualThreshold { t: 2 }, &[i(2)]),
    ];
    for &(block, inputs) in cases {
        let first = out(block, inputs);
        let second = out(block, inputs);
        assert!(
            first.bit_eq(&second),
            "determinism failed for {:?}",
            block.signature().class_path
        );
    }
}

#[test]
fn registry_constructs_logical_conversion_and_integer_classes() {
    let logical = (lookup("CDL.Logical.Sources.Constant").unwrap().make)(&ParamTable {
        values: vec![(Arc::from("k"), b(true))],
    });
    assert_one_out(logical.as_ref(), &[], b(true));

    let b2i = (lookup("CDL.Conversions.BooleanToInteger").unwrap().make)(&ParamTable {
        values: vec![
            (Arc::from("integerTrue"), i(10)),
            (Arc::from("integerFalse"), i(-10)),
        ],
    });
    assert_one_out(b2i.as_ref(), &[b(false)], i(-10));

    let b2r = (lookup("CDL.Conversions.BooleanToReal").unwrap().make)(&ParamTable {
        values: vec![
            (Arc::from("realTrue"), r(2.0)),
            (Arc::from("realFalse"), r(-2.0)),
        ],
    });
    assert_one_out(b2r.as_ref(), &[b(true)], r(2.0));

    let iconst = (lookup("CDL.Integers.Sources.Constant").unwrap().make)(&ParamTable {
        values: vec![(Arc::from("k"), i(42))],
    });
    assert_one_out(iconst.as_ref(), &[], i(42));

    let addp = (lookup("CDL.Integers.AddParameter").unwrap().make)(&ParamTable {
        values: vec![(Arc::from("p"), i(5))],
    });
    assert_one_out(addp.as_ref(), &[i(7)], i(12));

    let gt = (lookup("CDL.Integers.GreaterThreshold").unwrap().make)(&ParamTable {
        values: vec![(Arc::from("t"), i(3))],
    });
    assert_one_out(gt.as_ref(), &[i(4)], b(true));

    let le = (lookup("CDL.Integers.LessEqualThreshold").unwrap().make)(&ParamTable {
        values: vec![(Arc::from("t"), i(3))],
    });
    assert_one_out(le.as_ref(), &[i(4)], b(false));
}

mod sample_trigger_rule {
    //! `SampleTrigger` under issue #800: the initial frame emits `true` only on an instant (decision
    //! 3), a frame that spans several instants warns once (decision 2), and the warned mark lives
    //! in the state word without losing the index.

    use std::cell::Cell;

    use oce_model::{ParamTable, Value};

    use crate::logical::{sample_trigger_encode, sample_trigger_word};
    use crate::{Block, Ctx, Diagnostics, Time, lookup};

    #[derive(Default)]
    struct Count(Cell<usize>);
    impl Diagnostics for Count {
        fn warn(&self, _source: &str, _message: &str, _t: Time) {
            self.0.set(self.0.get() + 1);
        }
    }

    fn trigger(period: f64, shift: f64) -> Box<dyn Block> {
        let table = ParamTable {
            values: vec![
                (std::sync::Arc::from("period"), Value::Real(period)),
                (std::sync::Arc::from("shift"), Value::Real(shift)),
            ],
        };
        (lookup("CDL.Logical.Sources.SampleTrigger").unwrap().make)(&table)
    }

    /// Run frames at `times` (the first is the initial frame); the outputs, the warnings and the
    /// final word.
    fn run(block: &dyn Block, times: &[f64]) -> (Vec<bool>, usize, u64) {
        let mut region = vec![0u64; block.state_len()];
        block.init_state(&mut region, &ParamTable::default());
        let count = Count::default();
        let mut outs = Vec::new();
        for (k, &t) in times.iter().enumerate() {
            let cx = Ctx::new(t, &count).with_initial(k == 0);
            block.emit_from_state(&cx, &[], &region, &mut |_, v| {
                outs.push(matches!(v, Value::Boolean(true)));
            });
            block.update_state(&cx, &[], &mut region);
        }
        (outs, count.0.get(), region[0])
    }

    #[test]
    fn the_initial_frame_fires_only_on_an_instant() {
        // On the grid (t = 0, shift 0) and a hair before or after it (within 1e-9 * period).
        for t0 in [0.0, -1e-10, 1e-10, 5.0] {
            let (outs, _, _) = run(trigger(1.0, 0.0).as_ref(), &[t0, t0 + 1.0]);
            assert_eq!(outs, [true, true], "t0 {t0}");
        }
        // Off the grid: no instant at the start; the next frame reaches the next instant.
        for t0 in [0.25, 0.5, 0.999, 100.3] {
            let (outs, _, _) = run(trigger(1.0, 0.0).as_ref(), &[t0, t0 + 1.0]);
            assert_eq!(outs, [false, true], "t0 {t0}");
        }
        // Before the first instant (shift 0.5): false, then the instant at 0.5 s.
        let (outs, _, _) = run(trigger(1.0, 0.5).as_ref(), &[0.0, 1.0]);
        assert_eq!(outs, [false, true]);
    }

    #[test]
    fn a_frame_spanning_several_instants_warns_once_and_keeps_the_index() {
        let block = trigger(1.0, 0.0);
        // 0 -> 2.5 spans the instants 1 and 2: one warning; 2.5 -> 6 spans 3 to 6: none more.
        let (outs, warnings, word) = run(block.as_ref(), &[0.0, 2.5, 6.0, 7.0]);
        assert_eq!(outs, [true, true, true, true]);
        assert_eq!(warnings, 1);
        assert_eq!(sample_trigger_word(word), (7, true));
        assert_eq!(word.cast_signed(), -(7 + 2));
        // One instant per frame never warns; a large first step does not either (the start).
        assert_eq!(run(block.as_ref(), &[0.0, 1.0, 2.0, 3.0]).1, 0);
        assert_eq!(run(block.as_ref(), &[1000.0, 1001.0]).1, 0);
        // Before any instant (index -1) a frame that reaches two instants is a span too.
        let late = trigger(1.0, 0.5);
        let (_, warnings, word) = run(late.as_ref(), &[0.0, 2.0]);
        assert_eq!((warnings, sample_trigger_word(word)), (1, (1, true)));
    }

    #[test]
    fn the_warned_mark_does_not_change_what_the_block_emits() {
        let block = trigger(1.0, 0.0);
        let times = [0.0, 2.5, 3.0, 3.5, 4.0, 9.0, 9.5];
        // The same run, with the mark cleared after every frame: the same outputs.
        let mut a = vec![0u64; 1];
        let mut b = vec![0u64; 1];
        block.init_state(&mut a, &ParamTable::default());
        block.init_state(&mut b, &ParamTable::default());
        let count = Count::default();
        for (k, &t) in times.iter().enumerate() {
            let cx = Ctx::new(t, &count).with_initial(k == 0);
            let (mut ya, mut yb) = (None, None);
            block.emit_from_state(&cx, &[], &a, &mut |_, v| {
                ya = Some(matches!(v, Value::Boolean(true)))
            });
            block.emit_from_state(&cx, &[], &b, &mut |_, v| {
                yb = Some(matches!(v, Value::Boolean(true)))
            });
            assert_eq!(ya, yb, "t {t}");
            block.update_state(&cx, &[], &mut a);
            block.update_state(&cx, &[], &mut b);
            let (index, _) = sample_trigger_word(b[0]);
            b[0] = sample_trigger_encode(index, false);
            assert_eq!(sample_trigger_word(a[0]).0, index, "t {t}");
        }
    }

    #[test]
    fn the_state_contract_accepts_exactly_the_word_s_domain() {
        let block = trigger(1.0, 0.0);
        let ok = |word: i64, t: f64| block.validate_state(&[word.cast_unsigned()], t, t).is_ok();
        // Unwarned: the index of the model time, -1 before the first instant.
        assert!(ok(3, 3.5));
        assert!(ok(-1, -0.5));
        // Warned: `-(index + 2)` for an index of at least 1.
        assert!(ok(-(3 + 2), 3.5));
        // Refused: another index, ahead or behind, warned or not; a warned index below 1; the
        // extremes; a negative that decodes to an index ahead of the model time.
        for word in [
            2,
            4,
            -1,
            -2,
            -3,
            -(4 + 2),
            i64::MIN,
            i64::MAX,
            -(1_000_000 + 2),
        ] {
            assert!(!ok(word, 3.5), "word {word}");
        }
        assert!(!ok(-2, 0.5), "a warned index 0");
        // Encoding at the integer limit never overflows: an index that cannot carry the mark is
        // stored unwarned.
        assert_eq!(
            sample_trigger_word(sample_trigger_encode(i64::MAX, true)),
            (i64::MAX, false)
        );
        assert_eq!(
            sample_trigger_word(sample_trigger_encode(i64::MAX - 2, true)),
            (i64::MAX - 2, true)
        );
        assert_eq!(
            sample_trigger_word(i64::MIN.cast_unsigned()),
            (i64::MAX - 1, true)
        );
    }
}
