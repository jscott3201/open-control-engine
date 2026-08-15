//! HostTick v1 contract tests for `CDL.Logical.Pre`.

use std::sync::Arc;

use oce_model::{
    BlockId, BlockInstance, Connection, Connector, ConnectorId, Dir, ModelGraph, ParamTable, Value,
    ValueType,
};

use crate::{Engine, EngineStateSnapshot};

const INPUT_PATH: &str = "urn:test:pre-profile:u";
const OUTPUT_PATH: &str = "urn:test:pre-profile:y";
const NOT_OUTPUT_PATH: &str = "urn:test:pre-profile:not-y";

fn pre_model(pre_u_start: bool) -> (ModelGraph, ConnectorId) {
    let mut model = ModelGraph::new();
    let input = ConnectorId(0);
    let output = ConnectorId(1);
    model.connectors.push(
        Connector::new(input, BlockId(0), Dir::In, ValueType::Boolean, 0).with_iri(INPUT_PATH),
    );
    model.connectors.push(
        Connector::new(output, BlockId(0), Dir::Out, ValueType::Boolean, 0).with_iri(OUTPUT_PATH),
    );
    model.blocks.push(BlockInstance {
        id: BlockId(0),
        class_iri: Arc::from("CDL.Logical.Pre"),
        inputs: vec![input],
        outputs: vec![output],
        params: ParamTable {
            values: vec![(Arc::from("pre_u_start"), Value::Boolean(pre_u_start))],
        },
        decl_order: 0,
        instance_iri: Some(Arc::from("urn:test:pre-profile:pre")),
    });
    model.external_inputs.push(input);
    (model, output)
}

fn oscillating_feedback_model() -> (ModelGraph, ConnectorId, ConnectorId) {
    let mut model = ModelGraph::new();
    let pre_input = ConnectorId(0);
    let pre_output = ConnectorId(1);
    let not_input = ConnectorId(2);
    let not_output = ConnectorId(3);
    model.connectors.extend([
        Connector::new(pre_input, BlockId(0), Dir::In, ValueType::Boolean, 0).with_iri(INPUT_PATH),
        Connector::new(pre_output, BlockId(0), Dir::Out, ValueType::Boolean, 0)
            .with_iri(OUTPUT_PATH),
        Connector::new(not_input, BlockId(1), Dir::In, ValueType::Boolean, 0)
            .with_iri("urn:test:pre-profile:not-u"),
        Connector::new(not_output, BlockId(1), Dir::Out, ValueType::Boolean, 0)
            .with_iri(NOT_OUTPUT_PATH),
    ]);
    model.blocks.extend([
        BlockInstance {
            id: BlockId(0),
            class_iri: Arc::from("CDL.Logical.Pre"),
            inputs: vec![pre_input],
            outputs: vec![pre_output],
            params: ParamTable {
                values: vec![(Arc::from("pre_u_start"), Value::Boolean(false))],
            },
            decl_order: 0,
            instance_iri: Some(Arc::from("urn:test:pre-profile:pre")),
        },
        BlockInstance {
            id: BlockId(1),
            class_iri: Arc::from("CDL.Logical.Not"),
            inputs: vec![not_input],
            outputs: vec![not_output],
            params: ParamTable::default(),
            decl_order: 1,
            instance_iri: Some(Arc::from("urn:test:pre-profile:not")),
        },
    ]);
    model.connections.extend([
        Connection {
            from: pre_output,
            to: not_input,
        },
        Connection {
            from: not_output,
            to: pre_input,
        },
    ]);
    (model, pre_output, not_output)
}

fn assert_visible_output(engine: &Engine, output: ConnectorId, expected: bool) {
    let expected = Value::Boolean(expected);
    assert!(engine.outputs().get(output).unwrap().bit_eq(&expected));
    assert!(engine.get_output(OUTPUT_PATH).unwrap().bit_eq(&expected));
    let watched = engine.watch(&[OUTPUT_PATH]).unwrap();
    assert_eq!(watched.len(), 1);
    assert!(watched[0].1.bit_eq(&expected));
}

