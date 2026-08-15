//! End-to-end real-time output write-back, timestamp, port-count, and failure-path tests.

use std::sync::{Arc, Mutex};

use oce_model::Value;
use oce_store::{
    DomainKey, Durability, Durable, EquipmentDto, ModelStore, OcValue, PointHandle, PointListRow,
    PointSnapshot, PointStatus, PointStore, PointWrite, RelationDto, ResolvedModel, RetrievalHit,
    SemanticPayloadDto, SemanticQuery, SemanticStore, StoreError, StoreResult, TemplatePointReq,
};
use oce_store_mem::MemStore;

use super::common::{Engine, OcError, build_realtime_output_model};

const EPOCH: u64 = 1_700_000_000_000_000_000;
const PROJECTED_OUTPUTS: usize = 8;
const STRING_KEY: &str = "conn#14";

fn expected_store_value(value: &Value) -> OcValue {
    match value {
        Value::Real(value) => OcValue::Real(*value),
        Value::Integer(value) => OcValue::Int(*value),
        Value::Boolean(value) => OcValue::Bool(*value),
        Value::Enum { ordinal, .. } => OcValue::Int(i64::from(*ordinal)),
        Value::String(value) => OcValue::String(value.to_string()),
    }
}

fn assert_memstore_samples(engine: &Engine, timestamp: u64) {
    let outputs = engine.outputs().to_map();
    let snapshot = engine.store().snapshot().unwrap();
    for (path, connector_id) in engine.io.durable_columns() {
        let sample = snapshot
            .read_by_key(&DomainKey::new(path.clone()))
            .unwrap_or_else(|| panic!("missing sample for {path}"));
        assert_eq!(
            sample.value,
            expected_store_value(
                &outputs
                    .iter()
                    .find(|(output_path, _)| output_path == &path)
                    .unwrap()
                    .1,
            ),
            "wrong post-tick value for {path} (connector {})",
            connector_id.0
        );
        assert_eq!(sample.status, PointStatus::Ok, "wrong status for {path}");
        assert_eq!(
            sample.at_unix_nanos, timestamp,
            "wrong timestamp for {path}"
        );
    }
    assert!(
        snapshot.read_by_key(&DomainKey::new(STRING_KEY)).is_none(),
        "metadata-only String output must not reach the store"
    );
}

fn assert_memstore_has_no_output_samples(engine: &Engine) {
    let snapshot = engine.store().snapshot().unwrap();
    let columns = engine.io.durable_columns();
    assert_eq!(columns.len(), PROJECTED_OUTPUTS);
    for (path, _) in columns {
        assert!(
            snapshot
                .read_by_key(&DomainKey::new(path.clone()))
                .is_none(),
            "unexpected sample for {path}"
        );
    }
}

#[test]
fn realtime_steps_commit_post_tick_projected_values_at_exact_host_instants() {
    let mut engine = Engine::in_memory();
    engine
        .build_model_in_memory(build_realtime_output_model(), None)
        .unwrap();
    engine.set_realtime_epoch_unix_nanos(EPOCH);

    let first = engine.step_realtime(2.0).unwrap();
    assert_eq!(engine.realtime_epoch_unix_nanos(), Some(EPOCH));
    assert_eq!(first.written, PROJECTED_OUTPUTS);
    assert_memstore_samples(&engine, EPOCH + 2_000_000_000);

    let second = engine.step_realtime(3.0).unwrap();
    assert_eq!(second.written, PROJECTED_OUTPUTS);
    assert_memstore_samples(&engine, EPOCH + 3_000_000_000);
}

