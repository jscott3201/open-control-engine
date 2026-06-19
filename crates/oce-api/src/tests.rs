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

use super::{
    CollectSpec, Engine, InputSource, IoClass, OcError, PhysicalKind, PointDirection, RunMode,
    SemanticQuery, SimSpec, TemplateRef,
};
use oce_store_mem::MemStore;

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
    let mut model = mb.finish();
    model.external_inputs = vec![a_in[1], b_in[1]];
    model
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

// ============================ M1-PR-10: frozen-surface fill ============================

/// Load the canonical accumulator model into a fresh in-memory engine. Connector paths are
/// `conn#<id>` (hand-built, no `iri`); param paths are `b<id>.<name>` (no `instance_iri`).
fn loaded_accumulator() -> Engine<MemStore> {
    let (m, _, _, _) = build_accumulator_model();
    let mut eng = Engine::in_memory();
    eng.build_model_in_memory(m).expect("BUILD must succeed");
    eng
}

fn sim_spec(t_start: f64, t_stop: f64, step: f64, collect: CollectSpec) -> SimSpec {
    SimSpec {
        t_start,
        t_stop,
        step,
        inputs: InputSource::None,
        collect,
    }
}

/// A single `Add` block with **undriven** inputs (conn#0, conn#1 → conn#2). No connections, so the
/// inputs are external (host-staged) — the model needed to observe `set_input` / `InputSource`
/// values flow through to an output (the accumulator's inputs are all internally driven).
fn free_add_model() -> ModelGraph {
    let mut mb = Mb::new();
    let (_, inputs, _) = mb.block(
        "CDL.Reals.Add",
        &[ValueType::Real, ValueType::Real],
        &[ValueType::Real],
        vec![],
    );
    let mut model = mb.finish();
    model.external_inputs = inputs;
    model
}

// ---- R-PUB-7 / R-API-PY-1..8: the compile-shaped frozen-surface guards (frozen-signature pins,
// Clone family, owned-snapshot enumeration, and the Engine<MemStore>: Send + Sync assertion) live in
// the NON-test module `crate::guards` (M1-PR-12) so a drift fails the per-PR `cargo build`, not only
// this release-gate test target. This smoke test documents that intent; the real enforcement is that
// `guards.rs` compiles.
#[test]
fn frozen_surface_guards_compile() {
    // Intentionally empty — the assertions are compile-time in `crate::guards`.
}

// ---- non-panicking on adversarial / empty input (every path is a typed error) ----

#[test]
fn empty_engine_surface_is_inert_not_panicking() {
    let eng = Engine::in_memory(); // no model loaded
    assert!(eng.io().is_empty());
    assert_eq!(eng.io_summary().total, 0);
    assert_eq!(eng.params().iter().count(), 0);
    assert_eq!(eng.point_list(None).unwrap().len(), 0);
    assert!(matches!(eng.get_output("x"), Err(OcError::UnknownPoint(_))));
    assert!(matches!(eng.get_param("x"), Err(OcError::UnknownPoint(_))));
    assert_eq!(eng.mode(), RunMode::Running);
}

#[test]
fn set_input_resolves_validates_and_rejects() {
    let mut eng = Engine::in_memory();
    eng.build_model_in_memory(free_add_model()).unwrap();
    // conn#0 = Add.u0 (Real input). A wrong-typed value is a typed error — no coercion.
    assert!(matches!(
        eng.set_input("conn#0", Value::Boolean(true)),
        Err(OcError::InputType(_))
    ));
    eng.set_input("conn#0", Value::Real(5.0))
        .expect("a correctly-typed real input stages on u0");
    eng.set_input("conn#1", Value::Real(2.0))
        .expect("a correctly-typed real input stages on u1");
    eng.tick(0.0).unwrap();
    // Prove the staged values reached the resolved slots through the block output, without reading an
    // input through `get_output`.
    assert!(
        eng.get_output("conn#2").unwrap().bit_eq(&Value::Real(7.0)),
        "staged input values must propagate to the Add output"
    );
    assert!(matches!(
        eng.set_input("nope", Value::Real(1.0)),
        Err(OcError::UnknownPoint(_))
    ));
    // conn#2 is an OUTPUT, so set_input must reject it as an unknown *input*.
    assert!(matches!(
        eng.set_input("conn#2", Value::Real(1.0)),
        Err(OcError::UnknownPoint(_))
    ));
}

#[test]
fn get_output_on_input_point_is_unknown_point() {
    let mut eng = Engine::in_memory();
    eng.build_model_in_memory(free_add_model()).unwrap();
    eng.set_input("conn#0", Value::Real(5.0)).unwrap();
    assert!(matches!(
        eng.get_output("conn#0"),
        Err(OcError::UnknownPoint(_))
    ));
}

