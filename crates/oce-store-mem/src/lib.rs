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
//!
//! The model round-trip ([`oce_store::ModelStore`]), runtime point path
//! ([`oce_store::PointStore`]), and no-op durability hooks ([`oce_store::Durable`]) are implemented
//! with non-panicking bodies. Semantic graph operations are deliberate typed deferrals: they return
//! [`oce_store::StoreError::Unsupported`] or [`oce_store::StoreError::RetrievalUnsupported`], never
//! silent success.

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

const SEMANTIC_GRAPH_DEFERRED: &str = "MemStore: semantic graph operations deferred to M3";
const SEMANTIC_RETRIEVAL_DEFERRED: &str = "MemStore: semantic retrieval deferred to M3";

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
        Err(StoreError::Unsupported(SEMANTIC_GRAPH_DEFERRED))
    }
    fn add_relation(&self, _rel: &RelationDto) -> StoreResult<()> {
        Err(StoreError::Unsupported(SEMANTIC_GRAPH_DEFERRED))
    }
    fn put_semantic_payload(&self, _p: &SemanticPayloadDto) -> StoreResult<()> {
        Err(StoreError::Unsupported(SEMANTIC_GRAPH_DEFERRED))
    }
    fn get_semantic_payloads(&self, _subject: &DomainKey) -> StoreResult<Vec<SemanticPayloadDto>> {
        Err(StoreError::Unsupported(SEMANTIC_GRAPH_DEFERRED))
    }
    fn point_list(&self, _controlled_device: Option<&str>) -> StoreResult<Vec<PointListRow>> {
        Err(StoreError::Unsupported(SEMANTIC_GRAPH_DEFERRED))
    }
    fn retrieve(&self, _q: &SemanticQuery) -> StoreResult<Vec<RetrievalHit>> {
        Err(StoreError::RetrievalUnsupported(
            SEMANTIC_RETRIEVAL_DEFERRED,
        ))
    }
    fn match_template(&self, _required_points: &[TemplatePointReq]) -> StoreResult<Vec<DomainKey>> {
        Err(StoreError::Unsupported(SEMANTIC_GRAPH_DEFERRED))
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
    use oce_store::{
        Durability, OcValue, PointDirection, PointStatus, PointValueType, RelationKind, Store,
    };

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

    #[test]
    fn semantic_store_methods_return_typed_deferral_errors() {
        let store = MemStore::new();
        let equipment = EquipmentDto {
            key: DomainKey::new("equip:ahu"),
            subtype: Some("AHU".to_owned()),
            tags_json: "{}".to_owned(),
            embedding: None,
        };
        let upsert_err = store.upsert_equipment(&equipment).unwrap_err();
        assert_eq!(
            upsert_err.to_string(),
            "operation unsupported by this backend: MemStore: semantic graph operations deferred to M3"
        );
        assert!(matches!(
            upsert_err,
            StoreError::Unsupported(SEMANTIC_GRAPH_DEFERRED)
        ));

        let relation = RelationDto {
            src: DomainKey::new("equip:ahu"),
            dst: DomainKey::new("point:sat"),
            kind: RelationKind::HasPoint,
        };
        assert!(matches!(
            store.add_relation(&relation),
            Err(StoreError::Unsupported(SEMANTIC_GRAPH_DEFERRED))
        ));

        let payload = SemanticPayloadDto {
            subject: DomainKey::new("equip:ahu"),
            language: "Brick".to_owned(),
            mime: "application/ld+json".to_owned(),
            version: Some("1.3".to_owned()),
            payload: "{}".to_owned(),
        };
        assert!(matches!(
            store.put_semantic_payload(&payload),
            Err(StoreError::Unsupported(SEMANTIC_GRAPH_DEFERRED))
        ));
        assert!(matches!(
            store.get_semantic_payloads(&payload.subject),
            Err(StoreError::Unsupported(SEMANTIC_GRAPH_DEFERRED))
        ));
        assert!(matches!(
            store.point_list(Some("ahu")),
            Err(StoreError::Unsupported(SEMANTIC_GRAPH_DEFERRED))
        ));

        let query = SemanticQuery::FuzzyText {
            query: "supply air temperature".to_owned(),
            k: 3,
        };
        assert!(matches!(
            store.retrieve(&query),
            Err(StoreError::RetrievalUnsupported(
                SEMANTIC_RETRIEVAL_DEFERRED
            ))
        ));

        let required = [TemplatePointReq {
            quantity: Some("Temperature".to_owned()),
            value_type: PointValueType::Real,
            direction: PointDirection::Out,
        }];
        assert!(matches!(
            store.match_template(&required),
            Err(StoreError::Unsupported(SEMANTIC_GRAPH_DEFERRED))
        ));
    }
}
