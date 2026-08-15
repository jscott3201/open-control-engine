//! Unit tests for the engine load helpers and store-input conversion seam.

use oce_model::{BlockId, Connector, EnumClassId};
use oce_store::{
    Durable, EquipmentDto, ModelStore, PointListRow, PointStatus, PointStore, PointWrite,
    RelationDto, ResolvedModel, RetrievalHit, SemanticPayloadDto, SemanticQuery, SemanticStore,
    StoreResult, TemplatePointReq,
};

use super::*;

const PATH: &str = "test:input";
const ANALOG_WARNING: &str = include_str!(
    "../../oce-cxf/tests/fixtures/composite_contract/warned/analog_coerced_member.jsonld"
);

fn sample(value: OcValue) -> PointSample {
    PointSample {
        value,
        status: PointStatus::Fault,
        at_unix_nanos: 42,
    }
}

fn assert_input_type(result: Result<Value, OcError>) {
    match result {
        Err(OcError::InputType(path)) => assert_eq!(path, PATH),
        other => panic!("expected InputType for {PATH}, got {other:?}"),
    }
}

#[test]
fn sample_to_value_accepts_native_signal_carriers_and_string_helper_path() {
    match sample_to_value(sample(OcValue::Real(1.25)), ValueType::Real, PATH).unwrap() {
        Value::Real(v) => assert_eq!(v.to_bits(), 1.25f64.to_bits()),
        other => panic!("expected real value, got {other:?}"),
    }
    match sample_to_value(sample(OcValue::Int(7)), ValueType::Integer, PATH).unwrap() {
        Value::Integer(v) => assert_eq!(v, 7),
        other => panic!("expected integer value, got {other:?}"),
    }
    match sample_to_value(sample(OcValue::Bool(true)), ValueType::Boolean, PATH).unwrap() {
        Value::Boolean(v) => assert!(v),
        other => panic!("expected boolean value, got {other:?}"),
    }
    match sample_to_value(
        sample(OcValue::String("metadata".to_owned())),
        ValueType::String,
        PATH,
    )
    .unwrap()
    {
        Value::String(v) => assert_eq!(&*v, "metadata"),
        other => panic!("expected string value, got {other:?}"),
    }
}

#[test]
fn sample_to_value_accepts_positive_integer_enum_ordinals() {
    let class = EnumClassId(9);
    match sample_to_value(sample(OcValue::Int(3)), ValueType::Enum(class), PATH).unwrap() {
        Value::Enum {
            class: actual_class,
            ordinal,
        } => {
            assert_eq!(actual_class, class);
            assert_eq!(ordinal, 3);
        }
        other => panic!("expected enum value, got {other:?}"),
    }
}

#[test]
fn sample_to_value_rejects_invalid_integer_enum_ordinals() {
    let class = EnumClassId(9);
    for ordinal in [0, -1, i64::from(u32::MAX) + 1] {
        assert_input_type(sample_to_value(
            sample(OcValue::Int(ordinal)),
            ValueType::Enum(class),
            PATH,
        ));
    }
}

#[test]
fn sample_to_value_rejects_type_mismatch_pairs() {
    let class = EnumClassId(4);
    let cases = [
        (OcValue::Real(1.0), ValueType::Integer),
        (OcValue::Real(1.0), ValueType::Boolean),
        (OcValue::Real(1.0), ValueType::String),
        (OcValue::Real(1.0), ValueType::Enum(class)),
        (OcValue::Int(1), ValueType::Real),
        (OcValue::Int(1), ValueType::Boolean),
        (OcValue::Int(1), ValueType::String),
        (OcValue::Bool(true), ValueType::Real),
        (OcValue::Bool(true), ValueType::Integer),
        (OcValue::Bool(true), ValueType::String),
        (OcValue::Bool(true), ValueType::Enum(class)),
        (OcValue::String("value".to_owned()), ValueType::Real),
        (OcValue::String("value".to_owned()), ValueType::Integer),
        (OcValue::String("value".to_owned()), ValueType::Boolean),
        (OcValue::String("value".to_owned()), ValueType::Enum(class)),
    ];

    for (value, want) in cases {
        assert_input_type(sample_to_value(sample(value), want, PATH));
    }
}

#[test]
fn sample_to_value_rejects_native_enum_and_decimal_store_carriers() {
    let class = EnumClassId(4);
    let wants = [
        ValueType::Real,
        ValueType::Integer,
        ValueType::Boolean,
        ValueType::String,
        ValueType::Enum(class),
    ];
    for want in wants {
        assert_input_type(sample_to_value(
            sample(OcValue::Enum {
                type_iri: "CDL.Types.SimpleController".to_owned(),
                literal: "PI".to_owned(),
            }),
            want,
            PATH,
        ));
        assert_input_type(sample_to_value(
            sample(OcValue::Decimal("1.25".to_owned())),
            want,
            PATH,
        ));
    }
}

#[test]
fn resolve_store_inputs_rejects_mismatched_handle_count() {
    let store = LoadFailureStore::default();
    let mut model = ModelGraph::new();
    model.connectors.push(
        Connector::new(ConnectorId(0), BlockId(0), Dir::In, ValueType::Real, 0).with_iri(PATH),
    );
    let io = IoInventory::build_at_load(&model);

    let err = resolve_store_inputs(&store, &io).unwrap_err();
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
    Recover,
    SaveModel,
    ResolvePoints,
    #[default]
    HandleCount,
}

#[derive(Default)]
struct LoadFailureStore {
    inner: MemStore,
    failure: StoreFailure,
}

impl LoadFailureStore {
    fn new(failure: StoreFailure) -> Self {
        Self {
            inner: MemStore::default(),
            failure,
        }
    }
}

impl ModelStore for LoadFailureStore {
    fn save_model(&self, model: &ResolvedModel) -> StoreResult<()> {
        if matches!(self.failure, StoreFailure::SaveModel) {
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
        if matches!(self.failure, StoreFailure::ResolvePoints) {
            return Err(StoreError::Backend(
                "injected resolve_points failure".to_owned(),
            ));
        }
        let handles = self.inner.resolve_points(keys)?;
        if matches!(self.failure, StoreFailure::HandleCount) {
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
        if matches!(self.failure, StoreFailure::Recover) {
            return Err(StoreError::Backend("injected recover failure".to_owned()));
        }
        self.inner.recover()
    }
}
