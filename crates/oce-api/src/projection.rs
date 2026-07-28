//! Load-time durable projection from the flat executable model into store DTOs.

use std::collections::HashMap;

use oce_model::{BlockInstance, ConnectorId, ModelGraph, Value};
use oce_semantics::{
    EffectivePoint, PointKey, PointType as SemanticPointType, ResolvedSemantics,
    TrendInterval as SemanticTrendInterval,
};
use oce_store::{
    BlockClassDto, BlockInstanceDto, BlockKind, ConnectionDto, DomainKey, OcValue, PointDto,
    PointType, ResolvedModel, StoreError, StoreResult, TrendInterval,
};

use crate::io::point_rows_at_load;
use crate::stable_hash::StableHash;

const MODEL_SCHEMA_REV: u32 = 1;
const MODEL_ID_PREFIX: &str = "model:fnv1a128:";

pub(crate) fn project_resolved_model(
    model: &ModelGraph,
    semantics: &ResolvedSemantics,
    model_iri: Option<&str>,
) -> StoreResult<ResolvedModel> {
    let metadata_by_connector = metadata_index(semantics)?;
    let classes = project_classes(model);
    let blocks = project_blocks(model)?;
    let (points, point_keys_by_connector) = project_points(model, &metadata_by_connector)?;
    let connections = project_connections(model, &point_keys_by_connector)?;
    let mut resolved = ResolvedModel {
        model_id: DomainKey::default(),
        schema_rev: MODEL_SCHEMA_REV,
        classes,
        blocks,
        points,
        connections,
        containment: Vec::new(),
    };
    resolved.model_id = model_iri
        .map(DomainKey::new)
        .unwrap_or_else(|| synthesize_model_id(&resolved));
    Ok(resolved)
}

fn metadata_index(
    semantics: &ResolvedSemantics,
) -> StoreResult<HashMap<ConnectorId, &EffectivePoint>> {
    let mut by_connector = HashMap::with_capacity(semantics.points.len());
    for point in &semantics.points {
        if by_connector.insert(point.connector_id, point).is_some() {
            return Err(StoreError::Validation(format!(
                "duplicate semantic metadata for connector {}",
                point.connector_id.0
            )));
        }
    }
    Ok(by_connector)
}

fn project_classes(model: &ModelGraph) -> Vec<BlockClassDto> {
    let mut classes = Vec::new();
    for block in &model.blocks {
        let class_iri = block.class_iri.to_string();
        if classes
            .iter()
            .any(|class: &BlockClassDto| class.class_iri == class_iri)
        {
            continue;
        }
        classes.push(BlockClassDto {
            class_iri,
            kind: BlockKind::Elementary,
            is_library: true,
            shape_json: "{}".to_owned(),
        });
    }
    classes
}

fn project_blocks(model: &ModelGraph) -> StoreResult<Vec<BlockInstanceDto>> {
    model
        .blocks
        .iter()
        .map(|block| {
            let key = block_key(block);
            if key.as_str().is_empty() {
                return Err(StoreError::Validation(format!(
                    "block {} projected an empty DomainKey",
                    block.id.0
                )));
            }
            Ok(BlockInstanceDto {
                key,
                class_iri: block.class_iri.to_string(),
                params: block
                    .params
                    .values
                    .iter()
                    .map(|(name, value)| (name.to_string(), value_to_oc_value(value)))
                    .collect(),
                config_json: "{}".to_owned(),
            })
        })
        .collect()
}

