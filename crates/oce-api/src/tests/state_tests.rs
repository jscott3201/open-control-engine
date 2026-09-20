//! Checkpoint and durable snapshot continuation tests.

use super::common::*;
use crate::{EngineStateError, EngineStateSnapshot};
use oce_store::{
    EquipmentDto, PointHandle, PointListRow, PointSnapshot, PointStore, PointWrite, RelationDto,
    RetrievalHit, SemanticPayloadDto, SemanticStore, StoreResult, TemplatePointReq,
};

const MINIMAL_LOOP: &[u8] = include_bytes!("../../../oce-cxf/tests/fixtures/minimal_loop.jsonld");
const LOOP_INPUTS: [(&str, Value); 1] = [("http://example.org#MinLoop.uSet", Value::Real(0.0))];

pub(super) fn sampled_model(class_path: &str, period: f64) -> ModelGraph {
    let mut model = ModelGraph::new();
    let block = BlockId(0);
    let parameter = if class_path == "CDL.Logical.Sources.SampleTrigger" {
        "period"
    } else {
        "samplePeriod"
    };
    let mut inputs = Vec::new();
    if class_path != "CDL.Logical.Sources.SampleTrigger" {
        inputs.push(ConnectorId(0));
        model.connectors.push(Connector::new(
            ConnectorId(0),
            block,
            Dir::In,
            ValueType::Real,
            0,
        ));
        model.external_inputs.push(ConnectorId(0));
    }
    let output = ConnectorId(model.connectors.len() as u32);
    let output_type = if class_path == "CDL.Logical.Sources.SampleTrigger" {
        ValueType::Boolean
    } else {
        ValueType::Real
    };
    model.connectors.push(Connector::new(
        output,
        block,
        Dir::Out,
        output_type,
        output.0,
    ));
    model.blocks.push(BlockInstance {
        id: block,
        class_iri: Arc::from(class_path),
        inputs,
        outputs: vec![output],
        params: ParamTable {
            values: vec![(Arc::from(parameter), Value::Real(period))],
        },
        decl_order: 0,
        instance_iri: None,
    });
    model
}

#[test]
fn capture_requires_a_loaded_model() {
    let engine = Engine::in_memory();
    assert!(matches!(
        engine.checkpoint(),
        Err(OcError::State(EngineStateError::NoLoadedModel))
    ));
    assert!(matches!(
        engine.state_snapshot(),
        Err(OcError::State(EngineStateError::NoLoadedModel))
    ));
}

#[test]
fn checkpoint_rewinds_and_replays_a_hand_built_stateful_model() {
    let (model, add_output, _, _) = build_accumulator_model();
    let mut engine = Engine::in_memory();
    engine.build_model_in_memory(model, None).unwrap();
    advance(&mut engine, 0.0, &[]).unwrap();
    advance(&mut engine, 1.0, &[]).unwrap();
    let checkpoint = engine.checkpoint().unwrap();

    advance(&mut engine, 2.0, &[]).unwrap();
    let expected = engine.state.values[add_output.0 as usize].clone();
    advance(&mut engine, 3.0, &[]).unwrap();
    engine.restore_checkpoint(&checkpoint).unwrap();
    advance(&mut engine, 2.0, &[]).unwrap();
    let replayed = engine.state.values[add_output.0 as usize].clone();

    assert!(replayed.bit_eq(&expected));
    assert_eq!(engine.state.t.to_bits(), 2.0f64.to_bits());
    assert_eq!(engine.prev_t.map(f64::to_bits), Some(2.0f64.to_bits()));
}

