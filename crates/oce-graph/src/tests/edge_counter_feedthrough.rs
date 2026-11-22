use super::*;

/// `OnCounter` whose output reaches its own input `fed` (0 = trigger, 1 = reset) through an
/// integer threshold, optionally through a `Logical.Pre` on that path; the other input is a
/// constant false.
fn self_feeding_counter(fed: usize, with_pre: bool) -> (ModelBuilder, BlockId) {
    let mut b = ModelBuilder::default();
    let (counter, counter_in, counter_out) = b.block_real(make(
        "CDL.Integers.OnCounter",
        &[("y_start", Value::Integer(0))],
    ));
    let (_gt, gt_in, gt_out) = b.block_real(make(
        "CDL.Integers.GreaterThreshold",
        &[("t", Value::Integer(0))],
    ));
    let (_other, _, other_out) = b.block_real(make(
        "CDL.Logical.Sources.Constant",
        &[("k", Value::Boolean(false))],
    ));
    b.connect(counter_out[0], gt_in[0]);
    if with_pre {
        let (_pre, pre_in, pre_out) = b.block_real(make("CDL.Logical.Pre", &[]));
        b.connect(gt_out[0], pre_in[0]);
        b.connect(pre_out[0], counter_in[fed]);
    } else {
        b.connect(gt_out[0], counter_in[fed]);
    }
    b.connect(other_out[0], counter_in[1 - fed]);
    (b, counter)
}

#[test]
fn on_counter_feedback_into_an_event_input_is_an_algebraic_loop_naming_pre() {
    // The counter applies trigger and reset events at their own frame, so both feed through and
    // a counter that counts or resets itself is a connector-level loop.
    for fed in [0, 1] {
        let (b, _) = self_feeding_counter(fed, false);
        let err = compile(&b.model, &b.blocks).expect_err("feedback into an event input loops");
        let BuildError::AlgebraicLoop {
            members,
            event_inputs,
        } = &err
        else {
            panic!("expected a connector-level loop, got {err:?}");
        };
        assert_eq!(event_inputs.len(), 1, "{err}");
        assert_eq!(event_inputs[0].1, oce_blocks::EventInputRole::Condition);
        assert!(members.contains(&event_inputs[0].0));
        assert!(
            err.to_string()
                .contains("insert CDL.Logical.Pre on the path into it"),
            "{err}"
        );
    }
}

#[test]
fn on_counter_feedback_through_pre_compiles() {
    for fed in [0, 1] {
        let (b, counter) = self_feeding_counter(fed, true);
        let sched = compile(&b.model, &b.blocks).expect("Pre breaks the counter's feedback");
        assert!(sched.order.contains(&counter));
        let state = allocate_state(&b.model, &b.blocks).expect("state within budget");
        let slot = state.slot_of[counter.0 as usize];
        assert_ne!(slot, usize::MAX);
        assert_eq!(state.slots[slot].len, 4);
    }
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
