//! Durable model projection tests for the load-time store seam.

use std::collections::HashSet;
use std::sync::Arc;

use oce_model::{
    Attrs, BlockId, BlockInstance, Connection, Connector, ConnectorId, Dir, EnumClassId, NoAttrs,
    ParamTable, RealAttrs, Value, ValueType,
};
use oce_semantics::{
    EffectivePoint, PointKey, PointType as SemanticPointType, ResolvedSemantics,
    TrendInterval as SemanticTrendInterval, TrendSpec,
};
use oce_store::{
    DomainKey, ModelStore, ResolvedModel, StoreError, StoreResult, TrendInterval as StoreTrend,
};
use oce_store_mem::MemStore;

use super::common::*;
use crate::io::IoInventory;
use crate::projection::project_resolved_model;

const PROJECTION_GOLDEN: &str = include_str!("fixtures/golden/projection.resolved_model.json");
const MINIMAL_LOOP_PROJECTION_GOLDEN: &str =
    include_str!("fixtures/golden/minimal_loop.resolved_model.json");
const MINIMAL_LOOP: &str = include_str!("../../../oce-cxf/tests/fixtures/minimal_loop.jsonld");

#[test]
fn projection_maps_effective_metadata_to_resolved_model_golden() {
    let (model, semantics) = fully_populated_projection_fixture();
    let resolved = project_resolved_model(&model, &semantics, None).expect("projection succeeds");

    assert_eq!(canonical_json(&resolved), PROJECTION_GOLDEN);
    assert_eq!(
        resolved
            .points
            .iter()
            .map(|point| point.trend_interval_s)
            .collect::<Vec<_>>(),
        vec![
            StoreTrend::OnChange,
            StoreTrend::EverySeconds(60),
            StoreTrend::EverySeconds(5),
            StoreTrend::OnChange,
        ]
    );
    assert_eq!(
        resolved
            .points
            .iter()
            .map(|point| point.hardwired)
            .collect::<Vec<_>>(),
        vec![true, false, false, true]
    );
    assert_eq!(
        resolved
            .points
            .iter()
            .map(|point| point.in_pointlist)
            .collect::<Vec<_>>(),
        vec![true, true, false, true]
    );
}

