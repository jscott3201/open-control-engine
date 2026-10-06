use super::*;

#[test]
fn tick_feedforward_add_and_source_seed() {
    let mut b = ModelBuilder::default();
    let (_c0, _, o0) = b.block_real(make(
        "CDL.Reals.Sources.Constant",
        &[("k", Value::Real(2.0))],
    ));
    let (_c1, _, o1) = b.block_real(make(
        "CDL.Reals.Sources.Constant",
        &[("k", Value::Real(5.0))],
    ));
    let (_add, add_in, add_out) = b.block_real(make("CDL.Reals.Add", &[]));
    b.connect(o0[0], add_in[0]);
    b.connect(o1[0], add_in[1]);

    let sched = compile(&b.model, &b.blocks).unwrap();
    let mut state = allocate_state(&b.model, &b.blocks);
    // Constant outputs are seeded before the first tick (§7 req 3).
    assert!(state.values[o0[0].0 as usize].bit_eq(&Value::Real(2.0)));

    tick_once(&b.model, &sched, &b.blocks, &mut state, 0.0);
    assert!(state.values[add_out[0].0 as usize].bit_eq(&Value::Real(7.0)));
}

#[test]
fn init_warning_source_uses_noop_diagnostics_and_tick_delivers_to_sink() {
    let mut b = ModelBuilder::default();
    b.block_real(Box::new(WarningSource));
    let sched = compile(&b.model, &b.blocks).unwrap();
    let diag = CapturingDiagnostics::default();

    let mut state = allocate_state(&b.model, &b.blocks);
    assert!(
        diag.events.borrow().is_empty(),
        "init_values must use NoopDiagnostics and drop source warnings"
    );

    tick_with_diag(&b.model, &sched, &b.blocks, &mut state, 3.0, &diag);
    let events = diag.events.borrow();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, "test.WarningSource");
    assert_eq!(events[0].1, "init and tick warning");
    assert_eq!(events[0].2.to_bits(), 3.0f64.to_bits());
}

fn run_stateful_blocks_for(times: &[f64]) -> RunState {
    let mut b = ModelBuilder::default();
    let (_one, _, one_out) = b.block_real(make(
        "CDL.Reals.Sources.Constant",
        &[("k", Value::Real(1.0))],
    ));
    let (_delay, delay_in, _) = b.block_real(make(
        "CDL.Discrete.UnitDelay",
        &[("y_start", Value::Real(0.0))],
    ));
    b.connect(one_out[0], delay_in[0]);

    let (_hi, _, hi_out) = b.block_real(make(
        "CDL.Reals.Sources.Constant",
        &[("k", Value::Real(2.0))],
    ));
    let (_lo, _, lo_out) = b.block_real(make(
        "CDL.Reals.Sources.Constant",
        &[("k", Value::Real(1.0))],
    ));
    let (_gt, gt_in, gt_out) = b.block_real(make("CDL.Reals.Greater", &[]));
    b.connect(hi_out[0], gt_in[0]);
    b.connect(lo_out[0], gt_in[1]);

    let (_pre, pre_in, _) = b.block_real(make(
        "CDL.Logical.Pre",
        &[("pre_u_start", Value::Boolean(false))],
    ));
    let (_edge, edge_in, _) = b.block_real(make(
        "CDL.Logical.Edge",
        &[("pre_u_start", Value::Boolean(false))],
    ));
    b.connect(gt_out[0], pre_in[0]);
    b.connect(gt_out[0], edge_in[0]);

    let _sample = b.block_real(make(
        "CDL.Logical.Sources.SampleTrigger",
        &[("period", Value::Real(2.0)), ("shift", Value::Real(0.0))],
    ));

    let sched = compile(&b.model, &b.blocks).unwrap();
    let mut state = allocate_state(&b.model, &b.blocks);
    for &t in times {
        tick_once(&b.model, &sched, &b.blocks, &mut state, t);
    }
    state
}

#[test]
fn stateful_blocks_are_run_twice_deterministic() {
    let times = [0.0, 1.0, 2.0, 3.0, 4.0];
    let first = run_stateful_blocks_for(&times);
    let second = run_stateful_blocks_for(&times);
    assert_eq!(first.values.len(), second.values.len());
    for (idx, (a, b)) in first.values.iter().zip(&second.values).enumerate() {
        assert!(a.bit_eq(b), "state.values[{idx}] diverged: {a:?} vs {b:?}");
    }
    assert_eq!(first.words, second.words, "state words diverged");
}