#[test]
fn deferred_load_paths_return_typed_errors_not_panics() {
    let mut eng = Engine::in_memory();
    let q = SemanticQuery::FuzzyText {
        query: "x".into(),
        k: 1,
    };
    assert!(matches!(
        eng.load_from_semantic(&TemplateRef::new("tpl:x"), &q),
        Err(OcError::Load { .. })
    ));
    assert!(matches!(
        eng.load_modelica(std::path::Path::new("/x.mo")),
        Err(OcError::Load { .. })
    ));
    // A device-filtered point list is the deferred §7.7.5 traversal; the in-memory mirror is fine.
    assert!(matches!(
        eng.point_list(Some("AHU-1")),
        Err(OcError::Load { .. })
    ));
}

// ---- the live parameter table: halt → set → resume actually re-folds + re-instantiates ----

#[test]
fn param_lifecycle_halt_set_resume_refolds() {
    let mut eng = loaded_accumulator();
    // 4 params: b0.k=1.0, b3.k=2.5, b5.uMin=0.0, b5.uMax=3.0.
    assert_eq!(eng.params().len(), 4);
    assert!(eng.get_param("b0.k").unwrap().bit_eq(&Value::Real(1.0)));
    // The R-PUB-6 owned enumeration yields (path, value, declared attrs); M1 attrs are bounds-free.
    let rows = eng.params().to_vec();
    let (_, k0_val, k0_attrs) = rows
        .iter()
        .find(|(p, _, _)| p == "b0.k")
        .expect("b0.k present in the owned enumeration");
    assert!(k0_val.bit_eq(&Value::Real(1.0)));
    assert_eq!(k0_attrs.value_type, ValueType::Real);
    assert!(
        k0_attrs.min.is_none()
            && k0_attrs.max.is_none()
            && k0_attrs.unit.is_none()
            && k0_attrs.quantity.is_none(),
        "M1 ParamAttrs carry no declared bounds/units yet"
    );
    // set_param while Running is rejected (CDL §7.4.2).
    assert!(matches!(
        eng.set_param("b0.k", Value::Real(9.0)),
        Err(OcError::ParamWhileRunning { .. })
    ));
    eng.halt().unwrap();
    assert_eq!(eng.mode(), RunMode::Halted);
    // Wrong type / unknown path are typed errors.
    assert!(matches!(
        eng.set_param("b0.k", Value::Boolean(true)),
        Err(OcError::ParamType { .. })
    ));
    assert!(matches!(
        eng.set_param("nope.x", Value::Real(1.0)),
        Err(OcError::UnknownPoint(_))
    ));
    // Correct set, then resume folds it into block state and re-instantiates.
    eng.set_param("b0.k", Value::Real(9.0)).unwrap();
    eng.resume().unwrap();
    assert_eq!(eng.mode(), RunMode::Running);
    assert!(eng.get_param("b0.k").unwrap().bit_eq(&Value::Real(9.0)));
    // The re-folded Constant(9) now drives Add ⇒ accumulator starts at 9 (proves re-instantiation).
    eng.tick(0.0).unwrap();
    assert!(eng.get_output("conn#3").unwrap().bit_eq(&Value::Real(9.0)));
}

// ---- simulation mode: real loop, golden trace, bit-determinism, adversarial specs ----

#[test]
fn simulate_runs_horizon_and_collects_named_trace() {
    let mut eng = loaded_accumulator();
    // Record add_out (conn#3) + limiter_out (conn#11) over [0,3] step 1.
    let spec = sim_spec(
        0.0,
        3.0,
        1.0,
        CollectSpec::Named {
            points: vec!["conn#3".to_string(), "conn#11".to_string()],
            stride: 1,
        },
    );
    let m = eng.simulate(&spec).unwrap();
    assert_eq!(m.ticks, 4);
    assert_eq!(m.trace.rows(), 4);
    assert_eq!(
        m.trace.columns(),
        ["conn#3".to_string(), "conn#11".to_string()]
    );
    for column in m.trace.columns() {
        let info = eng
            .io()
            .iter()
            .find(|p| p.path == *column)
            .expect("trace column must be present in the IO inventory");
        assert_eq!(
            info.direction,
            PointDirection::Out,
            "CollectSpec::Named must record only outputs: {column}"
        );
    }
    for (j, expected) in [[1.0_f64, 2.0, 3.0, 4.0], [1.0_f64, 2.0, 3.0, 3.0]]
        .into_iter()
        .enumerate()
    {
        let col = m.trace.column(j).unwrap();
        for (i, e) in expected.into_iter().enumerate() {
            assert!(
                col[i].bit_eq(&Value::Real(e)),
                "col {j} row {i}: {:?}",
                col[i]
            );
        }
    }
    let times: Vec<u64> = m.trace.times().iter().map(|t| t.to_bits()).collect();
    let want: Vec<u64> = [0.0_f64, 1.0, 2.0, 3.0]
        .iter()
        .map(|t| t.to_bits())
        .collect();
    assert_eq!(times, want, "horizon times must be bit-exact");
}

