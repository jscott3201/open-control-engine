//! Tests for limited `CDL.Reals.PID` blocks. Expected traces are hand-derived from
//! the documented PID recurrence and Buildings limited-PID wiring, not recorded from the
//! implementation.

use oce_model::Value;

use super::{Block, BlockKind, Ctx, NoopDiagnostics, Pid, PidWithReset};
use crate::pid::ControllerConfig;

fn cfg(controller_type: oce_model::SimpleController) -> ControllerConfig {
    ControllerConfig {
        controller_type,
        y_min: -100.0,
        y_max: 100.0,
        ..ControllerConfig::default()
    }
}

fn pid_inputs(u_s: f64, u_m: f64) -> [Value; 2] {
    [Value::Real(u_s), Value::Real(u_m)]
}

fn pid_reset_inputs(u_s: f64, u_m: f64, trigger: bool, y_reset_in: f64) -> [Value; 4] {
    [
        Value::Real(u_s),
        Value::Real(u_m),
        Value::Boolean(trigger),
        Value::Real(y_reset_in),
    ]
}

fn init_region(block: &dyn Block) -> Vec<u64> {
    let mut region = vec![0u64; block.state_len()];
    block.init_state(&mut region, &oce_model::ParamTable::default());
    region
}

fn emit_real(block: &dyn Block, inputs: &[Value], region: &[u64], t: f64) -> Value {
    let diag = NoopDiagnostics;
    let cx = Ctx::new(t, &diag);
    let mut out = None;
    if block.kind() == BlockKind::Algebraic {
        block.step_algebraic(&cx, inputs, &mut |idx, val| {
            assert_eq!(idx, 0, "PID has one output");
            out = Some(val);
        });
    } else {
        block.emit_from_state(&cx, inputs, region, &mut |idx, val| {
            assert_eq!(idx, 0, "PID has one output");
            out = Some(val);
        });
    }
    out.expect("PID must emit one output")
}

fn tick(block: &dyn Block, inputs: &[Value], region: &mut [u64], t: f64) -> Value {
    let y = emit_real(block, inputs, region, t);
    if block.kind() == BlockKind::Stateful {
        let diag = NoopDiagnostics;
        let cx = Ctx::new(t, &diag);
        block.update_state(&cx, inputs, region);
    }
    y
}

fn assert_real_bits(got: &Value, want: u64) {
    let want = Value::Real(f64::from_bits(want));
    assert!(got.bit_eq(&want), "got {got:?}, want {want:?}");
}

fn assert_trace_bits(got: &[Value], want: &[u64]) {
    assert_eq!(got.len(), want.len());
    for (idx, (got, want)) in got.iter().zip(want).enumerate() {
        let want = Value::Real(f64::from_bits(*want));
        assert!(got.bit_eq(&want), "trace[{idx}] got {got:?}, want {want:?}");
    }
}

fn drive_pid(block: &dyn Block, steps: &[(f64, f64, f64)]) -> (Vec<Value>, Vec<u64>) {
    let mut region = init_region(block);
    let mut trace = Vec::with_capacity(steps.len());
    for &(t, u_s, u_m) in steps {
        trace.push(tick(block, &pid_inputs(u_s, u_m), &mut region, t));
    }
    (trace, region)
}

fn drive_pid_with_reset(
    block: &dyn Block,
    steps: &[(f64, f64, f64, bool, f64)],
) -> (Vec<Value>, Vec<u64>) {
    let mut region = init_region(block);
    let mut trace = Vec::with_capacity(steps.len());
    for &(t, u_s, u_m, trigger, y_reset_in) in steps {
        trace.push(tick(
            block,
            &pid_reset_inputs(u_s, u_m, trigger, y_reset_in),
            &mut region,
            t,
        ));
    }
    (trace, region)
}

