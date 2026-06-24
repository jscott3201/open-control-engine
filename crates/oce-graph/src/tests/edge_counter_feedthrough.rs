use super::*;

#[test]
fn integer_on_counter_cuts_feedback_loop_without_separate_delay() {
    let mut b = ModelBuilder::default();
    let (counter, counter_in, counter_out) = b.block_real(make(
        "CDL.Integers.OnCounter",
        &[("y_start", Value::Integer(0))],
    ));
    let (_gt, gt_in, gt_out) = b.block_real(make(
        "CDL.Integers.GreaterThreshold",
        &[("t", Value::Integer(0))],
    ));
    let (_reset, _, reset_out) = b.block_real(make(
        "CDL.Logical.Sources.Constant",
        &[("k", Value::Boolean(false))],
    ));

    b.connect(counter_out[0], gt_in[0]);
    b.connect(gt_out[0], counter_in[0]);
    b.connect(reset_out[0], counter_in[1]);

    let sched = compile(&b.model, &b.blocks).expect("OnCounter must cut trigger feedback");
    assert!(
        sched.order.contains(&counter),
        "compiled schedule must contain the counter"
    );

    let state = allocate_state(&b.model, &b.blocks);
    let slot = state.slot_of[counter.0 as usize];
    assert_ne!(slot, usize::MAX);
    assert_eq!(state.slots[slot].len, 4);
}

#[test]
fn logical_latch_feedback_requires_an_explicit_cut() {
    let mut direct = ModelBuilder::default();
    let (_latch, latch_in, latch_out) = direct.block_real(make("CDL.Logical.Latch", &[]));
    let (_clear, _, clear_out) = direct.block_real(make(
        "CDL.Logical.Sources.Constant",
        &[("k", Value::Boolean(false))],
    ));
    direct.connect(latch_out[0], latch_in[0]);
    direct.connect(clear_out[0], latch_in[1]);

    let err = compile(&direct.model, &direct.blocks)
        .expect_err("Latch is transparent and must not cut feedback");
    assert!(
        matches!(
            err,
            BuildError::AlgebraicLoop { .. } | BuildError::BlockAlgebraicLoop { .. }
        ),
        "expected algebraic loop rejection, got {err:?}"
    );

    let mut cut = ModelBuilder::default();
    let (_latch, latch_in, latch_out) = cut.block_real(make("CDL.Logical.Latch", &[]));
    let (_pre, pre_in, pre_out) = cut.block_real(make("CDL.Logical.Pre", &[]));
    let (_clear, _, clear_out) = cut.block_real(make(
        "CDL.Logical.Sources.Constant",
        &[("k", Value::Boolean(false))],
    ));
    cut.connect(latch_out[0], pre_in[0]);
    cut.connect(pre_out[0], latch_in[0]);
    cut.connect(clear_out[0], latch_in[1]);

    let sched = compile(&cut.model, &cut.blocks).expect("Pre supplies the explicit feedback cut");
    assert_eq!(sched.order.len(), 3);
}
