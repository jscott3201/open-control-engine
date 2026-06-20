//! M2-PR-A5 tests for dynamic `CDL.Reals` blocks. Expected integrator traces are hand-derived
//! from the documented forward-Euler recurrence, not recorded from the implementation.

use oce_model::{ParamTable, Value};

use super::{Block, BlockKind, Ctx, IntegratorWithReset, NoopDiagnostics};

fn inputs(u: f64, y_reset_in: f64, trigger: bool) -> [Value; 3] {
    [
        Value::Real(u),
        Value::Real(y_reset_in),
        Value::Boolean(trigger),
    ]
}

fn init_region(block: &dyn Block) -> Vec<u64> {
    let mut region = vec![0u64; block.state_len()];
    block.init_state(&mut region, &ParamTable::default());
    region
}

fn emit_real(block: &dyn Block, region: &[u64], t: f64, vals: &[Value]) -> f64 {
    let diag = NoopDiagnostics;
    let cx = Ctx::new(t, &diag);
    let mut out = None;
    block.emit_from_state(&cx, vals, region, &mut |idx, val| {
        assert_eq!(idx, 0, "IntegratorWithReset has one output");
        out = Some(val);
    });
    match out.expect("IntegratorWithReset must emit one output") {
        Value::Real(y) => y,
        other => panic!("expected Real output, got {other:?}"),
    }
}

fn step(block: &dyn Block, region: &mut [u64], t: f64, vals: &[Value]) -> Value {
    let y = Value::Real(emit_real(block, region, t, vals));
    let diag = NoopDiagnostics;
    let cx = Ctx::new(t, &diag);
    block.update_state(&cx, vals, region);
    y
}

fn drive(block: &dyn Block, steps: &[(f64, f64, f64, bool)]) -> (Vec<Value>, Vec<u64>) {
    let mut region = init_region(block);
    let mut trace = Vec::with_capacity(steps.len());
    for &(t, u, y_reset_in, trigger) in steps {
        trace.push(step(block, &mut region, t, &inputs(u, y_reset_in, trigger)));
    }
    (trace, region)
}

fn assert_trace(got: &[Value], want: &[f64]) {
    assert_eq!(got.len(), want.len());
    for (idx, (got, want)) in got.iter().zip(want).enumerate() {
        let want = Value::Real(*want);
        assert!(got.bit_eq(&want), "trace[{idx}] got {got:?}, want {want:?}");
    }
}

fn assert_trace_bits(got: &[Value], want: &[u64]) {
    assert_eq!(got.len(), want.len());
    for (idx, (got, want)) in got.iter().zip(want).enumerate() {
        let want = Value::Real(f64::from_bits(*want));
        assert!(got.bit_eq(&want), "trace[{idx}] got {got:?}, want {want:?}");
    }
}

#[test]
fn integrator_with_reset_state_and_feedthrough_contract() {
    let block = IntegratorWithReset { y_start: 1.0 };
    assert_eq!(
        block.signature().class_path,
        "CDL.Reals.IntegratorWithReset"
    );
    assert_eq!(block.signature().inputs.len(), 3);
    assert_eq!(block.kind(), BlockKind::Stateful);
    assert_eq!(block.state_len(), 3);
    for input in 0..3 {
        assert!(
            !block.feeds_through(input, 0),
            "IntegratorWithReset input {input} must be a loop-cut edge"
        );
    }
}

#[test]
fn forward_euler_variable_dt_trace_is_hand_derived() {
    let block = IntegratorWithReset { y_start: 1.0 };
    let steps = [
        (0.0, 2.0, 0.0, false),
        (0.5, 2.0, 0.0, false),
        (2.0, -1.0, 0.0, false),
        (3.25, 4.0, 0.0, false),
        (5.0, 0.5, 0.0, false),
        (5.5, 10.0, 0.0, false),
    ];
    let (trace, region) = drive(&block, &steps);
    // x0=1.0; first dt=0, then:
    // x2=1+2*0.5=2; x3=2+(-1)*1.5=0.5; x4=0.5+4*1.25=5.5;
    // x5=5.5+0.5*1.75=6.375; final x=6.375+10*0.5=11.375.
    assert_trace(&trace, &[1.0, 1.0, 2.0, 0.5, 5.5, 6.375]);
    assert_eq!(f64::from_bits(region[0]).to_bits(), 11.375f64.to_bits());
}

#[test]
fn forward_euler_non_dyadic_residue_trace_is_hand_derived() {
    let block = IntegratorWithReset { y_start: 0.0 };
    let steps = [
        (0.0, 0.2, 0.0, false),
        (0.1, 0.2, 0.0, false),
        (0.2, 0.2, 0.0, false),
        (0.3, 0.2, 0.0, false),
        (0.4, 0.2, 0.0, false),
        (0.5, 0.2, 0.0, false),
    ];
    let (trace, region) = drive(&block, &steps);
    // Hand-derived from the rounded f64 recurrence. The first nonzero increment is
    // 0.2 * 0.1 = 0x3f947ae147ae147c (clean 0.02 plus one ULP); later timestamp
    // differences use the adjacent 0.1 encodings from 0.3-0.2 and 0.4-0.3.
    assert_trace_bits(
        &trace,
        &[
            0x0000_0000_0000_0000,
            0x0000_0000_0000_0000,
            0x3f94_7ae1_47ae_147c,
            0x3fa4_7ae1_47ae_147c,
            0x3fae_b851_eb85_1eb9,
            0x3fb4_7ae1_47ae_147c,
        ],
    );
    assert_eq!(region[0], 0x3fb9_9999_9999_999a);
}

