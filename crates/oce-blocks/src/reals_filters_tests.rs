//! Tests for the `CDL.Reals` slew block plus the dynamic-block contracts shared with
//! `MovingAverage` (whose own suite is `reals_moving_average_tests`). Expected traces are hand-derived
//! from the documented discrete recurrences. The `Derivative` suite lives in
//! `reals_filters_derivative_tests` (split along the behavioral sub-topic).

use std::sync::Arc;

use oce_model::{ParamTable, Value};

use super::{
    Block, BlockKind, Ctx, Diagnostics, LimitSlewRate, MovingAverage, NoopDiagnostics, lookup,
};

fn input(u: f64) -> [Value; 1] {
    [Value::Real(u)]
}

fn init_region(block: &dyn Block) -> Vec<u64> {
    let mut region = vec![0u64; block.state_len()];
    block.init_state(&mut region, &ParamTable::default());
    region
}

fn emit_real(block: &dyn Block, region: &[u64], t: f64, u: f64) -> Value {
    let diag = NoopDiagnostics;
    let cx = Ctx::new(t, &diag);
    let mut out = None;
    block.emit_from_state(&cx, &input(u), region, &mut |idx, val| {
        assert_eq!(idx, 0);
        out = Some(val);
    });
    out.expect("dynamic Reals block must emit one output")
}

fn tick_with_diag(
    block: &dyn Block,
    region: &mut [u64],
    t: f64,
    u: f64,
    diag: &dyn Diagnostics,
) -> Value {
    let cx = Ctx::new(t, diag);
    let mut out = None;
    block.emit_from_state(&cx, &input(u), region, &mut |idx, val| {
        assert_eq!(idx, 0);
        out = Some(val);
    });
    block.update_state(&cx, &input(u), region);
    out.expect("dynamic Reals block must emit one output")
}

fn tick(block: &dyn Block, region: &mut [u64], t: f64, u: f64) -> Value {
    let diag = NoopDiagnostics;
    tick_with_diag(block, region, t, u, &diag)
}

fn drive(block: &dyn Block, steps: &[(f64, f64)]) -> (Vec<Value>, Vec<u64>) {
    let mut region = init_region(block);
    let mut trace = Vec::with_capacity(steps.len());
    for &(t, u) in steps {
        trace.push(tick(block, &mut region, t, u));
    }
    (trace, region)
}

fn parameter_table(values: &[(&str, Value)]) -> ParamTable {
    ParamTable {
        values: values
            .iter()
            .map(|(name, value)| (Arc::from(*name), value.clone()))
            .collect(),
    }
}

fn registry_trace(parameters: &ParamTable, steps: &[(f64, f64)]) -> Vec<Value> {
    let block = (lookup("CDL.Reals.LimitSlewRate").unwrap().make)(parameters);
    let mut region = vec![0; block.state_len()];
    block.init_state(&mut region, parameters);
    steps
        .iter()
        .map(|&(time, input)| tick(block.as_ref(), &mut region, time, input))
        .collect()
}

fn values_bit_equal(left: &[Value], right: &[Value]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| left.bit_eq(right))
}

#[test]
fn falling_slew_rate_tracks_raising_rate_and_changes_the_trace() {
    let derived = parameter_table(&[
        ("raisingSlewRate", Value::Real(2.0)),
        ("Td", Value::Real(0.01)),
    ]);
    let explicit = parameter_table(&[
        ("raisingSlewRate", Value::Real(2.0)),
        ("fallingSlewRate", Value::Real(-2.0)),
        ("Td", Value::Real(0.01)),
    ]);
    let sensitive = parameter_table(&[
        ("raisingSlewRate", Value::Real(2.0)),
        ("fallingSlewRate", Value::Real(-4.0)),
        ("Td", Value::Real(0.01)),
    ]);
    let steps = [(0.0, 10.0), (1.0, -10.0), (2.0, -10.0)];
    let derived_trace = registry_trace(&derived, &steps);
    assert!(values_bit_equal(
        &derived_trace,
        &registry_trace(&explicit, &steps)
    ));
    assert!(!values_bit_equal(
        &derived_trace,
        &registry_trace(&sensitive, &steps)
    ));
}

