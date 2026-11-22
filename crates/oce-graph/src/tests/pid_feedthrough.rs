use super::*;

/// The `SimpleController` member `member` as the resolver grounds a qualified member: an
/// enumeration value of that class (the one spelling a reader takes; issue #734, ruling R2).
fn enum_param(member: &str) -> Value {
    let ordinal = oce_model::enum_member_ordinal(oce_model::EnumClassId::SIMPLE_CONTROLLER, member)
        .unwrap_or_else(|| panic!("{member} is no SimpleController member"));
    Value::Enum {
        class: oce_model::EnumClassId::SIMPLE_CONTROLLER,
        ordinal,
    }
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
    let state = allocate_state(&b.model, &b.blocks).expect("state within budget");

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

/// `PIDWithReset` (PI, `k = Ti = 1`, `y_reset = 5`, `e = 1`) whose trigger comes from
/// `CivilTime > 0.5`, declared AFTER the controller, so only the trigger's emit-before edge can
/// place the threshold first.
fn reset_pid_with_late_declared_trigger() -> (ModelBuilder, BlockId, BlockId, ConnectorId) {
    let mut b = ModelBuilder::default();
    let (_set, _, set_out) = b.block_real(make(
        "CDL.Reals.Sources.Constant",
        &[("k", Value::Real(1.0))],
    ));
    let (_measured, _, measured_out) = b.block_real(make(
        "CDL.Reals.Sources.Constant",
        &[("k", Value::Real(0.0))],
    ));
    let (pid, pid_in, pid_out) = b.block_real(make(
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
    let (_clock, _, clock_out) = b.block_real(make("CDL.Reals.Sources.CivilTime", &[]));
    let (threshold, threshold_in, threshold_out) = b.block_real(make(
        "CDL.Reals.GreaterThreshold",
        &[("t", Value::Real(0.5))],
    ));
    b.connect(set_out[0], pid_in[0]);
    b.connect(measured_out[0], pid_in[1]);
    b.connect(clock_out[0], threshold_in[0]);
    b.connect(threshold_out[0], pid_in[2]);
    (b, pid, threshold, pid_out[0])
}

#[test]
fn pid_with_reset_trigger_resets_at_its_frame_end_to_end() {
    let (b, pid, threshold, y) = reset_pid_with_late_declared_trigger();
    let sched = compile(&b.model, &b.blocks).expect("an acyclic trigger path compiles");
    let position = |block| sched.order.iter().position(|&id| id == block).unwrap();
    assert!(
        position(threshold) < position(pid),
        "the trigger feeds through, so its producer is scheduled first: {:?}",
        sched.order
    );
    let mut state = allocate_state(&b.model, &b.blocks).expect("state within budget");
    // t = 0: trigger false, emits e = 1. t = 1: the trigger rises; that frame emits the reset
    // controller, 1 + (5 - 1) = 5. t = 2: held, emits 5. t = 3: 1 + 5 = 6.
    let mut trace = Vec::new();
    for t in [0.0, 1.0, 2.0, 3.0] {
        tick_once(&b.model, &sched, &b.blocks, &mut state, t);
        trace.push(state.values[y.0 as usize].clone());
    }
    for (got, want) in trace.iter().zip([1.0, 5.0, 5.0, 6.0]) {
        assert!(got.bit_eq(&Value::Real(want)), "{trace:?}");
    }
}

#[test]
fn pid_with_reset_trigger_true_from_the_first_frame_never_resets() {
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
    let mut state = allocate_state(&b.model, &b.blocks).expect("state within budget");
    // Frame 0 is initialisation: no reset, and the held trigger is no event afterwards.
    for (t, want) in [(0.0, 1.0), (1.0, 1.0), (2.0, 2.0)] {
        tick_once(&b.model, &sched, &b.blocks, &mut state, t);
        assert!(
            state.values[pid_out[0].0 as usize].bit_eq(&Value::Real(want)),
            "t = {t}: want {want}"
        );
    }
}
