//! M2-PR-A3 exact algebraic tests for Logical combinational, Conversions, and Integers.
//! Expected values are derived directly from `_spec/03` §4.2–§4.4 and compared bit-exactly.

use std::sync::Arc;

use oce_model::{ParamTable, Value};

use super::{
    Block, BlockKind, BooleanToInteger, BooleanToReal, Ctx, IntegerAbs, IntegerAdd,
    IntegerAddParameter, IntegerConstant, IntegerGreater, IntegerGreaterEqual,
    IntegerGreaterEqualThreshold, IntegerGreaterThreshold, IntegerLess, IntegerLessEqual,
    IntegerLessEqualThreshold, IntegerLessThreshold, IntegerMax, IntegerMin, IntegerMultiply,
    IntegerMultiplyByParameter, IntegerSubtract, IntegerSwitch, IntegerToReal, LogicalConstant,
    LogicalSwitch, Nand, NoopDiagnostics, Nor, Or, RealToInteger, Xor, lookup,
};

fn out(block: &dyn Block, inputs: &[Value]) -> Value {
    let diag = NoopDiagnostics;
    let cx = Ctx::new(0.0, &diag);
    let mut out = None;
    block.step_algebraic(&cx, inputs, &mut |idx, val| {
        assert_eq!(idx, 0, "A3 blocks emit one output");
        out = Some(val);
    });
    out.expect("A3 block must emit one output")
}

fn bool_out(block: &dyn Block, inputs: &[Value]) -> bool {
    match out(block, inputs) {
        Value::Boolean(y) => y,
        other => panic!("expected Boolean output, got {other:?}"),
    }
}

fn int_out(block: &dyn Block, inputs: &[Value]) -> i64 {
    match out(block, inputs) {
        Value::Integer(y) => y,
        other => panic!("expected Integer output, got {other:?}"),
    }
}

fn real_bits(block: &dyn Block, inputs: &[Value]) -> u64 {
    match out(block, inputs) {
        Value::Real(y) => y.to_bits(),
        other => panic!("expected Real output, got {other:?}"),
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
fn a3_logical_truth_table_and_source_constant_are_bit_goldens() {
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
fn a3_logical_switch_selector_order_and_non_leakage_are_pinned() {
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
fn a3_conversions_follow_spec_and_real_to_integer_half_up_table() {
    let b2i = BooleanToInteger {
        integer_true: 7,
        integer_false: -3,
    };
    assert_one_out(&b2i, &[b(true)], i(7));
    assert_one_out(&b2i, &[b(false)], i(-3));

    let b2r = BooleanToReal {
        real_true: 2.5,
        real_false: -0.25,
    };
    assert_eq!(real_bits(&b2r, &[b(true)]), 2.5f64.to_bits());
    assert_eq!(real_bits(&b2r, &[b(false)]), (-0.25f64).to_bits());

    assert_eq!(real_bits(&IntegerToReal, &[i(-42)]), (-42.0f64).to_bits());

    for (u, want) in [
        (2.5, 3),
        (2.4, 2),
        (-2.5, -2),
        (-2.6, -3),
        (-2.4, -2),
        (0.5, 1),
        (-0.5, 0),
    ] {
        assert_eq!(int_out(&RealToInteger, &[r(u)]), want, "u={u}");
    }
}

#[test]
fn a3_real_to_integer_non_finite_and_out_of_range_cast_policy_is_pinned() {
    assert_eq!(int_out(&RealToInteger, &[r(f64::NAN)]), 0);
    assert_eq!(int_out(&RealToInteger, &[r(f64::INFINITY)]), i64::MAX);
    assert_eq!(int_out(&RealToInteger, &[r(f64::NEG_INFINITY)]), i64::MIN);
    assert_eq!(int_out(&RealToInteger, &[r(f64::MAX)]), i64::MAX);
    assert_eq!(int_out(&RealToInteger, &[r(-f64::MAX)]), i64::MIN);
}

#[test]
fn a3_integer_arithmetic_hand_derived_goldens_and_wrap_edges() {
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
    assert_one_out(&IntegerMultiplyByParameter { k: -3 }, &[i(7)], i(-21));
    assert_one_out(&IntegerMultiplyByParameter { k: 2 }, &[i(i64::MAX)], i(-2));
    assert_one_out(&IntegerMax, &[i(i64::MIN), i(0)], i(0));
    assert_one_out(&IntegerMin, &[i(i64::MIN), i(0)], i(i64::MIN));
}

#[test]
fn a3_integer_switch_selector_order_and_non_leakage_are_pinned() {
    let sw = IntegerSwitch;
    assert_one_out(&sw, &[i(1), b(true), i(9)], i(1));
    assert_one_out(&sw, &[i(1), b(false), i(9)], i(9));
    assert_perturb_moves(&sw, &[i(1), b(true), i(9)], &[(0, i(2)), (1, b(false))]);
    assert_perturb_moves(&sw, &[i(1), b(false), i(9)], &[(2, i(10)), (1, b(true))]);
    assert_unselected_does_not_leak(&sw, &[i(1), b(true), i(9)], 2, i(42));
    assert_unselected_does_not_leak(&sw, &[i(1), b(false), i(9)], 0, i(42));
}

#[test]
fn a3_integer_comparators_are_pure_combinational_boundary_goldens() {
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

#[test]
fn a3_feedthrough_perturbation_matches_declared_contracts() {
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
    assert_perturb_moves(&IntegerMultiplyByParameter { k: 2 }, &[i(3)], &[(0, i(4))]);
    assert_perturb_moves(&IntegerMax, &[i(3), i(1)], &[(0, i(0)), (1, i(4))]);
    assert_perturb_moves(&IntegerMin, &[i(3), i(1)], &[(0, i(0)), (1, i(4))]);
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
fn a3_outputs_are_bit_deterministic_across_reruns() {
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
        (&IntegerMultiplyByParameter { k: 2 }, &[i(i64::MAX)]),
        (&IntegerMax, &[i(i64::MIN), i(0)]),
        (&IntegerMin, &[i(i64::MIN), i(0)]),
        (&IntegerSwitch, &[i(1), b(false), i(9)]),
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
fn a3_registry_constructs_new_classes_and_resolves_parameters() {
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

    let mulp = (lookup("CDL.Integers.MultiplyByParameter").unwrap().make)(&ParamTable {
        values: vec![(Arc::from("k"), i(-2))],
    });
    assert_one_out(mulp.as_ref(), &[i(7)], i(-14));

    let gt = (lookup("CDL.Integers.GreaterThreshold").unwrap().make)(&ParamTable {
        values: vec![(Arc::from("t"), i(3))],
    });
    assert_one_out(gt.as_ref(), &[i(4)], b(true));

    let le = (lookup("CDL.Integers.LessEqualThreshold").unwrap().make)(&ParamTable {
        values: vec![(Arc::from("t"), i(3))],
    });
    assert_one_out(le.as_ref(), &[i(4)], b(false));
}

#[test]
fn a3_deferred_vector_and_stateful_integer_logical_blocks_remain_unregistered() {
    for path in [
        "CDL.Logical.MultiAnd",
        "CDL.Logical.MultiOr",
        "CDL.Integers.MultiSum",
        "CDL.Integers.OnCounter",
        "CDL.Integers.Change",
        "CDL.Logical.Sources.Pulse",
    ] {
        assert!(
            lookup(path).is_none(),
            "{path} is explicitly deferred beyond A3"
        );
    }
}
