//! External-style compile and behavior fixture for the public storage-port paths.

use std::sync::Arc;

use oce_api::Engine;
use oce_api::oce_store::{
    DomainKey, Durable, EquipmentDto, ModelStore, OcValue, PointHandle, PointListRow, PointSample,
    PointSnapshot, PointStatus, PointStore, PointWrite, RelationDto, ResolvedModel, RetrievalHit,
    SemanticPayloadDto, SemanticQuery, SemanticStore, Store, StoreError, StoreResult,
    TemplatePointReq,
};

#[derive(Default)]
struct ExternalAdapter;

struct ExternalSnapshot;

impl ModelStore for ExternalAdapter {
    fn save_model(&self, _model: &ResolvedModel) -> StoreResult<()> {
        Ok(())
    }

    fn load_model(&self, model_id: &DomainKey) -> StoreResult<ResolvedModel> {
        Err(StoreError::ModelNotLoaded(model_id.clone()))
    }

    fn list_models(&self) -> StoreResult<Vec<DomainKey>> {
        Ok(Vec::new())
    }

    fn delete_model(&self, _model_id: &DomainKey) -> StoreResult<()> {
        Ok(())
    }
}

impl PointStore for ExternalAdapter {
    fn resolve_points(&self, keys: &[DomainKey]) -> StoreResult<Vec<PointHandle>> {
        keys.iter()
            .enumerate()
            .map(|(index, _)| {
                u64::try_from(index)
                    .ok()
                    .and_then(|value| value.checked_add(7))
                    .map(PointHandle)
                    .ok_or_else(|| StoreError::Validation("too many fixture points".to_owned()))
            })
            .collect()
    }

    fn snapshot(&self) -> StoreResult<Box<dyn PointSnapshot>> {
        Ok(Box::new(ExternalSnapshot))
    }

    fn write_points(&self, batch: &[PointWrite]) -> StoreResult<usize> {
        Ok(batch.len())
    }
}

impl PointSnapshot for ExternalSnapshot {
    fn read_resolved(&self, handle: PointHandle) -> Option<PointSample> {
        (handle.0 == 7).then_some(PointSample {
            value: OcValue::Bool(true),
            status: PointStatus::Ok,
            at_unix_nanos: 11,
        })
    }

    fn read_by_key(&self, _key: &DomainKey) -> Option<PointSample> {
        None
    }
}

impl SemanticStore for ExternalAdapter {
    fn upsert_equipment(&self, _equipment: &EquipmentDto) -> StoreResult<()> {
        Ok(())
    }

    fn add_relation(&self, _relation: &RelationDto) -> StoreResult<()> {
        Ok(())
    }

    fn put_semantic_payload(&self, _payload: &SemanticPayloadDto) -> StoreResult<()> {
        Ok(())
    }

    fn get_semantic_payloads(&self, _subject: &DomainKey) -> StoreResult<Vec<SemanticPayloadDto>> {
        Ok(Vec::new())
    }

    fn point_list(&self, _controlled_device: Option<&str>) -> StoreResult<Vec<PointListRow>> {
        Ok(Vec::new())
    }

    fn retrieve(&self, _query: &SemanticQuery) -> StoreResult<Vec<RetrievalHit>> {
        Ok(Vec::new())
    }

    fn match_template(&self, _required: &[TemplatePointReq]) -> StoreResult<Vec<DomainKey>> {
        Ok(Vec::new())
    }
}

impl Durable for ExternalAdapter {
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

fn accepts_public_store<T: Store>(_store: &T) {}

#[test]
fn external_adapter_uses_only_supported_public_paths() {
    let key = DomainKey::new("urn:fixture:point");
    assert_eq!(key.as_str(), "urn:fixture:point");

    let adapter = Arc::new(ExternalAdapter);
    let handles = adapter
        .resolve_points(std::slice::from_ref(&key))
        .expect("external adapter resolves its semantic key");
    assert_eq!(handles, vec![PointHandle(7)]);
    let sample = adapter
        .snapshot()
        .expect("external adapter snapshots")
        .read_resolved(handles[0])
        .expect("external adapter consumes its handle");
    assert_eq!(sample.value, OcValue::Bool(true));

    let engine = Engine::with_store(Arc::clone(&adapter));
    accepts_public_store(engine.store());
    assert!(std::ptr::eq(engine.store(), adapter.as_ref()));
}