#[test]
fn pid_contract_feedthrough_and_state_layout_are_param_dependent() {
    use oce_model::SimpleController::{P, Pd, Pi, Pid as PidMode};

    let p = Pid { config: cfg(P) };
    assert_eq!(p.signature().class_path, "CDL.Reals.PID");
    assert!(p.signature().stateful, "class hint stays stateful-capable");
    assert_eq!(p.kind(), BlockKind::Algebraic);
    assert_eq!(p.state_len(), 0);
    assert!(p.feeds_through(0, 0));
    assert!(p.feeds_through(1, 0));

    assert_eq!(Pid { config: cfg(Pi) }.state_len(), 2);
    assert_eq!(Pid { config: cfg(Pd) }.state_len(), 2);
    assert_eq!(
        Pid {
            config: cfg(PidMode)
        }
        .state_len(),
        3
    );
    assert_eq!(
        Pid {
            config: cfg(PidMode)
        }
        .kind(),
        BlockKind::Stateful
    );

    let p_reset = PidWithReset { config: cfg(P) };
    assert_eq!(p_reset.signature().class_path, "CDL.Reals.PIDWithReset");
    assert_eq!(p_reset.kind(), BlockKind::Algebraic);
    assert_eq!(p_reset.state_len(), 0);
    assert!(p_reset.feeds_through(0, 0));
    assert!(p_reset.feeds_through(1, 0));
    assert!(!p_reset.feeds_through(2, 0));
    assert!(!p_reset.feeds_through(3, 0));

    assert_eq!(PidWithReset { config: cfg(Pi) }.state_len(), 3);
    assert_eq!(PidWithReset { config: cfg(Pd) }.state_len(), 2);
    assert_eq!(
        PidWithReset {
            config: cfg(PidMode)
        }
        .state_len(),
        4
    );
}

#[test]
fn pid_mode_gating_reverse_acting_and_inactive_term_guards_are_pinned() {
    use oce_model::SimpleController::{P, Pd, Pi, Pid as PidMode};

    let p = Pid {
        config: ControllerConfig {
            controller_type: P,
            k: 2.0,
            ti: 0.0,
            td: 0.0,
            ni: 0.0,
            nd: 0.0,
            y_min: -10.0,
            y_max: 10.0,
            ..ControllerConfig::default()
        },
    };
    assert_real_bits(
        &emit_real(&p, &pid_inputs(1.0, 0.0), &[], 0.0),
        2.0f64.to_bits(),
    );

    let direct = Pid {
        config: ControllerConfig {
            controller_type: P,
            k: 2.0,
            reverse_acting: false,
            y_min: -10.0,
            y_max: 10.0,
            ..ControllerConfig::default()
        },
    };
    assert_real_bits(
        &emit_real(&direct, &pid_inputs(1.0, 0.0), &[], 0.0),
        (-2.0f64).to_bits(),
    );

    let pi = Pid {
        config: ControllerConfig {
            controller_type: Pi,
            k: 2.0,
            xi_start: 3.0,
            y_min: -10.0,
            y_max: 10.0,
            ..ControllerConfig::default()
        },
    };
    let pi_region = init_region(&pi);
    assert_real_bits(
        &emit_real(&pi, &pid_inputs(1.0, 0.0), &pi_region, 0.0),
        5.0f64.to_bits(),
    );

    let pd = Pid {
        config: ControllerConfig {
            controller_type: Pd,
            k: 2.0,
            yd_start: 0.25,
            y_min: -10.0,
            y_max: 10.0,
            ..ControllerConfig::default()
        },
    };
    let pd_region = init_region(&pd);
    assert_real_bits(
        &emit_real(&pd, &pid_inputs(1.0, 0.0), &pd_region, 0.0),
        2.25f64.to_bits(),
    );

    let pid = Pid {
        config: ControllerConfig {
            controller_type: PidMode,
            k: 2.0,
            xi_start: 3.0,
            yd_start: 0.25,
            y_min: -10.0,
            y_max: 10.0,
            ..ControllerConfig::default()
        },
    };
    let pid_region = init_region(&pid);
    assert_real_bits(
        &emit_real(&pid, &pid_inputs(1.0, 0.0), &pid_region, 0.0),
        5.25f64.to_bits(),
    );
}

