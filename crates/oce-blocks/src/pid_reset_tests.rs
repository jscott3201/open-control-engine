//! Reset timing of `CDL.Reals.PIDWithReset`: the reset applies at the frame where `trigger`
//! rises, and no reset fires at frame 0 (`docs/design/reset-timing.md` §2, decisions 4, 5 and 8).
//! Expected traces are hand-derived from the documented recurrence, not recorded from the
//! implementation.

use oce_model::{SimpleController, Value};

use super::{Block, BlockKind, Ctx, NoopDiagnostics, PidWithReset};
use crate::pid::ControllerConfig;

/// `k = 1`, `Ti = 1`, `xi_start = 0`, limits `[-100, 100]`, reset target `y_reset`.
fn controller(mode: SimpleController, y_reset: f64) -> PidWithReset {
    PidWithReset {
        config: ControllerConfig {
            controller_type: mode,
            k: 1.0,
            ti: 1.0,
            td: 0.5,
            y_reset,
            y_min: -100.0,
            y_max: 100.0,
            ..ControllerConfig::default()
        },
    }
}

fn emit(block: &dyn Block, inputs: &[Value], region: &[u64], t: f64) -> f64 {
    let diag = NoopDiagnostics;
    let cx = Ctx::new(t, &diag);
    let mut out = None;
    let mut sink = |_: usize, value: Value| out = Some(value);
    if block.kind() == BlockKind::Algebraic {
        block.step_algebraic(&cx, inputs, &mut sink);
    } else {
        block.emit_from_state(&cx, inputs, region, &mut sink);
    }
    match out.expect("PIDWithReset emits one output") {
        Value::Real(y) => y,
        other => panic!("expected Real, got {other:?}"),
    }
}

/// Run `(t, u_s, u_m, trigger)` frames from the initial state; returns the outputs and the state.
fn drive(block: &dyn Block, steps: &[(f64, f64, f64, bool)]) -> (Vec<f64>, Vec<u64>) {
    let mut region = vec![0u64; block.state_len()];
    block.init_state(&mut region, &oce_model::ParamTable::default());
    let mut trace = Vec::new();
    for &(t, u_s, u_m, trigger) in steps {
        let inputs = [Value::Real(u_s), Value::Real(u_m), Value::Boolean(trigger)];
        trace.push(emit(block, &inputs, &region, t));
        if block.kind() == BlockKind::Stateful {
            let diag = NoopDiagnostics;
            block.update_state(&Ctx::new(t, &diag), &inputs, &mut region);
        }
    }
    (trace, region)
}

/// `u_m = 0` frames one second apart with the given setpoints and triggers.
fn frames(u_s: &[f64], trigger: &[bool]) -> Vec<(f64, f64, f64, bool)> {
    u_s.iter()
        .zip(trigger)
        .enumerate()
        .map(|(i, (&u, &t))| (i as f64, u, 0.0, t))
        .collect()
}

fn assert_bits(got: &[f64], want: &[f64]) {
    assert_eq!(got.len(), want.len(), "{got:?} vs {want:?}");
    for (i, (g, w)) in got.iter().zip(want).enumerate() {
        assert_eq!(g.to_bits(), w.to_bits(), "frame {i}: got {g:?}, want {w:?}");
    }
}

fn bools(pattern: &str) -> Vec<bool> {
    pattern.chars().map(|c| c == 'T').collect()
}

#[test]
fn hand_derivation_of_the_design_note_is_reproduced() {
    // `docs/design/reset-timing.md` §3: PI, k = 1, Ti = 1, xi_start = 0, y_reset = 5, e = 1.
    // Rising at t = 2: frame 2 emits lim(5) = 5 with I = 5 - 1 = 4 held, frame 3 emits 5,
    // frame 4 emits 6. True from t = 0: no reset; I = 0, 0, 1, 2; outputs 1, 1, 2, 3.
    for mode in [SimpleController::Pi, SimpleController::Pid] {
        let block = controller(mode, 5.0);
        let (rising, _) = drive(&block, &frames(&[1.0; 5], &bools("FFTTT")));
        let (from_start, _) = drive(&block, &frames(&[1.0; 4], &bools("TTTT")));
        if mode == SimpleController::Pi {
            assert_bits(&rising, &[1.0, 1.0, 5.0, 5.0, 6.0]);
            assert_bits(&from_start, &[1.0, 1.0, 2.0, 3.0]);
        } else {
            // With a constant error the derivative term is yd_start = 0 at frame 0 and then
            // (k Td / (Td/Nd)) (e - xD) with xD seeded to e, so 0 throughout: PID equals PI here.
            assert_bits(&rising, &[1.0, 1.0, 5.0, 5.0, 6.0]);
            assert_bits(&from_start, &[1.0, 1.0, 2.0, 3.0]);
        }
    }
}