fn assert_trace_bits(got: &[Value], want: &[u64]) {
    assert_eq!(got.len(), want.len());
    for (idx, (got, want)) in got.iter().zip(want).enumerate() {
        let want = Value::Real(f64::from_bits(*want));
        assert!(got.bit_eq(&want), "trace[{idx}] got {got:?}, want {want:?}");
    }
}

fn assert_real_bits(got: &Value, want: u64) {
    let want = Value::Real(f64::from_bits(want));
    assert!(got.bit_eq(&want), "got {got:?}, want {want:?}");
}

#[test]
fn dynamic_reals_contracts_are_stateful_feedthrough_not_loop_cuts() {
    let blocks: [&dyn Block; 2] = [&LimitSlewRate::default(), &MovingAverage::default()];
    for block in blocks {
        assert_eq!(block.kind(), BlockKind::Stateful);
        assert_eq!(block.signature().inputs.len(), 1);
        assert_eq!(block.signature().outputs.len(), 1);
        assert!(block.feeds_through(0, 0));
    }
    assert_eq!(LimitSlewRate::default().state_len(), 2);
    assert_eq!(MovingAverage::default().state_len(), 6 + 2 * 2052);
}

#[test]
fn dynamic_reals_outputs_canonicalize_nan_bits() {
    let negative_nan = f64::from_bits(0xfff8_0000_0000_0000);

    let slew = LimitSlewRate {
        enable: false,
        ..LimitSlewRate::default()
    };
    assert_real_bits(
        &emit_real(&slew, &init_region(&slew), 0.0, negative_nan),
        0x7ff8000000000000,
    );

    let moving_average = MovingAverage::default();
    assert_real_bits(
        &emit_real(
            &moving_average,
            &init_region(&moving_average),
            0.0,
            negative_nan,
        ),
        0x7ff8000000000000,
    );
}

#[test]
fn limit_slew_rate_clamps_step_bounds_and_passthrough_disable() {
    let block = LimitSlewRate {
        raising_slew_rate: 2.0,
        falling_slew_rate: -3.0,
        td: 0.1,
        enable: true,
    };
    let steps = [(0.0, 0.0), (1.0, 10.0), (2.0, -10.0)];
    let (trace, region) = drive(&block, &steps);
    assert_trace_bits(
        &trace,
        &[
            0x0000_0000_0000_0000,
            0x4000_0000_0000_0000,
            0xbff0_0000_0000_0000,
        ],
    );
    assert_eq!(region[0], (-1.0f64).to_bits());

    let disabled = LimitSlewRate {
        enable: false,
        ..LimitSlewRate::default()
    };
    let (trace, region) = drive(&disabled, &[(0.0, 5.0), (1.0, -7.0)]);
    assert_trace_bits(&trace, &[5.0f64.to_bits(), (-7.0f64).to_bits()]);
    assert_eq!(region[0], (-7.0f64).to_bits());
}

#[test]
fn limit_slew_rate_implicit_lag_is_bounded_and_fp_residue_is_pinned() {
    let bounded = LimitSlewRate {
        raising_slew_rate: 1_000.0,
        falling_slew_rate: -1_000.0,
        td: 0.01,
        enable: true,
    };
    let (trace, region) = drive(
        &bounded,
        &[(0.0, 0.0), (1.0, 1.0), (2.0, 1.0), (3.0, 1.0), (4.0, 1.0)],
    );
    assert_trace_bits(
        &trace,
        &[
            0x0000_0000_0000_0000,
            0x3fef_aee4_1e6a_7498,
            0x3fef_ff32_6ac1_b00b,
            0x3fef_fffd_f6eb_1b17,
            0x3fef_ffff_fad7_3d19,
        ],
    );
    assert_eq!(region[0], 0x3fef_ffff_fad7_3d19);

    let residue = LimitSlewRate {
        raising_slew_rate: 100.0,
        falling_slew_rate: -100.0,
        td: 1.0,
        enable: true,
    };
    let (trace, region) = drive(&residue, &[(0.0, 0.3), (0.1, 0.5), (0.2, 0.5), (0.4, 0.1)]);
    assert_trace_bits(
        &trace,
        &[
            0x3fd3_3333_3333_3333,
            0x3fd4_5d17_45d1_745c,
            0x3fd5_6be6_9c8f_de24,
            0x3fd2_eafb_e8de_4a30,
        ],
    );
    assert_eq!(region[0], 0x3fd2_eafb_e8de_4a30);
}

