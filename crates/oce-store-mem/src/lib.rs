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
//! Status: **M0.** The model round-trip ([`oce_store::ModelStore`]), runtime point path
//! ([`oce_store::PointStore`]), and the no-op durability hooks ([`oce_store::Durable`]) are
//! implemented. The semantic-graph methods ([`oce_store::SemanticStore`]) remain M1 scaffold.

use std::collections::HashMap;
use std::sync::RwLock;

use oce_store::{
    DomainKey, Durable, EquipmentDto, ModelStore, PointHandle, PointListRow, PointSample,
    PointSnapshot, PointStore, PointWrite, RelationDto, ResolvedModel, RetrievalHit,
    SemanticPayloadDto, SemanticQuery, SemanticStore, StoreError, StoreResult, TemplatePointReq,
};

/// The default in-memory store. Constructed with [`MemStore::default`]; no filesystem touch.
#[derive(Default)]
pub struct MemStore {
    models: RwLock<HashMap<DomainKey, ResolvedModel>>,
    points: RwLock<MemPointState>,
    equipment: RwLock<HashMap<DomainKey, EquipmentDto>>,
    relations: RwLock<Vec<RelationDto>>,
    payloads: RwLock<HashMap<DomainKey, Vec<SemanticPayloadDto>>>,
}

/// Runtime point state: a dense `by_handle` array (the handle is its index) plus the
/// `domain_key → handle` map used to resolve keys and to write by key.
#[derive(Clone, Default)]
struct MemPointState {
    by_handle: Vec<Option<PointSample>>,
    by_key: HashMap<DomainKey, u64>,
}

impl MemPointState {
    /// Return the handle for `key`, allocating a fresh (empty) slot on first sight.
    fn handle_for(&mut self, key: &DomainKey) -> u64 {
        if let Some(&idx) = self.by_key.get(key) {
            return idx;
        }
        let idx = self.by_handle.len() as u64;
        self.by_handle.push(None);
        self.by_key.insert(key.clone(), idx);
        idx
    }
}

impl MemStore {
    /// Construct an empty in-memory store (no DB, no IO).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

fn backend_err(e: impl std::fmt::Display) -> StoreError {
    StoreError::Backend(e.to_string())
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
    fn save_model(&self, model: &ResolvedModel) -> StoreResult<()> {
        self.models
            .write()
            .map_err(backend_err)?
            .insert(model.model_id.clone(), model.clone());
        Ok(())
    }
    fn load_model(&self, model_id: &DomainKey) -> StoreResult<ResolvedModel> {
        self.models
            .read()
            .map_err(backend_err)?
            .get(model_id)
            .cloned()
            .ok_or_else(|| StoreError::ModelNotLoaded(model_id.clone()))
    }
    fn list_models(&self) -> StoreResult<Vec<DomainKey>> {
        Ok(self
            .models
            .read()
            .map_err(backend_err)?
            .keys()
            .cloned()
            .collect())
    }
    fn delete_model(&self, model_id: &DomainKey) -> StoreResult<()> {
        // Idempotent: dropping an absent model is a no-op success.
        self.models.write().map_err(backend_err)?.remove(model_id);
        Ok(())
    }
}

impl PointStore for MemStore {
    fn resolve_points(&self, keys: &[DomainKey]) -> StoreResult<Vec<PointHandle>> {
        let mut state = self.points.write().map_err(backend_err)?;
        Ok(keys
            .iter()
            .map(|k| PointHandle(state.handle_for(k)))
            .collect())
    }
    fn snapshot(&self) -> StoreResult<Box<dyn PointSnapshot>> {
        let state = self.points.read().map_err(backend_err)?;
        Ok(Box::new(MemSnapshot {
            by_handle: state.by_handle.clone(),
            by_key: state.by_key.clone(),
        }))
    }
    fn write_points(&self, batch: &[PointWrite]) -> StoreResult<usize> {
        // The in-memory store offers no crash durability (§5 R-6): it accepts every write into the
        // live map regardless of `Durability` intent, and returns the count accepted.
        let mut state = self.points.write().map_err(backend_err)?;
        for w in batch {
            let idx = state.handle_for(&w.key) as usize;
            state.by_handle[idx] = Some(w.sample.clone());
        }
        Ok(batch.len())
    }
}

impl SemanticStore for MemStore {
    fn upsert_equipment(&self, _eq: &EquipmentDto) -> StoreResult<()> {
        let _ = &self.equipment;
        unimplemented!("MemStore::upsert_equipment — M1 semantic-graph scaffold")
    }
    fn add_relation(&self, _rel: &RelationDto) -> StoreResult<()> {
        let _ = &self.relations;
        unimplemented!("MemStore::add_relation — M1 semantic-graph scaffold")
    }
    fn put_semantic_payload(&self, _p: &SemanticPayloadDto) -> StoreResult<()> {
        let _ = &self.payloads;
        unimplemented!("MemStore::put_semantic_payload — M1 semantic-graph scaffold")
    }
    fn get_semantic_payloads(&self, _subject: &DomainKey) -> StoreResult<Vec<SemanticPayloadDto>> {
        unimplemented!("MemStore::get_semantic_payloads — M1 semantic-graph scaffold")
    }
    fn point_list(&self, _controlled_device: Option<&str>) -> StoreResult<Vec<PointListRow>> {
        unimplemented!("MemStore::point_list — M1 semantic-graph scaffold")
    }
    fn retrieve(&self, _q: &SemanticQuery) -> StoreResult<Vec<RetrievalHit>> {
        unimplemented!("MemStore::retrieve — M1 semantic-graph scaffold")
    }
    fn match_template(&self, _required_points: &[TemplatePointReq]) -> StoreResult<Vec<DomainKey>> {
        unimplemented!("MemStore::match_template — M1 semantic-graph scaffold")
    }
}

/// No-op durability (`06` §4.1 / §5 R-6): the in-memory store keeps everything live in process and
/// provides **no** crash durability, so all three hooks succeed with nothing to do.
impl Durable for MemStore {
    fn commit(&self) -> StoreResult<()> {
        Ok(())
    }
    fn flush(&self) -> StoreResult<()> {
        Ok(())
    }
    fn recover(&self) -> StoreResult<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oce_store::{Durability, OcValue, PointStatus, Store};

