//! Tests for dynamic `CDL.Reals` integrator blocks. Expected integrator traces are hand-derived
//! from the documented forward-Euler recurrence and the reset-at-the-event rule
//! (`docs/design/reset-timing.md` §2), not recorded from the implementation.

use oce_model::{ParamTable, Value};

use super::{Block, BlockKind, Ctx, EventInputRole, IntegratorWithReset, NoopDiagnostics};

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
    let block = IntegratorWithReset {
        k: 1.0,
        y_start: 1.0,
    };
    assert_eq!(
        block.signature().class_path,
        "CDL.Reals.IntegratorWithReset"
    );
    assert_eq!(block.signature().inputs.len(), 3);
    assert_eq!(block.kind(), BlockKind::Stateful);
    assert_eq!(block.state_len(), 3);
    // `u` stays a loop cut; the reset frame emits this frame's `y_reset_in`, so `y_reset_in` and
    // `trigger` feed through.
    assert!(!block.feeds_through(0, 0), "u must stay a loop-cut edge");
    assert!(block.feeds_through(1, 0), "y_reset_in must feed through");
    assert!(block.feeds_through(2, 0), "trigger must feed through");
    assert_eq!(block.event_input_role(0), None);
    assert_eq!(block.event_input_role(1), Some(EventInputRole::ResetValue));
    assert_eq!(block.event_input_role(2), Some(EventInputRole::Condition));
    assert_eq!(block.event_input_role(3), None);
}

#[test]
fn forward_euler_variable_dt_trace_is_hand_derived() {
    let block = IntegratorWithReset {
        k: 1.0,
        y_start: 1.0,
    };
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
    let block = IntegratorWithReset {
        k: 1.0,
        y_start: 0.0,
    };
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
fn reset_applies_at_the_frame_where_the_trigger_rises() {
    let block = IntegratorWithReset {
        k: 1.0,
        y_start: 1.0,
    };
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
    // x = 1, 1 (dt = 0), 3 after t=1. The rising trigger at t=2 emits 7 and stores it with no
    // Euler step. Held high at t=3: emits 7, integrates 99 for one second (106) instead of
    // resetting to 11. t=4 emits 106. The trigger rises again at t=5: emits and stores -5.
    assert_trace(&trace, &[1.0, 1.0, 7.0, 7.0, 106.0, -5.0, -5.0]);
    assert_eq!(f64::from_bits(region[0]).to_bits(), (-5.0f64).to_bits());
}

#[test]
fn trigger_high_at_frame_zero_is_initialisation_not_a_reset() {
    let block = IntegratorWithReset {
        k: 1.0,
        y_start: 2.0,
    };
    let steps = [
        (0.0, 10.0, 7.0, true),
        (1.0, 10.0, 9.0, true),
        (2.0, 0.0, 0.0, false),
        (3.0, 0.0, 4.0, true),
    ];
    let (trace, region) = drive(&block, &steps);
    // Frame 0 is initialisation: the `when` is inactive, so y = y_start = 2 and the high trigger
    // only becomes pre(trigger). Held high at t=1 is no event either: emits 2, integrates 10 for
    // one second (12). t=2 emits 12. The first false-to-true transition, at t=3, resets to 4.
    assert_trace(&trace, &[2.0, 2.0, 12.0, 4.0]);
    assert_eq!(f64::from_bits(region[0]).to_bits(), 4.0f64.to_bits());
}

#[test]
fn reset_value_path_propagates_extreme_and_nan_bits() {
    let block = IntegratorWithReset {
        k: 1.0,
        y_start: 0.0,
    };
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
            f64::MAX.to_bits(),
            f64::MAX.to_bits(),
        ],
    );
    assert_eq!(max_region[0], f64::MAX.to_bits());

    // An infinite reset value is not trapped: emitted at the event and held.
    for infinity in [f64::INFINITY, f64::NEG_INFINITY] {
        let inf_steps = [
            (0.0, 1.0, 0.0, false),
            (1.0, 1.0, infinity, true),
            (2.0, 1.0, 0.0, true),
        ];
        let (inf_trace, inf_region) = drive(&block, &inf_steps);
        assert_trace_bits(&inf_trace, &[0, infinity.to_bits(), infinity.to_bits()]);
        assert_eq!(inf_region[0], infinity.to_bits());
    }

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
            0x7ff8_0000_0000_0000,
            0x7ff8_0000_0000_0000,
        ],
    );
    // A NaN with a payload and its sign set is emitted canonical at the event frame.
    let payload_nan = f64::from_bits(0xfff8_0000_dead_beef);
    let payload_steps = [(0.0, 1.0, 0.0, false), (1.0, 1.0, payload_nan, true)];
    let (payload_trace, _) = drive(&block, &payload_steps);
    assert_trace_bits(&payload_trace, &[0, 0x7ff8_0000_0000_0000]);
}