#[test]
fn one_checkpoint_branches_into_independently_loaded_engines() {
    let mut builder = Mb::new();
    let (_, inputs, outputs) = builder.block(
        "CDL.Reals.IntegratorWithReset",
        &[ValueType::Real, ValueType::Real, ValueType::Boolean],
        &[ValueType::Real],
        Vec::new(),
    );
    let mut model = builder.finish();
    model.external_inputs = inputs;
    let mut source = Engine::in_memory();
    source.build_model_in_memory(model.clone(), None).unwrap();
    let checkpoint = source.checkpoint().unwrap();

    let mut left = Engine::in_memory();
    left.build_model_in_memory(model.clone(), None).unwrap();
    let mut right = Engine::in_memory();
    right.build_model_in_memory(model, None).unwrap();
    left.restore_checkpoint(&checkpoint).unwrap();
    right.restore_checkpoint(&checkpoint).unwrap();

    let inputs = |value| {
        [
            ("conn#0", Value::Real(value)),
            ("conn#1", Value::Real(0.0)),
            ("conn#2", Value::Boolean(false)),
        ]
    };
    advance(&mut left, 0.0, &inputs(1.0)).unwrap();
    advance(&mut right, 0.0, &inputs(2.0)).unwrap();
    let left_initial = left.state.values[outputs[0].0 as usize].clone();
    let right_initial = right.state.values[outputs[0].0 as usize].clone();
    assert!(left_initial.bit_eq(&right_initial));
    advance(&mut left, 1.0, &inputs(1.0)).unwrap();
    advance(&mut right, 1.0, &inputs(2.0)).unwrap();
    advance(&mut left, 2.0, &inputs(1.0)).unwrap();
    advance(&mut right, 2.0, &inputs(2.0)).unwrap();
    let left_value = left.state.values[outputs[0].0 as usize].clone();
    let right_value = right.state.values[outputs[0].0 as usize].clone();
    assert!(!left_value.bit_eq(&right_value));
    advance(&mut left, 3.0, &inputs(1.0)).unwrap();
    assert_eq!(right.state.t.to_bits(), 2.0f64.to_bits());
}

#[test]
fn anonymous_hand_built_model_is_not_durable() {
    let mut engine = Engine::in_memory();
    engine
        .build_model_in_memory(build_accumulator_model().0, None)
        .unwrap();
    assert!(matches!(
        engine.state_snapshot(),
        Err(OcError::State(EngineStateError::IneligibleModel { .. }))
    ));
}

#[test]
fn durable_restore_refuses_an_advanced_target() {
    let mut source = Engine::in_memory();
    source.load_cxf(MINIMAL_LOOP).unwrap();
    let snapshot = source.state_snapshot().unwrap();

    let mut target = Engine::in_memory();
    target.load_cxf(MINIMAL_LOOP).unwrap();
    advance(&mut target, 0.0, &LOOP_INPUTS).unwrap();
    assert!(matches!(
        target.restore_state(&snapshot),
        Err(OcError::State(EngineStateError::DurableTargetAdvanced))
    ));
}

#[test]
fn checksum_corruption_is_typed() {
    let mut engine = Engine::in_memory();
    engine.load_cxf(MINIMAL_LOOP).unwrap();
    let mut bytes = engine.state_snapshot().unwrap().into_bytes();
    let middle = bytes.len() / 2;
    bytes[middle] ^= 0x80;
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&bytes),
        Err(EngineStateError::IntegrityMismatch { .. })
    ));
}

#[test]
fn every_g36_fixture_captures_before_and_after_its_first_tick() {
    let fixture_dir = format!(
        "{}/../oce-cxf/tests/fixtures/g36",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut fixtures = std::fs::read_dir(fixture_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "jsonld")
        })
        .collect::<Vec<_>>();
    fixtures.sort();
    assert_eq!(fixtures.len(), 47);

    for fixture in fixtures {
        let bytes = std::fs::read(&fixture).unwrap();
        let mut engine = Engine::in_memory();
        engine
            .load_cxf(&bytes)
            .unwrap_or_else(|error| panic!("{}: {error}", fixture.display()));
        let initial = engine
            .state_snapshot()
            .unwrap_or_else(|error| panic!("{} initial: {error}", fixture.display()));
        EngineStateSnapshot::from_bytes(initial.as_bytes())
            .unwrap_or_else(|error| panic!("{} initial decode: {error}", fixture.display()));
        advance_synthetic(&mut engine, 0.0)
            .unwrap_or_else(|error| panic!("{} tick: {error}", fixture.display()));
        let advanced = engine
            .state_snapshot()
            .unwrap_or_else(|error| panic!("{} advanced: {error}", fixture.display()));
        EngineStateSnapshot::from_bytes(advanced.as_bytes())
            .unwrap_or_else(|error| panic!("{} advanced decode: {error}", fixture.display()));
    }
}

