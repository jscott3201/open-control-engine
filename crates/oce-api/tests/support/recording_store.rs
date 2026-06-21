//! Recording `Store` test double for tick-purity assertions.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use oce_api::oce_store::{
    DomainKey, Durable, EquipmentDto, ModelStore, PointHandle, PointListRow, PointSample,
    PointSnapshot, PointStore, PointWrite, RelationDto, ResolvedModel, RetrievalHit,
    SemanticPayloadDto, SemanticQuery, SemanticStore, StoreResult, TemplatePointReq,
};
use oce_store_mem::MemStore;

#[derive(Default)]
pub(crate) struct RecordingStore {
    inner: MemStore,
    state: Arc<RecordingState>,
}

impl RecordingStore {
    pub(crate) fn reset_calls(&self) {
        self.state.calls.reset();
    }

    pub(crate) fn calls(&self) -> StoreCallSnapshot {
        self.state.calls.snapshot()
    }

    pub(crate) fn arm_hot_path_guard(&self) {
        self.state.panic_on_forbidden.store(true, Ordering::SeqCst);
    }
}

#[derive(Default)]
struct RecordingState {
    calls: StoreCalls,
    panic_on_forbidden: AtomicBool,
}

impl RecordingState {
    fn count_forbidden(&self, counter: &AtomicUsize, method: &'static str) {
        counter.fetch_add(1, Ordering::SeqCst);
        assert!(
            !self.panic_on_forbidden.load(Ordering::SeqCst),
            "forbidden store method reached the guarded tick path: {method}"
        );
    }

    fn count_allowed(&self, counter: &AtomicUsize) {
        counter.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Default)]
struct StoreCalls {
    save_model: AtomicUsize,
    load_model: AtomicUsize,
    list_models: AtomicUsize,
    delete_model: AtomicUsize,
    resolve_points: AtomicUsize,
    snapshot: AtomicUsize,
    write_points: AtomicUsize,
    read_resolved: AtomicUsize,
    read_by_key: AtomicUsize,
    upsert_equipment: AtomicUsize,
    add_relation: AtomicUsize,
    put_semantic_payload: AtomicUsize,
    get_semantic_payloads: AtomicUsize,
    point_list: AtomicUsize,
    retrieve: AtomicUsize,
    match_template: AtomicUsize,
    commit: AtomicUsize,
    flush: AtomicUsize,
    recover: AtomicUsize,
}

impl StoreCalls {
    fn reset(&self) {
        self.save_model.store(0, Ordering::SeqCst);
        self.load_model.store(0, Ordering::SeqCst);
        self.list_models.store(0, Ordering::SeqCst);
        self.delete_model.store(0, Ordering::SeqCst);
        self.resolve_points.store(0, Ordering::SeqCst);
        self.snapshot.store(0, Ordering::SeqCst);
        self.write_points.store(0, Ordering::SeqCst);
        self.read_resolved.store(0, Ordering::SeqCst);
        self.read_by_key.store(0, Ordering::SeqCst);
        self.upsert_equipment.store(0, Ordering::SeqCst);
        self.add_relation.store(0, Ordering::SeqCst);
        self.put_semantic_payload.store(0, Ordering::SeqCst);
        self.get_semantic_payloads.store(0, Ordering::SeqCst);
        self.point_list.store(0, Ordering::SeqCst);
        self.retrieve.store(0, Ordering::SeqCst);
        self.match_template.store(0, Ordering::SeqCst);
        self.commit.store(0, Ordering::SeqCst);
        self.flush.store(0, Ordering::SeqCst);
        self.recover.store(0, Ordering::SeqCst);
    }