#[test]
fn reset_is_rising_edge_and_visible_one_tick_later() {
    let block = IntegratorWithReset { y_start: 1.0 };
    let steps = [
        (0.0, 2.0, 0.0, false),
        (1.0, 2.0, 0.0, false),
        (2.0, 99.0, 7.0, true),
        (3.0, 99.0, 11.0, true),
        (4.0, 0.0, 11.0, false),
        (5.0, 0.0, -5.0, true),
        (6.0, 0.0, 0.0, false),
    ];
    let (trace, region) = drive(&block, &steps);
    // Rising trigger at t=2 emits the pre-reset x=3, then stores 7. Held-high at t=3 emits 7
    // and integrates 99 for one second instead of resetting to 11.
    assert_trace(&trace, &[1.0, 1.0, 3.0, 7.0, 106.0, 106.0, -5.0]);
    assert_eq!(f64::from_bits(region[0]).to_bits(), (-5.0f64).to_bits());
}

#[test]
fn first_tick_trigger_high_resets_for_next_emit_only() {
    let block = IntegratorWithReset { y_start: 2.0 };
    let steps = [
        (0.0, 10.0, 7.0, true),
        (1.0, 10.0, 9.0, true),
        (2.0, 0.0, 0.0, false),
    ];
    let (trace, region) = drive(&block, &steps);
    // The tick-0 high trigger is a rising edge from the initialized false state. It emits y_start,
    // stores 7, then the held-high tick integrates instead of re-resetting to 9.
    assert_trace(&trace, &[2.0, 7.0, 17.0]);
    assert_eq!(f64::from_bits(region[0]).to_bits(), 17.0f64.to_bits());
}

#[test]
fn reset_value_path_propagates_extreme_and_nan_bits() {
    let block = IntegratorWithReset { y_start: 0.0 };
    let quiet_nan = f64::from_bits(0x7ff8_0000_0000_0000);
    let max_steps = [
        (0.0, 1.0, 0.0, false),
        (1.0, 1.0, f64::MAX, true),
        (2.0, 0.0, 0.0, false),
    ];
    let (max_trace, max_region) = drive(&block, &max_steps);
    assert_trace_bits(
        &max_trace,
        &[
            0x0000_0000_0000_0000,
            0x0000_0000_0000_0000,
            f64::MAX.to_bits(),
        ],
    );
    assert_eq!(max_region[0], f64::MAX.to_bits());

    let nan_steps = [
        (0.0, 1.0, 0.0, false),
        (1.0, 1.0, quiet_nan, true),
        (2.0, 0.0, 0.0, false),
    ];
    let (nan_trace, _nan_region) = drive(&block, &nan_steps);
    assert_trace_bits(
        &nan_trace,
        &[
            0x0000_0000_0000_0000,
            0x0000_0000_0000_0000,
            0x7ff8_0000_0000_0000,
        ],
    );
}

#[test]
fn zero_dt_negative_u_and_nonzero_start_edges_are_pinned() {
    let block = IntegratorWithReset { y_start: 2.5 };
    let steps = [
        (1.0, -4.0, 0.0, false),
        (1.0, -4.0, 0.0, false),
        (2.0, -4.0, 0.0, false),
    ];
    let (trace, region) = drive(&block, &steps);
    assert_trace(&trace, &[2.5, 2.5, 2.5]);
    assert_eq!(f64::from_bits(region[0]).to_bits(), (-1.5f64).to_bits());
}

#[test]
fn non_finite_integrand_accumulation_is_current_ieee_behavior() {
    let block = IntegratorWithReset { y_start: 0.0 };
    let inf_steps = [
        (0.0, 0.0, 0.0, false),
        (1.0, f64::INFINITY, 0.0, false),
        (2.0, 1.0, 0.0, false),
    ];
    let (inf_trace, inf_region) = drive(&block, &inf_steps);
    assert_trace(&inf_trace, &[0.0, 0.0, f64::INFINITY]);
    assert_eq!(
        f64::from_bits(inf_region[0]).to_bits(),
        f64::INFINITY.to_bits()
    );

    let nan_steps = [
        (0.0, 0.0, 0.0, false),
        (1.0, f64::NAN, 0.0, false),
        (2.0, 1.0, 0.0, false),
    ];
    let (nan_trace, nan_region) = drive(&block, &nan_steps);
    assert!(nan_trace[0].bit_eq(&Value::Real(0.0)));
    assert!(nan_trace[1].bit_eq(&Value::Real(0.0)));
    match nan_trace[2] {
        Value::Real(y) => assert_eq!(y.to_bits(), 0x7ff8000000000000),
        ref other => panic!("expected Real NaN, got {other:?}"),
    }
    assert_eq!(f64::from_bits(nan_region[0]).to_bits(), 0x7ff8000000000000);
}

#[test]
fn integrator_trace_is_byte_deterministic_across_runs() {
    let block = IntegratorWithReset { y_start: -0.25 };
    let steps = [
        (0.0, 1.0, 0.0, false),
        (0.25, 4.0, 0.0, false),
        (1.0, -2.0, 3.5, true),
        (1.5, 100.0, 0.0, true),
        (2.0, 0.5, 0.0, false),
    ];
    let (trace_a, region_a) = drive(&block, &steps);
    let (trace_b, region_b) = drive(&block, &steps);
    for (idx, (a, b)) in trace_a.iter().zip(&trace_b).enumerate() {
        assert!(a.bit_eq(b), "trace[{idx}] diverged: {a:?} vs {b:?}");
    }
    assert_eq!(region_a, region_b, "state words diverged");
}
