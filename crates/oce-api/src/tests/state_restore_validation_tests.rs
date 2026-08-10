//! Restore-side block-state and time-invariant refusal tests.

use std::sync::Arc;

use oce_model::{
    BlockId, BlockInstance, Connector, ConnectorId, Dir, ModelGraph, ParamTable, Value, ValueType,
};

use super::state_tests::CountingStore;
use crate::{Engine, EngineCheckpoint, EngineStateError, EngineStateSnapshot, OcError, RunMode};

fn sampled_model(class_path: &str, period: f64) -> ModelGraph {
    let mut model = ModelGraph::new();
    model.blocks.push(BlockInstance {
        id: BlockId(0),
        class_iri: Arc::from(class_path),
        inputs: vec![ConnectorId(0)],
        outputs: vec![ConnectorId(1)],
        params: ParamTable {
            values: vec![(Arc::from("samplePeriod"), Value::Real(period))],
        },
        decl_order: 0,
        instance_iri: Some(Arc::from("urn:test:sampled")),
    });
    model.connectors.push(
        Connector::new(ConnectorId(0), BlockId(0), Dir::In, ValueType::Real, 0)
            .with_iri("urn:test:sampled.u"),
    );
    model.connectors.push(
        Connector::new(ConnectorId(1), BlockId(0), Dir::Out, ValueType::Real, 0)
            .with_iri("urn:test:sampled.y"),
    );
    model.external_inputs.push(ConnectorId(0));
    model
}

fn assert_atomic_refusal(engine: &mut Engine, checkpoint: EngineCheckpoint) {
    let values = engine.state.values.clone();
    let words = engine.state.words.clone();
    let state_t = engine.state.t.to_bits();
    let prev_t = engine.prev_t.map(f64::to_bits);
    let outputs = engine.outputs().to_map();
    assert!(matches!(
        engine.restore_checkpoint(&checkpoint),
        Err(OcError::State(EngineStateError::InvalidBlockState { .. }))
    ));
    assert!(
        engine
            .state
            .values
            .iter()
            .zip(&values)
            .all(|(left, right)| left.bit_eq(right))
    );
    assert_eq!(engine.state.words, words);
    assert_eq!(engine.state.t.to_bits(), state_t);
    assert_eq!(engine.prev_t.map(f64::to_bits), prev_t);
    let restored_outputs = engine.outputs().to_map();
    assert_eq!(restored_outputs.len(), outputs.len());
    assert!(
        restored_outputs
            .iter()
            .zip(&outputs)
            .all(|((left_key, left), (right_key, right))| {
                left_key == right_key && left.bit_eq(right)
            })
    );
}

#[test]
fn upward_rounded_sample_origins_remain_capturable_and_restorable() {
    for period in [1.000_000_6, 1.000_001_5] {
        for (class_path, t0_word) in [
            ("CDL.Discrete.Sampler", 1),
            ("CDL.Discrete.ZeroOrderHold", 1),
            ("CDL.Discrete.UnitDelay", 2),
            ("CDL.Discrete.FirstOrderHold", 1),
        ] {
            let graph = sampled_model(class_path, period);
            let mut source = Engine::in_memory();
            source.build_model_in_memory(graph.clone(), None).unwrap();
            source.tick(period).unwrap();
            assert!(f64::from_bits(source.state.words[t0_word]) > source.state.t);
            let checkpoint = source.checkpoint().unwrap();
            let mut target = Engine::in_memory();
            target.build_model_in_memory(graph, None).unwrap();
            target.restore_checkpoint(&checkpoint).unwrap();
            assert_eq!(target.state.words, source.state.words);
            assert_eq!(target.state.t.to_bits(), source.state.t.to_bits());
        }
    }
}

#[test]
fn downward_rounded_late_first_order_sample_remains_capturable_and_restorable() {
    let period = 1.000_000_4;
    let first_tick = 2.000_000_7;
    let graph = sampled_model("CDL.Discrete.FirstOrderHold", period);
    let mut source = Engine::in_memory();
    source.build_model_in_memory(graph.clone(), None).unwrap();
    source.tick(first_tick).unwrap();
    assert_eq!(
        f64::from_bits(source.state.words[1]).to_bits(),
        1.0f64.to_bits()
    );
    assert_eq!(source.state.words[2].cast_signed(), 1);
    assert_eq!(source.state.words[3], source.state.words[1]);
    let checkpoint = source.checkpoint().unwrap();
    let mut target = Engine::in_memory();
    target.build_model_in_memory(graph, None).unwrap();
    target.restore_checkpoint(&checkpoint).unwrap();
    assert_eq!(target.state.words, source.state.words);
}

#[test]
fn sample_origin_quotient_outside_i64_remains_capturable() {
    let graph = sampled_model("CDL.Discrete.Sampler", 1.0);
    let mut engine = Engine::in_memory();
    engine.build_model_in_memory(graph, None).unwrap();
    engine.tick(1.0e20).unwrap();
    engine.checkpoint().unwrap();
}

#[test]
fn epsilon_early_first_order_sample_remains_capturable_and_restorable() {
    let period = 1.000_000_6;
    let graph = sampled_model("CDL.Discrete.FirstOrderHold", period);
    let mut source = Engine::in_memory();
    source
        .build_model_in_memory(graph.clone(), Some("urn:test:sampled-model"))
        .unwrap();
    source.tick(period).unwrap();
    let t0 = f64::from_bits(source.state.words[1]);
    source.tick(t0 - period * 0.5e-9).unwrap();
    assert!(f64::from_bits(source.state.words[3]) < t0);

    let checkpoint = source.checkpoint().unwrap();
    let snapshot = source.state_snapshot().unwrap();
    let decoded = EngineStateSnapshot::from_bytes(snapshot.as_bytes()).unwrap();
    let mut local = Engine::in_memory();
    local
        .build_model_in_memory(graph.clone(), Some("urn:test:sampled-model"))
        .unwrap();
    local.restore_checkpoint(&checkpoint).unwrap();
    let mut durable = Engine::in_memory();
    durable
        .build_model_in_memory(graph, Some("urn:test:sampled-model"))
        .unwrap();
    durable.restore_state(&decoded).unwrap();
    assert_eq!(local.state.words, source.state.words);
    assert_eq!(durable.state.words, source.state.words);
}

