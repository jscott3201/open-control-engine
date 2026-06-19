//! M2-PR-A1 scalar `CDL.Reals` algebraic tests. Expected numeric values are hand-derived from
//! `_spec/03` §4.1 and pinned by IEEE-754 bits where exact equality matters.

use oce_model::Value;

use super::{
    Abs, AddParameter, Block, Ctx, Divide, Line, Max, Min, Multiply, NoopDiagnostics, read_real,
};

fn real_out(block: &dyn Block, inputs: &[Value]) -> f64 {
    let diag = NoopDiagnostics;
    let cx = Ctx::new(0.0, &diag);
    let mut out = None;
    block.step_algebraic(&cx, inputs, &mut |idx, val| {
        assert_eq!(idx, 0, "A1 Reals blocks have one output");
        out = Some(val);
    });
    match out.expect("block must emit one output") {
        Value::Real(y) => y,
        other => panic!("expected Real output, got {other:?}"),
    }
}

fn real_inputs(xs: &[f64]) -> Vec<Value> {
    xs.iter().copied().map(Value::Real).collect()
}

fn assert_real_bits(block: &dyn Block, inputs: &[f64], expected_bits: u64) {
    let y = real_out(block, &real_inputs(inputs));
    assert_eq!(y.to_bits(), expected_bits, "actual={y:?}");
}

fn assert_perturb_moves(block: &dyn Block, base: &[f64], variants: &[(usize, f64)]) {
    let base_y = real_out(block, &real_inputs(base));
    for &(idx, replacement) in variants {
        assert!(block.feeds_through(idx, 0));
        let mut changed = base.to_vec();
        changed[idx] = replacement;
        let y = real_out(block, &real_inputs(&changed));
        assert_ne!(
            y.to_bits(),
            base_y.to_bits(),
            "perturbing input {idx} must move output for R-FT-5"
        );
    }
}

#[test]
fn a1_reals_hand_derived_golden_bits() {
    assert_real_bits(&Multiply, &[0.1, 0.2], 0x3f947ae147ae147c);
    assert_real_bits(&Divide, &[2.0, 3.0], 0x3fe5555555555555);
    assert_real_bits(&AddParameter { p: 0.2 }, &[0.1], 0x3fd3333333333334);
    assert_real_bits(&Abs, &[-7.25], 0x401d000000000000);
    assert_real_bits(&Min, &[4.25, -3.5], 0xc00c000000000000);
    assert_real_bits(&Max, &[4.25, -3.5], 0x4011000000000000);
    assert_real_bits(&Line, &[0.0, 0.1, 3.0, 1.1, 1.0], 0x3fdbbbbbbbbbbbbd);
}

#[test]
fn a1_reals_feedthrough_perturbation_matches_declared_contract() {
    assert_perturb_moves(&Multiply, &[2.0, 3.0], &[(0, 4.0), (1, 5.0)]);
    assert_perturb_moves(&Divide, &[8.0, 4.0], &[(0, 12.0), (1, 2.0)]);
    assert_perturb_moves(&AddParameter { p: 2.0 }, &[3.0], &[(0, 4.0)]);
    assert_perturb_moves(&Abs, &[-2.0], &[(0, 3.0)]);
    assert_perturb_moves(&Min, &[4.0, 2.0], &[(0, 1.0), (1, 6.0)]);
    assert_perturb_moves(&Max, &[4.0, 2.0], &[(0, 1.0), (1, 6.0)]);
    assert_perturb_moves(
        &Line,
        &[0.0, 2.0, 4.0, 10.0, 1.5],
        &[(0, 0.5), (1, 3.0), (2, 5.0), (3, 12.0), (4, 2.0)],
    );
}