#[test]
fn pid_inactive_divisors_do_not_leak_nan_or_inf() {
    use oce_model::SimpleController::{Pd, Pi};

    let pd = Pid {
        config: ControllerConfig {
            controller_type: Pd,
            k: 1.0,
            ti: 0.0,
            td: 1.0,
            nd: 1.0,
            y_min: -100.0,
            y_max: 100.0,
            ..ControllerConfig::default()
        },
    };
    let (pd_trace, _pd_region) = drive_pid(&pd, &[(0.0, 1.0, 0.0), (1.0, 2.0, 0.0)]);
    assert_trace_bits(&pd_trace, &[1.0f64.to_bits(), 3.0f64.to_bits()]);

    let pi = Pid {
        config: ControllerConfig {
            controller_type: Pi,
            k: 1.0,
            ti: 1.0,
            td: 0.0,
            nd: 0.0,
            y_min: -100.0,
            y_max: 100.0,
            ..ControllerConfig::default()
        },
    };
    let (pi_trace, _pi_region) =
        drive_pid(&pi, &[(0.0, 1.0, 0.0), (1.0, 1.0, 0.0), (2.0, 1.0, 0.0)]);
    assert_trace_bits(
        &pi_trace,
        &[1.0f64.to_bits(), 1.0f64.to_bits(), 2.0f64.to_bits()],
    );
}

#[test]
fn pid_limiter_boundaries_are_pinned() {
    use oce_model::SimpleController::P;
    let block = Pid {
        config: ControllerConfig {
            controller_type: P,
            y_min: -1.0,
            y_max: 1.0,
            ..ControllerConfig::default()
        },
    };
    assert_real_bits(
        &emit_real(&block, &pid_inputs(1.0, 0.0), &[], 0.0),
        1.0f64.to_bits(),
    );
    assert_real_bits(
        &emit_real(&block, &pid_inputs(2.0, 0.0), &[], 0.0),
        1.0f64.to_bits(),
    );
    assert_real_bits(
        &emit_real(&block, &pid_inputs(-2.0, 0.0), &[], 0.0),
        (-1.0f64).to_bits(),
    );
}

#[test]
fn pid_output_limiter_signed_zero_boundaries_are_pinned() {
    use oce_model::SimpleController::P;

    let lower_floor = Pid {
        config: ControllerConfig {
            controller_type: P,
            k: 1.0,
            y_min: -0.0,
            y_max: 1.0,
            ..ControllerConfig::default()
        },
    };
    let upper_ceiling = Pid {
        config: ControllerConfig {
            controller_type: P,
            k: 1.0,
            y_min: -1.0,
            y_max: -0.0,
            ..ControllerConfig::default()
        },
    };

    assert_real_bits(
        &emit_real(&lower_floor, &pid_inputs(0.0, 0.0), &[], 0.0),
        0.0f64.to_bits(),
    );
    assert_real_bits(
        &emit_real(&upper_ceiling, &pid_inputs(0.0, 0.0), &[], 0.0),
        (-0.0f64).to_bits(),
    );
}

#[test]
fn stateful_pid_feeds_through_current_setpoint_and_measurement() {
    use oce_model::SimpleController::Pi;
    let block = Pid {
        config: ControllerConfig {
            controller_type: Pi,
            y_min: -10.0,
            y_max: 10.0,
            ..ControllerConfig::default()
        },
    };
    let region = init_region(&block);
    let low = emit_real(&block, &pid_inputs(1.0, 0.0), &region, 0.0);
    let high = emit_real(&block, &pid_inputs(2.0, 0.0), &region, 0.0);
    let measured = emit_real(&block, &pid_inputs(2.0, 0.5), &region, 0.0);
    assert_real_bits(&low, 1.0f64.to_bits());
    assert_real_bits(&high, 2.0f64.to_bits());
    assert_real_bits(&measured, 1.5f64.to_bits());
}