#[test]
fn zero_dt_negative_u_and_nonzero_start_edges_are_pinned() {
    let block = IntegratorWithReset {
        k: 1.0,
        y_start: 2.5,
    };
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
    let block = IntegratorWithReset {
        k: 1.0,
        y_start: 0.0,
    };
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
fn gain_scales_the_integrand_per_upstream_der_y_eq_k_times_u() {
    // Upstream Buildings `Reals/IntegratorWithReset.mo`: `der(y) = k*u`. Hand-derived with
    // k = 2.5: x0 = 1; x(t=0.5 update) = 1 + 2.5*2.0*0.5 = 3.5; x(t=1.5 update) =
    // 3.5 + 2.5*(-1.0)*1.0 = 1.0. Emits show the prior accumulator.
    let block = IntegratorWithReset {
        k: 2.5,
        y_start: 1.0,
    };
    let steps = [
        (0.0, 2.0, 0.0, false),
        (0.5, 2.0, 0.0, false),
        (1.5, -1.0, 0.0, false),
    ];
    let (trace, region) = drive(&block, &steps);
    assert_trace(&trace, &[1.0, 1.0, 3.5]);
    assert_eq!(f64::from_bits(region[0]).to_bits(), 1.0f64.to_bits());
}

#[test]
fn reset_value_is_not_scaled_by_gain() {
    // Upstream `reinit(y, y_reset_in)` assigns the state directly; the gain only scales the
    // integrand. With k = 3.0 the rising trigger at t=1 must emit and store exactly 7.0, not
    // 21.0, and the frame's Euler step (3 * 1 * 1) is not added.
    let block = IntegratorWithReset {
        k: 3.0,
        y_start: 0.0,
    };
    let steps = [
        (0.0, 1.0, 0.0, false),
        (1.0, 1.0, 7.0, true),
        (2.0, 0.0, 0.0, false),
    ];
    let (trace, region) = drive(&block, &steps);
    assert_trace(&trace, &[0.0, 7.0, 7.0]);
    assert_eq!(f64::from_bits(region[0]).to_bits(), 7.0f64.to_bits());
}

#[test]
fn default_gain_is_upstream_unity() {
    // Buildings declares `parameter Real k=1`; the bare Rust Default must match so direct
    // construction and an omitted CXF `k` agree.
    let block = IntegratorWithReset::default();
    assert_eq!(block.k.to_bits(), 1.0f64.to_bits());
    assert_eq!(block.y_start.to_bits(), 0.0f64.to_bits());
}

#[test]
fn integrator_trace_is_byte_deterministic_across_runs() {
    let block = IntegratorWithReset {
        k: 1.0,
        y_start: -0.25,
    };
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

#[test]
fn nan_y_start_seed_emits_canonical_nan_and_rising_trigger_recovers() {
    // Gain and start-state params carry NO finiteness rule by policy (upstream declares k=1,
    // y_start=0 with no min/max): a NaN y_start follows plain IEEE propagation — canonical-NaN
    // emits — until a rising trigger stores y_reset_in directly, which reads neither k nor
    // y_start and fully recovers the state.
    let block = IntegratorWithReset {
        k: 1.0,
        y_start: f64::from_bits(0xfff8_0000_0000_0000),
    };
    let steps = [
        (0.0, 1.0, 7.0, false),
        (0.5, 1.0, 7.0, false),
        (1.0, 1.0, 7.0, true),
        (1.5, 2.0, 7.0, true),
        (2.0, 0.0, 7.0, false),
    ];
    let (trace, region) = drive(&block, &steps);
    // The t=1.0 rising trigger resets at its own frame: it emits 7.0, the held-high t=1.5 frame
    // emits 7.0 and integrates 7 + 1*2*0.5 = 8, emitted at t=2.0.
    assert_trace_bits(
        &trace,
        &[
            0x7ff8_0000_0000_0000,
            0x7ff8_0000_0000_0000,
            7.0f64.to_bits(),
            7.0f64.to_bits(),
            8.0f64.to_bits(),
        ],
    );
    assert_eq!(region[0], 8.0f64.to_bits());
}

#[test]
fn non_finite_gain_poisons_state_at_first_update_via_ieee() {
    // k is unconstrained by policy (upstream k=1, no min/max). Emit-before-update means the
    // first tick still emits y_start; the first update computes slope = k*u with dt = 0, so
    // NaN*u (NaN) and inf*u*0 (inf*0 = NaN) both poison x at the very first update.
    for poisoned_gain in [f64::NAN, f64::INFINITY] {
        let block = IntegratorWithReset {
            k: poisoned_gain,
            y_start: 0.5,
        };
        let steps = [(0.0, 2.0, 0.0, false), (0.5, 2.0, 0.0, false)];
        let (trace, region) = drive(&block, &steps);
        assert_trace_bits(&trace, &[0.5f64.to_bits(), 0x7ff8_0000_0000_0000]);
        assert!(
            f64::from_bits(region[0]).is_nan(),
            "gain {poisoned_gain} must poison the accumulator"
        );
    }
}

#[test]
fn hand_derivation_of_the_design_note_is_reproduced() {
    // `docs/design/reset-timing.md` §3: k = 1, y_start = 0, u = 1, y_reset_in = 10, frames 1 s
    // apart. Rising at t = 2: 0, 0, 10, 10, 11. True from t = 0: 0, 0, 1, 2.
    let block = IntegratorWithReset {
        k: 1.0,
        y_start: 0.0,
    };
    let rising = [false, false, true, true, true];
    let steps: Vec<_> = rising
        .iter()
        .enumerate()
        .map(|(i, &trigger)| (i as f64, 1.0, 10.0, trigger))
        .collect();
    let (trace, _) = drive(&block, &steps);
    assert_trace(&trace, &[0.0, 0.0, 10.0, 10.0, 11.0]);

    let steps: Vec<_> = (0..4).map(|i| (f64::from(i), 1.0, 10.0, true)).collect();
    let (trace, _) = drive(&block, &steps);
    assert_trace(&trace, &[0.0, 0.0, 1.0, 2.0]);
}

#[test]
fn reset_takes_the_reset_value_of_its_own_frame() {
    // y_reset_in steps 10 -> 20 in the frame where the trigger rises: the reset uses 20 (this
    // frame's input), and later steps of y_reset_in are ignored while the trigger stays high.
    let block = IntegratorWithReset {
        k: 1.0,
        y_start: 0.0,
    };
    let steps = [
        (0.0, 1.0, 10.0, false),
        (1.0, 1.0, 10.0, false),
        (2.0, 1.0, 20.0, true),
        (3.0, 1.0, 30.0, true),
        (4.0, 1.0, 40.0, true),
    ];
    let (trace, _) = drive(&block, &steps);
    assert_trace(&trace, &[0.0, 0.0, 20.0, 20.0, 21.0]);
}

#[test]
fn rising_at_frame_one_and_alternating_triggers_reset_at_each_rise() {
    // Frame 1 is the first frame that can hold an event. F,T,F,T,F,T with u = 1 and reset 5:
    // each rise emits 5 and stores it without a step; each falling frame emits 5 and steps to 6.
    let block = IntegratorWithReset {
        k: 1.0,
        y_start: 0.0,
    };
    let steps: Vec<_> = [false, true, false, true, false, true]
        .iter()
        .enumerate()
        .map(|(i, &trigger)| (i as f64, 1.0, 5.0, trigger))
        .collect();
    let (trace, region) = drive(&block, &steps);
    assert_trace(&trace, &[0.0, 5.0, 5.0, 5.0, 5.0, 5.0]);
    assert_eq!(region[0], 5.0f64.to_bits());
}

#[test]
fn reset_at_a_repeated_timestamp_emits_the_reset_value() {
    // Equal timestamps give dt = 0; a trigger rising at the repeated time still resets at its own
    // frame, and the held frame at the same time adds nothing.
    let block = IntegratorWithReset {
        k: 1.0,
        y_start: 0.0,
    };
    let steps = [
        (0.0, 1.0, 10.0, false),
        (1.0, 1.0, 10.0, false),
        (1.0, 1.0, 10.0, true),
        (1.0, 1.0, 10.0, true),
        (2.0, 1.0, 10.0, true),
    ];
    let (trace, region) = drive(&block, &steps);
    // x: 0, 0 -> 1 after t=1; t=1 (rising) emits 10, stores 10; t=1 held: emits 10, dt = 0;
    // t=2: emits 10, integrates to 11.
    assert_trace(&trace, &[0.0, 0.0, 10.0, 10.0, 10.0]);
    assert_eq!(region[0], 11.0f64.to_bits());
}
