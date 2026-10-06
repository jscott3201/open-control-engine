//! Unit tests for engine load helpers and preserved load-time Store refusal boundaries.

use oce_model::{BlockId, Connector, ConnectorId, Dir, Value, ValueType};
use oce_store::{
    Durable, EquipmentDto, ModelStore, PointHandle, PointListRow, PointSnapshot, PointStore,
    PointWrite, RelationDto, ResolvedModel, RetrievalHit, SemanticPayloadDto, SemanticQuery,
    SemanticStore, StoreResult, TemplatePointReq,
};

use super::*;

const PATH: &str = "test:input";
const ANALOG_WARNING: &str = include_str!(
    "../../oce-cxf/tests/fixtures/composite_contract/warned/analog_coerced_member.jsonld"
);

#[test]
fn load_validation_rejects_mismatched_handle_count() {
    let store = LoadFailureStore::default();
    let mut model = ModelGraph::new();
    model.connectors.push(
        Connector::new(ConnectorId(0), BlockId(0), Dir::In, ValueType::Real, 0).with_iri(PATH),
    );
    let io = IoInventory::build_at_load(&model);

    let err = validate_store_inputs(&store, &io).unwrap_err();
    match err {
        OcError::Store(StoreError::Validation(detail)) => {
            assert!(detail.contains("0 handles for 1 input points"));
        }
        other => panic!("expected StoreError::Validation, got {other:?}"),
    }
}

#[test]
fn warning_context_survives_each_later_store_failure() {
    for (failure, expected) in [
        (StoreFailure::Recover, "injected recover failure"),
        (StoreFailure::SaveModel, "injected save_model failure"),
        (
            StoreFailure::ResolvePoints,
            "injected resolve_points failure",
        ),
        (StoreFailure::HandleCount, "0 handles for 1 input points"),
    ] {
        let mut engine = Engine::with_store(Arc::new(LoadFailureStore::new(failure)));
        let error = engine
            .load_cxf(ANALOG_WARNING.as_bytes())
            .expect_err("the injected store failure must refuse the load");
        assert!(matches!(error, OcError::LoadContext(_)));
        let source = std::error::Error::source(&error)
            .and_then(|source| source.downcast_ref::<OcError>())
            .expect("terminal OcError source");
        assert!(matches!(source, OcError::Store(_)), "{source:?}");
        assert!(source.to_string().contains(expected), "{source}");
        assert!(error.diagnostics().is_empty());
        assert_eq!(error.all_diagnostics().count(), 1);
        assert_eq!(
            error.all_diagnostics().next().expect("prior warning").code,
            oce_diag::DiagCode::AnalogCoercedToReal
        );
        assert!(!engine.loaded);
        assert!(engine.model.blocks.is_empty());
    }
}

#[derive(Clone, Copy, Default)]
enum StoreFailure {
    None,
    Recover,
    SaveModel,
    ResolvePoints,
    #[default]
    HandleCount,
}

#[derive(Default)]
struct LoadFailureStore {
    inner: MemStore,
    failure: std::sync::Mutex<StoreFailure>,
    calls: std::sync::Mutex<Vec<&'static str>>,
    resolved: std::sync::Mutex<Vec<PointHandle>>,
}

impl LoadFailureStore {
    fn new(failure: StoreFailure) -> Self {
        Self {
            failure: std::sync::Mutex::new(failure),
            ..Self::default()
        }
    }
}

