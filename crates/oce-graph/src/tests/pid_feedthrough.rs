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
