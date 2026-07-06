//! Tests for the `CDL.Reals.Derivative` filtered-derivative block: the upstream three-input
//! connector contract (`k`, `T`, `u` all live), pinned recurrence bits, the raw-`T` initial
//! equation, the `100*eps` time-constant floor with its one-shot warn, and non-finite input
//! propagation. Expected traces are hand-derived IEEE-double walks of the documented recurrence
//! `y = (k/T_nonZero)*(u-x)`, `x' = (x + (dt/T_nonZero)*u)/(1 + dt/T_nonZero)`, independent of
//! the engine.

use std::cell::RefCell;

use oce_model::{ParamTable, Value};

use super::{Block, BlockKind, Ctx, Derivative, Diagnostics, NoopDiagnostics};

const CANONICAL_NAN: u64 = 0x7ff8_0000_0000_0000;
const T_FLOOR_WARNING: &str = "Derivative: time-constant input T is NaN or below the 100*eps floor; clamping T_nonZero to 1e-13";

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

fn init_region(block: &dyn Block) -> Vec<u64> {
    let mut region = vec![0u64; block.state_len()];
    block.init_state(&mut region, &ParamTable::default());
    region
}

/// `Derivative` inputs in the upstream connector declaration order `k, T, u`.
fn derivative_inputs(k: f64, t_const: f64, u: f64) -> [Value; 3] {
    [Value::Real(k), Value::Real(t_const), Value::Real(u)]
}

fn derivative_emit(
    block: &Derivative,
    region: &[u64],
    t: f64,
    k: f64,
    t_const: f64,
    u: f64,
) -> Value {
    let diag = NoopDiagnostics;
    let cx = Ctx::new(t, &diag);
    let mut out = None;
    block.emit_from_state(
        &cx,
        &derivative_inputs(k, t_const, u),
        region,
        &mut |idx, val| {
            assert_eq!(idx, 0);
            out = Some(val);
        },
    );
    out.expect("Derivative must emit one output")
}

fn derivative_tick_with_diag(
    block: &Derivative,
    region: &mut [u64],
    t: f64,
    k: f64,
    t_const: f64,
    u: f64,
    diag: &dyn Diagnostics,
) -> Value {
    let cx = Ctx::new(t, diag);
    let mut out = None;
    block.emit_from_state(
        &cx,
        &derivative_inputs(k, t_const, u),
        region,
        &mut |idx, val| {
            assert_eq!(idx, 0);
            out = Some(val);
        },
    );
    block.update_state(&cx, &derivative_inputs(k, t_const, u), region);
    out.expect("Derivative must emit one output")
}

fn derivative_tick(
    block: &Derivative,
    region: &mut [u64],
    t: f64,
    k: f64,
    t_const: f64,
    u: f64,
) -> Value {
    let diag = NoopDiagnostics;
    derivative_tick_with_diag(block, region, t, k, t_const, u, &diag)
}

/// Drive a `Derivative` across `(t, k, T, u)` ticks — `k`/`T` are live inputs upstream.
fn derivative_drive_with_diag(
    block: &Derivative,
    steps: &[(f64, f64, f64, f64)],
    diag: &dyn Diagnostics,
) -> (Vec<Value>, Vec<u64>) {
    let mut region = init_region(block);
    let mut trace = Vec::with_capacity(steps.len());
    for &(t, k, t_const, u) in steps {
        trace.push(derivative_tick_with_diag(
            block,
            &mut region,
            t,
            k,
            t_const,
            u,
            diag,
        ));
    }
    (trace, region)
}

