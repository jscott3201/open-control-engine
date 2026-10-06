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

/// A single `Add` block with **undriven** inputs (conn#0, conn#1 → conn#2). No connections, so the
/// inputs are external — the model needed to observe complete-frame input
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
fn complete_observations_resolve_without_coercion_or_internal_input_access() {
    let mut eng = Engine::in_memory();
    eng.build_model_in_memory(free_add_model(), None).unwrap();
    // conn#0 = Add.u0 (Real input). A wrong-typed value is a typed error — no coercion.
    assert!(matches!(
        eng.prepare_frame(
            0.0,
            &[
                ("conn#0", Value::Boolean(true)),
                ("conn#1", Value::Real(2.0))
            ]
        ),
        Err(OcError::InputType(_))
    ));
    advance(
        &mut eng,
        0.0,
        &[("conn#0", Value::Real(5.0)), ("conn#1", Value::Real(2.0))],
    )
    .unwrap();
    // Prove the staged values reached the resolved slots through the block output, without reading an
    // input through `get_output`.
    assert!(
        eng.get_output("conn#2").unwrap().bit_eq(&Value::Real(7.0)),
        "staged input values must propagate to the Add output"
    );
    assert!(matches!(
        eng.prepare_frame(0.0, &[("nope", Value::Real(1.0))]),
        Err(OcError::FrameUnknownInput { .. })
    ));
    // conn#2 is an output, never an input determinant.
    assert!(matches!(
        eng.prepare_frame(0.0, &[("conn#2", Value::Real(1.0))]),
        Err(OcError::FrameNotInput { .. })
    ));
}

#[test]
fn get_output_on_input_point_is_unknown_point() {
    let mut eng = Engine::in_memory();
    eng.build_model_in_memory(free_add_model(), None).unwrap();
    advance(
        &mut eng,
        0.0,
        &[("conn#0", Value::Real(5.0)), ("conn#1", Value::Real(2.0))],
    )
    .unwrap();
    assert!(matches!(
        eng.get_output("conn#0"),
        Err(OcError::UnknownPoint(_))
    ));
}

#[test]
fn device_filtered_inventory_preserves_the_load_typed_refusal() {
    let eng = Engine::in_memory();
    // Device filtering is outside the supported profile; the in-memory mirror still works.
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
    advance(&mut eng, 0.0, &[]).unwrap();
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

#[test]
fn autonomous_feedback_and_limited_inspection_match_the_hand_recurrence() {
    for _ in 0..3 {
        let mut engine = loaded_accumulator();
        for (time, sum, limited) in [
            (0.0, 1.0, 1.0),
            (1.0, 2.0, 2.0),
            (2.0, 3.0, 3.0),
            (3.0, 4.0, 3.0),
        ] {
            let frame = advance(&mut engine, time, &[]).unwrap();
            assert_eq!(frame.time().to_bits(), time.to_bits());
            assert!(
                engine
                    .get_output("conn#3")
                    .unwrap()
                    .bit_eq(&Value::Real(sum))
            );
            assert!(
                engine
                    .get_output("conn#11")
                    .unwrap()
                    .bit_eq(&Value::Real(limited))
            );
        }
        assert!(matches!(
            engine.prepare_frame(0.0, &[]),
            Err(OcError::TimeRegression { .. })
        ));
    }
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
fn selected_inspection_echoes_output_paths_and_values() {
    let mut eng = loaded_accumulator();
    advance(&mut eng, 0.0, &[]).unwrap();
    let paths: Vec<_> = eng
        .io()
        .iter()
        .filter(|p| p.direction == PointDirection::Out)
        .map(|p| p.path)
        .collect();
    let names: Vec<_> = paths.iter().map(String::as_str).collect();
    let map = eng.watch(&names).unwrap();
    assert_eq!(map.len(), paths.len(), "every selected output is keyed 1:1");
    assert!(map.iter().all(|(p, _)| !p.is_empty()), "no empty path keys");
}
