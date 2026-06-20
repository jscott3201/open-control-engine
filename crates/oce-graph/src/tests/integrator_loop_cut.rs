use super::*;

fn real_value(state: &RunState, conn: ConnectorId) -> f64 {
    match &state.values[conn.0 as usize] {
        Value::Real(y) => *y,
        other => panic!("expected Real at {conn:?}, got {other:?}"),
    }
}

fn set(state: &mut RunState, conn: ConnectorId, value: Value) {
    state.values[conn.0 as usize] = value;
}

#[test]
fn integrator_with_reset_cuts_feedback_loop_and_accumulates_in_two_pass_tick() {
    let mut b = ModelBuilder::default();
    let (integrator, integrator_in, integrator_out) = b.block_real(make(
        "CDL.Reals.IntegratorWithReset",
        &[("y_start", Value::Real(0.0))],
    ));
    let (_one, _, one_out) = b.block_real(make(
        "CDL.Reals.Sources.Constant",
        &[("k", Value::Real(1.0))],
    ));
    let (_reset, _, reset_out) = b.block_real(make(
        "CDL.Reals.Sources.Constant",
        &[("k", Value::Real(0.0))],
    ));
    let (_trigger, _, trigger_out) = b.block_real(make(
        "CDL.Logical.Sources.Constant",
        &[("k", Value::Boolean(false))],
    ));
    let (add, add_in, add_out) = b.block_real(make("CDL.Reals.Add", &[]));

    b.connect(integrator_out[0], add_in[0]);
    b.connect(one_out[0], add_in[1]);
    b.connect(add_out[0], integrator_in[0]);
    b.connect(reset_out[0], integrator_in[1]);
    b.connect(trigger_out[0], integrator_in[2]);

    let sched = compile(&b.model, &b.blocks).expect("IntegratorWithReset must cut the loop");
    let pos = |id: BlockId| sched.order.iter().position(|&x| x == id).unwrap();
    assert!(
        pos(integrator) < pos(add),
        "the loop-cut output must be emitted before its feedback producer"
    );

    let mut state = allocate_state(&b.model, &b.blocks);
    let mut integrator_trace = Vec::new();
    let mut add_trace = Vec::new();
    for k in 0..5u32 {
        tick_once(&b.model, &sched, &b.blocks, &mut state, f64::from(k));
        integrator_trace.push(Value::Real(real_value(&state, integrator_out[0])));
        add_trace.push(Value::Real(real_value(&state, add_out[0])));
    }

    for (idx, (got, want)) in integrator_trace
        .iter()
        .zip([0.0, 0.0, 1.0, 3.0, 7.0])
        .enumerate()
    {
        assert!(got.bit_eq(&Value::Real(want)), "integrator[{idx}]={got:?}");
    }
    for (idx, (got, want)) in add_trace.iter().zip([1.0, 1.0, 2.0, 4.0, 8.0]).enumerate() {
        assert!(got.bit_eq(&Value::Real(want)), "add[{idx}]={got:?}");
    }

    let repeat = {
        let mut state = allocate_state(&b.model, &b.blocks);
        (0..5u32)
            .map(|k| {
                tick_once(&b.model, &sched, &b.blocks, &mut state, f64::from(k));
                Value::Real(real_value(&state, integrator_out[0]))
            })
            .collect::<Vec<_>>()
    };
    for (idx, (a, b)) in integrator_trace.iter().zip(&repeat).enumerate() {
        assert!(a.bit_eq(b), "determinism trace[{idx}] {a:?} vs {b:?}");
    }
}

#[test]
fn integrator_emit_ignores_current_tick_input_until_next_tick() {
    fn run(second_tick_u: f64) -> (Value, Value) {
        let mut b = ModelBuilder::default();
        let (_integrator, integrator_in, integrator_out) = b.block_real(make(
            "CDL.Reals.IntegratorWithReset",
            &[("y_start", Value::Real(5.0))],
        ));
        let sched = compile(&b.model, &b.blocks).unwrap();
        let mut state = allocate_state(&b.model, &b.blocks);
        set(&mut state, integrator_in[1], Value::Real(0.0));
        set(&mut state, integrator_in[2], Value::Boolean(false));

        set(&mut state, integrator_in[0], Value::Real(2.0));
        tick_once(&b.model, &sched, &b.blocks, &mut state, 0.0);

        set(&mut state, integrator_in[0], Value::Real(second_tick_u));
        tick_once(&b.model, &sched, &b.blocks, &mut state, 1.0);
        let current_tick_y = state.values[integrator_out[0].0 as usize].clone();

        set(&mut state, integrator_in[0], Value::Real(0.0));
        tick_once(&b.model, &sched, &b.blocks, &mut state, 2.0);
        let next_tick_y = state.values[integrator_out[0].0 as usize].clone();
        (current_tick_y, next_tick_y)
    }

    let (small_now, small_next) = run(1.0);
    let (large_now, large_next) = run(999.0);
    assert!(
        small_now.bit_eq(&large_now),
        "current-tick output must ignore current u: {small_now:?} vs {large_now:?}"
    );
    assert!(small_now.bit_eq(&Value::Real(5.0)));
    assert!(small_next.bit_eq(&Value::Real(6.0)));
    assert!(large_next.bit_eq(&Value::Real(1004.0)));
}