fn derivative_drive(block: &Derivative, steps: &[(f64, f64, f64, f64)]) -> (Vec<Value>, Vec<u64>) {
    let diag = NoopDiagnostics;
    derivative_drive_with_diag(block, steps, &diag)
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
fn derivative_contract_is_three_input_stateful_feedthrough() {
    // Upstream 3-input signature (k, T, u — declaration order); ALL THREE feed through: the
    // gain and time constant are live RealInputs, so a same-tick change to any of them changes y.
    let derivative = Derivative::default();
    assert_eq!(derivative.kind(), BlockKind::Stateful);
    assert_eq!(derivative.signature().inputs.len(), 3);
    assert_eq!(derivative.signature().outputs.len(), 1);
    for input in 0..3 {
        assert!(
            derivative.feeds_through(input, 0),
            "Derivative input {input} must feed through"
        );
    }
    // State: filter word x, previous-time word, and the one-shot T-floor warn latch.
    assert_eq!(derivative.state_len(), 3);
}

#[test]
fn derivative_nan_y_start_seed_emits_canonical_nan_bits() {
    let negative_nan = f64::from_bits(0xfff8_0000_0000_0000);
    let derivative = Derivative {
        y_start: negative_nan,
    };
    // First tick: x0 = u - T*y_start/k is NaN, so y = (k/T_nonZero)*(u - x0) is NaN — emitted
    // canonicalized. k=1, T=0.1 are live inputs.
    assert_real_bits(
        &derivative_emit(&derivative, &init_region(&derivative), 0.0, 1.0, 0.1, 1.0),
        CANONICAL_NAN,
    );
}

#[test]
fn derivative_yd_start_implicit_filter_and_bounded_regime_are_pinned() {
    // Steps are (t, k, T, u): k and T are live inputs held constant here. The pinned bits are
    // re-derived independently (IEEE double walk of the upstream `(k/T_nonZero)*(u-x)`
    // association) and are unchanged from the parameter-era pins.
    let yd = Derivative { y_start: 0.25 };
    let steps = [
        (0.0, 2.0, 1.0, 1.0),
        (0.5, 2.0, 1.0, 2.0),
        (1.0, 2.0, 1.0, 2.0),
    ];
    let (trace, region) = derivative_drive(&yd, &steps);
    // Initial x = u - T*y_start/k = 0.875. With alpha=0.5, x becomes 1.25 then 1.5.
    assert_trace_bits(
        &trace,
        &[
            0x3fd0_0000_0000_0000,
            0x4002_0000_0000_0000,
            0x3ff8_0000_0000_0000,
        ],
    );
    assert_eq!(region[0], 0x3ff8_0000_0000_0000);

    let bounded = Derivative { y_start: 0.0 };
    let steps = [
        (0.0, 1.0, 0.01, 0.0),
        (1.0, 1.0, 0.01, 1.0),
        (2.0, 1.0, 0.01, 1.0),
        (3.0, 1.0, 0.01, 1.0),
        (4.0, 1.0, 0.01, 1.0),
    ];
    let (trace, region) = derivative_drive(&bounded, &steps);
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
fn derivative_gain_and_time_constant_inputs_act_live() {
    // Upstream declares k and T as RealInput connectors: changing either mid-run changes y on
    // the SAME tick (feedthrough), with no re-initialization. Independently derived IEEE walk:
    // t=0.2 emits (2.5/0.5)*(1-x); t=0.3 emits (2.5/0.25)*(1-x); t=0.4 emits 0 (k=0);
    // t=0.5 emits (1/0.25)*(-1-x).
    let block = Derivative { y_start: 0.0 };
    let steps = [
        (0.0, 1.0, 0.5, 0.0),
        (0.1, 1.0, 0.5, 1.0),
        (0.2, 2.5, 0.5, 1.0),
        (0.3, 2.5, 0.25, 1.0),
        (0.4, 0.0, 0.25, 1.0),
        (0.5, 1.0, 0.25, -1.0),
    ];
    let (trace, _region) = derivative_drive(&block, &steps);
    let want = [
        0.0,
        2.0,
        4.166_666_666_666_666,
        6.944_444_444_444_445,
        0.0,
        -6.582_766_439_909_298,
    ];
    for (idx, (got, want)) in trace.iter().zip(want).enumerate() {
        assert!(
            got.bit_eq(&Value::Real(want)),
            "trace[{idx}] got {got:?}, want {want:?}"
        );
    }
}

#[test]
fn derivative_feedthrough_perturbation_and_determinism_are_pinned() {
    // All three inputs (k, T, u) feed through: perturbing any one of them on the emit tick
    // changes y without touching state. Baseline: k=1, T=0.1, x=1 after the t=0 tick.
    let block = Derivative { y_start: 0.0 };
    let mut region = init_region(&block);
    derivative_tick(&block, &mut region, 0.0, 1.0, 0.1, 1.0);
    let low = derivative_emit(&block, &region, 1.0, 1.0, 0.1, 1.0);
    let u_perturbed = derivative_emit(&block, &region, 1.0, 1.0, 0.1, 2.0);
    let k_perturbed = derivative_emit(&block, &region, 1.0, 3.0, 0.1, 2.0);
    let t_perturbed = derivative_emit(&block, &region, 1.0, 1.0, 0.2, 2.0);
    assert_real_bits(&low, 0.0f64.to_bits());
    assert_real_bits(&u_perturbed, 10.0f64.to_bits());
    assert_real_bits(&k_perturbed, 30.0f64.to_bits());
    assert_real_bits(&t_perturbed, 5.0f64.to_bits());

    let steps = [
        (0.0, 1.0, 0.1, 0.2),
        (0.1, 1.0, 0.1, 0.3),
        (0.2, 1.0, 0.1, 0.3),
        (0.4, 1.0, 0.1, 0.1),
    ];
    let (trace_a, region_a) = derivative_drive(&block, &steps);
    let (trace_b, region_b) = derivative_drive(&block, &steps);
    for (idx, (a, b)) in trace_a.iter().zip(&trace_b).enumerate() {
        assert!(a.bit_eq(b), "trace[{idx}] {a:?} vs {b:?}");
    }
    assert_eq!(region_a, region_b);
}

#[test]
fn derivative_time_constant_at_or_below_floor_clamps_and_warns_once() {
    // T at/below the 100*eps floor: zero (the upstream-documented "ideal derivative" wiring),
    // negative, and subnormal all clamp T_nonZero to exactly 1e-13 while the INITIAL equation
    // still uses RAW T. Tick 0 (T=0, y_start=0.5): x0 = u - 0*y_start/k = u exactly, so y must
    // be exactly +0.0 — a mutant that fed the CLAMPED T into the initial equation would emit
    // 0x3fdff973cafa8000 (~0.4996) instead. Tick 1 (T=-1): y = (2/1e-13)*(2-1) = 2e13. Tick 2
    // (T=5e-324 subnormal): y = (1/1e-13)*(2 - x) with x pulled to ~2 by the huge alpha.
    let block = Derivative { y_start: 0.5 };
    let diag = CapturingDiagnostics::default();
    let steps = [
        (0.0, 2.0, 0.0, 1.0),
        (1.0, 2.0, -1.0, 2.0),
        (2.0, 1.0, 5e-324, 2.0),
    ];
    let (trace, region) = derivative_drive_with_diag(&block, &steps, &diag);
    assert_trace_bits(
        &trace,
        &[
            0x0000_0000_0000_0000,
            0x42b2_309c_e540_0000,
            0x3fef_f973_cafa_8000,
        ],
    );
    assert_eq!(region[0], 2.0f64.to_bits());

    // Three consecutive clamped ticks warn exactly ONCE (per-instance latch), at the first one.
    let events = diag.events.borrow();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, "CDL.Reals.Derivative");
    assert_eq!(events[0].1, T_FLOOR_WARNING);
    assert_eq!(events[0].2.to_bits(), 0.0f64.to_bits());
}

#[test]
fn derivative_time_constant_at_floor_boundary_does_not_warn() {
    // Upstream annotates T(min=100*eps): T EQUAL to the floor is legal, must not warn, and
    // passes through the clamp unchanged.
    let block = Derivative { y_start: 0.0 };
    let mut region = init_region(&block);
    let diag = CapturingDiagnostics::default();
    let floor = 100.0 * 1e-15;
    let y = derivative_tick_with_diag(&block, &mut region, 0.0, 1.0, floor, 1.0, &diag);
    // y_start=0 seeds x0 = u, so y = (1/1e-13)*(u - u) = +0.0.
    assert_real_bits(&y, 0.0f64.to_bits());
    assert!(
        diag.events.borrow().is_empty(),
        "T == 100*eps must not warn"
    );
}

#[test]
fn derivative_first_tick_zero_gain_seeds_state_from_u() {
    // The |k| < eps branch of the initial equation is the divide-by-zero guard: with k=0 and
    // y_start != 0 it seeds x0 = u (deleting the guard gives x0 = u - inf = -inf, so y0 = NaN
    // and every later tick is poisoned). k=0 at startup is legitimate autotuner wiring.
    let block = Derivative { y_start: 0.5 };
    let steps = [
        (0.0, 0.0, 0.5, 2.0),
        (0.5, 1.0, 0.5, 2.0),
        (1.0, 1.0, 0.5, 3.0),
    ];
    let mut region = init_region(&block);
    let mut trace = Vec::new();
    for &(t, k, t_const, u) in &steps {
        trace.push(derivative_tick(&block, &mut region, t, k, t_const, u));
    }
    // x0 = 2 (guard), held through the dt=0 first update; t=0.5 emits 2*(2-2)=0; the t=0.5
    // update keeps x=2 (u=x), so t=1.0 emits (1/0.5)*(3-2) = 2.0 — finite, exact.
    assert_trace_bits(
        &trace,
        &[
            0x0000_0000_0000_0000,
            0x0000_0000_0000_0000,
            2.0f64.to_bits(),
        ],
    );

    // Signed-zero pin (TESTING.md boundary): after the t=1.0 update x = 2.5, so a k=0 emit
    // with u-x < 0 is +0.0 * negative = MINUS zero, bit-exact.
    let neg_zero = derivative_emit(&block, &region, 1.5, 0.0, 0.5, 1.0);
    assert_real_bits(&neg_zero, 0x8000_0000_0000_0000);
}

#[test]
fn derivative_mid_run_nan_gain_is_transient_and_state_recovers() {
    // The state update never reads k after the first tick, so a NaN on the k wire yields a
    // canonical-NaN y for THAT tick only; x keeps filtering toward u and the next finite-k tick
    // emits the exact value of the untainted walk: alpha=1 gives x=0.5, y=(1/0.5)*(1-0.5)=1.
    let block = Derivative { y_start: 0.0 };
    let diag = CapturingDiagnostics::default();
    let steps = [
        (0.0, 1.0, 0.5, 0.0),
        (0.5, f64::NAN, 0.5, 1.0),
        (1.0, 1.0, 0.5, 1.0),
    ];
    let (trace, _region) = derivative_drive_with_diag(&block, &steps, &diag);
    assert_trace_bits(
        &trace,
        &[0x0000_0000_0000_0000, CANONICAL_NAN, 1.0f64.to_bits()],
    );
    // The warn channel is for the T floor only; a NaN gain is plain IEEE propagation.
    assert!(diag.events.borrow().is_empty());

    // Infinite gain feeds through as IEEE infinity (u != x), not a clamp or a panic.
    let block = Derivative { y_start: 0.0 };
    let mut region = init_region(&block);
    derivative_tick(&block, &mut region, 0.0, 1.0, 0.5, 0.0);
    let inf_gain = derivative_emit(&block, &region, 0.5, f64::INFINITY, 0.5, 1.0);
    assert_real_bits(&inf_gain, 0x7ff0_0000_0000_0000);
}

#[test]
fn derivative_first_tick_non_finite_seed_poisons_state() {
    // The initial equation x = u - T*y_start/k reads BOTH wires on the first tick: a NaN k (or
    // a NaN T) seeds x = NaN, and the implicit filter keeps NaN forever — later finite inputs
    // do not recover. This is deliberate IEEE propagation, pinned here as canonical-NaN emits.
    let nan_gain = Derivative { y_start: 0.5 };
    let (trace, _region) = derivative_drive(
        &nan_gain,
        &[(0.0, f64::NAN, 0.5, 1.0), (0.5, 1.0, 0.5, 1.0)],
    );
    assert_trace_bits(&trace, &[CANONICAL_NAN, CANONICAL_NAN]);

    // NaN T on the first tick additionally crosses the clamp in update_state, so the one-shot
    // T-floor warn fires at t=0 even though the emitted y is already NaN.
    let nan_time_constant = Derivative { y_start: 0.5 };
    let diag = CapturingDiagnostics::default();
    let (trace, _region) = derivative_drive_with_diag(
        &nan_time_constant,
        &[(0.0, 1.0, f64::NAN, 1.0), (0.5, 1.0, 0.5, 1.0)],
        &diag,
    );
    assert_trace_bits(&trace, &[CANONICAL_NAN, CANONICAL_NAN]);
    let events = diag.events.borrow();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, "CDL.Reals.Derivative");
    assert_eq!(events[0].1, T_FLOOR_WARNING);
    assert_eq!(events[0].2.to_bits(), 0.0f64.to_bits());
}