#[test]
fn projection_is_deterministic_and_coherent_with_io_inventory() {
    let (model, _, _, _) = build_accumulator_model();
    let semantics = oce_semantics::resolve(&model).expect("semantics resolves");

    let first =
        project_resolved_model(&model, &semantics, None).expect("first projection succeeds");
    let second =
        project_resolved_model(&model, &semantics, None).expect("second projection succeeds");

    assert_eq!(canonical_json(&first), canonical_json(&second));
    assert_eq!(first.model_id, second.model_id);
    assert!(
        first.model_id.as_str().starts_with("model:fnv1a128:"),
        "synthetic model id must be explicit and stable"
    );
    let io_paths = IoInventory::build_at_load(&model)
        .iter()
        .map(|point| point.path)
        .collect::<Vec<_>>();
    let dto_paths = first
        .points
        .iter()
        .map(|point| point.key.as_str().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(dto_paths, io_paths);
    assert!(
        dto_paths.iter().all(|path| path.starts_with("conn#")),
        "all-None IRI models use deterministic connector fallbacks"
    );
}

#[test]
fn iri_keyed_projection_is_deterministic_and_coherent_with_io_inventory() {
    let (model, semantics) = fully_populated_projection_fixture();

    let first =
        project_resolved_model(&model, &semantics, None).expect("first projection succeeds");
    let second =
        project_resolved_model(&model, &semantics, None).expect("second projection succeeds");

    assert_eq!(canonical_json(&first), canonical_json(&second));
    assert_eq!(first.model_id, second.model_id);
    let io_paths = IoInventory::build_at_load(&model)
        .iter()
        .map(|point| point.path)
        .collect::<Vec<_>>();
    let dto_paths = first
        .points
        .iter()
        .map(|point| point.key.as_str().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(dto_paths, io_paths);
    assert!(
        dto_paths
            .iter()
            .all(|path| path.starts_with("http://example.org#")),
        "IRI-bearing models must keep source connector IRIs as point keys"
    );
}

#[test]
fn source_model_iri_becomes_projection_model_id() {
    let (model, semantics) = fully_populated_projection_fixture();
    let resolved = project_resolved_model(&model, &semantics, Some("http://example.org#ahu"))
        .expect("projection succeeds");

    assert_eq!(resolved.model_id, DomainKey::new("http://example.org#ahu"));
}

#[test]
fn empty_model_projects_to_empty_collections_with_stable_model_id() {
    let model = ModelGraph::new();
    let semantics = oce_semantics::resolve(&model).expect("empty semantics resolves");
    let resolved = project_resolved_model(&model, &semantics, None).expect("empty model projects");

    assert!(resolved.classes.is_empty());
    assert!(resolved.blocks.is_empty());
    assert!(resolved.points.is_empty());
    assert!(resolved.connections.is_empty());
    assert!(resolved.containment.is_empty());
    assert!(resolved.model_id.as_str().starts_with("model:fnv1a128:"));
    assert_eq!(
        resolved.model_id,
        project_resolved_model(&model, &semantics, None)
            .expect("second empty projection")
            .model_id
    );
}

#[test]
fn string_connectors_are_metadata_only_and_skipped() {
    let model = string_only_model();
    let semantics = oce_semantics::resolve(&model).expect("semantics resolves");
    let resolved = project_resolved_model(&model, &semantics, None).expect("projection succeeds");

    assert!(semantics.points.is_empty());
    assert!(resolved.points.is_empty());
    assert!(resolved.connections.is_empty());
}

#[test]
fn duplicate_connector_domain_key_is_rejected_before_store_handoff() {
    let model = duplicate_point_key_model();
    let semantics = oce_semantics::resolve(&model).expect("semantics resolves");

    assert_projection_validation(
        &model,
        &semantics,
        "duplicate point DomainKey",
        "duplicate connector IRI must fail producer-side",
    );
}

#[test]
fn duplicate_semantic_metadata_is_rejected_before_projection() {
    let (model, mut semantics) = fully_populated_projection_fixture();
    semantics.points.push(semantics.points[0].clone());

    assert_projection_validation(
        &model,
        &semantics,
        "duplicate semantic metadata for connector 0",
        "duplicate metadata must fail producer-side",
    );
}

#[test]
fn missing_semantic_metadata_is_rejected_before_projection() {
    let (model, mut semantics) = fully_populated_projection_fixture();
    semantics
        .points
        .retain(|point| point.connector_id != ConnectorId(2));

    assert_projection_validation(
        &model,
        &semantics,
        "missing semantic metadata for connector 2",
        "missing metadata must fail producer-side",
    );
}

#[test]
fn semantic_metadata_key_mismatch_is_rejected_before_projection() {
    let (model, mut semantics) = fully_populated_projection_fixture();
    semantics.points[0].key = PointKey::Iri(Arc::from("http://example.org#wrong"));

    assert_projection_validation(
        &model,
        &semantics,
        "semantic metadata key mismatch for connector 0",
        "metadata key desync must fail producer-side",
    );
}

#[test]
fn empty_point_domain_key_is_rejected_before_store_handoff() {
    let (mut model, mut semantics) = fully_populated_projection_fixture();
    model.connectors[0].iri = Some(Arc::from(""));
    semantics.points[0].key = PointKey::Iri(Arc::from(""));

    assert_projection_validation(
        &model,
        &semantics,
        "connector 0 projected an empty DomainKey",
        "empty point key must fail producer-side",
    );
}

#[test]
fn empty_block_domain_key_is_rejected_before_store_handoff() {
    let (mut model, semantics) = fully_populated_projection_fixture();
    model.blocks[0].instance_iri = Some(Arc::from(""));

    assert_projection_validation(
        &model,
        &semantics,
        "block 0 projected an empty DomainKey",
        "empty block key must fail producer-side",
    );
}

#[test]
fn out_of_range_connection_endpoints_are_rejected_before_projection() {
    let (mut model, semantics) = fully_populated_projection_fixture();
    model.connections = vec![Connection {
        from: ConnectorId(99),
        to: ConnectorId(0),
    }];
    assert_projection_validation(
        &model,
        &semantics,
        "connection from 99 is out of range",
        "out-of-range source endpoint must fail producer-side",
    );

    let (mut model, semantics) = fully_populated_projection_fixture();
    model.connections = vec![Connection {
        from: ConnectorId(1),
        to: ConnectorId(99),
    }];
    assert_projection_validation(
        &model,
        &semantics,
        "connection to 99 is out of range",
        "out-of-range target endpoint must fail producer-side",
    );
}

#[test]
fn out_of_arena_connector_id_is_rejected_before_projection() {
    let (mut model, semantics) = fully_populated_projection_fixture();
    model.connectors[0].id = ConnectorId(99);
    assert_projection_validation(
        &model,
        &semantics,
        "connector 99 is out of range",
        "out-of-arena connector id must fail producer-side",
    );
}

#[test]
fn out_of_range_connector_owner_block_is_rejected_before_projection() {
    let (mut model, semantics) = fully_populated_projection_fixture();
    model.connectors[0].block = BlockId(99);
    assert_projection_validation(
        &model,
        &semantics,
        "connector 0 has out-of-range owner",
        "out-of-range connector owner must fail producer-side",
    );
}

#[test]
fn string_to_store_visible_connection_is_rejected_before_projection() {
    let (mut model, semantics) = fully_populated_projection_fixture();
    model.connections = vec![Connection {
        from: ConnectorId(4),
        to: ConnectorId(0),
    }];

    assert_projection_validation(
        &model,
        &semantics,
        "connection 4 -> 0 has exactly one store-visible endpoint",
        "String-to-non-String connection must not be silently dropped",
    );
}

#[test]
fn zero_second_semantic_trend_projects_as_on_change_with_stable_model_id() {
    let (model, on_change_semantics) = fully_populated_projection_fixture();
    let mut zero_seconds_semantics = on_change_semantics.clone();
    zero_seconds_semantics.points[0].trend.interval = SemanticTrendInterval::EverySeconds(0);

    let on_change =
        project_resolved_model(&model, &on_change_semantics, None).expect("OnChange projects");
    let zero_seconds = project_resolved_model(&model, &zero_seconds_semantics, None)
        .expect("zero seconds projects");

    assert_eq!(
        zero_seconds.points[0].trend_interval_s,
        StoreTrend::OnChange
    );
    assert_eq!(
        zero_seconds.model_id, on_change.model_id,
        "raw interval 0 and OnChange must share durable identity"
    );
}

#[test]
fn adapter_validation_is_elective_not_memstore_mandated() {
    let (model, semantics) = fully_populated_projection_fixture();
    let resolved = project_resolved_model(&model, &semantics, None).expect("projection succeeds");
    let mem = MemStore::new();
    let validating = ValidatingModelStore;

    mem.save_model(&resolved)
        .expect("MemStore accepts the valid projection");
    validating
        .save_model(&resolved)
        .expect("validating double accepts the valid projection");

    let mut empty_key = resolved.clone();
    empty_key.points[0].key = DomainKey::default();
    mem.save_model(&empty_key)
        .expect("MemStore does not validate DTO schema");
    assert_validation_rejects(&validating, &empty_key, "empty point DomainKey");

    let mut duplicate_key = resolved.clone();
    duplicate_key.points[1].key = duplicate_key.points[0].key.clone();
    mem.save_model(&duplicate_key)
        .expect("MemStore accepts duplicate point keys");
    assert_validation_rejects(&validating, &duplicate_key, "duplicate point DomainKey");
}

#[test]
fn load_cxf_saves_resolved_model_matching_golden() {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(MINIMAL_LOOP.as_bytes())
        .expect("minimal_loop loads");
    assert!(!report.model_id.as_str().is_empty());

    let resolved = engine
        .store()
        .load_model(&report.model_id)
        .expect("load stores the resolved model");
    assert_eq!(resolved.model_id, report.model_id);
    assert_eq!(canonical_json(&resolved), MINIMAL_LOOP_PROJECTION_GOLDEN);
}

fn fully_populated_projection_fixture() -> (ModelGraph, ResolvedSemantics) {
    let source = BlockId(0);
    let controller = BlockId(1);
    let real_attrs = RealAttrs {
        quantity: Some(Arc::from("ThermodynamicTemperature")),
        unit: Some(Arc::from("K")),
        display_unit: Some(Arc::from("degC")),
        min: Some(250.0),
        max: Some(330.0),
        nominal: Some(293.15),
        unbounded: Some(false),
    };
    let connectors = vec![
        Connector::new(ConnectorId(0), controller, Dir::In, ValueType::Real, 0)
            .with_attrs(Attrs::Real(real_attrs))
            .expect("real attrs match")
            .with_iri("http://example.org#ahu.sat"),
        Connector::new(ConnectorId(1), source, Dir::Out, ValueType::Integer, 1)
            .with_attrs(Attrs::Integer(Default::default()))
            .expect("integer attrs match")
            .with_iri("http://example.org#ahu.mode"),
        Connector::new(ConnectorId(2), controller, Dir::In, ValueType::Boolean, 2)
            .with_attrs(Attrs::Boolean(NoAttrs))
            .expect("boolean attrs match")
            .with_iri("http://example.org#ahu.enable"),
        Connector::new(ConnectorId(3), source, Dir::Out, ValueType::Boolean, 3)
            .with_attrs(Attrs::Boolean(NoAttrs))
            .expect("boolean attrs match")
            .with_iri("http://example.org#ahu.command"),
        Connector::new(ConnectorId(4), source, Dir::Out, ValueType::String, 4)
            .with_attrs(Attrs::String(NoAttrs))
            .expect("string attrs match")
            .with_iri("http://example.org#ahu.label"),
    ];
    let model = ModelGraph {
        blocks: vec![
            BlockInstance {
                id: source,
                class_iri: Arc::from("CDL.Test.Source"),
                inputs: Vec::new(),
                outputs: vec![ConnectorId(1), ConnectorId(3), ConnectorId(4)],
                params: ParamTable {
                    values: vec![
                        (Arc::from("gain"), Value::Real(1.25)),
                        (Arc::from("count"), Value::Integer(3)),
                        (Arc::from("enabled"), Value::Boolean(true)),
                        (Arc::from("label"), Value::String(Arc::from("supply"))),
                        (
                            Arc::from("mode"),
                            Value::Enum {
                                class: EnumClassId(2),
                                ordinal: 3,
                            },
                        ),
                    ],
                },
                decl_order: 0,
                instance_iri: Some(Arc::from("http://example.org#ahu.source")),
            },
            BlockInstance {
                id: controller,
                class_iri: Arc::from("CDL.Test.Controller"),
                inputs: vec![ConnectorId(0), ConnectorId(2)],
                outputs: Vec::new(),
                params: ParamTable::default(),
                decl_order: 1,
                instance_iri: Some(Arc::from("http://example.org#ahu.controller")),
            },
        ],
        connectors,
        connections: vec![
            Connection {
                from: ConnectorId(1),
                to: ConnectorId(0),
            },
            Connection {
                from: ConnectorId(3),
                to: ConnectorId(2),
            },
        ],
        external_inputs: Vec::new(),
    };
    let semantics = ResolvedSemantics {
        points: vec![
            effective_point(0, "http://example.org#ahu.sat", SemanticPointType::Ai)
                .hardwired(true)
                .trend(0, true)
                .quantity("ThermodynamicTemperature")
                .unit("K")
                .display_unit("degC")
                .description("Supply air temperature")
                .build(),
            effective_point(1, "http://example.org#ahu.mode", SemanticPointType::Mode)
                .trend(60, true)
                .controlled_device("AHU-1")
                .description("Operating mode")
                .build(),
            effective_point(2, "http://example.org#ahu.enable", SemanticPointType::Di)
                .trend(5, false)
                .pointlist(false)
                .build(),
            effective_point(3, "http://example.org#ahu.command", SemanticPointType::Do)
                .hardwired(true)
                .build(),
        ],
        diagnostics: Vec::new(),
    };
    (model, semantics)
}

fn effective_point(id: u32, iri: &str, point_type: SemanticPointType) -> EffectivePointBuilder {
    EffectivePointBuilder {
        point: EffectivePoint {
            key: PointKey::Iri(Arc::from(iri)),
            connector_id: ConnectorId(id),
            point_type,
            hardwired: false,
            trend: TrendSpec::default(),
            quantity: None,
            unit: None,
            display_unit: None,
            in_pointlist: true,
            controlled_device: None,
            description: None,
        },
    }
}

struct EffectivePointBuilder {
    point: EffectivePoint,
}

impl EffectivePointBuilder {
    fn hardwired(mut self, hardwired: bool) -> Self {
        self.point.hardwired = hardwired;
        self
    }

    fn trend(mut self, interval: u32, enabled: bool) -> Self {
        self.point.trend = TrendSpec::from_interval_seconds(interval, enabled);
        self
    }

    fn pointlist(mut self, in_pointlist: bool) -> Self {
        self.point.in_pointlist = in_pointlist;
        self
    }

    fn quantity(mut self, quantity: &str) -> Self {
        self.point.quantity = Some(Arc::from(quantity));
        self
    }

    fn unit(mut self, unit: &str) -> Self {
        self.point.unit = Some(Arc::from(unit));
        self
    }

    fn display_unit(mut self, display_unit: &str) -> Self {
        self.point.display_unit = Some(Arc::from(display_unit));
        self
    }

    fn controlled_device(mut self, controlled_device: &str) -> Self {
        self.point.controlled_device = Some(Arc::from(controlled_device));
        self
    }

    fn description(mut self, description: &str) -> Self {
        self.point.description = Some(Arc::from(description));
        self
    }

    fn build(self) -> EffectivePoint {
        self.point
    }
}

fn string_only_model() -> ModelGraph {
    let block = BlockId(0);
    let connectors = vec![
        Connector::new(ConnectorId(0), block, Dir::Out, ValueType::String, 0)
            .with_attrs(Attrs::String(NoAttrs))
            .expect("string attrs match")
            .with_iri("http://example.org#label_out"),
        Connector::new(ConnectorId(1), block, Dir::In, ValueType::String, 1)
            .with_attrs(Attrs::String(NoAttrs))
            .expect("string attrs match")
            .with_iri("http://example.org#label_in"),
    ];
    ModelGraph {
        blocks: vec![BlockInstance {
            id: block,
            class_iri: Arc::from("CDL.Test.StringMetadata"),
            inputs: vec![ConnectorId(1)],
            outputs: vec![ConnectorId(0)],
            params: ParamTable::default(),
            decl_order: 0,
            instance_iri: None,
        }],
        connectors,
        connections: vec![Connection {
            from: ConnectorId(0),
            to: ConnectorId(1),
        }],
        external_inputs: Vec::new(),
    }
}

fn duplicate_point_key_model() -> ModelGraph {
    let block = BlockId(0);
    let connectors = vec![
        Connector::new(ConnectorId(0), block, Dir::Out, ValueType::Real, 0)
            .with_iri("http://example.org#dup"),
        Connector::new(ConnectorId(1), block, Dir::Out, ValueType::Boolean, 1)
            .with_iri("http://example.org#dup"),
    ];
    ModelGraph {
        blocks: vec![BlockInstance {
            id: block,
            class_iri: Arc::from("CDL.Test.Duplicate"),
            inputs: Vec::new(),
            outputs: vec![ConnectorId(0), ConnectorId(1)],
            params: ParamTable::default(),
            decl_order: 0,
            instance_iri: None,
        }],
        connectors,
        connections: Vec::new(),
        external_inputs: Vec::new(),
    }
}

fn canonical_json(model: &ResolvedModel) -> String {
    let mut text = serde_json::to_string_pretty(model).expect("serialize ResolvedModel");
    text.push('\n');
    text
}

fn assert_projection_validation(
    model: &ModelGraph,
    semantics: &ResolvedSemantics,
    needle: &str,
    context: &str,
) {
    let err = project_resolved_model(model, semantics, None).expect_err(context);
    assert!(
        matches!(err, StoreError::Validation(_)),
        "expected StoreError::Validation, got {err:?}"
    );
    assert!(
        err.to_string().contains(needle),
        "validation message should contain {needle:?}: {err}"
    );
}

fn assert_validation_rejects(store: &ValidatingModelStore, model: &ResolvedModel, needle: &str) {
    let err = store
        .save_model(model)
        .expect_err("validating store rejects injected defect");
    assert!(
        matches!(err, StoreError::Validation(_)),
        "expected validation error, got {err:?}"
    );
    assert!(
        err.to_string().contains(needle),
        "validation message should contain {needle:?}: {err}"
    );
}

struct ValidatingModelStore;

impl ModelStore for ValidatingModelStore {
    fn save_model(&self, model: &ResolvedModel) -> StoreResult<()> {
        if model.model_id.as_str().is_empty() {
            return Err(StoreError::Validation("empty model DomainKey".to_owned()));
        }
        let mut point_keys = HashSet::new();
        for point in &model.points {
            if point.key.as_str().is_empty() {
                return Err(StoreError::Validation("empty point DomainKey".to_owned()));
            }
            if point.owner_block.as_str().is_empty() {
                return Err(StoreError::Validation(
                    "empty owner block DomainKey".to_owned(),
                ));
            }
            if !point_keys.insert(point.key.as_str().to_owned()) {
                return Err(StoreError::Validation(format!(
                    "duplicate point DomainKey: {}",
                    point.key.as_str()
                )));
            }
        }
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
