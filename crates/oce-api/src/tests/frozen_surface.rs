//! Frozen facade behavior tests built on the hand-built accumulator model.
//!
//! - the `Engine` loads a hand-built `ModelGraph` and ticks it;
//! - a multi-tick run advances the canonical `Add`/`UnitDelay` feedback accumulator with a
//!   true **one-tick** delay (1, 2, 3, 4 — not the two-tick delay an inline emit-then-update gives);
//! - an injected feedthrough cycle is rejected with a typed [`BuildError::AlgebraicLoop`];
//! - determinism: two independent compiles of the same model produce **byte-identical**
//!   `order`/`connector_order`/`driver_of`;
//! - a `MemStore` model round-trip + no-op `commit`/`flush`/`recover` through the engine.

use super::common::*;
use crate::TopologyBlock;

#[test]
fn topology_block_equality_compares_real_parameters_by_bits() {
    let block = |value| TopologyBlock {
        instance_path: "block".to_string(),
        class_iri: "CDL.Reals.Sources.Constant".to_string(),
        inputs: Vec::new(),
        outputs: vec!["block.y".to_string()],
        params: vec![
            (
                "nan".to_string(),
                Value::Real(f64::from_bits(0x7ff8_0000_0000_0001)),
            ),
            ("zero".to_string(), Value::Real(value)),
        ],
    };
    assert_eq!(block(0.0), block(0.0), "equal NaN bits compare equal");
    assert_ne!(
        block(0.0),
        block(-0.0),
        "positive and negative zero compare unequal"
    );
}

/// Load the canonical accumulator model into a fresh in-memory engine. Connector paths are
/// `conn#<id>` (hand-built, no `iri`); param paths are `b<id>.<name>` (no `instance_iri`).
fn loaded_accumulator() -> Engine<MemStore> {
    let (m, _, _, _) = build_accumulator_model();
    let mut eng = Engine::in_memory();
    eng.build_model_in_memory(m, None)
        .expect("BUILD must succeed");
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

fn sample_trigger_model(period: f64) -> ModelGraph {
    let mut mb = Mb::new();
    mb.block(
        "CDL.Logical.Sources.SampleTrigger",
        &[],
        &[ValueType::Boolean],
        vec![rp("period", period), rp("shift", 0.0)],
    );
    mb.finish()
}

fn ramp_model() -> ModelGraph {
    let mut mb = Mb::new();
    let (_, inputs, _) = mb.block(
        "CDL.Reals.Ramp",
        &[ValueType::Real, ValueType::Boolean],
        &[ValueType::Real],
        vec![
            rp("raisingSlewRate", 2.0),
            rp("fallingSlewRate", -3.0),
            rp("Td", 0.1),
        ],
    );
    let mut model = mb.finish();
    model.external_inputs = inputs;
    model
}

fn stage_model(n: i64, hold_duration: f64, h: f64) -> ModelGraph {
    let mut mb = Mb::new();
    let (_, inputs, _) = mb.block(
        "CDL.Integers.Stage",
        &[ValueType::Real],
        &[ValueType::Integer],
        vec![
            (Arc::from("n"), Value::Integer(n)),
            rp("holdDuration", hold_duration),
            rp("h", h),
        ],
    );
    let mut model = mb.finish();
    model.external_inputs = inputs;
    model
}

fn stage_model_default_h(n: i64, hold_duration: f64) -> ModelGraph {
    let mut mb = Mb::new();
    let (_, inputs, _) = mb.block(
        "CDL.Integers.Stage",
        &[ValueType::Real],
        &[ValueType::Integer],
        vec![
            (Arc::from("n"), Value::Integer(n)),
            rp("holdDuration", hold_duration),
        ],
    );
    let mut model = mb.finish();
    model.external_inputs = inputs;
    model
}

// Frozen API shape guards live in `crate::guards`, where drift fails normal `cargo build`.
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
    assert!(eng.export_cxf().is_err());
    assert!(eng.topology().blocks.is_empty());
}

