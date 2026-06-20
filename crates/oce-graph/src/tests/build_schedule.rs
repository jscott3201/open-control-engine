use super::*;

#[test]
fn chain_schedule_is_declaration_order() {
    let mut b = ModelBuilder::default();
    let (c0, _, o0) = b.block("CDL.Reals.Sources.Constant", 0, 1, false, false);
    let (c1, _, o1) = b.block("CDL.Reals.Sources.Constant", 0, 1, false, false);
    let (add, add_in, _) = b.block("CDL.Reals.Add", 2, 1, true, false);
    b.connect(o0[0], add_in[0]);
    b.connect(o1[0], add_in[1]);

    let sched = compile(&b.model, &b.blocks).expect("acyclic");
    assert_eq!(sched.order, vec![c0, c1, add]);

    let dag = build_feedthrough_dag(&b.model, &b.blocks);
    assert!(is_valid_topo_order(&dag, &sched.connector_order));
    assert_eq!(
        reference_order(&dag, &b.model).unwrap(),
        sched.connector_order
    );
}

#[test]
fn pre_breaks_the_feedback_loop() {
    // source → add.u1 ; pre.out → add.u2 ; add.out → pre.in. Pre (feeds=false) cuts the loop.
    let mut b = ModelBuilder::default();
    let (source, _, src_out) = b.block("CDL.Reals.Sources.Constant", 0, 1, false, false);
    let (add, add_in, add_out) = b.block("CDL.Reals.Add", 2, 1, true, false);
    let (pre, pre_in, pre_out) = b.block("CDL.Logical.Pre", 1, 1, false, true);
    b.connect(src_out[0], add_in[0]);
    b.connect(pre_out[0], add_in[1]);
    b.connect(add_out[0], pre_in[0]);

    let sched = compile(&b.model, &b.blocks).expect("Pre cuts the loop → acyclic");
    assert_eq!(sched.order.len(), 3);
    let mut uniq = sched.order.clone();
    uniq.sort_unstable();
    uniq.dedup();
    assert_eq!(uniq.len(), 3, "each block scheduled exactly once");

    let pos = |id: BlockId| sched.order.iter().position(|&x| x == id).unwrap();
    // Pre emits its prior-state output before Add reads it; the source precedes Add too.
    assert!(pos(pre) < pos(add), "Pre must fire before Add");
    assert!(pos(source) < pos(add));

    let dag = build_feedthrough_dag(&b.model, &b.blocks);
    assert!(is_valid_topo_order(&dag, &sched.connector_order));
    assert_eq!(
        reference_order(&dag, &b.model).unwrap(),
        sched.connector_order
    );
}

#[test]
fn stateful_reals_comparator_does_not_break_feedback_loop() {
    // R-REALS-1 trap: `Greater(h>0)` is stateful but direct-feedthrough, so it cannot cut a
    // feedback loop the way `Pre`/`UnitDelay` do.
    let mut b = ModelBuilder::default();
    let (_gt, gt_in, gt_out) = b.block_real(make("CDL.Reals.Greater", &[("h", Value::Real(1.0))]));
    b.connect(gt_out[0], gt_in[0]);

    let err = compile(&b.model, &b.blocks).expect_err("stateful comparator must not cut the loop");
    assert!(
        matches!(
            err,
            BuildError::AlgebraicLoop { .. } | BuildError::BlockAlgebraicLoop { .. }
        ),
        "expected algebraic loop rejection, got {err:?}"
    );
}

#[test]
fn algebraic_loop_is_rejected_with_cycle_members() {
    // Same topology as the Pre test, but the loop element feeds through (an [A] passthrough), so
    // the feedback is a true algebraic loop and BUILD must reject it.
    let mut b = ModelBuilder::default();
    let (_gen, _, gen_out) = b.block("CDL.Reals.Sources.Constant", 0, 1, false, false);
    let (_add, add_in, add_out) = b.block("CDL.Reals.Add", 2, 1, true, false);
    let (_pass, pass_in, pass_out) = b.block("test.Passthrough", 1, 1, true, false);
    b.connect(gen_out[0], add_in[0]);
    b.connect(pass_out[0], add_in[1]);
    b.connect(add_out[0], pass_in[0]);

    let err = compile(&b.model, &b.blocks).expect_err("all-feedthrough cycle is an algebraic loop");
    let msg = err.to_string();
    let members = match err {
        BuildError::AlgebraicLoop { members } => members,
        other => panic!("expected connector-level AlgebraicLoop, got {other:?}"),
    };
    assert!(
        members.len() >= 2,
        "a cycle must name at least two connectors, got {members:?}"
    );
    // The diagnostic carries the verbatim CDL §7.16 remedy.
    assert!(msg.contains("CDL §7.16"), "missing §7.16 reference: {msg}");
    assert!(
        msg.contains("CDL.Logical.Pre"),
        "missing remedy block: {msg}"
    );
}

