//! External-style compile and behavior fixture for the public storage-port paths.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use oce_api::oce_store::{
    DomainKey, Durable, EquipmentDto, ModelStore, OcValue, PointHandle, PointListRow, PointSample,
    PointSnapshot, PointStatus, PointStore, PointType, PointWrite, RelationDto, ResolvedModel,
    RetrievalHit, SemanticPayloadDto, SemanticQuery, SemanticStore, Store, StoreError, StoreResult,
    TemplatePointReq, TrendInterval,
};
use oce_api::{Engine, OcError, Value};

#[derive(Default)]
struct ExternalAdapter {
    calls: AtomicUsize,
}

impl ExternalAdapter {
    fn called(&self) {
        self.calls.fetch_add(1, Ordering::SeqCst);
    }
}

struct ExternalSnapshot;

impl ModelStore for ExternalAdapter {
    fn save_model(&self, _model: &ResolvedModel) -> StoreResult<()> {
        self.called();
        Ok(())
    }

    fn load_model(&self, model_id: &DomainKey) -> StoreResult<ResolvedModel> {
        self.called();
        Err(StoreError::ModelNotLoaded(model_id.clone()))
    }

    fn list_models(&self) -> StoreResult<Vec<DomainKey>> {
        self.called();
        Ok(Vec::new())
    }

    fn delete_model(&self, _model_id: &DomainKey) -> StoreResult<()> {
        self.called();
        Ok(())
    }
}

impl PointStore for ExternalAdapter {
    fn resolve_points(&self, keys: &[DomainKey]) -> StoreResult<Vec<PointHandle>> {
        self.called();
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
        self.called();
        Ok(Box::new(ExternalSnapshot))
    }

    fn write_points(&self, batch: &[PointWrite]) -> StoreResult<usize> {
        self.called();
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
        self.called();
        Ok(())
    }

    fn add_relation(&self, _relation: &RelationDto) -> StoreResult<()> {
        self.called();
        Ok(())
    }

    fn put_semantic_payload(&self, _payload: &SemanticPayloadDto) -> StoreResult<()> {
        self.called();
        Ok(())
    }

    fn get_semantic_payloads(&self, _subject: &DomainKey) -> StoreResult<Vec<SemanticPayloadDto>> {
        self.called();
        Ok(Vec::new())
    }

    fn point_list(&self, controlled_device: Option<&str>) -> StoreResult<Vec<PointListRow>> {
        self.called();
        Ok(vec![PointListRow {
            controlled_device: controlled_device.map(str::to_owned),
            point: DomainKey::new("urn:external:point"),
            name: "external".to_owned(),
            point_type: PointType::Ai,
            hardwired: true,
            trend_interval_s: TrendInterval::OnChange,
            description: None,
        }])
    }

    fn retrieve(&self, _query: &SemanticQuery) -> StoreResult<Vec<RetrievalHit>> {
        self.called();
        Ok(Vec::new())
    }

    fn match_template(&self, _required: &[TemplatePointReq]) -> StoreResult<Vec<DomainKey>> {
        self.called();
        Ok(Vec::new())
    }
}

impl Durable for ExternalAdapter {
    fn commit(&self) -> StoreResult<()> {
        self.called();
        Ok(())
    }

    fn flush(&self) -> StoreResult<()> {
        self.called();
        Ok(())
    }

    fn recover(&self) -> StoreResult<()> {
        self.called();
        Ok(())
    }
}

fn accepts_public_store<T: Store>(_store: &T) {}

#[test]
fn external_adapter_uses_only_supported_public_paths() {
    let key = DomainKey::new("urn:fixture:point");
    assert_eq!(key.as_str(), "urn:fixture:point");

    let adapter = Arc::new(ExternalAdapter::default());
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

#[test]
fn filtered_inventory_refuses_without_store_calls_or_engine_mutation() {
    let adapter = Arc::new(ExternalAdapter::default());
    // Positive capability control: this is not MemStore's unsupported filtered query.
    let external = adapter.point_list(Some("AHU-1")).unwrap();
    assert_eq!(external.len(), 1);
    assert_eq!(external[0].point.as_str(), "urn:external:point");
    assert_eq!(external[0].controlled_device.as_deref(), Some("AHU-1"));
    assert_eq!(adapter.calls.load(Ordering::SeqCst), 1);
    let mut engine = Engine::with_store(Arc::clone(&adapter));
    engine
        .load_cxf(include_bytes!("fixtures/assertion_model.jsonld"))
        .unwrap();
    engine
        .set_input("urn:assert#u", Value::Boolean(true))
        .unwrap();
    engine.tick(2.0).unwrap();
    let before = engine.state_snapshot().unwrap();
    let outputs = engine.outputs().to_map();
    adapter.calls.store(0, Ordering::SeqCst);
    for _ in 0..3 {
        for device in ["AHU-1", "", "unknown", "\0\n设备"] {
            assert!(matches!(
                engine.point_list(Some(device)),
                Err(OcError::Load { .. })
            ));
        }
        let own = engine.point_list(None).unwrap();
        // One declared boundary input fans out to both sinks; it remains one host point.
        assert_eq!(
            own.iter().map(|p| p.path.as_str()).collect::<Vec<_>>(),
            ["urn:assert#u", "urn:assert#invert.y",]
        );
        assert_eq!(
            engine.state_snapshot().unwrap().as_bytes(),
            before.as_bytes()
        );
        for ((path, value), (old_path, old_value)) in engine.outputs().to_map().iter().zip(&outputs)
        {
            assert_eq!(path, old_path);
            assert!(value.bit_eq(old_value));
        }
        assert_eq!(
            adapter.calls.load(Ordering::SeqCst),
            0,
            "no store method is consulted"
        );
    }
    assert!(
        matches!(engine.tick(1.0), Err(OcError::TimeRegression { now, prev })
        if now.to_bits() == 1.0_f64.to_bits() && prev.to_bits() == 2.0_f64.to_bits())
    );
}
