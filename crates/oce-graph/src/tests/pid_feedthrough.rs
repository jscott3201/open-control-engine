use super::*;

fn enum_param(member: &str) -> Value {
    Value::String(Arc::from(format!(
        "Buildings.Controls.OBC.CDL.Types.SimpleController.{member}"
    )))
}

#[test]
fn pid_feedback_loop_requires_a_separate_cut() {
    let mut direct = ModelBuilder::default();
    let (_pid, pid_in, pid_out) = direct.block_real(make(
        "CDL.Reals.PID",
        &[("controllerType", enum_param("PI"))],
    ));
    let (_zero, _, zero_out) = direct.block_real(make(
        "CDL.Reals.Sources.Constant",
        &[("k", Value::Real(0.0))],
    ));
    direct.connect(pid_out[0], pid_in[0]);
    direct.connect(zero_out[0], pid_in[1]);
    let err = compile(&direct.model, &direct.blocks).expect_err("PID itself must not cut feedback");
    assert!(
        matches!(
            err,
            BuildError::AlgebraicLoop { .. } | BuildError::BlockAlgebraicLoop { .. }
        ),
        "expected algebraic loop rejection, got {err:?}"
    );

    let mut cut = ModelBuilder::default();
    let (_pid, pid_in, pid_out) = cut.block_real(make(
        "CDL.Reals.PID",
        &[("controllerType", enum_param("PI"))],
    ));
    let (_delay, delay_in, delay_out) = cut.block_real(make(
        "CDL.Discrete.UnitDelay",
        &[("y_start", Value::Real(0.0))],
    ));
    let (_zero, _, zero_out) = cut.block_real(make(
        "CDL.Reals.Sources.Constant",
        &[("k", Value::Real(0.0))],
    ));
    cut.connect(pid_out[0], delay_in[0]);
    cut.connect(delay_out[0], pid_in[0]);
    cut.connect(zero_out[0], pid_in[1]);
    let sched = compile(&cut.model, &cut.blocks).expect("UnitDelay supplies the feedback cut");
    assert_eq!(sched.order.len(), 3);
}

#[test]
fn pid_mode_state_words_are_allocated_from_resolved_controller_type() {
    let mut b = ModelBuilder::default();
    let (p_mode, _, _) = b.block_real(make(
        "CDL.Reals.PID",
        &[("controllerType", enum_param("P"))],
    ));
    let (pid_mode, _, _) = b.block_real(make(
        "CDL.Reals.PID",
        &[("controllerType", enum_param("PID"))],
    ));
    let sched = compile(&b.model, &b.blocks).unwrap();
    let state = allocate_state(&b.model, &b.blocks);

    let p_block = b.blocks[p_mode.0 as usize].as_ref();
    assert!(p_block.signature().stateful);
    assert_eq!(p_block.kind(), BlockKind::Algebraic);
    assert_eq!(p_block.state_len(), 0);
    assert_eq!(state.slot_of[p_mode.0 as usize], usize::MAX);

    let pid_block = b.blocks[pid_mode.0 as usize].as_ref();
    assert_eq!(pid_block.kind(), BlockKind::Stateful);
    assert_eq!(pid_block.state_len(), 3);
    let slot = state.slot_of[pid_mode.0 as usize];
    assert_ne!(slot, usize::MAX);
    assert_eq!(state.slots[slot].len, 3);
    assert_eq!(state.words.len(), 3);
    assert_eq!(sched.order, vec![p_mode, pid_mode]);
}

#[test]
fn pid_with_reset_trigger_is_update_routed_end_to_end() {
    let mut b = ModelBuilder::default();
    let (_set, _, set_out) = b.block_real(make(
        "CDL.Reals.Sources.Constant",
        &[("k", Value::Real(1.0))],
    ));
    let (_measured, _, measured_out) = b.block_real(make(
        "CDL.Reals.Sources.Constant",
        &[("k", Value::Real(0.0))],
    ));
    let (_trigger, _, trigger_out) = b.block_real(make(
        "CDL.Logical.Sources.Constant",
        &[("k", Value::Boolean(true))],
    ));
    let (_pid, pid_in, pid_out) = b.block_real(make(
        "CDL.Reals.PIDWithReset",
        &[
            ("controllerType", enum_param("PI")),
            ("k", Value::Real(1.0)),
            ("Ti", Value::Real(1.0)),
            ("y_reset", Value::Real(5.0)),
            ("yMin", Value::Real(-100.0)),
            ("yMax", Value::Real(100.0)),
        ],
    ));
    b.connect(set_out[0], pid_in[0]);
    b.connect(measured_out[0], pid_in[1]);
    b.connect(trigger_out[0], pid_in[2]);

    let sched = compile(&b.model, &b.blocks).expect("PIDWithReset trigger must not add loops");
    let mut state = allocate_state(&b.model, &b.blocks);

    tick_once(&b.model, &sched, &b.blocks, &mut state, 0.0);
    assert!(
        state.values[pid_out[0].0 as usize].bit_eq(&Value::Real(1.0)),
        "rising reset must not affect the same tick output"
    );

    tick_once(&b.model, &sched, &b.blocks, &mut state, 1.0);
    assert!(
        state.values[pid_out[0].0 as usize].bit_eq(&Value::Real(5.0)),
        "reset target is visible on the following tick through the update pass"
    );
}