#[test]
fn removing_previous_time_from_an_advanced_image_refuses_atomically() {
    let mut engine = Engine::in_memory();
    engine
        .build_model_in_memory(sampled_model("CDL.Discrete.UnitDelay", 1.0), None)
        .unwrap();
    engine.tick(0.0).unwrap();
    let mut image = (*engine.checkpoint().unwrap().image).clone();
    image.prev_t = None;
    assert_atomic_refusal(
        &mut engine,
        EngineCheckpoint {
            image: Arc::new(image),
        },
    );
}

#[test]
fn invalid_state_errors_bound_hostile_block_identity() {
    let mut graph = sampled_model("CDL.Discrete.UnitDelay", 1.0);
    graph.blocks[0].instance_iri = Some(Arc::from("x".repeat(1024 * 1024)));
    let mut engine = Engine::in_memory();
    engine.build_model_in_memory(graph, None).unwrap();
    engine.tick(0.0).unwrap();
    let mut image = (*engine.checkpoint().unwrap().image).clone();
    image.words[4] = 2;
    let error = engine
        .restore_checkpoint(&EngineCheckpoint {
            image: Arc::new(image),
        })
        .unwrap_err();
    let OcError::State(EngineStateError::InvalidBlockState { block, .. }) = error else {
        panic!("invalid state returned the wrong error: {error:?}")
    };
    assert!(block.len() <= 259, "{}", block.len());
}

#[test]
fn off_grid_sample_origin_refuses_atomically() {
    let mut engine = Engine::in_memory();
    engine
        .build_model_in_memory(sampled_model("CDL.Discrete.Sampler", 1.0), None)
        .unwrap();
    for time in [0.0, 1.0, 2.0] {
        engine.tick(time).unwrap();
    }
    let mut image = (*engine.checkpoint().unwrap().image).clone();
    image.words[1] = 0.5f64.to_bits();
    image.words[2] = 1i64.cast_unsigned();
    assert_atomic_refusal(
        &mut engine,
        EngineCheckpoint {
            image: Arc::new(image),
        },
    );
}

#[test]
fn resigned_future_first_order_history_refuses_durable_restore_atomically() {
    let graph = sampled_model("CDL.Discrete.FirstOrderHold", 1.0);
    let mut source = Engine::in_memory();
    source
        .build_model_in_memory(graph.clone(), Some("urn:test:sampled-model"))
        .unwrap();
    source.tick(0.0).unwrap();
    let mut image = (*source.state_snapshot().unwrap().image).clone();
    image.words[3] = f64::MAX.to_bits();
    let bytes = crate::state_codec::encode_snapshot(&image, false).unwrap();
    let corrupted = EngineStateSnapshot::from_bytes(&bytes).unwrap();

    let store = Arc::new(CountingStore::default());
    let mut target = Engine::with_store(Arc::clone(&store));
    target
        .build_model_in_memory(graph, Some("urn:test:sampled-model"))
        .unwrap();
    target.halt().unwrap();
    target.set_realtime_epoch_unix_nanos(77);
    let model = Arc::clone(&target.model);
    let schedule = format!("{:?}", target.schedule);
    let params = format!("{:?}", target.params);
    let io = format!("{:?}", target.io);
    let store_inputs = format!("{:?}", target.store_inputs);
    let durable_batch = format!("{:?}", target.durable_batch);
    let warnings = format!("{:?}", target.semantic_warnings);
    let values = target.state.values.clone();
    let words = target.state.words.clone();
    let state_t = target.state.t.to_bits();
    let prev_t = target.prev_t.map(f64::to_bits);
    let outputs = target.outputs().to_map();
    let calls = store.calls();

    assert!(matches!(
        target.restore_state(&corrupted),
        Err(OcError::State(EngineStateError::InvalidBlockState { .. }))
    ));
    assert!(Arc::ptr_eq(&target.model, &model));
    assert_eq!(format!("{:?}", target.schedule), schedule);
    assert_eq!(format!("{:?}", target.params), params);
    assert_eq!(format!("{:?}", target.io), io);
    assert_eq!(format!("{:?}", target.store_inputs), store_inputs);
    assert_eq!(format!("{:?}", target.durable_batch), durable_batch);
    assert_eq!(format!("{:?}", target.semantic_warnings), warnings);
    assert!(
        target
            .state
            .values
            .iter()
            .zip(&values)
            .all(|(left, right)| left.bit_eq(right))
    );
    assert_eq!(target.state.words, words);
    assert_eq!(target.state.t.to_bits(), state_t);
    assert_eq!(target.prev_t.map(f64::to_bits), prev_t);
    let restored_outputs = target.outputs().to_map();
    assert_eq!(restored_outputs.len(), outputs.len());
    assert!(restored_outputs.iter().zip(&outputs).all(
        |((left_path, left), (right_path, right))| left_path == right_path && left.bit_eq(right)
    ));
    assert_eq!(target.mode(), RunMode::Halted);
    assert_eq!(target.realtime_epoch_unix_nanos(), Some(77));
    assert!(target.durable_restore_ready);
    assert_eq!(store.calls(), calls);
}
