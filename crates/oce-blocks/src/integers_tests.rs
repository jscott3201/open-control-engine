//! Exact algebraic tests for `CDL.Integers` blocks.
//! Expected values are derived directly from `_spec/03` §4.2–§4.4 and compared bit-exactly.

use std::sync::Arc;

use oce_model::{ParamTable, Value};

use super::{
    Block, BlockKind, Ctx, IntegerAbs, IntegerAdd, IntegerAddParameter, IntegerConstant,
    IntegerEqual, IntegerGreater, IntegerGreaterEqual, IntegerGreaterEqualThreshold,
    IntegerGreaterThreshold, IntegerLess, IntegerLessEqual, IntegerLessEqualThreshold,
    IntegerLessThreshold, IntegerMax, IntegerMin, IntegerMultiSum, IntegerMultiply,
    IntegerSubtract, IntegerSwitch, NoopDiagnostics, PortKind, lookup,
};

fn out(block: &dyn Block, inputs: &[Value]) -> Value {
    let diag = NoopDiagnostics;
    let cx = Ctx::new(0.0, &diag);
    let mut out = None;
    block.step_algebraic(&cx, inputs, &mut |idx, val| {
        assert_eq!(idx, 0, "integer blocks emit one output");
        out = Some(val);
    });
    out.expect("integer block must emit one output")
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
fn integer_arithmetic_hand_derived_goldens_and_wrap_edges() {
    assert_one_out(&IntegerConstant { k: -11 }, &[], i(-11));
    assert_one_out(&IntegerAbs, &[i(-9)], i(9));
    assert_one_out(&IntegerAbs, &[i(i64::MIN)], i(i64::MIN));
    assert_one_out(&IntegerAdd, &[i(12), i(-5)], i(7));
    assert_one_out(&IntegerAdd, &[i(i64::MAX), i(1)], i(i64::MIN));
    assert_one_out(&IntegerSubtract, &[i(-10), i(4)], i(-14));
    assert_one_out(&IntegerSubtract, &[i(i64::MIN), i(1)], i(i64::MAX));
    assert_one_out(&IntegerMultiply, &[i(-6), i(7)], i(-42));
    assert_one_out(&IntegerMultiply, &[i(i64::MAX), i(2)], i(-2));
    assert_one_out(&IntegerAddParameter { p: -4 }, &[i(9)], i(5));
    assert_one_out(&IntegerAddParameter { p: 1 }, &[i(i64::MAX)], i(i64::MIN));
    assert_one_out(&IntegerMax, &[i(i64::MIN), i(0)], i(0));
    assert_one_out(&IntegerMin, &[i(i64::MIN), i(0)], i(i64::MIN));
}

#[test]
fn integer_multi_sum_uses_resolved_width_gains_and_wrapping_arithmetic() {
    assert_one_out(&IntegerMultiSum::new(Vec::new()), &[], i(0));
    assert_one_out(&IntegerMultiSum::new(vec![1]), &[i(-7)], i(-7));
    assert_one_out(
        &IntegerMultiSum::new(vec![2, -1, 3]),
        &[i(10), i(4), i(-2)],
        i(10),
    );
    assert_one_out(
        &IntegerMultiSum::new(vec![1, 1, 1]),
        &[i(i64::MAX), i(1), i(1)],
        i(i64::MIN + 1),
    );

    let default_gains = (lookup("CDL.Integers.MultiSum").unwrap().make)(&ParamTable {
        values: vec![(Arc::from("nin"), i(3))],
    });
    assert_one_out(default_gains.as_ref(), &[i(1), i(2), i(3)], i(6));

    let explicit_gains = (lookup("CDL.Integers.MultiSum").unwrap().make)(&ParamTable {
        values: vec![
            (Arc::from("nin"), i(3)),
            (Arc::from("k_1"), i(2)),
            (Arc::from("k_2"), i(-1)),
            (Arc::from("k_3"), i(3)),
        ],
    });
    assert_one_out(explicit_gains.as_ref(), &[i(10), i(4), i(-2)], i(10));
    assert_eq!(
        explicit_gains.resolved_signature().inputs.as_ref(),
        &[PortKind::Integer, PortKind::Integer, PortKind::Integer]
    );
}

#[test]
fn integer_switch_selector_order_and_non_leakage_are_pinned() {
    let sw = IntegerSwitch;
    assert_one_out(&sw, &[i(1), b(true), i(9)], i(1));
    assert_one_out(&sw, &[i(1), b(false), i(9)], i(9));
    assert_perturb_moves(&sw, &[i(1), b(true), i(9)], &[(0, i(2)), (1, b(false))]);
    assert_perturb_moves(&sw, &[i(1), b(false), i(9)], &[(2, i(10)), (1, b(true))]);
    assert_unselected_does_not_leak(&sw, &[i(1), b(true), i(9)], 2, i(42));
    assert_unselected_does_not_leak(&sw, &[i(1), b(false), i(9)], 0, i(42));
}

#[test]
fn integer_comparators_are_pure_combinational_boundary_goldens() {
    assert_eq!(
        IntegerEqual.signature().inputs,
        &[PortKind::Integer, PortKind::Integer]
    );
    assert_eq!(IntegerEqual.signature().outputs, &[PortKind::Boolean]);
    assert_eq!(IntegerEqual.kind(), BlockKind::Algebraic);
    assert_eq!(IntegerEqual.state_len(), 0);
    assert!(IntegerEqual.feeds_through(0, 0));
    assert!(IntegerEqual.feeds_through(1, 0));

    assert!(bool_out(&IntegerEqual, &[i(0), i(0)]));
    assert!(bool_out(&IntegerEqual, &[i(-42), i(-42)]));
    assert!(bool_out(
        &IntegerEqual,
        &[i(9_007_199_254_740_993), i(9_007_199_254_740_993)]
    ));
    assert!(bool_out(&IntegerEqual, &[i(i64::MAX), i(i64::MAX)]));
    assert!(bool_out(&IntegerEqual, &[i(i64::MIN), i(i64::MIN)]));
    assert!(!bool_out(&IntegerEqual, &[i(0), i(1)]));
    assert!(!bool_out(
        &IntegerEqual,
        &[i(9_007_199_254_740_993), i(9_007_199_254_740_992)]
    ));
    assert!(!bool_out(
        &IntegerEqual,
        &[i(-9_007_199_254_740_993), i(-9_007_199_254_740_992)]
    ));
    assert!(!bool_out(&IntegerEqual, &[i(i64::MAX), i(i64::MAX - 1)]));
    assert!(!bool_out(&IntegerEqual, &[i(i64::MIN), i(i64::MIN + 1)]));
    assert!(!bool_out(&IntegerEqual, &[i(i64::MAX), i(i64::MIN)]));

    assert!(bool_out(&IntegerGreater, &[i(3), i(2)]));
    assert!(!bool_out(&IntegerGreater, &[i(2), i(2)]));
    assert!(bool_out(&IntegerGreaterThreshold { t: 2 }, &[i(3)]));
    assert!(!bool_out(&IntegerGreaterThreshold { t: 2 }, &[i(2)]));

    assert!(bool_out(&IntegerGreaterEqual, &[i(2), i(2)]));
    assert!(!bool_out(&IntegerGreaterEqual, &[i(1), i(2)]));
    assert!(bool_out(&IntegerGreaterEqualThreshold { t: 2 }, &[i(2)]));
    assert!(!bool_out(&IntegerGreaterEqualThreshold { t: 2 }, &[i(1)]));

    assert!(bool_out(&IntegerLess, &[i(1), i(2)]));
    assert!(!bool_out(&IntegerLess, &[i(2), i(2)]));
    assert!(bool_out(&IntegerLessThreshold { t: 2 }, &[i(1)]));
    assert!(!bool_out(&IntegerLessThreshold { t: 2 }, &[i(2)]));

    assert!(bool_out(&IntegerLessEqual, &[i(2), i(2)]));
    assert!(!bool_out(&IntegerLessEqual, &[i(3), i(2)]));
    assert!(bool_out(&IntegerLessEqualThreshold { t: 2 }, &[i(2)]));
    assert!(!bool_out(&IntegerLessEqualThreshold { t: 2 }, &[i(3)]));

    for block in [
        &IntegerEqual as &dyn Block,
        &IntegerGreater as &dyn Block,
        &IntegerGreaterThreshold { t: 0 },
        &IntegerGreaterEqual,
        &IntegerGreaterEqualThreshold { t: 0 },
        &IntegerLess,
        &IntegerLessThreshold { t: 0 },
        &IntegerLessEqual,
        &IntegerLessEqualThreshold { t: 0 },
    ] {
        assert_eq!(block.kind(), BlockKind::Algebraic);
        assert_eq!(block.state_len(), 0);
    }
}

#[cfg(debug_assertions)]
#[test]
fn integer_equal_rejects_non_integer_inputs_in_debug_builds() {
    assert!(
        std::panic::catch_unwind(|| {
            let _ = out(&IntegerEqual, &[Value::Real(1.0), i(1)]);
        })
        .is_err()
    );
    assert!(
        std::panic::catch_unwind(|| {
            let _ = out(&IntegerEqual, &[Value::String(Arc::from("1")), i(1)]);
        })
        .is_err()
    );
    assert!(
        std::panic::catch_unwind(|| {
            let _ = out(&IntegerEqual, &[i(1), Value::Real(1.0)]);
        })
        .is_err()
    );
    assert!(
        std::panic::catch_unwind(|| {
            let _ = out(&IntegerEqual, &[i(1), Value::String(Arc::from("1"))]);
        })
        .is_err()
    );
}