#[test]
fn sampled_clocks_refuse_unrepresentable_time_before_mutation() {
    for class_path in [
        "CDL.Discrete.Sampler",
        "CDL.Discrete.ZeroOrderHold",
        "CDL.Discrete.FirstOrderHold",
        "CDL.Discrete.UnitDelay",
        "CDL.Logical.Sources.SampleTrigger",
    ] {
        let mut engine = Engine::in_memory();
        engine
            .build_model_in_memory(sampled_model(class_path, 1.0), None)
            .unwrap();
        let words = engine.state.words.clone();
        let values = engine.state.values.clone();
        let error = engine.prepare_frame(f64::MAX, &[]).unwrap_err();
        assert!(
            matches!(error, OcError::ModelTimeUnrepresentable { now } if now == f64::MAX),
            "{class_path}: {error:?}"
        );
        assert_eq!(engine.state.words, words, "{class_path}");
        assert!(
            engine
                .state
                .values
                .iter()
                .zip(&values)
                .all(|(left, right)| left.bit_eq(right)),
            "{class_path}"
        );
        assert_eq!(engine.prev_t, None, "{class_path}");
        assert!(engine.durable_restore_ready, "{class_path}");
    }
}

#[test]
fn initialized_max_period_clocks_accept_maximum_finite_time() {
    for class_path in [
        "CDL.Discrete.Sampler",
        "CDL.Discrete.ZeroOrderHold",
        "CDL.Discrete.FirstOrderHold",
        "CDL.Discrete.UnitDelay",
        "CDL.Logical.Sources.SampleTrigger",
    ] {
        let mut engine = Engine::in_memory();
        engine
            .build_model_in_memory(sampled_model(class_path, f64::MAX), None)
            .unwrap();
        let inputs = if class_path == "CDL.Logical.Sources.SampleTrigger" {
            vec![]
        } else {
            vec![("conn#0", Value::Real(0.0))]
        };
        advance(&mut engine, 0.0, &inputs).unwrap();
        advance(&mut engine, f64::MAX, &inputs)
            .unwrap_or_else(|error| panic!("{class_path}: {error}"));
        assert_eq!(engine.prev_t.map(f64::to_bits), Some(f64::MAX.to_bits()));
        engine
            .checkpoint()
            .unwrap_or_else(|error| panic!("{class_path}: {error}"));
    }
}

#[test]
fn unrepresentable_preparation_time_preserves_the_prior_run() {
    let mut engine = Engine::in_memory();
    engine
        .build_model_in_memory(sampled_model("CDL.Discrete.Sampler", 1.0), None)
        .unwrap();
    advance(&mut engine, 1.0, &[("conn#0", Value::Real(0.0))]).unwrap();
    let words = engine.state.words.clone();
    let state_t = engine.state.t.to_bits();
    let prev_t = engine.prev_t.map(f64::to_bits);
    assert!(matches!(
        engine.prepare_frame(f64::MAX, &[("conn#0", Value::Real(0.0))]),
        Err(OcError::ModelTimeUnrepresentable { .. })
    ));
    assert_eq!(engine.state.words, words);
    assert_eq!(engine.state.t.to_bits(), state_t);
    assert_eq!(engine.prev_t.map(f64::to_bits), prev_t);
}

fn minimal_loop_snapshot() -> EngineStateSnapshot {
    let mut source = Engine::in_memory();
    source.load_cxf(MINIMAL_LOOP).unwrap();
    advance(&mut source, 0.0, &LOOP_INPUTS).unwrap();
    source.state_snapshot().unwrap()
}

fn authored_empty_snapshot() -> EngineStateSnapshot {
    let mut engine = Engine::in_memory();
    engine
        .build_model_in_memory(ModelGraph::new(), Some("urn:test:empty-model"))
        .unwrap();
    engine.state_snapshot().unwrap()
}