#[test]
fn trigger_true_from_frame_zero_never_resets_even_with_p_plus_d_nonzero() {
    // y_reset = 5 differs from xi_start = 0, and P + D = 2 at frame 0, so a frame-0 reset would
    // be visible (5 instead of 2). Frame 0 is initialisation; the held trigger is no event.
    let block = controller(SimpleController::Pi, 5.0);
    let (trace, region) = drive(&block, &frames(&[2.0; 4], &bools("TTTT")));
    assert_bits(&trace, &[2.0, 2.0, 4.0, 6.0]);
    assert_eq!(region[2], 1, "the held trigger is latched as pre(trigger)");
    // y_reset equal to xi_start but P + D = 2: a frame-0 reset would back-solve I = -2 and emit
    // 0 instead of 2; none fires.
    let block = controller(SimpleController::Pi, 0.0);
    let (trace, _) = drive(&block, &frames(&[2.0; 3], &bools("TTT")));
    assert_bits(&trace, &[2.0, 2.0, 4.0]);
    // The first event is the next false-to-true transition.
    let block = controller(SimpleController::Pi, 5.0);
    let (trace, _) = drive(&block, &frames(&[1.0; 5], &bools("TTFTT")));
    // I: 0, 0 -> 1, 1 -> 2; frame 3 rises: I = 5 - 1 = 4, emits 5; frame 4 emits 5.
    assert_bits(&trace, &[1.0, 1.0, 2.0, 5.0, 5.0]);
}

#[test]
fn trigger_rising_at_frame_one_resets_at_frame_one() {
    let block = controller(SimpleController::Pi, 5.0);
    let (trace, _) = drive(&block, &frames(&[1.0; 5], &bools("FTTTT")));
    assert_bits(&trace, &[1.0, 5.0, 5.0, 6.0, 7.0]);
}

#[test]
fn falling_and_rising_on_consecutive_frames_resets_at_every_rise() {
    // Each rise emits 5 and stores I = 4 without a step; each falling frame emits 5 and steps
    // I to 5, so the next rise back-solves to 4 again.
    let block = controller(SimpleController::Pi, 5.0);
    let (trace, region) = drive(&block, &frames(&[1.0; 6], &bools("FTFTFT")));
    assert_bits(&trace, &[1.0, 5.0, 5.0, 5.0, 5.0, 5.0]);
    assert_eq!(region[0], 4.0f64.to_bits());
}

#[test]
fn reset_uses_the_setpoint_and_measurement_of_its_own_frame() {
    // u_s steps 1 -> 3 in the frame where the trigger rises: P = 3 there, so I = 5 - 3 = 2 and
    // the frame emits 5. Frame 3 emits 3 + 2 = 5 and steps I to 5; frame 4 emits 8.
    let block = controller(SimpleController::Pi, 5.0);
    let (trace, _) = drive(&block, &frames(&[1.0, 1.0, 3.0, 3.0, 3.0], &bools("FFTTT")));
    assert_bits(&trace, &[1.0, 1.0, 5.0, 5.0, 8.0]);
    // The same with the step in u_m (e = u_s - u_m, reverse acting by default): u_m 0 -> -2.
    let steps = [
        (0.0, 1.0, 0.0, false),
        (1.0, 1.0, 0.0, false),
        (2.0, 1.0, -2.0, true),
        (3.0, 1.0, -2.0, true),
        (4.0, 1.0, -2.0, true),
    ];
    let (trace, _) = drive(&block, &steps);
    assert_bits(&trace, &[1.0, 1.0, 5.0, 5.0, 8.0]);
}

