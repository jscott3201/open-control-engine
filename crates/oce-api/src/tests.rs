//! M0 exit-criteria harness (`08` / FRAME M0 §exit). Builds a **hand-built** flattened model with
//! **no parser** and asserts the engine's load → BUILD → tick contract end to end:
//!
//! - exit #1/#2: the `Engine` loads a hand-built `ModelGraph` and ticks it (scaffold shape is real);
//! - exit #3: a multi-tick run advances the canonical `Add`/`UnitDelay` feedback accumulator with a
//!   true **one-tick** delay (1, 2, 3, 4 — not the two-tick delay an inline emit-then-update gives);
//! - exit #4: an injected feedthrough cycle is rejected with a typed [`BuildError::AlgebraicLoop`];
//! - exit #3 (determinism): two independent compiles of the same model produce **byte-identical**
//!   `order`/`connector_order`/`driver_of`;
//! - exit #5: a `MemStore` model round-trip + no-op `commit`/`flush`/`recover` through the engine.

use std::sync::Arc;

use oce_graph::BuildError;
use oce_model::{
    BlockId, BlockInstance, Connection, Connector, ConnectorId, Dir, ModelGraph, ParamTable, Value,
    ValueType,
};
use oce_store::{DomainKey, Durable, ModelStore, ResolvedModel};

use super::{Engine, OcError};

/// Tiny hand-builder for a flattened [`ModelGraph`]: dense block/connector ids are assigned in
/// declaration order (so `decl_order == id`), which is exactly what the deterministic scheduler
/// tie-breaks on. Keeps the exit-criteria graphs readable without a parser.
struct Mb {
    m: ModelGraph,
}

impl Mb {
    fn new() -> Self {
        Self {
            m: ModelGraph::new(),
        }
    }

    /// Append a block with `ins` input connectors and `outs` output connectors (typed), plus its
    /// ground params; returns `(block, input connector ids, output connector ids)`.
    fn block(
        &mut self,
        class: &str,
        ins: &[ValueType],
        outs: &[ValueType],
        params: Vec<(Arc<str>, Value)>,
    ) -> (BlockId, Vec<ConnectorId>, Vec<ConnectorId>) {
        let bid = BlockId(self.m.blocks.len() as u32);
        let push_conn = |m: &mut ModelGraph, dir: Dir, vt: ValueType| {
            let id = ConnectorId(m.connectors.len() as u32);
            m.connectors.push(Connector::new(id, bid, dir, vt, id.0));
            id
        };
        let in_ids: Vec<ConnectorId> = ins
            .iter()
            .map(|&vt| push_conn(&mut self.m, Dir::In, vt))
            .collect();
        let out_ids: Vec<ConnectorId> = outs
            .iter()
            .map(|&vt| push_conn(&mut self.m, Dir::Out, vt))
            .collect();
        self.m.blocks.push(BlockInstance {
            id: bid,
            class_iri: Arc::from(class),
            inputs: in_ids.clone(),
            outputs: out_ids.clone(),
            params: ParamTable { values: params },
            decl_order: bid.0,
            instance_iri: None,
        });
        (bid, in_ids, out_ids)
    }

    fn connect(&mut self, from: ConnectorId, to: ConnectorId) {
        self.m.connections.push(Connection { from, to });
    }

    fn finish(self) -> ModelGraph {
        self.m
    }
}

fn rp(name: &str, v: f64) -> (Arc<str>, Value) {
    (Arc::from(name), Value::Real(v))
}

