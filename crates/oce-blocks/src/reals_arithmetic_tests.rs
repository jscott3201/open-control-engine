//! Scalar `CDL.Reals` algebraic tests. Expected numeric values are hand-derived from
//! `_spec/03` §4.1 and pinned by IEEE-754 bits where exact equality matters.

use oce_model::Value;

use super::{
    Abs, Add, AddParameter, Average, Block, Constant, Ctx, Divide, Limiter, Line, Max, Min, Modulo,
    Multiply, MultiplyByParameter, NoopDiagnostics, Round, Sqrt, Subtract, Switch, read_real,
};

fn real_out(block: &dyn Block, inputs: &[Value]) -> f64 {
    let diag = NoopDiagnostics;
    let cx = Ctx::new(0.0, &diag);
    let mut out = None;
    block.step_algebraic(&cx, inputs, &mut |idx, val| {
        assert_eq!(idx, 0, "scalar Reals blocks have one output");
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
fn reals_arithmetic_hand_derived_golden_bits() {
    assert_real_bits(&Multiply, &[0.1, 0.2], 0x3f947ae147ae147c);
    assert_real_bits(&Divide, &[2.0, 3.0], 0x3fe5555555555555);
    assert_real_bits(&AddParameter { p: 0.2 }, &[0.1], 0x3fd3333333333334);
    assert_real_bits(&Abs, &[-7.25], 0x401d000000000000);
    assert_real_bits(&Min, &[4.25, -3.5], 0xc00c000000000000);
    assert_real_bits(&Max, &[4.25, -3.5], 0x4011000000000000);
    assert_real_bits(
        &Line::default(),
        &[0.0, 0.1, 3.0, 1.1, 1.0],
        0x3fdbbbbbbbbbbbbd,
    );
}

#[test]
fn reals_arithmetic_feedthrough_perturbation_matches_declared_contract() {
    assert_perturb_moves(&Multiply, &[2.0, 3.0], &[(0, 4.0), (1, 5.0)]);
    assert_perturb_moves(&Divide, &[8.0, 4.0], &[(0, 12.0), (1, 2.0)]);
    assert_perturb_moves(&Sqrt, &[4.0], &[(0, 9.0)]);
    assert_perturb_moves(&Average, &[2.0, 4.0], &[(0, 6.0), (1, 8.0)]);
    assert_perturb_moves(&Modulo, &[5.0, 3.0], &[(0, 7.0), (1, 2.0)]);
    assert_perturb_moves(&Round { n: 1 }, &[1.24], &[(0, 1.26)]);
    assert_perturb_moves(&AddParameter { p: 2.0 }, &[3.0], &[(0, 4.0)]);
    assert_perturb_moves(&Abs, &[-2.0], &[(0, 3.0)]);
    assert_perturb_moves(&Min, &[4.0, 2.0], &[(0, 1.0), (1, 6.0)]);
    assert_perturb_moves(&Max, &[4.0, 2.0], &[(0, 1.0), (1, 6.0)]);
    assert_perturb_moves(
        &Line::default(),
        &[0.0, 2.0, 4.0, 10.0, 1.5],
        &[(0, 0.5), (1, 3.0), (2, 5.0), (3, 12.0), (4, 2.0)],
    );
}

#[test]
fn sqrt_edges_are_ieee_and_panic_free() {
    assert_real_bits(&Sqrt, &[0.0], 0.0f64.to_bits());
    assert_real_bits(&Sqrt, &[4.0], 2.0f64.to_bits());
    assert_real_bits(&Sqrt, &[-1.0], 0x7ff8000000000000);
    assert_real_bits(&Sqrt, &[f64::INFINITY], f64::INFINITY.to_bits());
}

#[test]
fn average_edges_follow_sum_then_half_formula() {
    assert_real_bits(&Average, &[2.0, 4.0], 3.0f64.to_bits());
    assert_real_bits(&Average, &[-2.0, -4.0], (-3.0f64).to_bits());
    assert_real_bits(&Average, &[f64::MAX, f64::MAX], f64::INFINITY.to_bits());
    assert_real_bits(&Average, &[1.0, f64::NAN], 0x7ff8000000000000);
}

#[test]
fn modulo_uses_floored_modelica_sign_rule() {
    assert_real_bits(&Modulo, &[-5.0, 3.0], 1.0f64.to_bits());
    assert_real_bits(&Modulo, &[5.0, -3.0], (-1.0f64).to_bits());
    assert_real_bits(&Modulo, &[-5.0, -3.0], (-2.0f64).to_bits());
}

#[test]
fn modulo_zero_edges_are_ieee_and_panic_free() {
    assert_real_bits(&Modulo, &[0.0, 3.0], 0.0f64.to_bits());
    assert_real_bits(&Modulo, &[1.0, 0.0], 0x7ff8000000000000);
}

#[test]
fn round_half_boundaries_are_away_from_zero() {
    assert_real_bits(&Round { n: 0 }, &[2.5], 3.0f64.to_bits());
    assert_real_bits(&Round { n: 0 }, &[-2.5], (-3.0f64).to_bits());
    assert_real_bits(&Round { n: 0 }, &[0.0], (-0.0f64).to_bits());
}

#[test]
fn round_decimal_digits_use_deterministic_integer_factor() {
    assert_real_bits(&Round { n: 0 }, &[-2.4], (-2.0f64).to_bits());
    assert_real_bits(&Round { n: 1 }, &[1.25], 1.3f64.to_bits());
    assert_real_bits(&Round { n: 2 }, &[1.125], 1.13f64.to_bits());
    assert_real_bits(&Round { n: -1 }, &[149.0], 150.0f64.to_bits());
}
#[test]
fn multiply_edges_are_ieee_and_panic_free() {
    assert_real_bits(&Multiply, &[0.0, f64::INFINITY], 0x7ff8000000000000);
    assert_real_bits(&Multiply, &[f64::MAX, f64::MAX], f64::INFINITY.to_bits());
    assert_real_bits(&Multiply, &[1.0, f64::NAN], 0x7ff8000000000000);
    assert_real_bits(&Multiply, &[-0.0, 1.0], (-0.0f64).to_bits());
}

#[test]
fn real_emit_canonicalizes_nan_bits_across_algebraic_sources() {
    let negative_nan = f64::from_bits(0xfff8_0000_0000_0000);

    assert_real_bits(&Constant { k: negative_nan }, &[], 0x7ff8000000000000);
    assert_real_bits(
        &Add,
        &[f64::INFINITY, f64::NEG_INFINITY],
        0x7ff8000000000000,
    );
    assert_real_bits(
        &Subtract,
        &[f64::INFINITY, f64::INFINITY],
        0x7ff8000000000000,
    );
    assert_real_bits(
        &MultiplyByParameter { k: 0.0 },
        &[f64::INFINITY],
        0x7ff8000000000000,
    );
    assert_real_bits(&Abs, &[negative_nan], 0x7ff8000000000000);

    let y = real_out(
        &Switch,
        &[
            Value::Real(negative_nan),
            Value::Boolean(true),
            Value::Real(1.0),
        ],
    );
    assert_eq!(y.to_bits(), 0x7ff8000000000000);
}

#[test]
fn divide_edges_are_ieee_and_panic_free() {
    assert_real_bits(&Divide, &[1.0, 0.0], f64::INFINITY.to_bits());
    assert_real_bits(&Divide, &[1.0, -0.0], f64::NEG_INFINITY.to_bits());
    assert_real_bits(&Divide, &[1.0, f64::INFINITY], 0.0f64.to_bits());
    assert_real_bits(&Divide, &[-1.0, f64::INFINITY], (-0.0f64).to_bits());
    assert_real_bits(&Divide, &[1.0, f64::NAN], 0x7ff8000000000000);
    assert_real_bits(&Divide, &[0.0, 0.0], 0x7ff8000000000000);
    assert_real_bits(&Divide, &[f64::INFINITY, f64::INFINITY], 0x7ff8000000000000);
    assert_real_bits(&Divide, &[-0.0, -0.0], 0x7ff8000000000000);
}

#[test]
fn add_parameter_edges_are_ieee_and_panic_free() {
    assert_real_bits(
        &AddParameter { p: f64::MAX },
        &[f64::MAX],
        f64::INFINITY.to_bits(),
    );
    assert_real_bits(&AddParameter { p: 1.0 }, &[f64::NAN], 0x7ff8000000000000);
    assert_real_bits(
        &AddParameter { p: f64::INFINITY },
        &[1.0],
        f64::INFINITY.to_bits(),
    );
}

#[test]
fn abs_edges_preserve_specified_ieee_behavior() {
    assert_real_bits(&Abs, &[-0.0], 0.0f64.to_bits());
    assert_real_bits(&Abs, &[f64::NAN], 0x7ff8000000000000);
    assert_real_bits(&Abs, &[f64::INFINITY], f64::INFINITY.to_bits());
    assert_real_bits(&Abs, &[f64::NEG_INFINITY], f64::INFINITY.to_bits());
}

#[test]
fn min_max_edges_follow_scalar_expression_policy() {
    assert_real_bits(&Min, &[f64::NAN, 2.0], 2.0f64.to_bits());
    assert_real_bits(&Min, &[2.0, f64::NAN], 2.0f64.to_bits());
    assert_real_bits(&Max, &[f64::NAN, 2.0], 2.0f64.to_bits());
    assert_real_bits(&Max, &[2.0, f64::NAN], 2.0f64.to_bits());
    assert_real_bits(&Min, &[f64::NAN, f64::NAN], 0x7ff8000000000000);
    assert_real_bits(&Max, &[f64::NAN, f64::NAN], 0x7ff8000000000000);
    assert_real_bits(
        &Min,
        &[f64::INFINITY, f64::NEG_INFINITY],
        f64::NEG_INFINITY.to_bits(),
    );
    assert_real_bits(
        &Max,
        &[f64::INFINITY, f64::NEG_INFINITY],
        f64::INFINITY.to_bits(),
    );

    assert_real_bits(&Min, &[-0.0, 0.0], (-0.0f64).to_bits());
    assert_real_bits(&Min, &[0.0, -0.0], (-0.0f64).to_bits());
    assert_real_bits(&Max, &[-0.0, 0.0], 0.0f64.to_bits());
    assert_real_bits(&Max, &[0.0, -0.0], 0.0f64.to_bits());
}

#[test]
fn limiter_signed_zero_clamp_boundaries_are_pinned() {
    // The upstream comparison chain treats +0.0 and -0.0 as EQUAL, so a boundary-equal u takes
    // the passthrough arm and keeps ITS OWN zero sign — the pre-upstream-alignment det chain
    // zero-pinned u=+0.0 against a -0.0 ceiling to -0.0, a bit upstream never produces.
    assert_real_bits(
        &Limiter {
            u_min: -0.0,
            u_max: 1.0,
        },
        &[0.0],
        0.0f64.to_bits(),
    );
    assert_real_bits(
        &Limiter {
            u_min: -1.0,
            u_max: -0.0,
        },
        &[0.0],
        0.0f64.to_bits(),
    );
    // Mirror: a -0.0 input at a +0.0 floor passes through as -0.0.
    assert_real_bits(
        &Limiter {
            u_min: 0.0,
            u_max: 1.0,
        },
        &[-0.0],
        (-0.0f64).to_bits(),
    );
}

#[test]
fn limiter_non_finite_and_boundary_inputs_follow_upstream_comparison_chain() {
    let lim = Limiter {
        u_min: -1.5,
        u_max: 2.5,
    };
    // Upstream Limiter.mo: `if u > uMax then uMax elseif u < uMin then uMin else u`. A NaN u
    // fails both comparisons and PASSES THROUGH fail-visible (canonicalized on emit) instead of
    // being silently absorbed into uMin as the old det chain did — a faulted sensor must stay
    // visible downstream.
    assert_real_bits(&lim, &[f64::NAN], 0x7ff8_0000_0000_0000);
    assert_real_bits(
        &lim,
        &[f64::from_bits(0xfff8_0000_0000_0000)],
        0x7ff8_0000_0000_0000,
    );
    // Infinities clamp to the bounds, exactly as upstream.
    assert_real_bits(&lim, &[f64::INFINITY], 2.5f64.to_bits());
    assert_real_bits(&lim, &[f64::NEG_INFINITY], (-1.5f64).to_bits());
    // Boundary-equal inputs take the passthrough arm and return u's own bits.
    assert_real_bits(&lim, &[2.5], 2.5f64.to_bits());
    assert_real_bits(&lim, &[-1.5], (-1.5f64).to_bits());

    // A NaN bound disables only that side's clamp (load-time validation rejects NaN bounds;
    // this pins the panic-free degrade for unvalidated embedders).
    let nan_floor = Limiter {
        u_min: f64::NAN,
        u_max: 2.5,
    };
    assert_real_bits(&nan_floor, &[-9.0], (-9.0f64).to_bits());
    assert_real_bits(&nan_floor, &[9.0], 2.5f64.to_bits());
}

#[test]
fn limiter_inverted_bounds_degrade_by_chain_order_without_panicking() {
    // Load-time validation rejects u_min > u_max; unvalidated embedders get the upstream
    // chain's arm order: `u > u_max` wins first, then `u < u_min`, else passthrough.
    let inv = Limiter {
        u_min: 10.0,
        u_max: 0.0,
    };
    assert_real_bits(&inv, &[5.0], 0.0f64.to_bits());
    assert_real_bits(&inv, &[-5.0], 10.0f64.to_bits());
    assert_real_bits(&inv, &[f64::NAN], 0x7ff8_0000_0000_0000);
}

#[test]
fn line_signed_zero_clamp_boundaries_follow_current_formula() {
    assert_real_bits(
        &Line::default(),
        &[-0.0, 0.0, 1.0, 1.0, -0.0],
        0.0f64.to_bits(),
    );
    assert_real_bits(
        &Line::default(),
        &[-1.0, -1.0, -0.0, -0.0, 0.0],
        0.0f64.to_bits(),
    );
}

#[test]
fn line_clamps_at_breakpoints_and_degrades_on_degenerate_domain() {
    let inputs = |u| [2.0, 10.0, 6.0, 18.0, u];
    assert_real_bits(&Line::default(), &inputs(1.0), 10.0f64.to_bits());
    assert_real_bits(&Line::default(), &inputs(2.0), 10.0f64.to_bits());
    assert_real_bits(&Line::default(), &inputs(5.0), 16.0f64.to_bits());
    assert_real_bits(&Line::default(), &inputs(6.0), 18.0f64.to_bits());
    assert_real_bits(&Line::default(), &inputs(7.0), 18.0f64.to_bits());

    let degenerate = real_out(
        &Line::default(),
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
fn line_limit_flags_gate_clamping_and_extrapolation() {
    let inputs = |u| [0.0, 0.0, 10.0, 10.0, u];

    let default_both = Line::default();
    let explicit_both = Line {
        limit_below: true,
        limit_above: true,
    };
    let below_only = Line {
        limit_below: true,
        limit_above: false,
    };
    let above_only = Line {
        limit_below: false,
        limit_above: true,
    };
    let neither = Line {
        limit_below: false,
        limit_above: false,
    };

    assert_real_bits(&default_both, &inputs(-5.0), 0.0f64.to_bits());
    assert_real_bits(&default_both, &inputs(15.0), 10.0f64.to_bits());
    assert_real_bits(&explicit_both, &inputs(-5.0), 0.0f64.to_bits());
    assert_real_bits(&explicit_both, &inputs(15.0), 10.0f64.to_bits());

    assert_real_bits(&below_only, &inputs(-5.0), 0.0f64.to_bits());
    assert_real_bits(&below_only, &inputs(15.0), 15.0f64.to_bits());

    assert_real_bits(&above_only, &inputs(-5.0), (-5.0f64).to_bits());
    assert_real_bits(&above_only, &inputs(15.0), 10.0f64.to_bits());

    assert_real_bits(&neither, &inputs(-5.0), (-5.0f64).to_bits());
    assert_real_bits(&neither, &inputs(15.0), 15.0f64.to_bits());
}

#[test]
fn line_pins_current_endpoint_nan_and_inverted_domain_behavior() {
    // Pins the current slope-intercept endpoint behavior pending the canonical-form decision.
    let non_power_two = |u| [0.0, 0.1, 3.0, 1.1, u];
    assert_real_bits(&Line::default(), &non_power_two(-1.0), 0x3fb99999999999a0);
    assert_real_bits(&Line::default(), &non_power_two(4.0), 1.1f64.to_bits());
    assert_real_bits(
        &Line::default(),
        &non_power_two(f64::NAN),
        0x3fb99999999999a0,
    );

    assert_real_bits(
        &Line::default(),
        &[5.0, 0.0, 2.0, 10.0, 3.0],
        10.0f64.to_bits(),
    );
}

#[test]
fn reals_arithmetic_outputs_are_bit_deterministic_across_reruns() {
    let line = Line::default();
    let cases: &[(&dyn Block, &[f64])] = &[
        (&Multiply, &[0.1, 0.2]),
        (&Divide, &[0.0, 0.0]),
        (&Sqrt, &[-1.0]),
        (&Average, &[f64::MAX, f64::MAX]),
        (&Modulo, &[-5.0, -3.0]),
        (&Round { n: 2 }, &[1.125]),
        (&AddParameter { p: 0.2 }, &[0.1]),
        (&Abs, &[-0.0]),
        (&Min, &[f64::NAN, 2.0]),
        (&Max, &[0.0, -0.0]),
        (&line, &[3.0, 4.0, 3.0, 8.0, 3.0]),
    ];
    for &(block, inputs) in cases {
        let first = real_out(block, &real_inputs(inputs));
        let second = real_out(block, &real_inputs(inputs));
        assert_eq!(first.to_bits(), second.to_bits());
    }
}

#[test]
fn read_real_release_degrade_remains_zero_for_reals_blocks() {
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