#[test]
fn read_only_lifecycle_calls_keep_the_durable_restore_window_open() {
    let snapshot = minimal_loop_snapshot();
    let mut target = Engine::in_memory();
    target.load_cxf(MINIMAL_LOOP).unwrap();
    target.state_snapshot().unwrap();
    target.checkpoint().unwrap();
    target.halt().unwrap();
    target.resume().unwrap();
    target.halt().unwrap();
    target.restore_state(&snapshot).unwrap();
    assert_eq!(target.mode(), RunMode::Halted);
}

#[test]
fn refused_inputs_and_time_keep_the_durable_restore_window_open() {
    let snapshot = minimal_loop_snapshot();
    let mut target = Engine::in_memory();
    target.load_cxf(MINIMAL_LOOP).unwrap();
    assert!(matches!(
        target.prepare_frame(0.0, &[("missing", Value::Real(1.0))]),
        Err(OcError::FrameUnknownInput { .. })
    ));
    assert!(matches!(
        target.prepare_frame(
            0.0,
            &[("http://example.org#MinLoop.uSet", Value::Boolean(true))]
        ),
        Err(OcError::InputType(_))
    ));
    assert!(matches!(
        target.prepare_frame(f64::NAN, &LOOP_INPUTS),
        Err(OcError::NonFiniteTime { .. })
    ));
    target.restore_state(&snapshot).unwrap();
}

#[test]
fn mutation_boundaries_close_the_durable_restore_window() {
    let snapshot = minimal_loop_snapshot();

    let mut input_target = Engine::in_memory();
    input_target.load_cxf(MINIMAL_LOOP).unwrap();
    let prepared = input_target.prepare_frame(0.0, &LOOP_INPUTS).unwrap();
    assert!(
        input_target.durable_restore_ready,
        "preparation is not a mutation"
    );
    input_target.execute_frame(prepared).unwrap();
    assert!(matches!(
        input_target.restore_state(&snapshot),
        Err(OcError::State(EngineStateError::DurableTargetAdvanced))
    ));

    let mut resume_target = Engine::in_memory();
    resume_target.load_cxf(MINIMAL_LOOP).unwrap();
    resume_target.halt().unwrap();
    resume_target
        .set_param("http://example.org#MinLoop.con.k", Value::Real(3.0))
        .unwrap();
    resume_target.resume().unwrap();
    assert!(matches!(
        resume_target.restore_state(&snapshot),
        Err(OcError::State(EngineStateError::DurableTargetAdvanced))
    ));

    let mut checkpoint_target = Engine::in_memory();
    checkpoint_target.load_cxf(MINIMAL_LOOP).unwrap();
    let checkpoint = checkpoint_target.checkpoint().unwrap();
    checkpoint_target.restore_checkpoint(&checkpoint).unwrap();
    assert!(matches!(
        checkpoint_target.restore_state(&snapshot),
        Err(OcError::State(EngineStateError::DurableTargetAdvanced))
    ));
}

#[test]
fn pending_parameter_edits_take_precedence_over_restore_readiness() {
    let snapshot = minimal_loop_snapshot();
    let mut target = Engine::in_memory();
    target.load_cxf(MINIMAL_LOOP).unwrap();
    target.halt().unwrap();
    target
        .set_param("http://example.org#MinLoop.con.k", Value::Real(3.0))
        .unwrap();
    assert!(matches!(
        target.state_snapshot(),
        Err(OcError::State(EngineStateError::PendingParameterEdits))
    ));
    assert!(matches!(
        target.restore_state(&snapshot),
        Err(OcError::State(EngineStateError::PendingParameterEdits))
    ));
}

#[test]
fn failed_reload_preserves_the_prior_durable_restore_window() {
    let snapshot = minimal_loop_snapshot();
    let mut target = Engine::in_memory();
    target.load_cxf(MINIMAL_LOOP).unwrap();
    assert!(target.load_cxf(b"not json").is_err());
    target.restore_state(&snapshot).unwrap();
}

#[test]
fn a_successfully_loaded_empty_model_is_capture_eligible() {
    let mut engine = Engine::in_memory();
    engine
        .build_model_in_memory(ModelGraph::new(), None)
        .unwrap();
    engine.checkpoint().unwrap();
    let snapshot = engine.state_snapshot().unwrap();
    EngineStateSnapshot::from_bytes(snapshot.as_bytes()).unwrap();
}