fn project_points(
    model: &ModelGraph,
    metadata_by_connector: &HashMap<ConnectorId, &EffectivePoint>,
) -> StoreResult<(Vec<PointDto>, Vec<Option<DomainKey>>)> {
    #[derive(Clone)]
    struct SeenPoint {
        direction: oce_store::PointDirection,
        value_type: oce_store::PointValueType,
        key: DomainKey,
        external_input: bool,
    }

    let mut points = Vec::new();
    let mut seen: HashMap<String, SeenPoint> = HashMap::new();
    let mut point_keys_by_connector = vec![None; model.connectors.len()];
    for row in point_rows_at_load(model) {
        let connector = model
            .connectors
            .get(row.connector_id.0 as usize)
            .ok_or_else(|| {
                malformed(format!("connector {} is out of range", row.connector_id.0))
            })?;
        let metadata = metadata_by_connector
            .get(&row.connector_id)
            .ok_or_else(|| {
                StoreError::Validation(format!(
                    "missing semantic metadata for connector {}",
                    row.connector_id.0
                ))
            })?;
        validate_metadata_key(connector.iri.as_ref(), connector.id, metadata)?;

        let key = DomainKey::new(row.info.path.clone());
        if key.as_str().is_empty() {
            return Err(StoreError::Validation(format!(
                "connector {} projected an empty DomainKey",
                row.connector_id.0
            )));
        }
        let is_external_input = row.info.direction == oce_store::PointDirection::In
            && model.external_inputs.contains(&row.connector_id);
        if let Some(existing) = seen.get(key.as_str()) {
            if existing.external_input
                && is_external_input
                && existing.direction == row.info.direction
                && existing.value_type == row.info.value_type
            {
                point_keys_by_connector[row.connector_id.0 as usize] = Some(existing.key.clone());
                continue;
            }
            return Err(StoreError::Validation(format!(
                "duplicate point DomainKey: {}",
                key.as_str()
            )));
        }
        let owner_block = model
            .blocks
            .get(connector.block.0 as usize)
            .ok_or_else(|| {
                malformed(format!(
                    "connector {} has out-of-range owner",
                    connector.id.0
                ))
            })
            .map(block_key)?;
        point_keys_by_connector[row.connector_id.0 as usize] = Some(key.clone());
        seen.insert(
            key.as_str().to_owned(),
            SeenPoint {
                direction: row.info.direction,
                value_type: row.info.value_type,
                key: key.clone(),
                external_input: is_external_input,
            },
        );
        points.push(PointDto {
            key,
            owner_block,
            direction: row.info.direction,
            value_type: row.info.value_type,
            point_type: store_point_type(metadata.point_type),
            hardwired: metadata.hardwired,
            trend_interval_s: store_trend_interval(metadata.trend.interval),
            trend_enable: metadata.trend.enabled,
            quantity: metadata.quantity.as_deref().map(str::to_owned),
            unit: metadata.unit.as_deref().map(str::to_owned),
            display_unit: metadata.display_unit.as_deref().map(str::to_owned),
            description: metadata.description.as_deref().map(str::to_owned),
            in_pointlist: metadata.in_pointlist,
            controlled_device: metadata.controlled_device.as_deref().map(str::to_owned),
            search_text: None,
            tags_json: None,
            embedding: None,
        });
    }
    Ok((points, point_keys_by_connector))
}

fn project_connections(
    model: &ModelGraph,
    point_keys_by_connector: &[Option<DomainKey>],
) -> StoreResult<Vec<ConnectionDto>> {
    let mut connections = Vec::with_capacity(model.connections.len());
    for connection in &model.connections {
        let from_point = point_keys_by_connector
            .get(connection.from.0 as usize)
            .ok_or_else(|| {
                malformed(format!(
                    "connection from {} is out of range",
                    connection.from.0
                ))
            })?;
        let to_point = point_keys_by_connector
            .get(connection.to.0 as usize)
            .ok_or_else(|| {
                malformed(format!("connection to {} is out of range", connection.to.0))
            })?;
        match (from_point, to_point) {
            (Some(from_point), Some(to_point)) => {
                connections.push(ConnectionDto {
                    from_point: from_point.clone(),
                    to_point: to_point.clone(),
                });
            }
            (None, None) => {}
            (Some(_), None) | (None, Some(_)) => {
                return Err(StoreError::Validation(format!(
                    "connection {} -> {} has exactly one store-visible endpoint",
                    connection.from.0, connection.to.0
                )));
            }
        }
    }
    connections.sort_by(|left, right| {
        left.from_point
            .as_str()
            .cmp(right.from_point.as_str())
            .then_with(|| left.to_point.as_str().cmp(right.to_point.as_str()))
    });
    Ok(connections)
}

fn validate_metadata_key(
    iri: Option<&std::sync::Arc<str>>,
    id: ConnectorId,
    metadata: &EffectivePoint,
) -> StoreResult<()> {
    let expected = PointKey::from_connector(iri, id);
    if metadata.key == expected {
        Ok(())
    } else {
        Err(StoreError::Validation(format!(
            "semantic metadata key mismatch for connector {}",
            id.0
        )))
    }
}

fn block_key(block: &BlockInstance) -> DomainKey {
    block
        .instance_iri
        .as_deref()
        .map(|iri| DomainKey::new(iri.to_owned()))
        .unwrap_or_else(|| DomainKey::new(format!("block#{}", block.id.0)))
}

fn value_to_oc_value(value: &Value) -> OcValue {
    match value {
        Value::Real(v) => OcValue::Real(*v),
        Value::Integer(v) => OcValue::Int(*v),
        Value::Boolean(v) => OcValue::Bool(*v),
        Value::String(v) => OcValue::String(v.to_string()),
        Value::Enum { class, ordinal } => OcValue::Enum {
            type_iri: format!("enum#{}", class.0),
            literal: ordinal.to_string(),
        },
    }
}

fn store_point_type(point_type: SemanticPointType) -> PointType {
    match point_type {
        SemanticPointType::Ai => PointType::Ai,
        SemanticPointType::Ao => PointType::Ao,
        SemanticPointType::Di => PointType::Di,
        SemanticPointType::Do => PointType::Do,
        SemanticPointType::Mode => PointType::Mode,
    }
}