impl ModelStore for LoadFailureStore {
    fn save_model(&self, model: &ResolvedModel) -> StoreResult<()> {
        self.calls.lock().unwrap().push("save_model");
        if matches!(*self.failure.lock().unwrap(), StoreFailure::SaveModel) {
            return Err(StoreError::Backend(
                "injected save_model failure".to_owned(),
            ));
        }
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

impl PointStore for LoadFailureStore {
    fn resolve_points(&self, keys: &[DomainKey]) -> StoreResult<Vec<PointHandle>> {
        self.calls.lock().unwrap().push("resolve_points");
        if matches!(*self.failure.lock().unwrap(), StoreFailure::ResolvePoints) {
            return Err(StoreError::Backend(
                "injected resolve_points failure".to_owned(),
            ));
        }
        let handles = self.inner.resolve_points(keys)?;
        *self.resolved.lock().unwrap() = handles.clone();
        if matches!(*self.failure.lock().unwrap(), StoreFailure::HandleCount) {
            Ok(Vec::new())
        } else {
            Ok(handles)
        }
    }

    fn snapshot(&self) -> StoreResult<Box<dyn PointSnapshot>> {
        self.inner.snapshot()
    }

    fn write_points(&self, batch: &[PointWrite]) -> StoreResult<usize> {
        self.inner.write_points(batch)
    }
}

impl SemanticStore for LoadFailureStore {
    fn upsert_equipment(&self, eq: &EquipmentDto) -> StoreResult<()> {
        self.inner.upsert_equipment(eq)
    }

    fn add_relation(&self, rel: &RelationDto) -> StoreResult<()> {
        self.inner.add_relation(rel)
    }

    fn put_semantic_payload(&self, p: &SemanticPayloadDto) -> StoreResult<()> {
        self.inner.put_semantic_payload(p)
    }

    fn get_semantic_payloads(&self, subject: &DomainKey) -> StoreResult<Vec<SemanticPayloadDto>> {
        self.inner.get_semantic_payloads(subject)
    }

    fn point_list(&self, controlled_device: Option<&str>) -> StoreResult<Vec<PointListRow>> {
        self.inner.point_list(controlled_device)
    }

    fn retrieve(&self, q: &SemanticQuery) -> StoreResult<Vec<RetrievalHit>> {
        self.inner.retrieve(q)
    }

    fn match_template(&self, required_points: &[TemplatePointReq]) -> StoreResult<Vec<DomainKey>> {
        self.inner.match_template(required_points)
    }
}

impl Durable for LoadFailureStore {
    fn commit(&self) -> StoreResult<()> {
        self.inner.commit()
    }

    fn flush(&self) -> StoreResult<()> {
        self.inner.flush()
    }

    fn recover(&self) -> StoreResult<()> {
        self.calls.lock().unwrap().push("recover");
        if matches!(*self.failure.lock().unwrap(), StoreFailure::Recover) {
            return Err(StoreError::Backend("injected recover failure".to_owned()));
        }
        self.inner.recover()
    }
}

#[path = "tests/reload_tests.rs"]
mod reload;

#[test]
fn receipt_preserves_prior_evidence_at_each_store_boundary() {
    use crate::DiagnosticStage;
    for (failure, stage) in [
        (StoreFailure::Recover, DiagnosticStage::StoreRecovery),
        (StoreFailure::SaveModel, DiagnosticStage::StoreSave),
        (StoreFailure::ResolvePoints, DiagnosticStage::StoreInputs),
        (StoreFailure::HandleCount, DiagnosticStage::StoreInputs),
    ] {
        let mut engine = Engine::with_store(Arc::new(LoadFailureStore::new(failure)));
        let failure = engine
            .load_cxf_with_receipt(ANALOG_WARNING.as_bytes())
            .unwrap_err();
        assert_eq!(failure.stage(), stage);
        assert!(matches!(failure.error(), OcError::LoadContext(_)));
        let terminal = std::error::Error::source(failure.error())
            .unwrap()
            .downcast_ref::<OcError>()
            .unwrap();
        assert!(matches!(terminal, OcError::Store(_)));
        assert!(failure.error().diagnostics().is_empty());
        let records = failure.diagnostics().records();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].key().stage(), DiagnosticStage::Import);
        assert_eq!(records[0].key().code(), "analog-coerced-to-real");
        assert!(!engine.loaded);
        assert!(engine.model.blocks.is_empty());
    }
}