#[test]
fn derivative_mid_run_nan_time_constant_floors_deterministically_and_warns() {
    // Mid-run NaN T is dropped by det_max on EVERY arch (raw f64::max diverges between aarch64
    // fmaxnm and x86_64 for signaling NaN): T_nonZero = 1e-13, so y = (1/1e-13)*(2-0) = 2e13 —
    // huge but finite and bit-pinned — and the same tick's warn makes the fault visible. The
    // floored alpha drags x to 1.9999999999998002, so the recovered T=0.5 tick emits the tiny
    // residual 7.998046669399628e-13.
    let block = Derivative { y_start: 0.0 };
    let diag = CapturingDiagnostics::default();
    let steps = [
        (0.0, 1.0, 0.5, 0.0),
        (0.5, 1.0, f64::NAN, 2.0),
        (1.0, 1.0, 0.5, 2.0),
    ];
    let (trace, region) = derivative_drive_with_diag(&block, &steps, &diag);
    assert_trace_bits(
        &trace,
        &[
            0x0000_0000_0000_0000,
            0x42b2_309c_e540_0000,
            0x3d6c_2400_0000_0000,
        ],
    );
    assert_eq!(region[0], 0x3fff_ffff_ffff_fc7c);

    let events = diag.events.borrow();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, "CDL.Reals.Derivative");
    assert_eq!(events[0].1, T_FLOOR_WARNING);
    assert_eq!(events[0].2.to_bits(), 0.5f64.to_bits());
}