#[test]
fn capture_and_restore_call_no_store_method() {
    let snapshot = minimal_loop_snapshot();
    let store = Arc::new(CountingStore::default());
    let mut target = Engine::with_store(Arc::clone(&store));
    target.load_cxf(MINIMAL_LOOP).unwrap();
    let after_load = store.calls();

    target.checkpoint().unwrap();
    target.state_snapshot().unwrap();
    assert_eq!(store.calls(), after_load, "capture reached the store seam");
    target.restore_state(&snapshot).unwrap();
    assert_eq!(store.calls(), after_load, "restore reached the store seam");
}

#[test]
fn checkpoint_refusal_is_bit_atomic() {
    let mut engine = Engine::in_memory();
    engine
        .build_model_in_memory(sampled_model("CDL.Discrete.UnitDelay", 1.0), None)
        .unwrap();
    advance(&mut engine, 0.0, &[("conn#0", Value::Real(0.0))]).unwrap();
    engine.halt().unwrap();
    let checkpoint = engine.checkpoint().unwrap();
    let mut image = (*checkpoint.image).clone();
    image.words[4] = 2;
    let corrupted = crate::EngineCheckpoint {
        image: Arc::new(image),
    };
    let words = engine.state.words.clone();
    let values = engine.state.values.clone();
    let state_t = engine.state.t.to_bits();
    let prev_t = engine.prev_t.map(f64::to_bits);
    let outputs = output_image(&engine);
    let ready = engine.durable_restore_ready;

    assert!(matches!(
        engine.restore_checkpoint(&corrupted),
        Err(OcError::State(EngineStateError::InvalidBlockState { .. }))
    ));
    assert_eq!(engine.state.words, words);
    assert!(
        engine
            .state
            .values
            .iter()
            .zip(&values)
            .all(|(left, right)| left.bit_eq(right))
    );
    assert_eq!(engine.state.t.to_bits(), state_t);
    assert_eq!(engine.prev_t.map(f64::to_bits), prev_t);
    assert_eq!(output_image(&engine).len(), outputs.len());
    assert!(output_image(&engine).iter().zip(&outputs).all(
        |((left_path, left), (right_path, right))| left_path == right_path && left.bit_eq(right)
    ));
    assert_eq!(engine.mode(), RunMode::Halted);
    assert_eq!(engine.durable_restore_ready, ready);
}

#[test]
fn connection_vector_permutation_does_not_change_execution_identity() {
    let (model, _, _, _) = build_accumulator_model();
    let mut permuted = model.clone();
    permuted.connections.reverse();
    let mut left = Engine::in_memory();
    left.build_model_in_memory(model, None).unwrap();
    let mut right = Engine::in_memory();
    right.build_model_in_memory(permuted, None).unwrap();
    let left = left.checkpoint().unwrap();
    let right = right.checkpoint().unwrap();
    assert_eq!(left.image.fingerprint, right.image.fingerprint);
    assert_eq!(left.image.manifest, right.image.manifest);
}

#[test]
fn diagnostic_model_identity_is_not_an_execution_compatibility_key() {
    let snapshot = minimal_loop_snapshot();
    let mut target = Engine::in_memory();
    target.load_cxf(MINIMAL_LOOP).unwrap();
    target.model_id = DomainKey::new("urn:test:different-diagnostic-model-id");
    target.restore_state(&snapshot).unwrap();
}

#[test]
fn changed_parameter_refuses_checkpoint_restore() {
    let mut source = Engine::in_memory();
    source
        .build_model_in_memory(build_accumulator_model().0, None)
        .unwrap();
    let checkpoint = source.checkpoint().unwrap();

    let (mut changed, _, _, _) = build_accumulator_model();
    changed.blocks[0].params.values[0].1 = Value::Real(2.0);
    let mut target = Engine::in_memory();
    target.build_model_in_memory(changed, None).unwrap();
    assert!(matches!(
        target.restore_checkpoint(&checkpoint),
        Err(OcError::State(
            EngineStateError::IncompatibleExecution { .. }
        ))
    ));
}

