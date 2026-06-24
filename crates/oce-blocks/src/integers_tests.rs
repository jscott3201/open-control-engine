//! Exact algebraic tests for `CDL.Integers` blocks.
//! Expected values are derived directly from `_spec/03` §4.2–§4.4 and compared bit-exactly.

use oce_model::Value;

use super::{
    Block, BlockKind, Ctx, IntegerAbs, IntegerAdd, IntegerAddParameter, IntegerConstant,
    IntegerGreater, IntegerGreaterEqual, IntegerGreaterEqualThreshold, IntegerGreaterThreshold,
    IntegerLess, IntegerLessEqual, IntegerLessEqualThreshold, IntegerLessThreshold, IntegerMax,
    IntegerMin, IntegerMultiply, IntegerSubtract, IntegerSwitch, NoopDiagnostics,
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