#[test]
fn schedule_is_deterministic_and_connection_order_independent() {
    // Identical declarations; connections listed in opposite order. The DAG — and therefore the
    // schedule — must be byte-identical (`01` §6.1: result invariant to connection order).
    fn build_one(reverse_conns: bool) -> Schedule {
        let mut b = ModelBuilder::default();
        let (_c0, _, o0) = b.block("CDL.Reals.Sources.Constant", 0, 1, false, false);
        let (_c1, _, o1) = b.block("CDL.Reals.Sources.Constant", 0, 1, false, false);
        let (_add, add_in, _) = b.block("CDL.Reals.Add", 2, 1, true, false);
        if reverse_conns {
            b.connect(o1[0], add_in[1]);
            b.connect(o0[0], add_in[0]);
        } else {
            b.connect(o0[0], add_in[0]);
            b.connect(o1[0], add_in[1]);
        }
        compile(&b.model, &b.blocks).unwrap()
    }

    let forward = build_one(false);
    let reversed = build_one(true);
    let again = build_one(false);
    assert_eq!(forward.order, reversed.order);
    assert_eq!(forward.connector_order, reversed.connector_order);
    assert_eq!(forward.order, again.order);
    assert_eq!(forward.connector_order, again.connector_order);
}

#[test]
fn matches_reference_oracle_on_a_fan_out_dag() {
    // A source feeding a fan-out block whose two outputs reconverge at an adder — multiple
    // vertices become ready simultaneously, exercising the heap tie-break.
    let mut b = ModelBuilder::default();
    let (_src, _, s) = b.block("CDL.Reals.Sources.Constant", 0, 1, false, false);
    let (_fan, fan_in, fan_out) = b.block("CDL.Reals.Gain", 1, 2, true, false);
    let (_join, join_in, _) = b.block("CDL.Reals.Add", 2, 1, true, false);
    b.connect(s[0], fan_in[0]);
    b.connect(fan_out[0], join_in[0]);
    b.connect(fan_out[1], join_in[1]);

    let sched = compile(&b.model, &b.blocks).unwrap();
    let dag = build_feedthrough_dag(&b.model, &b.blocks);
    assert!(is_valid_topo_order(&dag, &sched.connector_order));
    assert_eq!(
        reference_order(&dag, &b.model).unwrap(),
        sched.connector_order
    );
}

#[test]
fn output_less_sink_fires_after_its_inputs() {
    // An [A] sink (no outputs) still reads its input at emit time, so it has an emit-before edge
    // from its source and fires after it.
    let mut b = ModelBuilder::default();
    let (src, _, s) = b.block("CDL.Reals.Sources.Constant", 0, 1, false, false);
    let (sink, sink_in, _) = b.block("CDL.Utilities.Assert", 1, 0, true, false);
    b.connect(s[0], sink_in[0]);

    let sched = compile(&b.model, &b.blocks).unwrap();
    assert_eq!(sched.order.len(), 2);
    let pos = |id: BlockId| sched.order.iter().position(|&x| x == id).unwrap();
    assert!(pos(sink) > pos(src), "sink fires after its input source");
}