/// The canonical M0 graph (6 blocks): a `Constant(1)` drives an `Add` that is closed into a
/// one-sample feedback loop by a `UnitDelay` (the accumulator `acc(k) = acc(k-1) + 1`), with a
/// `Constant(2.5)` + `Greater` comparator and a `Limiter[0,3]` reading the accumulator. Returns the
/// model plus the `(add_out, greater_out, limiter_out)` connectors to probe.
fn build_accumulator_model() -> (ModelGraph, ConnectorId, ConnectorId, ConnectorId) {
    let mut mb = Mb::new();
    let (_, _, c1) = mb.block(
        "CDL.Reals.Sources.Constant",
        &[],
        &[ValueType::Real],
        vec![rp("k", 1.0)],
    );
    let (_, add_in, add_out) = mb.block(
        "CDL.Reals.Add",
        &[ValueType::Real, ValueType::Real],
        &[ValueType::Real],
        vec![],
    );
    let (_, ud_in, ud_out) = mb.block(
        "CDL.Discrete.UnitDelay",
        &[ValueType::Real],
        &[ValueType::Real],
        vec![],
    );
    let (_, _, c2) = mb.block(
        "CDL.Reals.Sources.Constant",
        &[],
        &[ValueType::Real],
        vec![rp("k", 2.5)],
    );
    let (_, gt_in, gt_out) = mb.block(
        "CDL.Reals.Greater",
        &[ValueType::Real, ValueType::Real],
        &[ValueType::Boolean],
        vec![],
    );
    let (_, lim_in, lim_out) = mb.block(
        "CDL.Reals.Limiter",
        &[ValueType::Real],
        &[ValueType::Real],
        vec![rp("uMin", 0.0), rp("uMax", 3.0)],
    );

    mb.connect(c1[0], add_in[0]); // Constant(1) → Add.u1
    mb.connect(ud_out[0], add_in[1]); // UnitDelay → Add.u2  (the feedback edge)
    mb.connect(add_out[0], ud_in[0]); // Add → UnitDelay      (closes the loop; UnitDelay cuts it)
    mb.connect(add_out[0], gt_in[0]); // Add → Greater.u1
    mb.connect(c2[0], gt_in[1]); // Constant(2.5) → Greater.u2
    mb.connect(add_out[0], lim_in[0]); // Add → Limiter

    (mb.finish(), add_out[0], gt_out[0], lim_out[0])
}

/// Two `Add` blocks driving each other: a feedthrough cycle with **no** state-holding loop-breaker
/// — the model an engine must reject (CDL §7.16), not silently solve.
fn build_algebraic_loop_model() -> ModelGraph {
    let mut mb = Mb::new();
    let (_, a_in, a_out) = mb.block(
        "CDL.Reals.Add",
        &[ValueType::Real, ValueType::Real],
        &[ValueType::Real],
        vec![],
    );
    let (_, b_in, b_out) = mb.block(
        "CDL.Reals.Add",
        &[ValueType::Real, ValueType::Real],
        &[ValueType::Real],
        vec![],
    );
    mb.connect(a_out[0], b_in[0]);
    mb.connect(b_out[0], a_in[0]);
    mb.finish()
}

#[test]
fn m0_hand_built_graph_builds_advances_and_is_byte_identical() {
    let (m1, add_out, gt_out, lim_out) = build_accumulator_model();
    let (m2, _, _, _) = build_accumulator_model();

    let mut eng = Engine::in_memory();
    eng.build_model_in_memory(m1)
        .expect("BUILD must succeed for an acyclic (loop-broken) graph");

    // Exit #3 (determinism): an independent compile of the same model is byte-identical.
    let mut eng2 = Engine::in_memory();
    eng2.build_model_in_memory(m2).expect("BUILD must succeed");
    assert_eq!(
        eng.schedule().order,
        eng2.schedule().order,
        "block order must be byte-identical"
    );
    assert_eq!(
        eng.schedule().connector_order,
        eng2.schedule().connector_order,
        "connector order must be byte-identical"
    );
    assert_eq!(
        eng.schedule().driver_of,
        eng2.schedule().driver_of,
        "alias map must be byte-identical"
    );
    // Determinism extends to the allocated state LAYOUT, not just the schedule: identical `[S]`
    // word seeding and slot directory (reaching the private `state` field — the test is in-crate).
    assert_eq!(
        eng.state.words, eng2.state.words,
        "seeded state words must be byte-identical"
    );
    assert_eq!(
        eng.state.slot_of, eng2.state.slot_of,
        "state slot directory must be byte-identical"
    );

    // Exit #3: a one-tick feedback delay ⇒ accumulator 1,2,3,4; Greater(>2.5) F,F,T,T; Limiter 1,2,3,3.
    let expected_acc = [1.0, 2.0, 3.0, 4.0];
    let expected_gt = [false, false, true, true];
    let expected_lim = [1.0, 2.0, 3.0, 3.0];
    for (k, t) in [0.0_f64, 1.0, 2.0, 3.0].into_iter().enumerate() {
        let out = eng.tick(t).expect("monotonic tick must not regress");
        assert!(
            out.get(add_out)
                .unwrap()
                .bit_eq(&Value::Real(expected_acc[k])),
            "accumulator at tick {k}: {:?}",
            out.get(add_out)
        );
        assert!(
            out.get(gt_out)
                .unwrap()
                .bit_eq(&Value::Boolean(expected_gt[k])),
            "greater at tick {k}: {:?}",
            out.get(gt_out)
        );
        assert!(
            out.get(lim_out)
                .unwrap()
                .bit_eq(&Value::Real(expected_lim[k])),
            "limiter at tick {k}: {:?}",
            out.get(lim_out)
        );
    }

    // `outputs()` mirrors the snapshot returned by the last `tick`.
    assert!(
        eng.outputs()
            .get(add_out)
            .unwrap()
            .bit_eq(&Value::Real(4.0))
    );
    assert_eq!(
        eng.outputs().len(),
        6,
        "six output connectors (4 sources/derived + Greater + Limiter)"
    );
}