fn store_trend_interval(interval: SemanticTrendInterval) -> TrendInterval {
    match interval {
        SemanticTrendInterval::OnChange => TrendInterval::OnChange,
        SemanticTrendInterval::EverySeconds(0) => TrendInterval::OnChange,
        SemanticTrendInterval::EverySeconds(seconds) => TrendInterval::EverySeconds(seconds),
    }
}

fn malformed(message: String) -> StoreError {
    StoreError::Validation(message)
}

fn synthesize_model_id(model: &ResolvedModel) -> DomainKey {
    let mut hash = StableHash::new();
    hash.write_str("ResolvedModel");
    hash.write_u32(model.schema_rev);
    hash_classes(&mut hash, &model.classes);
    hash_blocks(&mut hash, &model.blocks);
    hash_points(&mut hash, &model.points);
    hash_connections(&mut hash, &model.connections);
    hash.write_usize(model.containment.len());
    for (parent, child) in &model.containment {
        hash.write_domain_key(parent);
        hash.write_domain_key(child);
    }
    DomainKey::new(format!("{MODEL_ID_PREFIX}{:032x}", hash.finish()))
}

fn hash_classes(hash: &mut StableHash, classes: &[BlockClassDto]) {
    hash.write_usize(classes.len());
    for class in classes {
        hash.write_str(&class.class_iri);
        hash.write_str(match class.kind {
            BlockKind::Elementary => "elementary",
            BlockKind::Composite => "composite",
            BlockKind::Extension => "extension",
        });
        hash.write_bool(class.is_library);
        hash.write_str(&class.shape_json);
    }
}

fn hash_blocks(hash: &mut StableHash, blocks: &[BlockInstanceDto]) {
    hash.write_usize(blocks.len());
    for block in blocks {
        hash.write_domain_key(&block.key);
        hash.write_str(&block.class_iri);
        hash.write_usize(block.params.len());
        for (name, value) in &block.params {
            hash.write_str(name);
            hash_oc_value(hash, value);
        }
        hash.write_str(&block.config_json);
    }
}

fn hash_points(hash: &mut StableHash, points: &[PointDto]) {
    hash.write_usize(points.len());
    for point in points {
        hash.write_domain_key(&point.key);
        hash.write_domain_key(&point.owner_block);
        hash.write_str(match point.direction {
            oce_store::PointDirection::In => "in",
            oce_store::PointDirection::Out => "out",
        });
        hash.write_str(match point.value_type {
            oce_store::PointValueType::Real => "real",
            oce_store::PointValueType::Int => "int",
            oce_store::PointValueType::Bool => "bool",
        });
        hash.write_str(match point.point_type {
            PointType::Ai => "ai",
            PointType::Ao => "ao",
            PointType::Di => "di",
            PointType::Do => "do",
            PointType::Mode => "mode",
        });
        hash.write_bool(point.hardwired);
        match point.trend_interval_s {
            TrendInterval::OnChange => {
                hash.write_str("trend:on-change");
            }
            TrendInterval::EverySeconds(seconds) => {
                hash.write_str("trend:every-seconds");
                hash.write_u32(seconds);
            }
        }
        hash.write_bool(point.trend_enable);
        hash.write_opt_str(point.quantity.as_deref());
        hash.write_opt_str(point.unit.as_deref());
        hash.write_opt_str(point.display_unit.as_deref());
        hash.write_opt_str(point.description.as_deref());
        hash.write_bool(point.in_pointlist);
        hash.write_opt_str(point.controlled_device.as_deref());
        hash.write_opt_str(point.search_text.as_deref());
        hash.write_opt_str(point.tags_json.as_deref());
        hash.write_usize(point.embedding.as_ref().map_or(0, Vec::len));
        if let Some(embedding) = &point.embedding {
            for v in embedding {
                hash.write_u32(v.to_bits());
            }
        }
    }
}

fn hash_connections(hash: &mut StableHash, connections: &[ConnectionDto]) {
    hash.write_usize(connections.len());
    for connection in connections {
        hash.write_domain_key(&connection.from_point);
        hash.write_domain_key(&connection.to_point);
    }
}

fn hash_oc_value(hash: &mut StableHash, value: &OcValue) {
    match value {
        OcValue::Bool(v) => {
            hash.write_str("bool");
            hash.write_bool(*v);
        }
        OcValue::Int(v) => {
            hash.write_str("int");
            hash.write_i64(*v);
        }
        OcValue::Real(v) => {
            hash.write_str("real");
            hash.write_u64(v.to_bits());
        }
        OcValue::Decimal(v) => {
            hash.write_str("decimal");
            hash.write_str(v);
        }
        OcValue::Enum { type_iri, literal } => {
            hash.write_str("enum");
            hash.write_str(type_iri);
            hash.write_str(literal);
        }
        OcValue::String(v) => {
            hash.write_str("string");
            hash.write_str(v);
        }
    }
}