    fn model(id: &str) -> ResolvedModel {
        ResolvedModel {
            model_id: DomainKey::new(id),
            schema_rev: 1,
            classes: Vec::new(),
            blocks: Vec::new(),
            points: Vec::new(),
            connections: Vec::new(),
            containment: Vec::new(),
        }
    }

    fn sample(v: f64) -> PointSample {
        PointSample {
            value: OcValue::Real(v),
            status: PointStatus::Ok,
            at_unix_nanos: 42,
        }
    }

    /// Compile-time proof MemStore satisfies the full umbrella (incl. the new `Durable` bound).
    fn _assert_is_store<S: Store>() {}

    #[test]
    fn mem_store_is_a_full_store() {
        _assert_is_store::<MemStore>();
    }

    #[test]
    fn model_round_trip_and_idempotent_delete() {
        let store = MemStore::new();
        let m = model("seq:ahu1");
        store.save_model(&m).unwrap();
        assert_eq!(store.load_model(&m.model_id).unwrap().model_id, m.model_id);
        assert_eq!(store.list_models().unwrap(), vec![m.model_id.clone()]);

        store.delete_model(&m.model_id).unwrap();
        assert!(matches!(
            store.load_model(&m.model_id),
            Err(StoreError::ModelNotLoaded(_))
        ));
        // Deleting again is a no-op success (idempotent).
        store.delete_model(&m.model_id).unwrap();
        assert!(store.list_models().unwrap().is_empty());
    }

    #[test]
    fn point_resolve_write_and_snapshot_read() {
        let store = MemStore::new();
        let k0 = DomainKey::new("pt:a");
        let k1 = DomainKey::new("pt:b");
        let handles = store.resolve_points(&[k0.clone(), k1.clone()]).unwrap();
        assert_eq!(handles, vec![PointHandle(0), PointHandle(1)]);
        // Resolving again returns the same handles (stable).
        assert_eq!(
            store.resolve_points(std::slice::from_ref(&k1)).unwrap(),
            vec![PointHandle(1)]
        );

        let written = store
            .write_points(&[
                PointWrite {
                    key: k0.clone(),
                    sample: sample(1.5),
                    durability: Durability::Critical,
                },
                PointWrite {
                    key: k1.clone(),
                    sample: sample(2.5),
                    durability: Durability::Telemetry,
                },
            ])
            .unwrap();
        assert_eq!(written, 2);

        let snap = store.snapshot().unwrap();
        assert_eq!(
            snap.read_resolved(PointHandle(0)).unwrap().value,
            OcValue::Real(1.5)
        );
        assert_eq!(snap.read_by_key(&k1).unwrap().value, OcValue::Real(2.5));
        assert!(snap.read_resolved(PointHandle(99)).is_none());
    }

    #[test]
    fn write_points_auto_resolves_unknown_keys() {
        let store = MemStore::new();
        let k = DomainKey::new("pt:new");
        let n = store
            .write_points(&[PointWrite {
                key: k.clone(),
                sample: sample(7.0),
                durability: Durability::Critical,
            }])
            .unwrap();
        assert_eq!(n, 1);
        assert_eq!(
            store.snapshot().unwrap().read_by_key(&k).unwrap().value,
            OcValue::Real(7.0)
        );
    }

    #[test]
    fn durability_hooks_are_noops() {
        let store = MemStore::new();
        store.recover().unwrap();
        store.commit().unwrap();
        store.flush().unwrap();
    }
}