// ---- TICK tests with real blocks -------------------------------------------------------------
#[test]
fn mixed_feedthrough_block_cycle_is_rejected() {
    // P is a stateful 2-output block: out0 is state-only (cut), out1 feeds through from in0.
    // X feeds through. Wiring P.out0 → X.in0 and X.out0 → P.in0 is acyclic at the CONNECTOR level
    // (P.out0 doesn't depend on P.in0), but atomic block firing cannot schedule it: X needs P's
    // output, and P (to emit out1) needs X's output. BUILD must reject it loudly.
    let mut b = ModelBuilder::default();
    let (_p, p_in, p_out) = b.block_mixed("test.MixedPre", 1, 2, true, &[(0, 1)]); // in0→out1 only
    let (_x, x_in, x_out) = b.block_mixed("test.Pass", 1, 1, false, &[(0, 0)]);
    b.connect(p_out[0], x_in[0]); // P.out0 (cut) → X.in0
    b.connect(x_out[0], p_in[0]); // X.out0 → P.in0 (feeds P.out1)

    // The connector-level graph is acyclic — so this is specifically the block-granularity case.
    let dag = build_feedthrough_dag(&b.model, &b.blocks);
    assert!(
        is_valid_topo_order(&dag, &reference_order(&dag, &b.model).unwrap()),
        "connector graph must be acyclic for this case to be meaningful"
    );

    let err = compile(&b.model, &b.blocks).expect_err("atomic firing cannot schedule this");
    let msg = err.to_string();
    let members = match err {
        BuildError::BlockAlgebraicLoop { members } => members,
        other => panic!("expected BlockAlgebraicLoop, got {other:?}"),
    };
    assert!(
        members.len() >= 2,
        "block cycle needs ≥2 blocks, got {members:?}"
    );
    assert!(
        msg.contains("block-granularity") && msg.contains("CDL §7.16"),
        "diagnostic must explain the block-granularity cause + remedy: {msg}"
    );
}

#[test]
fn mixed_feedthrough_acyclic_orders_producer_before_consumer() {
    // The regression the naïve "last-output collapse" got WRONG: C consumes P's *early* state-only
    // output, while P's *late* feedthrough output sits further down the connector order. The old
    // collapse keyed P by its last output and emitted [src, C, P] — C before its producer P (stale
    // read). The block-level emit-before sort must instead emit [src, P, C].
    let mut b = ModelBuilder::default();
    let (c, c_in, c_out) = b.block_mixed("test.Consumer", 1, 1, false, &[(0, 0)]); // block 0 (early decl)
    let (src, _, src_out) = b.block("test.Src", 0, 1, false, false); // block 1
    let (p, p_in, p_out) = b.block_mixed("test.MixedPre", 1, 2, true, &[(0, 1)]); // block 2: out0 cut, out1 feeds
    b.connect(src_out[0], p_in[0]); // src → P.in0 (feeds P.out1)
    b.connect(p_out[0], c_in[0]); // P.out0 (cut, early) → C.in0
    let _ = c_out;

    let sched = compile(&b.model, &b.blocks).expect("acyclic at both levels");
    let pos = |id: BlockId| sched.order.iter().position(|&x| x == id).unwrap();
    assert!(
        pos(p) < pos(c),
        "C consumes P's output → P must fire first (was [src, C, P])"
    );
    assert!(pos(src) < pos(p), "src feeds P → src first");
    assert_eq!(sched.order.len(), 3);
}

#[test]
fn decl_key_collision_is_broken_by_connector_id() {
    // Force a decl_key collision (two blocks and two connectors all sharing decl_order 0). The
    // load-bearing final `ConnectorId` / `BlockId` tie-break in the ready set must still yield a
    // total, deterministic order — by id. A regression dropping that tie-break would make the
    // heap's equal-key pop order unspecified.
    let mut b = ModelBuilder::default();
    let (c0, _, o0) = b.block("test.Const", 0, 1, false, false);
    let (c1, _, o1) = b.block("test.Const", 0, 1, false, false);
    for blk in &mut b.model.blocks {
        blk.decl_order = 0;
    }
    for conn in &mut b.model.connectors {
        conn.decl_order = 0;
    }

    let first = compile(&b.model, &b.blocks).unwrap();
    let second = compile(&b.model, &b.blocks).unwrap();
    assert_eq!(
        first.connector_order,
        vec![o0[0], o1[0]],
        "ties break by ConnectorId"
    );
    assert_eq!(first.order, vec![c0, c1], "ties break by BlockId");
    assert_eq!(
        first.connector_order, second.connector_order,
        "deterministic"
    );
    assert_eq!(first.order, second.order, "deterministic");
}