#[test]
fn m0_injected_algebraic_loop_is_rejected() {
    let mut eng = Engine::in_memory();
    let err = eng
        .build_model_in_memory(build_algebraic_loop_model())
        .expect_err("a feedthrough cycle with no loop-breaker must be rejected (CDL §7.16)");
    assert!(
        matches!(err, OcError::Build(BuildError::AlgebraicLoop { .. })),
        "expected a typed AlgebraicLoop build error, got {err:?}"
    );
}

#[test]
fn m0_unknown_block_class_is_a_typed_load_error() {
    let mut mb = Mb::new();
    mb.block("CDL.Reals.NotARealBlock", &[], &[ValueType::Real], vec![]);
    let mut eng = Engine::in_memory();
    let err = eng
        .build_model_in_memory(mb.finish())
        .expect_err("an unknown class IRI must not panic");
    assert!(
        matches!(err, OcError::Load { .. }),
        "expected a typed Load error, got {err:?}"
    );
}

#[test]
fn m0_tick_time_must_be_monotonic() {
    let (m, _, _, _) = build_accumulator_model();
    let mut eng = Engine::in_memory();
    eng.build_model_in_memory(m).unwrap();

    eng.tick(0.0).unwrap();
    eng.tick(5.0).unwrap();
    eng.tick(5.0).unwrap(); // equal time is allowed (non-decreasing)

    // A decrease is a typed host error — and must not advance the model.
    let err = eng.tick(4.0).expect_err("time regression must be rejected");
    assert!(
        matches!(err, OcError::TimeRegression { .. }),
        "expected TimeRegression, got {err:?}"
    );
}

#[test]
fn m0_non_finite_tick_time_is_rejected() {
    let (m, _, _, _) = build_accumulator_model();
    let mut eng = Engine::in_memory();
    eng.build_model_in_memory(m).unwrap();

    // NaN/∞ are rejected up front: a NaN would otherwise slip past `t_now < prev` and silently
    // disable the monotonic guard. The rejected tick must not advance the model time.
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let err = eng.tick(bad).expect_err("non-finite time must be rejected");
        assert!(
            matches!(err, OcError::NonFiniteTime { .. }),
            "expected NonFiniteTime for {bad}, got {err:?}"
        );
    }
    // A finite tick still works afterwards (state was never corrupted by the rejected ticks).
    eng.tick(0.0).unwrap();
    eng.tick(1.0).unwrap();
}

#[test]
fn m0_engine_store_round_trips_and_durability_is_noop() {
    let (m, _, _, _) = build_accumulator_model();
    let mut eng = Engine::in_memory();
    eng.build_model_in_memory(m).unwrap();

    // Exit #5: a model round-trips through the engine's wired store…
    let rm = ResolvedModel {
        model_id: DomainKey::new("seq:m0-harness"),
        schema_rev: 1,
        classes: Vec::new(),
        blocks: Vec::new(),
        points: Vec::new(),
        connections: Vec::new(),
        containment: Vec::new(),
    };
    eng.store().save_model(&rm).unwrap();
    assert_eq!(
        eng.store().load_model(&rm.model_id).unwrap().model_id,
        rm.model_id
    );

    // …and the no-op durability hooks all succeed (MemStore offers no crash durability; §5 R-6).
    eng.store().commit().unwrap();
    eng.store().flush().unwrap();
    eng.store().recover().unwrap();
}
