//! The event plan of issue #800 (design note `docs/design/pulse-event-inputs.md`, option (g)):
//! which cones it activates, which it sends to the rising edge, that it adds no scheduling edge,
//! that it is linear in the model, and that a shadowed run fires as the rule says.

use super::*;
use crate::{EventPlan, ShadowSource, needs_plan, topo_sort};

fn plan(b: &ModelBuilder) -> (Schedule, EventPlan) {
    let schedule = compile(&b.model, &b.blocks).expect("compiles");
    let plan = schedule.events.clone();
    (schedule, plan)
}

fn counter(b: &mut ModelBuilder) -> (BlockId, Vec<ConnectorId>) {
    let (id, inputs, _) = b.block_real(make(
        "CDL.Integers.OnCounter",
        &[("y_start", Value::Integer(0))],
    ));
    let (_, _, fal) = b.block_real(make(
        "CDL.Logical.Sources.Constant",
        &[("k", Value::Boolean(false))],
    ));
    b.connect(fal[0], inputs[1]);
    (id, inputs)
}

fn real_block(b: &mut ModelBuilder, class: &str) -> (BlockId, Vec<ConnectorId>, Vec<ConnectorId>) {
    b.block_real(make(class, &[]))
}

#[test]
fn a_change_wired_straight_in_is_a_primitive_pulse() {
    let mut b = ModelBuilder::default();
    let (_, cha_in, cha_out) = real_block(&mut b, "CDL.Logical.Change");
    let (cnt, cnt_in) = counter(&mut b);
    b.connect(cha_out[0], cnt_in[0]);
    let (schedule, plan) = plan(&b);
    let hidden = plan
        .hidden(cnt.0 as usize)
        .expect("the counter receives settled values");
    assert_eq!(hidden.len(), 2);
    assert_eq!(hidden[0], ShadowSource::Pulse);
    assert!(
        matches!(hidden[1], ShadowSource::Level(_)),
        "reset: a constant, a level"
    );
    // No logic block to evaluate: no step.
    assert!((0..b.model.blocks.len()).all(|i| plan.step(i).is_none()));
    // And a run counts a change on every frame, consecutive frames included.
    let mut state = allocate_state(&b.model, &b.blocks).unwrap();
    let mut counts = Vec::new();
    for (k, u) in [false, true, false, true, true].into_iter().enumerate() {
        state.values[cha_in[0].0 as usize] = Value::Boolean(u);
        tick_initial(&b, &schedule, &mut state, k as f64, k == 0);
        counts.push(state.values[b.model.blocks[cnt.0 as usize].outputs[0].0 as usize].clone());
    }
    let counts: Vec<i64> = counts
        .iter()
        .map(|v| match v {
            Value::Integer(n) => *n,
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(counts, [0, 1, 2, 3, 3], "every frame where u changed");
}

/// One tick with the initial flag, as the frame driver sets it.
fn tick_initial(
    b: &ModelBuilder,
    schedule: &Schedule,
    state: &mut RunState,
    t: f64,
    initial: bool,
) {
    let diag = oce_blocks::NoopDiagnostics;
    let mut ctx = EvalContext {
        model: &b.model,
        schedule,
        blocks: &b.blocks,
        diagnostics: &diag,
        state,
        initial,
    };
    eval_tick(&mut ctx, t);
}

#[test]
fn a_level_only_model_needs_no_plan_and_gets_none() {
    let mut b = ModelBuilder::default();
    let (_, not_in, not_out) = real_block(&mut b, "CDL.Logical.Not");
    let (_, cnt_in) = counter(&mut b);
    b.connect(not_out[0], cnt_in[0]);
    let _ = not_in;
    assert!(!needs_plan(&b.blocks));
    let (_, plan) = plan(&b);
    assert!(plan.is_empty());
    let state = allocate_state(&b.model, &b.blocks).unwrap();
    assert!(
        state.shadow_w.is_empty() && state.shadow_s.is_empty() && state.shadow_scratch.is_empty()
    );
}

#[test]
fn not_of_a_pulse_is_an_active_cone_with_one_step() {
    let mut b = ModelBuilder::default();
    let (_, _, cha_out) = real_block(&mut b, "CDL.Logical.Change");
    let (not_id, not_in, not_out) = real_block(&mut b, "CDL.Logical.Not");
    let (cnt, cnt_in) = counter(&mut b);
    b.connect(cha_out[0], not_in[0]);
    b.connect(not_out[0], cnt_in[0]);
    let (_, plan) = plan(&b);
    assert_eq!(
        plan.hidden(cnt.0 as usize).unwrap()[0],
        ShadowSource::Node(not_out[0])
    );
    assert_eq!(
        plan.step(not_id.0 as usize),
        Some(&[ShadowSource::Pulse][..])
    );
}

/// `cone(b)` builds `Or(Change(u), x)` into a counter, `x` from `leaf`; the plan's verdict.
fn or_with(leaf: impl FnOnce(&mut ModelBuilder) -> ConnectorId) -> EventPlan {
    let mut b = ModelBuilder::default();
    let (_, _, cha_out) = real_block(&mut b, "CDL.Logical.Change");
    let (_, or_in, or_out) = real_block(&mut b, "CDL.Logical.Or");
    let x = leaf(&mut b);
    let (_, cnt_in) = counter(&mut b);
    b.connect(cha_out[0], or_in[0]);
    b.connect(x, or_in[1]);
    b.connect(or_out[0], cnt_in[0]);
    plan(&b).1
}

#[test]
fn a_cone_with_a_pre_or_an_edge_block_of_a_pulse_falls_back_to_the_rising_edge() {
    // Pre of a level.
    assert!(or_with(|b| real_block(b, "CDL.Logical.Pre").2[0]).is_empty());
    // An edge block of a pulse: Change(Edge(u)).
    assert!(
        or_with(|b| {
            let (_, _, edge_out) = real_block(b, "CDL.Logical.Edge");
            let (_, cha_in, cha_out) = real_block(b, "CDL.Logical.Change");
            b.connect(edge_out[0], cha_in[0]);
            cha_out[0]
        })
        .is_empty()
    );
    // A pulse hidden in an Integer: Integers.Change(BooleanToInteger(Change(v))).
    assert!(
        or_with(|b| {
            let (_, _, cha_out) = real_block(b, "CDL.Logical.Change");
            let (_, conv_in, conv_out) = real_block(b, "CDL.Conversions.BooleanToInteger");
            let (_, ich_in, ich_out) = real_block(b, "CDL.Integers.Change");
            b.connect(cha_out[0], conv_in[0]);
            b.connect(conv_out[0], ich_in[0]);
            ich_out[0]
        })
        .is_empty()
    );
    // A level leaf that carries a pulse through a conversion and a comparator.
    assert!(
        or_with(|b| {
            let (_, _, cha_out) = real_block(b, "CDL.Logical.Change");
            let (_, conv_in, conv_out) = real_block(b, "CDL.Conversions.BooleanToReal");
            let (_, gt_in, gt_out) = b.block_real(make(
                "CDL.Reals.GreaterThreshold",
                &[("t", Value::Real(0.5))],
            ));
            b.connect(cha_out[0], conv_in[0]);
            b.connect(conv_out[0], gt_in[0]);
            gt_out[0]
        })
        .is_empty()
    );
    // The control: a plain level (a model input) keeps the cone active.
    assert!(!or_with(|b| real_block(b, "CDL.Logical.Not").2[0]).is_empty());
}

#[test]
fn a_sample_is_a_leaf_of_its_own_kind() {
    let mut b = ModelBuilder::default();
    let (_, _, s_out) = b.block_real(make(
        "CDL.Logical.Sources.SampleTrigger",
        &[("period", Value::Real(1.0)), ("shift", Value::Real(0.0))],
    ));
    let (sampler, sampler_in, _) = b.block_real(make("CDL.Discrete.TriggeredSampler", &[]));
    b.connect(s_out[0], sampler_in[1]);
    let (_, plan) = plan(&b);
    assert_eq!(
        plan.hidden(sampler.0 as usize),
        Some(&[ShadowSource::Sample(s_out[0])][..])
    );
}

#[test]
fn a_cone_shared_by_many_consumers_is_evaluated_once() {
    let mut b = ModelBuilder::default();
    let (_, _, c1) = real_block(&mut b, "CDL.Logical.Change");
    let (_, _, c2) = real_block(&mut b, "CDL.Logical.Change");
    let (or_id, or_in, or_out) = real_block(&mut b, "CDL.Logical.Or");
    b.connect(c1[0], or_in[0]);
    b.connect(c2[0], or_in[1]);
    for _ in 0..3 {
        let (_, cnt_in) = counter(&mut b);
        b.connect(or_out[0], cnt_in[0]);
    }
    let (_, plan) = plan(&b);
    let steps = (0..b.model.blocks.len())
        .filter(|&i| plan.step(i).is_some())
        .count();
    assert_eq!(steps, 1);
    assert!(plan.step(or_id.0 as usize).is_some());
}

/// A chain of `n` `Not` blocks from `Change(u)` into `consumers` counters.
fn not_chain(n: usize, consumers: usize) -> (ModelBuilder, EventPlan) {
    let mut b = ModelBuilder::default();
    let (_, _, cha_out) = real_block(&mut b, "CDL.Logical.Change");
    let mut last = cha_out[0];
    for _ in 0..n {
        let (_, i, o) = real_block(&mut b, "CDL.Logical.Not");
        b.connect(last, i[0]);
        last = o[0];
    }
    for _ in 0..consumers {
        let (_, cnt_in) = counter(&mut b);
        b.connect(last, cnt_in[0]);
    }
    let p = plan(&b).1;
    (b, p)
}

fn steps(plan: &EventPlan, blocks: usize) -> usize {
    (0..blocks).filter(|&i| plan.step(i).is_some()).count()
}

#[test]
fn the_plan_grows_linearly_with_the_cone_and_the_fan_out() {
    // 1x, 2x, 4x of a chain and of the consumers it feeds: steps and settled inputs scale
    // exactly, one step per logic block and two settled inputs per counter, however many share.
    for scale in [1usize, 2, 4] {
        let (b, plan) = not_chain(100 * scale, 10 * scale);
        assert_eq!(steps(&plan, b.model.blocks.len()), 100 * scale);
        let hidden: usize = (0..b.model.blocks.len())
            .filter_map(|i| plan.hidden(i).map(<[_]>::len))
            .sum();
        assert_eq!(hidden, 2 * 10 * scale);
    }
}

#[test]
fn a_hundred_thousand_deep_not_chain_derives_without_recursion_and_runs() {
    // An explicit stack: no stack overflow on a deep cone (the test thread's default stack).
    let (b, plan) = not_chain(100_000, 1);
    assert_eq!(steps(&plan, b.model.blocks.len()), 100_000);
    let schedule = compile(&b.model, &b.blocks).unwrap();
    let mut state = allocate_state(&b.model, &b.blocks).unwrap();
    tick_initial(&b, &schedule, &mut state, 0.0, true);
    tick_initial(&b, &schedule, &mut state, 1.0, false);
}

#[test]
fn the_shadow_pass_adds_no_scheduling_edge() {
    // The same model compiles to the same block order with and without settled inputs being
    // active: Change into the counter directly, or through an `Or` with a level.
    let (b, _) = not_chain(3, 2);
    let schedule = compile(&b.model, &b.blocks).unwrap();
    let dag = build_feedthrough_dag(&b.model, &b.blocks);
    let again = topo_sort(&dag, &b.model, &b.blocks).unwrap();
    assert_eq!(schedule.order, again.order);
    assert!(again.events.is_empty(), "topo_sort alone derives no plan");
}
