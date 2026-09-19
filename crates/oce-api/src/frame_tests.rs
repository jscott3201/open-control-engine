//! Private-plan and incarnation controls, without adding a public validator or commit API.
//! The plan golden is hand-derived from authored input names and IEEE-754 -0/2 bit patterns.
//! Fan-out expectations come from the source fixture's two distinct consuming blocks.

use std::fmt::Write as _;

use allocation_counter::measure;
use oce_model::{
    Attrs, BlockId, BlockInstance, Connector, Dir, EnumClassId, IntAttrs, ModelGraph, ParamTable,
    ValueType,
};

use super::*;

const ADD: &[u8] = include_bytes!("../tests/fixtures/legacy_frame_add.jsonld");
const DELAY: &[u8] = include_bytes!("../tests/fixtures/frame_delay.jsonld");
const A: &str = "urn:legacy-frame:a";
const B: &str = "urn:legacy-frame:b";

fn loaded(bytes: &[u8]) -> Engine {
    let mut engine = Engine::in_memory();
    engine.load_cxf(bytes).unwrap();
    engine
}

fn entries() -> [(&'static str, Value); 2] {
    [(B, Value::Real(2.0)), (A, Value::Real(-0.0))]
}

fn plan_text(engine: &Engine, frame: &PreparedInputFrame) -> String {
    let mut result = format!("time:{:016x}\n", frame.time.to_bits());
    for (targets, value) in &frame.inputs {
        let path = engine.model.connectors[targets[0].0 as usize]
            .iri
            .as_deref()
            .unwrap();
        let Value::Real(real) = value else {
            panic!("Real fixture");
        };
        writeln!(
            result,
            "{path}:{:016x}:targets={}",
            real.to_bits(),
            targets.len()
        )
        .unwrap();
    }
    result
}

#[test]
fn canonical_plan_owns_values_and_repeats_bit_exactly_under_entry_permutations() {
    let engine = loaded(ADD);
    let before = engine.state_snapshot().unwrap();
    let mut values = entries();
    let plan = engine.prepare_frame(-0.0, &values).unwrap();
    values[0].1 = Value::Real(999.0);
    assert_eq!(
        plan_text(&engine, &plan).as_bytes(),
        include_bytes!("../tests/fixtures/frame_plan.txt")
    );
    for _ in 0..4 {
        let mut values = entries();
        values.reverse();
        let repeat = engine.prepare_frame(-0.0, &values).unwrap();
        assert_eq!(plan_text(&engine, &repeat), plan_text(&engine, &plan));
    }
    assert_eq!(
        engine.state_snapshot().unwrap().as_bytes(),
        before.as_bytes()
    );
    // Permanent mutation control: a changed bit is observable to the comparison, not hidden by ==.
    let changed = engine.prepare_frame(0.0, &entries()).unwrap();
    assert_ne!(plan_text(&engine, &changed), plan_text(&engine, &plan));
}

#[test]
fn one_boundary_value_retains_every_fanout_target_without_internal_inputs() {
    let engine = loaded(include_bytes!(
        "../../oce-cxf/tests/fixtures/boundary_fanout.jsonld"
    ));
    let definitions = engine.input_definitions().unwrap();
    assert_eq!(definitions.len(), 1);
    let path = "http://example.org#g36.profile.boundary_fanout.u";
    assert_eq!(definitions[0].path, path);
    let plan = engine
        .prepare_frame(0.0, &[(path, Value::Real(4.0))])
        .unwrap();
    assert_eq!(plan.inputs.len(), 1);
    assert_eq!(plan.inputs[0].0.len(), 2, "dropping the tail must fail");
    let targets = &plan.inputs[0].0;
    assert_ne!(targets[0], targets[1]);
    assert_ne!(
        engine.model.connectors[targets[0].0 as usize].block,
        engine.model.connectors[targets[1].0 as usize].block
    );
    assert!(plan.inputs[0].1.bit_eq(&Value::Real(4.0)));
    assert_eq!(targets, &engine.model.external_inputs);
    assert!(engine.io().len() > definitions.len());
}

#[test]
fn reload_and_cross_engine_identity_refuse_even_identical_model_bytes() {
    let mut engine = loaded(ADD);
    let plan = engine.prepare_frame(0.0, &entries()).unwrap();
    let other = loaded(ADD);
    assert!(matches!(
        other.check_prepared_frame(&plan),
        Err(OcError::StalePreparedFrame)
    ));
    assert!(matches!(engine.load_cxf(b"{"), Err(OcError::Cxf(_))));
    engine.check_prepared_frame(&plan).unwrap();
    for bytes in [ADD, DELAY] {
        engine.load_cxf(bytes).unwrap();
        let before = engine.state_snapshot().unwrap();
        assert!(matches!(
            engine.check_prepared_frame(&plan),
            Err(OcError::StalePreparedFrame)
        ));
        assert_eq!(
            engine.state_snapshot().unwrap().as_bytes(),
            before.as_bytes()
        );
    }
}

#[test]
fn dirty_resume_invalidates_but_clean_resume_and_compatible_restore_retain_context() {
    let mut engine = loaded(DELAY);
    let values = [
        ("urn:frame:a", Value::Real(0.0)),
        ("urn:frame:b", Value::Real(2.0)),
    ];
    let plan = engine.prepare_frame(4.0, &values).unwrap();
    let snapshot = engine.state_snapshot().unwrap();
    engine.restore_state(&snapshot).unwrap();
    engine.halt().unwrap();
    engine.resume().unwrap();
    engine.check_prepared_frame(&plan).unwrap();
    let checkpoint = engine.checkpoint().unwrap();
    engine.tick(5.0).unwrap();
    let before = engine.state_snapshot().unwrap();
    assert!(matches!(
        engine.check_prepared_frame(&plan),
        Err(OcError::TimeRegression { .. })
    ));
    assert_eq!(
        engine.state_snapshot().unwrap().as_bytes(),
        before.as_bytes()
    );
    engine.restore_checkpoint(&checkpoint).unwrap();
    engine.check_prepared_frame(&plan).unwrap();
    engine.halt().unwrap();
    engine
        .set_param("urn:frame:delay.samplePeriod", Value::Real(1.0))
        .unwrap();
    assert!(matches!(
        engine.check_prepared_frame(&plan),
        Err(OcError::State(EngineStateError::PendingParameterEdits))
    ));
    // Even a same-value successful edit triggers the dirty rebuild and a new incarnation.
    engine.resume().unwrap();
    let before = engine.state_snapshot().unwrap();
    assert!(matches!(
        engine.check_prepared_frame(&plan),
        Err(OcError::StalePreparedFrame)
    ));
    assert_eq!(
        engine.state_snapshot().unwrap().as_bytes(),
        before.as_bytes()
    );
    engine.prepare_frame(0.0, &values).unwrap();
}

#[test]
fn successful_empty_executable_is_not_an_unloaded_engine() {
    let mut engine = Engine::in_memory();
    engine
        .build_model_in_memory(ModelGraph::new(), None)
        .unwrap();
    assert!(engine.input_definitions().unwrap().is_empty());
    let before = engine.state_snapshot().unwrap();
    let plan = engine.prepare_frame(-1.0, &[]).unwrap();
    assert!(plan.inputs.is_empty());
    assert!(matches!(
        engine.prepare_frame(0.0, &[("extra", Value::Real(1.0))]),
        Err(OcError::FrameUnknownInput { .. })
    ));
    assert!(matches!(
        engine.prepare_frame(f64::NAN, &[]),
        Err(OcError::NonFiniteTime { .. })
    ));
    assert_eq!(
        engine.state_snapshot().unwrap().as_bytes(),
        before.as_bytes()
    );
}

// No executable registry block has Enum/String signal ports. Detached probes are accepted by the
// private model gate and cover total schema handling without claiming public CXF signal support.
fn domain_engine(value_type: ValueType, attrs: Attrs) -> Engine {
    let mut model = ModelGraph::new();
    model.blocks.push(BlockInstance {
        id: BlockId(0),
        class_iri: Arc::from("CDL.Reals.Sources.Constant"),
        inputs: vec![],
        outputs: vec![ConnectorId(0)],
        params: ParamTable {
            values: vec![(Arc::from("k"), Value::Real(0.0))],
        },
        decl_order: 0,
        instance_iri: Some(Arc::from("urn:domain:constant")),
    });
    model.connectors.push(
        Connector::new(ConnectorId(0), BlockId(0), Dir::Out, ValueType::Real, 0)
            .with_iri("urn:domain:y"),
    );
    model.connectors.push(
        Connector::new(ConnectorId(1), BlockId(0), Dir::In, value_type, 1)
            .with_iri("urn:domain:u")
            .with_attrs(attrs)
            .unwrap(),
    );
    model.external_inputs.push(ConnectorId(1));
    if value_type == ValueType::Integer {
        model.blocks[0].class_iri = Arc::from("CDL.Integers.Abs");
        model.blocks[0].inputs.push(ConnectorId(1));
        model.blocks[0].params.values.clear();
        model.connectors[0].value_type = ValueType::Integer;
        model.connectors[0].attrs = Attrs::default_for(ValueType::Integer);
    }
    let mut engine = Engine::in_memory();
    engine
        .build_model_in_memory(model, Some("urn:domain:model"))
        .unwrap();
    engine
}

#[test]
fn integer_domains_are_exact_at_default_limits_and_above_binary64_precision() {
    for (min, max, attrs) in [
        (
            i64::from(i32::MIN),
            i64::from(i32::MAX),
            IntAttrs::default(),
        ),
        (
            9_007_199_254_740_993,
            9_007_199_254_740_995,
            IntAttrs {
                min: Some(9_007_199_254_740_993),
                max: Some(9_007_199_254_740_995),
            },
        ),
    ] {
        let engine = domain_engine(ValueType::Integer, Attrs::Integer(attrs));
        let before = engine.state_snapshot().unwrap();
        let defs = engine.input_definitions().unwrap();
        assert!(defs[0].min.as_ref().unwrap().bit_eq(&Value::Integer(min)));
        assert!(defs[0].max.as_ref().unwrap().bit_eq(&Value::Integer(max)));
        for value in [min, max] {
            engine
                .prepare_frame(0.0, &[("urn:domain:u", Value::Integer(value))])
                .unwrap();
        }
        for value in [min - 1, max + 1, i64::MIN, i64::MAX] {
            assert!(
                matches!(engine.prepare_frame(0.0, &[("urn:domain:u", Value::Integer(value))]), Err(OcError::InputDomain(p)) if p == "urn:domain:u")
            );
            assert_eq!(
                engine.state_snapshot().unwrap().as_bytes(),
                before.as_bytes()
            );
        }
        assert!(matches!(
            engine.prepare_frame(0.0, &[("urn:domain:u", Value::Real(min as f64))]),
            Err(OcError::InputType(_))
        ));
    }
}

#[test]
fn enum_class_and_ordinal_are_not_integer_carriers_and_strings_remain_owned() {
    let class = EnumClassId::SIMPLE_CONTROLLER;
    let engine = domain_engine(
        ValueType::Enum(class),
        Attrs::default_for(ValueType::Enum(class)),
    );
    // Detached probes deliberately have no state-manifest port identity. Compare the actual
    // run image here; public executable schemas use snapshot comparisons in the integration suite.
    let before = format!(
        "{:?} {:?} {:?}",
        engine.state, engine.outputs, engine.prev_t
    );
    for ordinal in 1..=4 {
        engine
            .prepare_frame(0.0, &[("urn:domain:u", Value::Enum { class, ordinal })])
            .unwrap();
    }
    for ordinal in [0, 5, u32::MAX] {
        assert!(matches!(
            engine.prepare_frame(0.0, &[("urn:domain:u", Value::Enum { class, ordinal })]),
            Err(OcError::InputDomain(_))
        ));
    }
    for value in [
        Value::Integer(1),
        Value::Enum {
            class: EnumClassId::SMOOTHNESS,
            ordinal: 1,
        },
    ] {
        assert!(matches!(
            engine.prepare_frame(0.0, &[("urn:domain:u", value)]),
            Err(OcError::InputType(_))
        ));
    }
    assert_eq!(
        format!(
            "{:?} {:?} {:?}",
            engine.state, engine.outputs, engine.prev_t
        ),
        before
    );
    let mut engine = domain_engine(ValueType::String, Attrs::default_for(ValueType::String));
    let plan = engine
        .prepare_frame(0.0, &[("urn:domain:u", Value::String(Arc::from("owned")))])
        .unwrap();
    assert!(plan.inputs[0].1.bit_eq(&Value::String(Arc::from("owned"))));
    assert!(
        matches!(
            engine.set_input("urn:domain:u", Value::String(Arc::from("legacy"))),
            Err(OcError::UnknownPoint(_))
        ),
        "legacy point inventory still excludes strings"
    );
}

#[test]
fn repeated_large_fixture_preparation_has_a_linear_capacity_and_allocation_census() {
    let engine = loaded(include_bytes!(
        "../../oce-cxf/tests/fixtures/g36/cooling_only_controller.jsonld"
    ));
    assert!(engine.model.blocks.len() > 100);
    let definitions = engine.input_definitions().unwrap();
    let values: Vec<_> = definitions
        .iter()
        .map(|d| {
            (
                d.path.as_str(),
                d.min.clone().unwrap_or_else(|| d.value_type.zero_value()),
            )
        })
        .collect();
    assert!(values.len() > 10);
    let plan = engine.prepare_frame(0.0, &values).unwrap();
    assert_eq!(plan.inputs.capacity(), definitions.len());
    let targets: usize = plan
        .inputs
        .iter()
        .map(|(ids, _)| {
            assert_eq!(ids.len(), ids.capacity());
            ids.len()
        })
        .sum();
    assert_eq!(targets, engine.model.external_inputs.len());
    let repetitions = 128;
    let census = measure(|| {
        for _ in 0..repetitions {
            std::hint::black_box(engine.prepare_frame(0.0, &values).unwrap());
        }
    });
    assert_eq!(
        census.count_total,
        (definitions.len() as u64 + 2) * repetitions
    );
    let bytes = definitions.len()
        * (std::mem::size_of::<Option<&Value>>()
            + std::mem::size_of::<(Vec<ConnectorId>, Value)>())
        + targets * std::mem::size_of::<ConnectorId>();
    assert_eq!(census.bytes_total, bytes as u64 * repetitions);
    assert_eq!(census.bytes_current, 0);
    assert_eq!(census.count_current, 0);
    assert!(
        measure(|| {
            std::hint::black_box(vec![0_u8; 1024]);
        })
        .count_total
            > 0,
        "counter positive control"
    );
}