/// Build the `S(t) = 1 + S(t-1)` UnitDelay feedback loop and return `add.out` after each of
/// `n` ticks. With the seed `y_start = 0`, the correct one-tick-delay sequence is `1, 2, 3, …`.
fn unit_delay_loop_sequence(n: usize) -> Vec<Value> {
    let mut b = ModelBuilder::default();
    let (_gen, _, g) = b.block_real(make(
        "CDL.Reals.Sources.Constant",
        &[("k", Value::Real(1.0))],
    ));
    let (_add, add_in, add_out) = b.block_real(make("CDL.Reals.Add", &[]));
    let (_ud, ud_in, ud_out) = b.block_real(make(
        "CDL.Discrete.UnitDelay",
        &[("y_start", Value::Real(0.0))],
    ));
    b.connect(g[0], add_in[0]);
    b.connect(ud_out[0], add_in[1]);
    b.connect(add_out[0], ud_in[0]);

    let sched = compile(&b.model, &b.blocks).unwrap();
    let mut state = allocate_state(&b.model, &b.blocks);
    (0..n)
        .map(|k| {
            tick_once(&b.model, &sched, &b.blocks, &mut state, k as f64);
            state.values[add_out[0].0 as usize].clone()
        })
        .collect()
}

#[test]
fn tick_unit_delay_feedback_is_a_one_tick_delay() {
    // Proves the two-pass eval: an inline per-block update would give a TWO-tick delay here.
    let seq = unit_delay_loop_sequence(4);
    for (k, got) in seq.iter().enumerate() {
        let want = Value::Real((k + 1) as f64);
        assert!(got.bit_eq(&want), "tick {k}: got {got:?}, want {want:?}");
    }
}

#[test]
fn tick_is_deterministic_across_runs() {
    let first = unit_delay_loop_sequence(6);
    let second = unit_delay_loop_sequence(6);
    assert_eq!(first.len(), second.len());
    for (x, y) in first.iter().zip(&second) {
        assert!(x.bit_eq(y), "non-deterministic: {x:?} vs {y:?}");
    }
}

#[test]
fn tick_loop_breaker_scheduled_before_its_producer() {
    // Declare the UnitDelay FIRST so it sorts to the very front of `order` (its cut input gives it
    // no emit-before edge) — i.e. the loop-breaker fires before its producer Add. The separate
    // update pass must still yield the correct one-tick delay (S(t) = 1 + S(t-1)).
    let mut b = ModelBuilder::default();
    let (ud, ud_in, ud_out) = b.block_real(make(
        "CDL.Discrete.UnitDelay",
        &[("y_start", Value::Real(0.0))],
    ));
    let (_gen, _, g) = b.block_real(make(
        "CDL.Reals.Sources.Constant",
        &[("k", Value::Real(1.0))],
    ));
    let (add, add_in, add_out) = b.block_real(make("CDL.Reals.Add", &[]));
    b.connect(g[0], add_in[0]);
    b.connect(ud_out[0], add_in[1]);
    b.connect(add_out[0], ud_in[0]);

    let sched = compile(&b.model, &b.blocks).unwrap();
    let pos = |id: BlockId| sched.order.iter().position(|&x| x == id).unwrap();
    assert!(
        pos(ud) < pos(add),
        "the loop-breaker is scheduled before its producer"
    );

    let mut state = allocate_state(&b.model, &b.blocks);
    for k in 0..4u32 {
        tick_once(&b.model, &sched, &b.blocks, &mut state, f64::from(k));
        let want = Value::Real(f64::from(k + 1));
        let got = &state.values[add_out[0].0 as usize];
        assert!(got.bit_eq(&want), "tick {k}: got {got:?}, want {want:?}");
    }
}

#[test]
fn pre_cut_feedback_advances_without_event_iteration() {
    // This loop has no same-time Boolean fixed point. HostTick v1 accepts it and advances once per
    // call, including repeated calls at one timestamp.
    let mut b = ModelBuilder::default();
    let (_pre, pre_in, pre_out) = b.block_real(make(
        "CDL.Logical.Pre",
        &[("pre_u_start", Value::Boolean(false))],
    ));
    let (_not, not_in, not_out) = b.block_real(make("CDL.Logical.Not", &[]));
    b.connect(pre_out[0], not_in[0]);
    b.connect(not_out[0], pre_in[0]);

    let sched = compile(&b.model, &b.blocks).unwrap();
    let mut state = allocate_state(&b.model, &b.blocks);
    let expected = [false, true, false, true];
    for (k, exp) in expected.iter().enumerate() {
        tick_once(&b.model, &sched, &b.blocks, &mut state, 0.0);
        let got = &state.values[pre_out[0].0 as usize];
        assert!(
            got.bit_eq(&Value::Boolean(*exp)),
            "tick {k}: got {got:?}, want {exp}"
        );
    }
}