#[test]
fn reset_outside_the_limits_is_clamped_at_the_event() {
    // y_reset = 500 with yMax = 100: the rising frame back-solves I = 499 and emits lim(500) =
    // 100. The held frame also emits 100, and its anti-windup step pulls I back:
    // I = 499 + (1 - (500 - 100) / (k Ni)) * 1 with Ni = 0.9.
    let block = controller(SimpleController::Pi, 500.0);
    let (trace, _) = drive(&block, &frames(&[1.0; 5], &bools("FFTTT")));
    let i_after = 499.0 + (1.0 - (500.0 - 100.0) / (1.0 * 0.9)) * 1.0;
    assert_bits(&trace, &[1.0, 1.0, 100.0, 100.0, 1.0 + i_after]);
    // Below the lower limit: y_reset = -500 emits yMin = -100.
    let block = controller(SimpleController::Pi, -500.0);
    let (trace, _) = drive(&block, &frames(&[1.0; 3], &bools("FFT")));
    assert_bits(&trace, &[1.0, 1.0, -100.0]);
}

#[test]
fn reset_frame_emits_through_the_normal_path_not_y_reset_exactly() {
    // Decision 8: the frame emits P + D + (y_reset - (P + D)), which can differ from y_reset by
    // an ulp. With e = 0.7 and y_reset = 0.1: 0.7 + (0.1 - 0.7) = 0.09999999999999998.
    let block = controller(SimpleController::Pi, 0.1);
    let (trace, region) = drive(&block, &frames(&[0.7; 3], &bools("FFT")));
    let pd: f64 = 0.7;
    let expected = pd + (0.1 - pd);
    assert_ne!(
        expected.to_bits(),
        0.1f64.to_bits(),
        "the example must show the ulp"
    );
    assert_bits(&trace, &[0.7, 0.7, expected]);
    assert_eq!(region[0], (0.1 - pd).to_bits());
}

#[test]
fn repeated_equal_timestamps_do_not_disturb_the_reset() {
    // t = 0, 1, 1, 1, 2. I: 0, then 1 after t = 1; the rising frame at the repeated t = 1 emits
    // 5 and stores 4; the held frame at the same time adds nothing (dt = 0); t = 2 emits 5.
    let block = controller(SimpleController::Pi, 5.0);
    let steps = [
        (0.0, 1.0, 0.0, false),
        (1.0, 1.0, 0.0, false),
        (1.0, 1.0, 0.0, true),
        (1.0, 1.0, 0.0, true),
        (2.0, 1.0, 0.0, true),
    ];
    let (trace, region) = drive(&block, &steps);
    assert_bits(&trace, &[1.0, 1.0, 5.0, 5.0, 5.0]);
    assert_eq!(region[0], 5.0f64.to_bits());
}

#[test]
fn modes_without_an_integrator_ignore_the_trigger() {
    // P and PD hold no trigger state and their output never depends on `trigger`.
    for mode in [SimpleController::P, SimpleController::Pd] {
        let block = controller(mode, 5.0);
        let (with_trigger, _) = drive(&block, &frames(&[1.0; 5], &bools("TFTTF")));
        let (without, _) = drive(&block, &frames(&[1.0; 5], &bools("FFFFF")));
        assert_bits(&with_trigger, &without);
        assert_bits(&with_trigger, &[1.0; 5]);
    }
}

#[test]
fn reset_frame_is_reproduced_from_a_copied_state() {
    // Continuing from a copy of the state taken just before the reset frame, or just after it,
    // gives the uninterrupted trace bit for bit (the block keeps nothing outside its region).
    let block = controller(SimpleController::Pid, 5.0);
    let steps = frames(&[1.0, 2.0, 3.0, 1.5, 0.5, 2.5], &bools("FFTTFT"));
    let (whole, _) = drive(&block, &steps);
    for split in 1..steps.len() {
        let (head, region) = drive(&block, &steps[..split]);
        let mut region = region.clone();
        let mut tail = Vec::new();
        for &(t, u_s, u_m, trigger) in &steps[split..] {
            let inputs = [Value::Real(u_s), Value::Real(u_m), Value::Boolean(trigger)];
            tail.push(emit(&block, &inputs, &region, t));
            let diag = NoopDiagnostics;
            block.update_state(&Ctx::new(t, &diag), &inputs, &mut region);
        }
        let joined: Vec<f64> = head.into_iter().chain(tail).collect();
        assert_bits(&joined, &whole);
    }
}

#[test]
fn reset_traces_repeat_bit_exactly() {
    let block = controller(SimpleController::Pid, 0.3);
    let steps = frames(&[0.2, 0.4, -0.1, 0.5, 0.0, 0.7], &bools("FTTFTF"));
    let (a, region_a) = drive(&block, &steps);
    let (b, region_b) = drive(&block, &steps);
    assert_bits(&a, &b);
    assert_eq!(region_a, region_b);
}