#[test]
fn limit_slew_rate_feedthrough_perturbation_and_determinism_are_pinned() {
    let block = LimitSlewRate {
        raising_slew_rate: 10.0,
        falling_slew_rate: -10.0,
        td: 1.0,
        enable: true,
    };
    let mut region = init_region(&block);
    tick(&block, &mut region, 0.0, 1.0);
    let low = emit_real(&block, &region, 0.5, 1.0);
    let high = emit_real(&block, &region, 0.5, 2.0);
    assert_real_bits(&low, 1.0f64.to_bits());
    assert_real_bits(&high, 0x3ff5_5555_5555_5555);

    let steps = [(0.0, 0.1), (0.1, 0.4), (0.3, -0.2), (0.6, 0.7)];
    let (trace_a, region_a) = drive(&block, &steps);
    let (trace_b, region_b) = drive(&block, &steps);
    for (idx, (a, b)) in trace_a.iter().zip(&trace_b).enumerate() {
        assert!(a.bit_eq(b), "trace[{idx}] {a:?} vs {b:?}");
    }
    assert_eq!(region_a, region_b);
}

#[test]
fn limit_slew_rate_bound_holds_over_seeded_dt_input_sweep() {
    let block = LimitSlewRate {
        raising_slew_rate: 0.75,
        falling_slew_rate: -1.25,
        td: 0.05,
        enable: true,
    };
    let mut region = init_region(&block);
    let mut seed = 0x0ce5_1eed_u64;
    let mut t = 0.0;
    let mut rising_clamp_hit = false;
    let mut falling_clamp_hit = false;
    for step in 0..128 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let dt = 0.001 + f64::from(((seed >> 32) & 0xff) as u32) / 1270.0;
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let u = f64::from(((seed >> 32) & 0x3ff) as u32) / 102.3 - 5.0;
        t += dt;
        let prev_state = (step > 0).then(|| f64::from_bits(region[0]));
        let effective_dt = (step > 0).then(|| t - f64::from_bits(region[1]));

        let Value::Real(y) = tick(&block, &mut region, t, u) else {
            panic!("LimitSlewRate emits Real");
        };
        if let (Some(prev), Some(dt_eff)) = (prev_state, effective_dt) {
            let upper = block.raising_slew_rate * dt_eff;
            let lower = block.falling_slew_rate * dt_eff;
            let dy = y - prev;
            assert!(
                dy >= lower - 1e-12,
                "dy={dy} below falling bound at dt={dt_eff}"
            );
            assert!(
                dy <= upper + 1e-12,
                "dy={dy} above raising bound at dt={dt_eff}"
            );
            let alpha = dt_eff / block.td;
            let requested = (prev + alpha * u) / (1.0 + alpha);
            let requested_dy = requested - prev;
            if requested_dy > upper {
                assert_eq!(y.to_bits(), (prev + upper).to_bits());
                rising_clamp_hit = true;
            } else if requested_dy < lower {
                assert_eq!(y.to_bits(), (prev + lower).to_bits());
                falling_clamp_hit = true;
            }
        }
    }
    assert!(
        rising_clamp_hit,
        "seeded sweep must exercise the rising clamp"
    );
    assert!(
        falling_clamp_hit,
        "seeded sweep must exercise the falling clamp"
    );
}
