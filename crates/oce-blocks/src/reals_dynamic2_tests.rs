//! M2-PR-A6b tests for Derivative, LimitSlewRate, and MovingAverage.
//! Expected traces are hand-derived from the documented discrete recurrences.

use std::cell::RefCell;

use oce_model::{ParamTable, Value};

use super::{
    Block, BlockKind, Ctx, Derivative, Diagnostics, LimitSlewRate, MovingAverage, NoopDiagnostics,
};

#[derive(Default)]
struct CapturingDiagnostics {
    events: RefCell<Vec<(String, String, f64)>>,
}

impl Diagnostics for CapturingDiagnostics {
    fn warn(&self, source: &str, message: &str, t: f64) {
        self.events
            .borrow_mut()
            .push((source.to_string(), message.to_string(), t));
    }
}

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
    let blocks: [&dyn Block; 3] = [
        &Derivative::default(),
        &LimitSlewRate::default(),
        &MovingAverage::default(),
    ];
    for block in blocks {
        assert_eq!(block.kind(), BlockKind::Stateful);
        assert_eq!(block.signature().inputs.len(), 1);
        assert_eq!(block.signature().outputs.len(), 1);
        assert!(block.feeds_through(0, 0));
    }
    assert_eq!(Derivative::default().state_len(), 2);
    assert_eq!(LimitSlewRate::default().state_len(), 2);
    assert_eq!(MovingAverage::default().state_len(), 133);
}

#[test]
fn derivative_yd_start_implicit_filter_and_bounded_regime_are_pinned() {
    let yd = Derivative {
        k: 2.0,
        t: 1.0,
        y_start: 0.25,
    };
    let steps = [(0.0, 1.0), (0.5, 2.0), (1.0, 2.0)];
    let (trace, region) = drive(&yd, &steps);
    // Initial x=1 - T*y_start/k = 0.875. With alpha=0.5, x becomes 1.25 then 1.5.
    assert_trace_bits(
        &trace,
        &[
            0x3fd0_0000_0000_0000,
            0x4002_0000_0000_0000,
            0x3ff8_0000_0000_0000,
        ],
    );
    assert_eq!(region[0], 0x3ff8_0000_0000_0000);

    let bounded = Derivative {
        k: 1.0,
        t: 0.01,
        y_start: 0.0,
    };
    let steps = [(0.0, 0.0), (1.0, 1.0), (2.0, 1.0), (3.0, 1.0), (4.0, 1.0)];
    let (trace, region) = drive(&bounded, &steps);
    // T=0.01, alpha=100. Explicit Euler would diverge; implicit Euler leaves residual /101.
    assert_trace_bits(
        &trace,
        &[
            0x0000_0000_0000_0000,
            0x4059_0000_0000_0000,
            0x3fef_aee4_1e6a_74a0,
            0x3f84_1393_15ce_ed00,
            0x3f19_7185_2d60_8000,
        ],
    );
    assert_eq!(region[0], 0x3fef_ffff_fad7_3d19);
}

#[test]
fn derivative_feedthrough_perturbation_and_determinism_are_pinned() {
    let block = Derivative::default();
    let mut region = init_region(&block);
    tick(&block, &mut region, 0.0, 1.0);
    let low = emit_real(&block, &region, 1.0, 1.0);
    let high = emit_real(&block, &region, 1.0, 2.0);
    assert_real_bits(&low, 0.0f64.to_bits());
    assert_real_bits(&high, 10.0f64.to_bits());

    let steps = [(0.0, 0.2), (0.1, 0.3), (0.2, 0.3), (0.4, 0.1)];
    let (trace_a, region_a) = drive(&block, &steps);
    let (trace_b, region_b) = drive(&block, &steps);
    for (idx, (a, b)) in trace_a.iter().zip(&trace_b).enumerate() {
        assert!(a.bit_eq(b), "trace[{idx}] {a:?} vs {b:?}");
    }
    assert_eq!(region_a, region_b);
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
fn moving_average_startup_steady_and_variable_dt_window_are_pinned() {
    let block = MovingAverage { delta: 1.0 };
    let steps = [(0.0, 2.0), (0.5, 2.0), (1.0, 4.0), (1.5, 4.0), (2.25, 1.0)];
    let (trace, region) = drive(&block, &steps);
    assert_trace_bits(
        &trace,
        &[
            0x0000_0000_0000_0000,
            0x3fff_efa6_115f_8d8a,
            0x4008_0000_0000_0000,
            0x4010_0000_0000_0000,
            0x3ffc_0000_0000_0000,
        ],
    );
    assert_eq!(region[0], 5.75f64.to_bits());
}

#[test]
fn moving_average_non_dyadic_residue_feedthrough_and_determinism_are_pinned() {
    let block = MovingAverage { delta: 0.3 };
    let steps = [
        (0.0, 0.2),
        (0.1, 0.2),
        (0.2, 0.2),
        (0.3, 0.2),
        (0.4, 0.2),
        (0.5, 0.2),
    ];
    let (trace, region) = drive(&block, &steps);
    assert_trace_bits(
        &trace,
        &[
            0x0000_0000_0000_0000,
            0x3fc9_58b6_7ebb_907b,
            0x3fc9_78fe_b9f3_4382,
            0x3fc9_9999_9999_999a,
            0x3fc9_9999_9999_999a,
            0x3fc9_9999_9999_999a,
        ],
    );
    assert_eq!(region[0], 0x3fb9_9999_9999_999a);

    let mut one_tick = init_region(&block);
    tick(&block, &mut one_tick, 0.0, 1.0);
    let low = emit_real(&block, &one_tick, 0.1, 1.0);
    let high = emit_real(&block, &one_tick, 0.1, 3.0);
    assert!(
        !low.bit_eq(&high),
        "MovingAverage output must depend on current u in the same tick"
    );

    let (trace_a, region_a) = drive(&block, &steps);
    let (trace_b, region_b) = drive(&block, &steps);
    for (idx, (a, b)) in trace_a.iter().zip(&trace_b).enumerate() {
        assert!(a.bit_eq(b), "trace[{idx}] {a:?} vs {b:?}");
    }
    assert_eq!(region_a, region_b);
}

#[test]
fn moving_average_ring_overflow_warns_instead_of_panicking() {
    let block = MovingAverage { delta: 100.0 };
    let mut region = init_region(&block);
    let diag = CapturingDiagnostics::default();
    for k in 0..=64 {
        tick_with_diag(&block, &mut region, f64::from(k), 1.0, &diag);
    }
    let events = diag.events.borrow();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, "CDL.Reals.MovingAverage");
    assert_eq!(
        events[0].1,
        "MovingAverage: checkpoint ring capacity exceeded; oldest in-window sample dropped"
    );
    assert_eq!(events[0].2.to_bits(), 64.0f64.to_bits());
}
