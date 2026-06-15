#![forbid(unsafe_code)]
//! `oce-store-mem` — the **default** in-memory [`oce_store::Store`] backend for the Open Control
//! Engine, so the engine runs with no database (FRAME D-OWNER-1).
//!
//! It is database-free (R-SEAM-1), depends only on `oce-store` + std, and is the reference
//! implementation used in unit tests and fuzzing. It is **not** a database: no WAL, no fsync, no
//! ANN index — an in-process `HashMap`/`Vec` projection that satisfies the trait contracts just
//! enough that the engine's load → flatten → validate → schedule → tick → simulate loop works
//! end to end. It mirrors a durable adapter's *read discipline* (acquire one snapshot per tick,
//! O(1) reads), not its cost, so swapping `MemStore` for any other `Store` changes no engine code.
//!
//! Status: **M0 scaffold.** The storage maps are wired; method bodies are stubs
//! (`unimplemented!()`) and land in M0/M1.

use std::collections::HashMap;
use std::sync::{Mutex, RwLock};

use oce_store::{
    DomainKey, EquipmentDto, ModelStore, PointHandle, PointListRow, PointSample, PointSnapshot,
    PointStore, PointWrite, RelationDto, ResolvedModel, RetrievalHit, SemanticPayloadDto,
    SemanticQuery, SemanticStore, StoreError, StoreResult, TemplatePointReq,
};

/// The default in-memory store. Constructed with [`MemStore::default`]; no filesystem touch.
#[derive(Default)]
pub struct MemStore {
    models: RwLock<HashMap<DomainKey, ResolvedModel>>,
    points: RwLock<MemPointState>,
    point_write_lock: Mutex<()>,
    handle_map: RwLock<HashMap<DomainKey, u64>>,
    equipment: RwLock<HashMap<DomainKey, EquipmentDto>>,
    relations: RwLock<Vec<RelationDto>>,
    payloads: RwLock<HashMap<DomainKey, Vec<SemanticPayloadDto>>>,
}

#[derive(Clone, Default)]
struct MemPointState {
    by_handle: Vec<Option<PointSample>>,
    by_key: HashMap<DomainKey, u64>,
}

impl MemStore {
    /// Construct an empty in-memory store (no DB, no IO).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// An immutable point-state snapshot handed to the hot read path.
struct MemSnapshot {
    by_handle: Vec<Option<PointSample>>,
    by_key: HashMap<DomainKey, u64>,
}

impl PointSnapshot for MemSnapshot {
    fn read_resolved(&self, handle: PointHandle) -> Option<PointSample> {
        self.by_handle.get(handle.0 as usize).cloned().flatten()
    }
    fn read_by_key(&self, key: &DomainKey) -> Option<PointSample> {
        let idx = *self.by_key.get(key)?;
        self.by_handle.get(idx as usize).cloned().flatten()
    }
}

impl ModelStore for MemStore {
    fn save_model(&self, _model: &ResolvedModel) -> StoreResult<()> {
        let _ = &self.models;
        unimplemented!("MemStore::save_model — M0 scaffold")
    }
    fn load_model(&self, model_id: &DomainKey) -> StoreResult<ResolvedModel> {
        self.models
            .read()
            .map_err(|e| StoreError::Backend(e.to_string()))?
            .get(model_id)
            .cloned()
            .ok_or_else(|| StoreError::ModelNotLoaded(model_id.clone()))
    }
    fn list_models(&self) -> StoreResult<Vec<DomainKey>> {
        Ok(self
            .models
            .read()
            .map_err(|e| StoreError::Backend(e.to_string()))?
            .keys()
            .cloned()
            .collect())
    }
    fn delete_model(&self, _model_id: &DomainKey) -> StoreResult<()> {
        unimplemented!("MemStore::delete_model — M0 scaffold")
    }
}

impl PointStore for MemStore {
    fn resolve_points(&self, _keys: &[DomainKey]) -> StoreResult<Vec<PointHandle>> {
        let _ = (&self.handle_map, &self.point_write_lock);
        unimplemented!("MemStore::resolve_points — M0 scaffold")
    }
    fn snapshot(&self) -> StoreResult<Box<dyn PointSnapshot>> {
        let state = self
            .points
            .read()
            .map_err(|e| StoreError::Backend(e.to_string()))?;
        Ok(Box::new(MemSnapshot {
            by_handle: state.by_handle.clone(),
            by_key: state.by_key.clone(),
        }))
    }
    fn write_points(&self, _batch: &[PointWrite]) -> StoreResult<usize> {
        unimplemented!("MemStore::write_points — M0 scaffold")
    }
}

impl SemanticStore for MemStore {
    fn upsert_equipment(&self, _eq: &EquipmentDto) -> StoreResult<()> {
        let _ = &self.equipment;
        unimplemented!("MemStore::upsert_equipment — M0 scaffold")
    }
    fn add_relation(&self, _rel: &RelationDto) -> StoreResult<()> {
        let _ = &self.relations;
        unimplemented!("MemStore::add_relation — M0 scaffold")
    }
    fn put_semantic_payload(&self, _p: &SemanticPayloadDto) -> StoreResult<()> {
        let _ = &self.payloads;
        unimplemented!("MemStore::put_semantic_payload — M0 scaffold")
    }
    fn get_semantic_payloads(&self, _subject: &DomainKey) -> StoreResult<Vec<SemanticPayloadDto>> {
        unimplemented!("MemStore::get_semantic_payloads — M0 scaffold")
    }
    fn point_list(&self, _controlled_device: Option<&str>) -> StoreResult<Vec<PointListRow>> {
        unimplemented!("MemStore::point_list — M0 scaffold")
    }
    fn retrieve(&self, _q: &SemanticQuery) -> StoreResult<Vec<RetrievalHit>> {
        unimplemented!("MemStore::retrieve — M0 scaffold")
    }
    fn match_template(&self, _required_points: &[TemplatePointReq]) -> StoreResult<Vec<DomainKey>> {
        unimplemented!("MemStore::match_template — M0 scaffold")
    }
}