#[test]
fn host_epoch_is_required_and_exact_mapping_handles_signed_model_time() {
    let mut unset = Engine::in_memory();
    unset
        .build_model_in_memory(build_realtime_output_model(), None)
        .unwrap();
    assert!(matches!(
        unset.step_realtime(0.0),
        Err(OcError::RealtimeEpochUnset)
    ));

    let cases = [
        (-5.0, Ok(EPOCH - 5_000_000_000)),
        (-1_000_000_000.0, Ok(700_000_000_000_000_000)),
        (1.0e30, Err(())),
    ];
    for (time, expected) in cases {
        let mut engine = Engine::in_memory();
        engine
            .build_model_in_memory(build_realtime_output_model(), None)
            .unwrap();
        engine.set_realtime_epoch_unix_nanos(EPOCH);
        match expected {
            Ok(timestamp) => {
                engine.step_realtime(time).unwrap();
                assert_memstore_samples(&engine, timestamp);
            }
            Err(()) => assert!(matches!(
                engine.step_realtime(time),
                Err(OcError::RealtimeInstantUnrepresentable {
                    epoch_unix_nanos: EPOCH,
                    t_now
                }) if t_now == time
            )),
        }
    }

    for time in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut engine = Engine::in_memory();
        engine
            .build_model_in_memory(build_realtime_output_model(), None)
            .unwrap();
        engine.set_realtime_epoch_unix_nanos(EPOCH);
        assert!(matches!(
            engine.step_realtime(time),
            Err(OcError::RealtimeInstantUnrepresentable {
                epoch_unix_nanos: EPOCH,
                t_now
            }) if t_now.to_bits() == time.to_bits()
        ));
    }

    let mut before_epoch = Engine::in_memory();
    before_epoch
        .build_model_in_memory(build_realtime_output_model(), None)
        .unwrap();
    before_epoch.set_realtime_epoch_unix_nanos(0);
    assert!(matches!(
        before_epoch.step_realtime(-5.0),
        Err(OcError::RealtimeInstantUnrepresentable {
            epoch_unix_nanos: 0,
            t_now: -5.0
        })
    ));
}

#[test]
fn unrepresentable_realtime_instant_does_not_tick_or_write() {
    let mut engine = Engine::in_memory();
    engine
        .build_model_in_memory(build_realtime_output_model(), None)
        .unwrap();
    engine.tick(1.0).unwrap();
    engine.set_realtime_epoch_unix_nanos(EPOCH);

    let values = engine.state.values.clone();
    let words = engine.state.words.clone();
    let state_t = engine.state.t.to_bits();
    let prev_t = engine.prev_t.map(f64::to_bits);
    let outputs = engine.outputs().to_map();
    assert_memstore_has_no_output_samples(&engine);

    let time = 20_000_000_000.0;
    assert!(matches!(
        engine.step_realtime(time),
        Err(OcError::RealtimeInstantUnrepresentable {
            epoch_unix_nanos: EPOCH,
            t_now
        }) if t_now == time
    ));

    assert_eq!(engine.state.values.len(), values.len());
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
    assert_eq!(engine.outputs().len(), outputs.len());
    assert!(engine.outputs().to_map().iter().zip(&outputs).all(
        |((left_path, left), (right_path, right))| {
            left_path == right_path && left.bit_eq(right)
        }
    ));
    assert_memstore_has_no_output_samples(&engine);
}

#[derive(Clone, Copy)]
enum WriteBehavior {
    AcceptOneFewer,
    Fail,
}

struct ProbeStore {
    inner: MemStore,
    behavior: WriteBehavior,
    writes: Mutex<Vec<PointWrite>>,
}

impl ProbeStore {
    fn new(behavior: WriteBehavior) -> Self {
        Self {
            inner: MemStore::default(),
            behavior,
            writes: Mutex::new(Vec::new()),
        }
    }
}

impl ModelStore for ProbeStore {
    fn save_model(&self, model: &ResolvedModel) -> StoreResult<()> {
        self.inner.save_model(model)
    }
    fn load_model(&self, model_id: &DomainKey) -> StoreResult<ResolvedModel> {
        self.inner.load_model(model_id)
    }
    fn list_models(&self) -> StoreResult<Vec<DomainKey>> {
        self.inner.list_models()
    }
    fn delete_model(&self, model_id: &DomainKey) -> StoreResult<()> {
        self.inner.delete_model(model_id)
    }
}