#[test]
fn parameter_seed_is_first_call_output_and_equal_time_calls_advance_memory() {
    let (model, output) = pre_model(false);
    let mut engine = Engine::in_memory();
    engine.build_model_in_memory(model, None).unwrap();

    // Allocation seeds connector values independently of block state. The parameter is not visible
    // until the block's first HostTick call.
    assert_visible_output(&engine, output, false);
    engine.set_input(INPUT_PATH, Value::Boolean(true)).unwrap();
    let first = engine.tick(4.0).unwrap().get(output).unwrap().clone();
    assert!(first.bit_eq(&Value::Boolean(false)));
    assert_visible_output(&engine, output, false);

    engine.set_input(INPUT_PATH, Value::Boolean(false)).unwrap();
    let second = engine.tick(4.0).unwrap().get(output).unwrap().clone();
    assert!(second.bit_eq(&Value::Boolean(true)));
    assert_visible_output(&engine, output, true);

    engine.tick(4.0).unwrap();
    assert_visible_output(&engine, output, false);

    let (seeded_model, seeded_output) = pre_model(true);
    let mut seeded = Engine::in_memory();
    seeded.build_model_in_memory(seeded_model, None).unwrap();
    assert_visible_output(&seeded, seeded_output, false);
    seeded.tick(0.0).unwrap();
    assert_visible_output(&seeded, seeded_output, true);
}

#[test]
fn nonconvergent_boolean_feedback_is_accepted_and_advances_per_call() {
    let (model, pre_output, not_output) = oscillating_feedback_model();
    let mut engine = Engine::in_memory();
    engine
        .build_model_in_memory(model, None)
        .expect("Pre cuts the feedthrough loop without event fixed-point analysis");
    assert_eq!(engine.schedule().order.len(), 2);

    for (index, expected_pre) in [false, true, false, true].into_iter().enumerate() {
        engine.tick(2.0).unwrap();
        assert!(
            engine
                .outputs()
                .get(pre_output)
                .unwrap()
                .bit_eq(&Value::Boolean(expected_pre)),
            "Pre output on call {index}"
        );
        assert!(
            engine
                .outputs()
                .get(not_output)
                .unwrap()
                .bit_eq(&Value::Boolean(!expected_pre)),
            "Not output on call {index}"
        );
    }
}

#[test]
fn snapshot_restores_next_pre_output_at_same_timestamp() {
    let (model, output) = pre_model(false);
    let mut uninterrupted = Engine::in_memory();
    uninterrupted
        .build_model_in_memory(model.clone(), Some("urn:test:pre-profile:model"))
        .unwrap();
    uninterrupted
        .set_input(INPUT_PATH, Value::Boolean(true))
        .unwrap();
    uninterrupted.tick(9.0).unwrap();
    assert_visible_output(&uninterrupted, output, false);

    let snapshot = uninterrupted.state_snapshot().unwrap();
    let decoded = EngineStateSnapshot::from_bytes(snapshot.as_bytes()).unwrap();
    let mut restored = Engine::in_memory();
    restored
        .build_model_in_memory(model, Some("urn:test:pre-profile:model"))
        .unwrap();
    restored.restore_state(&decoded).unwrap();
    assert_visible_output(&restored, output, false);

    for engine in [&mut uninterrupted, &mut restored] {
        engine.set_input(INPUT_PATH, Value::Boolean(false)).unwrap();
        engine.tick(9.0).unwrap();
        assert_visible_output(engine, output, true);
    }

    assert_eq!(uninterrupted.state.words, restored.state.words);
    assert_eq!(
        uninterrupted.state_snapshot().unwrap().as_bytes(),
        restored.state_snapshot().unwrap().as_bytes()
    );
}