#[test]
fn divide_edges_are_ieee_and_panic_free() {
    assert_real_bits(&Divide, &[1.0, 0.0], f64::INFINITY.to_bits());
    assert_real_bits(&Divide, &[1.0, -0.0], f64::NEG_INFINITY.to_bits());
    assert_real_bits(&Divide, &[1.0, f64::INFINITY], 0.0f64.to_bits());
    assert_real_bits(&Divide, &[-1.0, f64::INFINITY], (-0.0f64).to_bits());

    let nan_divisor = real_out(&Divide, &[Value::Real(1.0), Value::Real(f64::NAN)]);
    assert!(nan_divisor.is_nan());
    let zero_over_zero = real_out(&Divide, &[Value::Real(0.0), Value::Real(0.0)]);
    assert!(zero_over_zero.is_nan());
}

#[test]
fn abs_edges_preserve_specified_ieee_behavior() {
    assert_real_bits(&Abs, &[-0.0], 0.0f64.to_bits());
    let y = real_out(&Abs, &[Value::Real(f64::NAN)]);
    assert!(y.is_nan());
}

#[test]
fn min_max_edges_follow_scalar_expression_policy() {
    assert_real_bits(&Min, &[f64::NAN, 2.0], 2.0f64.to_bits());
    assert_real_bits(&Min, &[2.0, f64::NAN], 2.0f64.to_bits());
    assert_real_bits(&Max, &[f64::NAN, 2.0], 2.0f64.to_bits());
    assert_real_bits(&Max, &[2.0, f64::NAN], 2.0f64.to_bits());

    assert_real_bits(&Min, &[-0.0, 0.0], (-0.0f64).to_bits());
    assert_real_bits(&Min, &[0.0, -0.0], (-0.0f64).to_bits());
    assert_real_bits(&Max, &[-0.0, 0.0], 0.0f64.to_bits());
    assert_real_bits(&Max, &[0.0, -0.0], 0.0f64.to_bits());
}

#[test]
fn line_clamps_at_breakpoints_and_degrades_on_degenerate_domain() {
    let inputs = |u| [2.0, 10.0, 6.0, 18.0, u];
    assert_real_bits(&Line, &inputs(1.0), 10.0f64.to_bits());
    assert_real_bits(&Line, &inputs(2.0), 10.0f64.to_bits());
    assert_real_bits(&Line, &inputs(5.0), 16.0f64.to_bits());
    assert_real_bits(&Line, &inputs(6.0), 18.0f64.to_bits());
    assert_real_bits(&Line, &inputs(7.0), 18.0f64.to_bits());

    let degenerate = real_out(
        &Line,
        &[
            Value::Real(3.0),
            Value::Real(4.0),
            Value::Real(3.0),
            Value::Real(8.0),
            Value::Real(3.0),
        ],
    );
    assert!(degenerate.is_nan());
}

#[test]
fn a1_reals_outputs_are_bit_deterministic_across_reruns() {
    let cases: &[(&dyn Block, &[f64])] = &[
        (&Multiply, &[0.1, 0.2]),
        (&Divide, &[0.0, 0.0]),
        (&AddParameter { p: 0.2 }, &[0.1]),
        (&Abs, &[-0.0]),
        (&Min, &[f64::NAN, 2.0]),
        (&Max, &[0.0, -0.0]),
        (&Line, &[3.0, 4.0, 3.0, 8.0, 3.0]),
    ];
    for &(block, inputs) in cases {
        let first = real_out(block, &real_inputs(inputs));
        let second = real_out(block, &real_inputs(inputs));
        assert_eq!(first.to_bits(), second.to_bits());
    }
}

#[test]
fn read_real_release_degrade_remains_zero_for_a1_blocks() {
    if cfg!(debug_assertions) {
        assert!(
            std::panic::catch_unwind(|| read_real(&[Value::Boolean(true)], 0)).is_err(),
            "debug builds must trip the validation-bug assertion"
        );
    } else {
        assert_eq!(
            read_real(&[Value::Boolean(true)], 0).to_bits(),
            0.0f64.to_bits()
        );
    }
}