impl PointStore for ProbeStore {
    fn resolve_points(&self, keys: &[DomainKey]) -> StoreResult<Vec<PointHandle>> {
        self.inner.resolve_points(keys)
    }
    fn snapshot(&self) -> StoreResult<Box<dyn PointSnapshot>> {
        self.inner.snapshot()
    }
    fn write_points(&self, batch: &[PointWrite]) -> StoreResult<usize> {
        self.writes.lock().unwrap().extend(batch.iter().cloned());
        match self.behavior {
            WriteBehavior::AcceptOneFewer => Ok(batch.len().saturating_sub(1)),
            WriteBehavior::Fail => Err(StoreError::Durability("injected write failure".to_owned())),
        }
    }
}

impl SemanticStore for ProbeStore {
    fn upsert_equipment(&self, eq: &EquipmentDto) -> StoreResult<()> {
        self.inner.upsert_equipment(eq)
    }
    fn add_relation(&self, rel: &RelationDto) -> StoreResult<()> {
        self.inner.add_relation(rel)
    }
    fn put_semantic_payload(&self, payload: &SemanticPayloadDto) -> StoreResult<()> {
        self.inner.put_semantic_payload(payload)
    }
    fn get_semantic_payloads(&self, subject: &DomainKey) -> StoreResult<Vec<SemanticPayloadDto>> {
        self.inner.get_semantic_payloads(subject)
    }
    fn point_list(&self, device: Option<&str>) -> StoreResult<Vec<PointListRow>> {
        self.inner.point_list(device)
    }
    fn retrieve(&self, query: &SemanticQuery) -> StoreResult<Vec<RetrievalHit>> {
        self.inner.retrieve(query)
    }
    fn match_template(&self, points: &[TemplatePointReq]) -> StoreResult<Vec<DomainKey>> {
        self.inner.match_template(points)
    }
}

impl Durable for ProbeStore {
    fn commit(&self) -> StoreResult<()> {
        self.inner.commit()
    }
    fn flush(&self) -> StoreResult<()> {
        self.inner.flush()
    }
    fn recover(&self) -> StoreResult<()> {
        self.inner.recover()
    }
}

#[test]
fn report_uses_port_accepted_count_and_batch_carries_telemetry_durability() {
    let store = Arc::new(ProbeStore::new(WriteBehavior::AcceptOneFewer));
    let mut engine = Engine::with_store(Arc::clone(&store));
    engine
        .build_model_in_memory(build_realtime_output_model(), None)
        .unwrap();
    engine.set_realtime_epoch_unix_nanos(EPOCH);

    let report = engine.step_realtime(2.0).unwrap();
    assert_eq!(report.written, PROJECTED_OUTPUTS - 1);
    let writes = store.writes.lock().unwrap();
    assert_eq!(writes.len(), PROJECTED_OUTPUTS);
    assert!(
        writes
            .iter()
            .all(|write| write.durability == Durability::Telemetry)
    );
}

#[test]
fn failed_store_write_returns_typed_error_after_tick_remains_applied() {
    let store = Arc::new(ProbeStore::new(WriteBehavior::Fail));
    let mut engine = Engine::with_store(store);
    engine
        .build_model_in_memory(build_realtime_output_model(), None)
        .unwrap();
    engine.set_realtime_epoch_unix_nanos(EPOCH);

    let error = engine.step_realtime(2.0).unwrap_err();
    assert!(matches!(
        error,
        OcError::Store(StoreError::Durability(ref detail)) if detail == "injected write failure"
    ));
    assert!(matches!(engine.get_output("conn#3"), Ok(Value::Real(value)) if value == 1.0));
    assert!(matches!(
        engine.step_realtime(1.0),
        Err(OcError::TimeRegression {
            now: 1.0,
            prev: 2.0
        })
    ));
}
