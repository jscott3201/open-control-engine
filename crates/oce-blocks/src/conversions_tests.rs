//! Exact algebraic tests for `CDL.Conversions` blocks.
//! Expected values are derived directly from `_spec/03` §4.2-§4.4 and compared bit-exactly.

use oce_model::Value;

use super::{
    Block, BooleanToInteger, BooleanToReal, Ctx, IntegerToReal, NoopDiagnostics, RealToInteger,
};

fn out(block: &dyn Block, inputs: &[Value]) -> Value {
    let diag = NoopDiagnostics;
    let cx = Ctx::new(0.0, &diag);
    let mut out = None;
    block.step_algebraic(&cx, inputs, &mut |idx, val| {
        assert_eq!(idx, 0, "conversion blocks emit one output");
        out = Some(val);
    });
    out.expect("conversion block must emit one output")
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

#[test]
fn conversions_follow_spec_and_real_to_integer_half_away_table() {
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
        (-2.5, -3),
        (-2.6, -3),
        (-2.4, -2),
        (0.5, 1),
        (-0.5, -1),
    ] {
        assert_eq!(int_out(&RealToInteger, &[r(u)]), want, "u={u}");
    }
}

#[test]
fn conversion_real_outputs_canonicalize_nan_bits() {
    let b2r = BooleanToReal {
        real_true: f64::from_bits(0xfff8_0000_0000_0000),
        real_false: 1.0,
    };
    assert_eq!(real_bits(&b2r, &[b(true)]), 0x7ff8000000000000);
}

#[test]
fn real_to_integer_non_finite_and_out_of_range_cast_policy_is_pinned() {
    assert_eq!(int_out(&RealToInteger, &[r(f64::NAN)]), 0);
    assert_eq!(int_out(&RealToInteger, &[r(f64::INFINITY)]), i64::MAX);
    assert_eq!(int_out(&RealToInteger, &[r(f64::NEG_INFINITY)]), i64::MIN);
    assert_eq!(int_out(&RealToInteger, &[r(f64::MAX)]), i64::MAX);
    assert_eq!(int_out(&RealToInteger, &[r(-f64::MAX)]), i64::MIN);
}