#[test]
fn pid_anti_windup_leaves_saturation_promptly() {
    use oce_model::SimpleController::Pi;
    let block = Pid {
        config: ControllerConfig {
            controller_type: Pi,
            k: 1.0,
            ti: 1.0,
            ni: 1.0,
            y_min: 0.0,
            y_max: 1.0,
            ..ControllerConfig::default()
        },
    };
    let steps = [
        (0.0, 10.0, 0.0),
        (1.0, 10.0, 0.0),
        (2.0, 10.0, 0.0),
        (3.0, 10.0, 0.0),
        (4.0, -1.0, 0.0),
        (5.0, -1.0, 0.0),
    ];
    let (trace, region) = drive_pid(&block, &steps);
    // With y saturated at 1, antWinGai=(y_u-y)/(k*Ni) keeps xI at 1 instead of winding to 30.
    // When the setpoint reverses at t=4, y immediately leaves the upper limit.
    assert_trace_bits(
        &trace,
        &[
            1.0f64.to_bits(),
            1.0f64.to_bits(),
            1.0f64.to_bits(),
            1.0f64.to_bits(),
            0.0f64.to_bits(),
            0.0f64.to_bits(),
        ],
    );
    assert_eq!(region[0], 0.0f64.to_bits());

    let mut no_aw_x = 0.0;
    let mut no_aw_prev_t = None;
    let mut no_aw_trace = Vec::new();
    for &(t, u_s, u_m) in &steps {
        let e = u_s - u_m;
        no_aw_trace.push((e + no_aw_x).clamp(0.0, 1.0));
        let dt = no_aw_prev_t.map_or(0.0, |prev| t - prev);
        no_aw_x += e * dt;
        no_aw_prev_t = Some(t);
    }
    assert_eq!(
        no_aw_trace,
        vec![1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        "without back-calculation the reversal remains saturated"
    );
}

#[test]
fn pid_forward_euler_non_dyadic_residue_trace_is_hand_derived() {
    use oce_model::SimpleController::Pi;
    let block = Pid {
        config: ControllerConfig {
            controller_type: Pi,
            k: 1.0,
            ti: 1.0,
            xi_start: 0.1,
            y_min: -10.0,
            y_max: 10.0,
            ..ControllerConfig::default()
        },
    };
    let steps = [
        (0.0, 0.3, 0.0),
        (0.1, 0.3, 0.0),
        (0.2, 0.3, 0.0),
        (0.3, 0.3, 0.0),
        (0.4, 0.3, 0.0),
        (0.5, 0.3, 0.0),
    ];
    let (trace, region) = drive_pid(&block, &steps);
    // Hand-derived from xI0=0.1 and xI += 0.3*dt. The first nonzero increment is the rounded
    // 0.3*0.1 = 0x3f9eb851eb851eb8; later dt values use the adjacent 0.1 encodings from decimal
    // timestamps, so the add-rounding residue is visible in both y and xI bits.
    assert_trace_bits(
        &trace,
        &[
            0x3fd9_9999_9999_999a,
            0x3fd9_9999_9999_999a,
            0x3fdb_851e_b851_eb85,
            0x3fdd_70a3_d70a_3d70,
            0x3fdf_5c28_f5c2_8f5c,
            0x3fe0_a3d7_0a3d_70a4,
        ],
    );
    assert_eq!(region[0], 0x3fd0_0000_0000_0000);
}

#[test]
fn pid_derivative_filter_uses_yd_start_and_implicit_euler_update() {
    use oce_model::SimpleController::Pd;
    let block = Pid {
        config: ControllerConfig {
            controller_type: Pd,
            k: 1.0,
            td: 1.0,
            nd: 1.0,
            yd_start: 0.25,
            y_min: -10.0,
            y_max: 10.0,
            ..ControllerConfig::default()
        },
    };
    let steps = [(0.0, 1.0, 0.0), (0.5, 2.0, 0.0), (1.0, 2.0, 0.0)];
    let (trace, region) = drive_pid(&block, &steps);
    // T=1, kDer=1. First emit uses yd_start=0.25 and first update initializes xD=1-0.25=0.75.
    // With alpha=dt/T=0.5, xD=(0.75+0.5*2)/1.5=7/6 and yD=2-7/6=5/6 on
    // the next emit; the final stored xD is (7/6+0.5*2)/1.5=13/9.
    assert_trace_bits(
        &trace,
        &[
            0x3ff4_0000_0000_0000,
            0x400a_0000_0000_0000,
            0x4006_aaaa_aaaa_aaaa,
        ],
    );
    assert_eq!(region[1], 0x3ff7_1c71_c71c_71c8);
}

#[test]
fn pid_derivative_filter_is_bounded_in_formerly_divergent_regime() {
    use oce_model::SimpleController::Pd;
    let block = Pid {
        config: ControllerConfig {
            controller_type: Pd,
            k: 1.0,
            td: 0.1,
            nd: 10.0,
            y_min: -1_000.0,
            y_max: 1_000.0,
            ..ControllerConfig::default()
        },
    };
    let steps = [
        (0.0, 0.0, 0.0),
        (1.0, 1.0, 0.0),
        (2.0, 1.0, 0.0),
        (3.0, 1.0, 0.0),
        (4.0, 1.0, 0.0),
    ];
    let (trace, region) = drive_pid(&block, &steps);
    // T=Td/Nd=0.01 and alpha=dt/T=100. Explicit Euler would make xD'=-99*xD+100e.
    // Implicit Euler instead leaves a residual divided by 101 per tick:
    // y=[0, 11, 1+10/101, 1+10/10201, 1+10/1030301].
    assert_trace_bits(
        &trace,
        &[
            0x0000_0000_0000_0000,
            0x4026_0000_0000_0000,
            0x3ff1_958b_67eb_b908,
            0x3ff0_0403_ea37_8fc9,
            0x3ff0_000a_2d68_788d,
        ],
    );
    assert_eq!(region[1], 0x3fef_ffff_fad7_3d19);
}

#[test]
fn pid_with_reset_back_solves_next_output_and_held_high_does_not_re_reset() {
    use oce_model::SimpleController::Pi;
    let block = PidWithReset {
        config: ControllerConfig {
            controller_type: Pi,
            k: 1.0,
            ti: 1.0,
            y_min: -100.0,
            y_max: 100.0,
            ..ControllerConfig::default()
        },
    };
    let steps = [
        (0.0, 2.0, 0.0, false, 0.0),
        (1.0, 2.0, 0.0, true, 7.0),
        (2.0, 2.0, 0.0, true, 11.0),
        (3.0, 2.0, 0.0, false, 0.0),
    ];
    let (trace, region) = drive_pid_with_reset(&block, &steps);
    // Rising trigger at t=1 emits the pre-reset output 2, then stores xI=7-yP=5. Held high at
    // t=2 emits 7 and integrates instead of resetting to 11.
    assert_trace_bits(
        &trace,
        &[
            2.0f64.to_bits(),
            2.0f64.to_bits(),
            7.0f64.to_bits(),
            9.0f64.to_bits(),
        ],
    );
    assert_eq!(region[0], 9.0f64.to_bits());
    assert_eq!(region[2], 0, "trigger low is stored after the final tick");
}

#[test]
fn pid_with_reset_pid_mode_pins_derivative_ordering_across_reset() {
    use oce_model::SimpleController::Pid as PidMode;
    let block = PidWithReset {
        config: ControllerConfig {
            controller_type: PidMode,
            k: 1.0,
            ti: 1.0,
            td: 1.0,
            nd: 1.0,
            y_min: -100.0,
            y_max: 100.0,
            ..ControllerConfig::default()
        },
    };
    let steps = [
        (0.0, 1.0, 0.0, false, 0.0),
        (1.0, 2.0, 0.0, true, 5.0),
        (2.0, 2.0, 0.0, true, 9.0),
        (3.0, 2.0, 0.0, false, 0.0),
    ];
    let (trace, region) = drive_pid_with_reset(&block, &steps);
    // At t=1 the reset back-solve uses old yD=1, storing xI=5-(yP+yD)=2, then
    // advances xD from 1 to 1.5. The next emit is therefore 5+(0.5-1)=4.5,
    // pinning the discrete I/D ordering across the reset boundary.
    assert_trace_bits(
        &trace,
        &[
            0x3ff0_0000_0000_0000,
            0x4008_0000_0000_0000,
            0x4012_0000_0000_0000,
            0x4019_0000_0000_0000,
        ],
    );
    assert_eq!(region[0], 0x4018_0000_0000_0000);
    assert_eq!(region[2], 0x3ffe_0000_0000_0000);
    assert_eq!(region[3], 0, "trigger low is stored after the final tick");
}

#[test]
fn pid_with_reset_extreme_reset_value_is_bit_pinned() {
    use oce_model::SimpleController::Pi;
    let block = PidWithReset {
        config: ControllerConfig {
            controller_type: Pi,
            k: 1.0,
            ti: 1.0,
            y_min: f64::NEG_INFINITY,
            y_max: f64::INFINITY,
            ..ControllerConfig::default()
        },
    };
    let steps = [
        (0.0, 1.0, 0.0, false, 0.0),
        (1.0, 1.0, 0.0, true, f64::MAX),
        (2.0, 1.0, 0.0, false, 0.0),
    ];
    let (trace, region) = drive_pid_with_reset(&block, &steps);
    assert_trace_bits(
        &trace,
        &[1.0f64.to_bits(), 1.0f64.to_bits(), f64::MAX.to_bits()],
    );
    assert_eq!(region[0], f64::MAX.to_bits());
}

#[test]
fn pid_with_reset_non_finite_reset_value_path_is_pinned() {
    use oce_model::SimpleController::Pi;
    let block = PidWithReset {
        config: ControllerConfig {
            controller_type: Pi,
            k: 1.0,
            ti: 1.0,
            y_min: f64::NEG_INFINITY,
            y_max: f64::INFINITY,
            ..ControllerConfig::default()
        },
    };

    let quiet_nan = f64::from_bits(0x7ff8_0000_0000_0000);
    let (nan_trace, nan_region) = drive_pid_with_reset(
        &block,
        &[
            (0.0, 1.0, 0.0, false, 0.0),
            (1.0, 1.0, 0.0, true, quiet_nan),
        ],
    );
    assert_trace_bits(&nan_trace, &[1.0f64.to_bits(), 1.0f64.to_bits()]);
    assert_eq!(nan_region[0], 0x7ff8_0000_0000_0000);

    let (inf_trace, inf_region) = drive_pid_with_reset(
        &block,
        &[
            (0.0, 1.0, 0.0, false, 0.0),
            (1.0, 1.0, 0.0, true, f64::INFINITY),
            (2.0, 1.0, 0.0, false, 0.0),
        ],
    );
    assert_trace_bits(
        &inf_trace,
        &[1.0f64.to_bits(), 1.0f64.to_bits(), f64::INFINITY.to_bits()],
    );
    // The reset stores +Inf and the next emit propagates it. The following update then evaluates
    // the deferred non-finite anti-windup seam (`Inf - Inf`) and stores canonical NaN.
    assert_eq!(inf_region[0], 0x7ff8_0000_0000_0000);
}

#[test]
fn pid_non_finite_input_current_behavior_is_panic_free_and_pinned() {
    use oce_model::SimpleController::P;
    let block = Pid {
        config: ControllerConfig {
            controller_type: P,
            y_min: 0.0,
            y_max: 1.0,
            ..ControllerConfig::default()
        },
    };
    // The manual limiter mirrors `CDL.Reals.Limiter`: f64::max/min absorb NaN to the bound.
    assert_real_bits(
        &emit_real(&block, &pid_inputs(f64::NAN, 0.0), &[], 0.0),
        0.0f64.to_bits(),
    );
    assert_real_bits(
        &emit_real(&block, &pid_inputs(f64::INFINITY, 0.0), &[], 0.0),
        1.0f64.to_bits(),
    );
}

#[test]
fn pid_output_and_full_state_are_byte_deterministic_across_runs() {
    use oce_model::SimpleController::Pid as PidMode;
    let block = Pid {
        config: ControllerConfig {
            controller_type: PidMode,
            k: 1.5,
            ti: 0.75,
            td: 0.2,
            nd: 4.0,
            xi_start: 0.125,
            yd_start: -0.25,
            y_min: -2.0,
            y_max: 2.0,
            ..ControllerConfig::default()
        },
    };
    let steps = [
        (0.0, 0.2, 0.0),
        (0.3, 0.4, 0.1),
        (0.7, -0.1, 0.2),
        (1.1, 0.5, -0.2),
        (1.8, 0.0, 0.0),
    ];
    let (trace_a, region_a) = drive_pid(&block, &steps);
    let (trace_b, region_b) = drive_pid(&block, &steps);
    for (idx, (a, b)) in trace_a.iter().zip(&trace_b).enumerate() {
        assert!(a.bit_eq(b), "trace[{idx}] {a:?} vs {b:?}");
    }
    assert_eq!(region_a, region_b);
}