#[test]
fn simulate_is_bit_deterministic() {
    let run = || {
        let mut e = loaded_accumulator();
        e.simulate(&sim_spec(0.0, 5.0, 1.0, CollectSpec::All { stride: 1 }))
            .unwrap()
    };
    let a = run();
    let b = run();
    assert_eq!(a.ticks, b.ticks);
    assert_eq!(a.trace.columns(), b.trace.columns());
    let ta: Vec<u64> = a.trace.times().iter().map(|t| t.to_bits()).collect();
    let tb: Vec<u64> = b.trace.times().iter().map(|t| t.to_bits()).collect();
    assert_eq!(ta, tb, "trace times must be byte-identical across runs");
    for j in 0..a.trace.columns().len() {
        let (ca, cb) = (a.trace.column(j).unwrap(), b.trace.column(j).unwrap());
        assert_eq!(ca.len(), cb.len());
        for (x, y) in ca.iter().zip(cb) {
            assert!(x.bit_eq(y), "col {j} diverged: {x:?} vs {y:?}");
        }
    }
}

#[test]
fn simulate_rejects_bad_spec_without_panicking() {
    let mut eng = loaded_accumulator();
    for bad in [0.0, -1.0, f64::NAN] {
        assert!(
            matches!(
                eng.simulate(&sim_spec(0.0, 3.0, bad, CollectSpec::None)),
                Err(OcError::Load { .. })
            ),
            "step {bad} must be a typed Load error"
        );
    }
    assert!(matches!(
        eng.simulate(&sim_spec(f64::NAN, 3.0, 1.0, CollectSpec::None)),
        Err(OcError::NonFiniteTime { .. })
    ));
    assert!(matches!(
        eng.simulate(&sim_spec(f64::INFINITY, 3.0, 1.0, CollectSpec::None)),
        Err(OcError::NonFiniteTime { .. })
    ));
    assert!(matches!(
        eng.simulate(&sim_spec(3.0, 0.0, 1.0, CollectSpec::None)),
        Err(OcError::TimeRegression { .. })
    ));
    // CSV InputSource is frozen-as-variant but deferred ⇒ typed Load error.
    let csv = SimSpec {
        t_start: 0.0,
        t_stop: 1.0,
        step: 1.0,
        inputs: InputSource::Csv {
            path: "x.csv".into(),
            bindings: vec![],
        },
        collect: CollectSpec::None,
    };
    assert!(matches!(eng.simulate(&csv), Err(OcError::Load { .. })));
    // A Named collect with an unknown output fails fast (no partial trace).
    let bad_named = sim_spec(
        0.0,
        3.0,
        1.0,
        CollectSpec::Named {
            points: vec!["nope".to_string()],
            stride: 1,
        },
    );
    assert!(matches!(
        eng.simulate(&bad_named),
        Err(OcError::UnknownPoint(_))
    ));
}

#[test]
fn collect_named_rejects_input_point() {
    let mut eng = Engine::in_memory();
    eng.build_model_in_memory(free_add_model()).unwrap();
    let spec = SimSpec {
        t_start: 0.0,
        t_stop: 1.0,
        step: 1.0,
        inputs: InputSource::None,
        collect: CollectSpec::Named {
            points: vec!["conn#0".to_string()],
            stride: 1,
        },
    };
    assert!(matches!(eng.simulate(&spec), Err(OcError::UnknownPoint(_))));
}

#[test]
fn get_output_on_valid_output_returns_bit_exact_value() {
    let mut eng = Engine::in_memory();
    eng.build_model_in_memory(free_add_model()).unwrap();
    eng.set_input("conn#0", Value::Real(3.0)).unwrap();
    eng.set_input("conn#1", Value::Real(4.0)).unwrap();
    eng.tick(0.0).unwrap();
    assert!(eng.get_output("conn#2").unwrap().bit_eq(&Value::Real(7.0)));
}

#[test]
fn simulate_resets_run_clock_at_entry() {
    let mut eng = loaded_accumulator();
    eng.tick(100.0).unwrap(); // prev_t = 100
    // simulate over [0,3] resets prev_t at entry, so ticking at t=0 does NOT regress.
    let m = eng
        .simulate(&sim_spec(0.0, 3.0, 1.0, CollectSpec::None))
        .unwrap();
    assert_eq!(m.ticks, 4);
    // CollectSpec::None ⇒ a genuinely empty trace: no columns AND no phantom time rows.
    assert!(m.trace.columns().is_empty());
    assert_eq!(m.trace.rows(), 0, "timing-only run records no rows");
    assert!(m.trace.times().is_empty());
}