#[test]
fn set_input_resolves_validates_and_rejects() {
    let mut eng = Engine::in_memory();
    eng.build_model_in_memory(free_add_model(), None).unwrap();
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
    eng.build_model_in_memory(free_add_model(), None).unwrap();
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
    // 5 params: b0.k=1.0, b2.samplePeriod=1.0, b3.k=2.5, b5.uMin=0.0, b5.uMax=3.0.
    assert_eq!(eng.params().len(), 5);
    assert!(eng.get_param("b0.k").unwrap().bit_eq(&Value::Real(1.0)));
    // The R-PUB-6 owned enumeration yields (path, value, declared attrs). Unconstrained params
    // remain bounds-free.
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
        "unconstrained ParamAttrs carry no bounds/units"
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

#[test]
fn positive_param_rules_surface_attrs_and_reject_zero_at_rest() {
    let mut eng = Engine::in_memory();
    eng.build_model_in_memory(sample_trigger_model(1.0), None)
        .expect("valid SampleTrigger loads");
    let (_, period_value, period_attrs) = eng
        .params()
        .to_vec()
        .into_iter()
        .find(|(p, _, _)| p == "b0.period")
        .expect("period param must be present");
    assert!(period_value.bit_eq(&Value::Real(1.0)));
    assert_eq!(period_attrs.value_type, ValueType::Real);
    assert_eq!(period_attrs.min, Some(0.0));
    assert_eq!(period_attrs.max, None);

    eng.halt().unwrap();
    assert!(matches!(
        eng.set_param("b0.period", Value::Real(-1.0)),
        Err(OcError::ParamRange { .. })
    ));
    assert!(matches!(
        eng.set_param("b0.period", Value::Real(0.0)),
        Err(OcError::ParamRange { .. })
    ));
    eng.set_param("b0.period", Value::Real(2.0)).unwrap();
}

#[test]
fn limiter_cross_param_rules_reject_inverted_at_rest() {
    let mut eng = loaded_accumulator();
    eng.halt().unwrap();
    assert!(matches!(
        eng.set_param("b5.uMin", Value::Real(4.0)),
        Err(OcError::ParamRange { .. })
    ));
    assert!(matches!(
        eng.set_param("b5.uMax", Value::Real(-1.0)),
        Err(OcError::ParamRange { .. })
    ));
    eng.set_param("b5.uMax", Value::Real(0.0))
        .expect("equal Limiter bounds are permitted at rest");
    assert!(matches!(
        eng.set_param("b5.uMin", Value::Real(1.0)),
        Err(OcError::ParamRange { .. })
    ));
}

#[test]
fn ramp_param_rules_surface_attrs_and_reject_invalid_edits_at_rest() {
    let mut eng = Engine::in_memory();
    eng.build_model_in_memory(ramp_model(), None)
        .expect("valid Ramp loads");
    let rows = eng.params().to_vec();

    let (_, raising_value, raising_attrs) = rows
        .iter()
        .find(|(p, _, _)| p == "b0.raisingSlewRate")
        .expect("raisingSlewRate param must be present");
    assert!(raising_value.bit_eq(&Value::Real(2.0)));
    assert_eq!(raising_attrs.min, Some(1e-37));
    assert_eq!(raising_attrs.max, None);

    let (_, falling_value, falling_attrs) = rows
        .iter()
        .find(|(p, _, _)| p == "b0.fallingSlewRate")
        .expect("fallingSlewRate param must be present");
    assert!(falling_value.bit_eq(&Value::Real(-3.0)));
    assert_eq!(falling_attrs.min, None);
    assert_eq!(falling_attrs.max, Some(-1e-37));

    let (_, td_value, td_attrs) = rows
        .iter()
        .find(|(p, _, _)| p == "b0.Td")
        .expect("Td param must be present");
    assert!(td_value.bit_eq(&Value::Real(0.1)));
    assert_eq!(td_attrs.min, Some(1e-15));
    assert_eq!(td_attrs.max, None);

    eng.halt().unwrap();
    assert!(matches!(
        eng.set_param("b0.raisingSlewRate", Value::Real(0.0)),
        Err(OcError::ParamRange { .. })
    ));
    assert!(matches!(
        eng.set_param("b0.fallingSlewRate", Value::Real(-1e-38)),
        Err(OcError::ParamRange { .. })
    ));
    assert!(matches!(
        eng.set_param("b0.Td", Value::Real(0.0)),
        Err(OcError::ParamRange { .. })
    ));
    eng.set_param("b0.raisingSlewRate", Value::Real(1e-37))
        .expect("raising Ramp boundary is valid");
    eng.set_param("b0.fallingSlewRate", Value::Real(-1e-37))
        .expect("falling Ramp boundary is valid");
    eng.set_param("b0.Td", Value::Real(1e-15))
        .expect("Td Ramp boundary is valid");
}

#[test]
fn stage_dependent_param_rules_reject_invalid_edits_at_rest() {
    let mut eng = Engine::in_memory();
    eng.build_model_in_memory(stage_model(4, 0.0, 0.02 / 4.0), None)
        .expect("valid Stage loads");
    let rows = eng.params().to_vec();
    let (_, n_value, n_attrs) = rows
        .iter()
        .find(|(p, _, _)| p == "b0.n")
        .expect("n param must be present");
    assert!(n_value.bit_eq(&Value::Integer(4)));
    assert_eq!(n_attrs.min, Some(1.0));

    let (_, hold_value, hold_attrs) = rows
        .iter()
        .find(|(p, _, _)| p == "b0.holdDuration")
        .expect("holdDuration param must be present");
    assert!(hold_value.bit_eq(&Value::Real(0.0)));
    assert_eq!(hold_attrs.min, Some(0.0));

    let (_, h_value, h_attrs) = rows
        .iter()
        .find(|(p, _, _)| p == "b0.h")
        .expect("h param must be present");
    assert!(h_value.bit_eq(&Value::Real(0.02 / 4.0)));
    assert!(
        h_attrs.min.is_none() && h_attrs.max.is_none(),
        "dependent h bounds are enforced dynamically, not exposed as static attrs"
    );

    eng.halt().unwrap();
    assert!(matches!(
        eng.set_param("b0.n", Value::Integer(0)),
        Err(OcError::ParamRange { .. })
    ));
    assert!(matches!(
        eng.set_param("b0.holdDuration", Value::Real(-1.0)),
        Err(OcError::ParamRange { .. })
    ));
    assert!(matches!(
        eng.set_param("b0.h", Value::Real(0.0002)),
        Err(OcError::ParamRange { .. })
    ));
    assert!(matches!(
        eng.set_param("b0.h", Value::Real(0.126)),
        Err(OcError::ParamRange { .. })
    ));
    eng.set_param("b0.h", Value::Real(0.001 / 4.0))
        .expect("lower h boundary is valid");
    eng.set_param("b0.h", Value::Real(0.5 / 4.0))
        .expect("upper h boundary is valid");

    let mut default_h = Engine::in_memory();
    default_h
        .build_model_in_memory(stage_model_default_h(4, 0.0), None)
        .expect("valid Stage with default h loads");
    default_h.halt().unwrap();
    default_h
        .set_param("b0.n", Value::Integer(8))
        .expect("editing n is valid when h is omitted and defaults from n");
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
    eng.build_model_in_memory(free_add_model(), None).unwrap();
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
    eng.build_model_in_memory(free_add_model(), None).unwrap();
    eng.set_input("conn#0", Value::Real(3.0)).unwrap();
    eng.set_input("conn#1", Value::Real(4.0)).unwrap();
    eng.tick(0.0).unwrap();
    assert!(eng.get_output("conn#2").unwrap().bit_eq(&Value::Real(7.0)));
}

#[test]
fn a_valid_horizon_resets_the_prior_run_clock() {
    let mut eng = loaded_accumulator();
    eng.tick(100.0).unwrap(); // prev_t = 100
    // Preflight succeeds before the [0,3] horizon resets `prev_t`, so t=0 does not regress.
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
    eng.build_model_in_memory(free_add_model(), None).unwrap();
    eng.set_input("conn#0", Value::Real(3.0)).unwrap();
    eng.set_input("conn#1", Value::Real(4.0)).unwrap();
    eng.tick(0.0).unwrap();
    assert!(eng.get_output("conn#2").unwrap().bit_eq(&Value::Real(7.0)));
}

#[test]
fn simulate_constant_input_source_flows_through() {
    // InputSource::Constant is a live path: stage a fixed (point,value) each step.
    let mut eng = Engine::in_memory();
    eng.build_model_in_memory(free_add_model(), None).unwrap();
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
    let mut eng = Engine::in_memory();
    eng.build_model_in_memory(free_add_model(), None).unwrap();
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
    // Non-zero host epoch exercises the public timestamp mapping surface.
    eng.set_realtime_epoch_unix_nanos(1_000_000_000);
    let r0 = eng.step_realtime(0.0).unwrap();
    assert!(r0.asserts.is_empty());
    assert_eq!(r0.written, 6, "all projected outputs are committed");
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
        assert_eq!(p.physical, PhysicalKind::SoftwarePoint, "current default");
        assert!(p.in_pointlist);
        assert!(!p.hardwired);
        assert!(p.trend.is_none());
        assert_ne!(
            p.io_class,
            IoClass::Network,
            "Network classification is semantic"
        );
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
