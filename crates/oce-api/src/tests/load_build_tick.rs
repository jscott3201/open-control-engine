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

use super::common::*;

#[test]
fn hand_built_graph_builds_advances_and_is_byte_identical() {
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
fn injected_algebraic_loop_is_rejected() {
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
fn unknown_block_class_is_a_typed_load_error() {
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
fn tick_time_must_be_monotonic() {
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
fn non_finite_tick_time_is_rejected() {
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
fn engine_store_round_trips_and_durability_is_noop() {
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