#[test]
fn simulate_closure_input_source_is_callable_through_frozen_spec() {
    let mut eng = loaded_accumulator();
    let spec = SimSpec {
        t_start: 0.0,
        t_stop: 1.0,
        step: 1.0,
        inputs: InputSource::Closure(Box::new(|_t| {
            vec![("conn#1".to_string(), Value::Real(0.0))]
        })),
        collect: CollectSpec::None,
    };
    let m = eng.simulate(&spec).unwrap();
    assert_eq!(m.ticks, 2);
}

#[test]
fn set_input_flows_through_to_an_undriven_output() {
    // On the free-Add model, staged inputs are NOT overwritten by a connection, so they reach the
    // output: set 3 + 4, tick, read conn#2 == 7 bit-exactly.
    let mut eng = Engine::in_memory();
    eng.build_model_in_memory(free_add_model()).unwrap();
    eng.set_input("conn#0", Value::Real(3.0)).unwrap();
    eng.set_input("conn#1", Value::Real(4.0)).unwrap();
    eng.tick(0.0).unwrap();
    assert!(eng.get_output("conn#2").unwrap().bit_eq(&Value::Real(7.0)));
}

#[test]
fn simulate_constant_input_source_flows_through() {
    // InputSource::Constant is a live (non-deferred) M1 path: stage a fixed (point,value) each step.
    let mut eng = Engine::in_memory();
    eng.build_model_in_memory(free_add_model()).unwrap();
    let spec = SimSpec {
        t_start: 0.0,
        t_stop: 1.0,
        step: 1.0,
        inputs: InputSource::Constant(vec![
            ("conn#0".to_string(), Value::Real(2.0)),
            ("conn#1".to_string(), Value::Real(5.0)),
        ]),
        collect: CollectSpec::Named {
            points: vec!["conn#2".to_string()],
            stride: 1,
        },
    };
    let m = eng.simulate(&spec).unwrap();
    assert_eq!(m.ticks, 2);
    let col = m.trace.column(0).unwrap();
    assert!(
        col.iter().all(|v| v.bit_eq(&Value::Real(7.0))),
        "the staged Constant inputs sum to 7.0 every tick: {col:?}"
    );
}

#[test]
fn constant_input_source_propagates_type_error() {
    // A wrong-typed Constant pair surfaces as OcError::InputType through stage_inputs -> set_input.
    let mut eng = Engine::in_memory();
    eng.build_model_in_memory(free_add_model()).unwrap();
    let spec = SimSpec {
        t_start: 0.0,
        t_stop: 1.0,
        step: 1.0,
        inputs: InputSource::Constant(vec![("conn#0".to_string(), Value::Boolean(true))]),
        collect: CollectSpec::None,
    };
    assert!(matches!(eng.simulate(&spec), Err(OcError::InputType(_))));
}

// ---- real-time / batch step ----

#[test]
fn step_realtime_advances_and_reports() {
    let mut eng = loaded_accumulator();
    let r0 = eng.step_realtime(0.0).unwrap();
    assert!(r0.asserts.is_empty());
    assert_eq!(r0.written, 0, "MemStore commits no points in M1");
    eng.step_realtime(1.0).unwrap();
    // A backwards step is a typed time regression (delegated tick guard).
    assert!(matches!(
        eng.step_realtime(0.5),
        Err(OcError::TimeRegression { .. })
    ));
}

// ---- typed IO inventory ----

#[test]
fn io_inventory_is_built_from_connectors() {
    let eng = loaded_accumulator();
    let s = eng.io_summary();
    // 12 connectors: 6 Real inputs (AI), 5 Real outputs (AO), 1 Bool output (DO).
    assert_eq!(s.total, 12);
    assert_eq!(s.analog_inputs, 6);
    assert_eq!(s.analog_outputs, 5);
    assert_eq!(s.digital_outputs, 1);
    assert_eq!(s.digital_inputs, 0);
    assert_eq!(s.network, 0);
    for p in eng.io().iter() {
        assert_eq!(p.physical, PhysicalKind::SoftwarePoint, "M1 default");
        assert!(p.in_pointlist);
        assert!(!p.hardwired);
        assert!(p.trend.is_none());
        assert_ne!(p.io_class, IoClass::Network, "M1 never classifies Network");
    }
    assert_eq!(eng.io().to_vec().len(), 12);
}

#[test]
fn outputs_to_map_zips_paths_to_values() {
    let mut eng = loaded_accumulator();
    eng.tick(0.0).unwrap();
    let map = eng.outputs().to_map();
    assert_eq!(map.len(), eng.outputs().len(), "every output is keyed 1:1");
    assert!(map.iter().all(|(p, _)| !p.is_empty()), "no empty path keys");
}