    fn snapshot(&self) -> StoreCallSnapshot {
        StoreCallSnapshot {
            save_model: self.save_model.load(Ordering::SeqCst),
            load_model: self.load_model.load(Ordering::SeqCst),
            list_models: self.list_models.load(Ordering::SeqCst),
            delete_model: self.delete_model.load(Ordering::SeqCst),
            resolve_points: self.resolve_points.load(Ordering::SeqCst),
            snapshot: self.snapshot.load(Ordering::SeqCst),
            write_points: self.write_points.load(Ordering::SeqCst),
            read_resolved: self.read_resolved.load(Ordering::SeqCst),
            read_by_key: self.read_by_key.load(Ordering::SeqCst),
            upsert_equipment: self.upsert_equipment.load(Ordering::SeqCst),
            add_relation: self.add_relation.load(Ordering::SeqCst),
            put_semantic_payload: self.put_semantic_payload.load(Ordering::SeqCst),
            get_semantic_payloads: self.get_semantic_payloads.load(Ordering::SeqCst),
            point_list: self.point_list.load(Ordering::SeqCst),
            retrieve: self.retrieve.load(Ordering::SeqCst),
            match_template: self.match_template.load(Ordering::SeqCst),
            commit: self.commit.load(Ordering::SeqCst),
            flush: self.flush.load(Ordering::SeqCst),
            recover: self.recover.load(Ordering::SeqCst),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct StoreCallSnapshot {
    pub(crate) save_model: usize,
    pub(crate) load_model: usize,
    pub(crate) list_models: usize,
    pub(crate) delete_model: usize,
    pub(crate) resolve_points: usize,
    pub(crate) snapshot: usize,
    pub(crate) write_points: usize,
    pub(crate) read_resolved: usize,
    pub(crate) read_by_key: usize,
    pub(crate) upsert_equipment: usize,
    pub(crate) add_relation: usize,
    pub(crate) put_semantic_payload: usize,
    pub(crate) get_semantic_payloads: usize,
    pub(crate) point_list: usize,
    pub(crate) retrieve: usize,
    pub(crate) match_template: usize,
    pub(crate) commit: usize,
    pub(crate) flush: usize,
    pub(crate) recover: usize,
}

impl ModelStore for RecordingStore {
    fn save_model(&self, model: &ResolvedModel) -> StoreResult<()> {
        self.state
            .count_forbidden(&self.state.calls.save_model, "save_model");
        self.inner.save_model(model)
    }

    fn load_model(&self, model_id: &DomainKey) -> StoreResult<ResolvedModel> {
        self.state
            .count_forbidden(&self.state.calls.load_model, "load_model");
        self.inner.load_model(model_id)
    }

    fn list_models(&self) -> StoreResult<Vec<DomainKey>> {
        self.state
            .count_forbidden(&self.state.calls.list_models, "list_models");
        self.inner.list_models()
    }

    fn delete_model(&self, model_id: &DomainKey) -> StoreResult<()> {
        self.state
            .count_forbidden(&self.state.calls.delete_model, "delete_model");
        self.inner.delete_model(model_id)
    }
}

impl PointStore for RecordingStore {
    fn resolve_points(&self, keys: &[DomainKey]) -> StoreResult<Vec<PointHandle>> {
        self.state
            .count_forbidden(&self.state.calls.resolve_points, "resolve_points");
        self.inner.resolve_points(keys)
    }

    fn snapshot(&self) -> StoreResult<Box<dyn PointSnapshot>> {
        self.state.count_allowed(&self.state.calls.snapshot);
        let inner = self.inner.snapshot()?;
        Ok(Box::new(RecordingSnapshot {
            inner,
            state: Arc::clone(&self.state),
        }))
    }

    fn write_points(&self, batch: &[PointWrite]) -> StoreResult<usize> {
        self.state
            .count_forbidden(&self.state.calls.write_points, "write_points");
        self.inner.write_points(batch)
    }
}

struct RecordingSnapshot {
    inner: Box<dyn PointSnapshot>,
    state: Arc<RecordingState>,
}

impl PointSnapshot for RecordingSnapshot {
    fn read_resolved(&self, handle: PointHandle) -> Option<PointSample> {
        self.state.count_allowed(&self.state.calls.read_resolved);
        self.inner.read_resolved(handle)
    }

    fn read_by_key(&self, key: &DomainKey) -> Option<PointSample> {
        self.state
            .count_forbidden(&self.state.calls.read_by_key, "read_by_key");
        self.inner.read_by_key(key)
    }
}

impl SemanticStore for RecordingStore {
    fn upsert_equipment(&self, eq: &EquipmentDto) -> StoreResult<()> {
        self.state
            .count_forbidden(&self.state.calls.upsert_equipment, "upsert_equipment");
        self.inner.upsert_equipment(eq)
    }

    fn add_relation(&self, rel: &RelationDto) -> StoreResult<()> {
        self.state
            .count_forbidden(&self.state.calls.add_relation, "add_relation");
        self.inner.add_relation(rel)
    }

    fn put_semantic_payload(&self, p: &SemanticPayloadDto) -> StoreResult<()> {
        self.state.count_forbidden(
            &self.state.calls.put_semantic_payload,
            "put_semantic_payload",
        );
        self.inner.put_semantic_payload(p)
    }

    fn get_semantic_payloads(&self, subject: &DomainKey) -> StoreResult<Vec<SemanticPayloadDto>> {
        self.state.count_forbidden(
            &self.state.calls.get_semantic_payloads,
            "get_semantic_payloads",
        );
        self.inner.get_semantic_payloads(subject)
    }

    fn point_list(&self, controlled_device: Option<&str>) -> StoreResult<Vec<PointListRow>> {
        self.state
            .count_forbidden(&self.state.calls.point_list, "point_list");
        self.inner.point_list(controlled_device)
    }

    fn retrieve(&self, q: &SemanticQuery) -> StoreResult<Vec<RetrievalHit>> {
        self.state
            .count_forbidden(&self.state.calls.retrieve, "retrieve");
        self.inner.retrieve(q)
    }

    fn match_template(&self, required_points: &[TemplatePointReq]) -> StoreResult<Vec<DomainKey>> {
        self.state
            .count_forbidden(&self.state.calls.match_template, "match_template");
        self.inner.match_template(required_points)
    }
}

impl Durable for RecordingStore {
    fn commit(&self) -> StoreResult<()> {
        self.state
            .count_forbidden(&self.state.calls.commit, "commit");
        self.inner.commit()
    }

    fn flush(&self) -> StoreResult<()> {
        self.state.count_forbidden(&self.state.calls.flush, "flush");
        self.inner.flush()
    }

    fn recover(&self) -> StoreResult<()> {
        self.state
            .count_forbidden(&self.state.calls.recover, "recover");
        self.inner.recover()
    }
}