#[test]
fn decoder_size_cap_precedes_header_validation() {
    let exact = vec![0; crate::state::MAX_SNAPSHOT_BYTES as usize];
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&exact),
        Err(EngineStateError::MalformedSnapshot { .. })
    ));
    let over = vec![0; crate::state::MAX_SNAPSHOT_BYTES as usize + 1];
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&over),
        Err(EngineStateError::SnapshotTooLarge {
            actual_bytes,
            max_bytes
        }) if actual_bytes == max_bytes + 1
    ));
}

#[test]
fn empty_authored_model_has_pinned_durable_bytes_and_fingerprint() {
    let snapshot = authored_empty_snapshot();
    let bytes = snapshot.into_bytes();
    let fingerprint = u128::from_le_bytes(bytes[24..40].try_into().unwrap());
    let actual = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(
        fingerprint,
        12_166_013_797_975_197_438_589_920_839_995_253_557
    );
    assert_eq!(
        actual,
        "4f43455354415400020000000200000076000000000000003563bc4e4d6f7568e3b58985ca1627091400000075726e3a746573743a656d7074792d6d6f64656c0000000000000000002d00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000002cece89772fdd4ad50df3cf14722371d"
    );
}

#[derive(Default)]
pub(super) struct CountingStore {
    inner: MemStore,
    calls: std::sync::atomic::AtomicUsize,
}

impl CountingStore {
    fn bump(&self) {
        self.calls
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub(super) fn calls(&self) -> usize {
        self.calls.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl ModelStore for CountingStore {
    fn save_model(&self, model: &ResolvedModel) -> StoreResult<()> {
        self.bump();
        self.inner.save_model(model)
    }

    fn load_model(&self, model_id: &DomainKey) -> StoreResult<ResolvedModel> {
        self.bump();
        self.inner.load_model(model_id)
    }

    fn list_models(&self) -> StoreResult<Vec<DomainKey>> {
        self.bump();
        self.inner.list_models()
    }

    fn delete_model(&self, model_id: &DomainKey) -> StoreResult<()> {
        self.bump();
        self.inner.delete_model(model_id)
    }
}

impl PointStore for CountingStore {
    fn resolve_points(&self, keys: &[DomainKey]) -> StoreResult<Vec<PointHandle>> {
        self.bump();
        self.inner.resolve_points(keys)
    }

    fn snapshot(&self) -> StoreResult<Box<dyn PointSnapshot>> {
        self.bump();
        self.inner.snapshot()
    }

    fn write_points(&self, batch: &[PointWrite]) -> StoreResult<usize> {
        self.bump();
        self.inner.write_points(batch)
    }
}

impl SemanticStore for CountingStore {
    fn upsert_equipment(&self, equipment: &EquipmentDto) -> StoreResult<()> {
        self.bump();
        self.inner.upsert_equipment(equipment)
    }

    fn add_relation(&self, relation: &RelationDto) -> StoreResult<()> {
        self.bump();
        self.inner.add_relation(relation)
    }

    fn put_semantic_payload(&self, payload: &SemanticPayloadDto) -> StoreResult<()> {
        self.bump();
        self.inner.put_semantic_payload(payload)
    }

    fn get_semantic_payloads(&self, subject: &DomainKey) -> StoreResult<Vec<SemanticPayloadDto>> {
        self.bump();
        self.inner.get_semantic_payloads(subject)
    }

    fn point_list(&self, controlled_device: Option<&str>) -> StoreResult<Vec<PointListRow>> {
        self.bump();
        self.inner.point_list(controlled_device)
    }

    fn retrieve(&self, query: &SemanticQuery) -> StoreResult<Vec<RetrievalHit>> {
        self.bump();
        self.inner.retrieve(query)
    }

    fn match_template(&self, required_points: &[TemplatePointReq]) -> StoreResult<Vec<DomainKey>> {
        self.bump();
        self.inner.match_template(required_points)
    }
}

impl Durable for CountingStore {
    fn commit(&self) -> StoreResult<()> {
        self.bump();
        self.inner.commit()
    }

    fn flush(&self) -> StoreResult<()> {
        self.bump();
        self.inner.flush()
    }

    fn recover(&self) -> StoreResult<()> {
        self.bump();
        self.inner.recover()
    }
}
