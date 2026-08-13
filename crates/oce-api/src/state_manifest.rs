//! Execution-manifest construction and compatibility diagnostics.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use oce_blocks::BlockKind;
use oce_model::{ConnectorId, ValueType, enum_descriptor};
use oce_store::Store;

use crate::engine::Engine;
use crate::stable_hash::StableHash;
use crate::state::{
    BlockKey, BlockManifestEntry, ConnectorKey, ConnectorManifestEntry, EngineStateError,
    EnumManifestEntry, ExecutionManifest, Portability, WireDir, WireValue, WireValueType,
};

const PASS_THROUGH_PREFIX: &str = "urn:oce:lowering#PassThrough.";

pub(crate) const TARGET_BOUND_CLASSES: &[&str] = &[
    "CDL.Psychrometrics.DewPoint_TDryBulPhi",
    "CDL.Psychrometrics.SpecificEnthalpy_TDryBulPhi",
    "CDL.Psychrometrics.WetBulb_TDryBulPhi",
    "CDL.Reals.Acos",
    "CDL.Reals.Asin",
    "CDL.Reals.Atan",
    "CDL.Reals.Atan2",
    "CDL.Reals.Cos",
    "CDL.Reals.Exp",
    "CDL.Reals.Log",
    "CDL.Reals.Log10",
    "CDL.Reals.Sin",
    "CDL.Reals.Sources.Sin",
    "CDL.Reals.Tan",
    "CDL.Utilities.SunRiseSet",
];

pub(crate) struct BuiltManifest {
    pub(crate) manifest: ExecutionManifest,
    pub(crate) fingerprint: u128,
    pub(crate) connector_keys_by_dense: Vec<ConnectorKey>,
    pub(crate) dense_by_sorted_key: BTreeMap<ConnectorKey, usize>,
}

pub(crate) fn build_manifest<S: Store>(
    engine: &Engine<S>,
    durable: bool,
) -> Result<BuiltManifest, EngineStateError> {
    let mut block_keys = Vec::with_capacity(engine.model.blocks.len());
    let mut block_key_set = BTreeSet::new();
    for (index, block) in engine.model.blocks.iter().enumerate() {
        let key = block_key(engine, index, durable)?;
        if !block_key_set.insert(key.clone()) {
            return Err(EngineStateError::IneligibleModel {
                subject: key.subject(),
                detail: "block key is not unique".into(),
            });
        }
        if block.id.0 as usize != index {
            return Err(EngineStateError::IneligibleModel {
                subject: key.subject(),
                detail: "block id differs from its arena position".into(),
            });
        }
        block_keys.push(key);
    }

    let mut connector_keys_by_dense = vec![None; engine.model.connectors.len()];
    for (block_index, block) in engine.model.blocks.iter().enumerate() {
        let owner = block_keys[block_index].clone();
        for (port_index, connector_id) in block.inputs.iter().copied().enumerate() {
            assign_connector_key(
                &mut connector_keys_by_dense,
                connector_id,
                ConnectorKey {
                    owner: owner.clone(),
                    direction: WireDir::In,
                    port_index: u32::try_from(port_index).map_err(|_| too_large(port_index))?,
                },
            )?;
        }
        for (port_index, connector_id) in block.outputs.iter().copied().enumerate() {
            assign_connector_key(
                &mut connector_keys_by_dense,
                connector_id,
                ConnectorKey {
                    owner: owner.clone(),
                    direction: WireDir::Out,
                    port_index: u32::try_from(port_index).map_err(|_| too_large(port_index))?,
                },
            )?;
        }
    }
    let connector_keys_by_dense = connector_keys_by_dense
        .into_iter()
        .enumerate()
        .map(|(index, key)| {
            key.ok_or_else(|| EngineStateError::IneligibleModel {
                subject: format!("connector#{index}"),
                detail: "connector is absent from its owner's ordered ports".into(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut dense_by_sorted_key = BTreeMap::new();
    for (dense, key) in connector_keys_by_dense.iter().cloned().enumerate() {
        if dense_by_sorted_key.insert(key.clone(), dense).is_some() {
            return Err(EngineStateError::IneligibleModel {
                subject: key.owner.subject(),
                detail: "connector key is not unique".into(),
            });
        }
    }

    let mut enum_classes = BTreeMap::<String, Vec<String>>::new();
    let mut blocks = Vec::with_capacity(engine.model.blocks.len());
    for (index, (model_block, block)) in engine.model.blocks.iter().zip(&engine.blocks).enumerate()
    {
        let kind = block.kind();
        let state_revision = block.state_contract_revision();
        if kind == BlockKind::Stateful && state_revision == 0 {
            return Err(EngineStateError::IneligibleModel {
                subject: block_keys[index].subject(),
                detail: "stateful block has no registered state-contract revision".into(),
            });
        }
        if kind == BlockKind::Algebraic && (state_revision != 0 || block.state_len() != 0) {
            return Err(EngineStateError::IneligibleModel {
                subject: block_keys[index].subject(),
                detail: "algebraic block reports state".into(),
            });
        }
        let mut params = Vec::with_capacity(model_block.params.values.len());
        for (name, value) in &model_block.params.values {
            let wire = WireValue::from_value(value)?;
            record_wire_value_enum(&wire, &mut enum_classes)?;
            params.push((name.to_string(), wire));
        }
        params.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
        if params.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err(EngineStateError::IneligibleModel {
                subject: block_keys[index].subject(),
                detail: "duplicate raw parameter name".into(),
            });
        }
        blocks.push(BlockManifestEntry {
            key: block_keys[index].clone(),
            class_path: block.signature().class_path.to_owned(),
            kind,
            state_revision,
            state_len: u64::try_from(block.state_len())
                .map_err(|_| too_large(block.state_len()))?,
            params,
            inputs: model_block
                .inputs
                .iter()
                .map(|id| connector_keys_by_dense[id.0 as usize].clone())
                .collect(),
            outputs: model_block
                .outputs
                .iter()
                .map(|id| connector_keys_by_dense[id.0 as usize].clone())
                .collect(),
        });
    }
    blocks.sort_by(|left, right| left.key.cmp(&right.key));

    let mut connectors = Vec::with_capacity(engine.model.connectors.len());
    for (dense, connector) in engine.model.connectors.iter().enumerate() {
        if connector.id.0 as usize != dense {
            return Err(EngineStateError::IneligibleModel {
                subject: format!("connector#{}", connector.id.0),
                detail: "connector id differs from its arena position".into(),
            });
        }
        let value_type = wire_value_type(connector.value_type, &mut enum_classes)?;
        let path = match connector.iri.as_deref() {
            Some(path) => path.to_owned(),
            None if durable => {
                return Err(EngineStateError::IneligibleModel {
                    subject: connector_keys_by_dense[dense].owner.subject(),
                    detail: "durable connector has no authored host-visible path".into(),
                });
            }
            None => crate::io::connector_path(None, connector.id),
        };
        connectors.push(ConnectorManifestEntry {
            key: connector_keys_by_dense[dense].clone(),
            path,
            declaration_order: connector.decl_order,
            value_type,
        });
    }
    connectors.sort_by(|left, right| left.key.cmp(&right.key));

    let enums = enum_classes
        .into_iter()
        .map(|(class_path, members)| EnumManifestEntry {
            class_path,
            members: members.into_iter().map(Arc::from).collect(),
        })
        .collect();
    let mut connections = engine
        .model
        .connections
        .iter()
        .map(|connection| {
            (
                connector_keys_by_dense[connection.from.0 as usize].clone(),
                connector_keys_by_dense[connection.to.0 as usize].clone(),
            )
        })
        .collect::<Vec<_>>();
    connections.sort();
    let schedule = engine
        .schedule
        .order
        .iter()
        .map(|id| block_keys[id.0 as usize].clone())
        .collect();
    let connector_order = engine
        .schedule
        .connector_order
        .iter()
        .map(|id| connector_keys_by_dense[id.0 as usize].clone())
        .collect();
    let mut driver_of = engine
        .schedule
        .driver_of
        .iter()
        .enumerate()
        .map(|(dense, driver)| {
            (
                connector_keys_by_dense[dense].clone(),
                connector_keys_by_dense[driver.0 as usize].clone(),
            )
        })
        .collect::<Vec<_>>();
    driver_of.sort_by(|left, right| left.0.cmp(&right.0));
    let mut state_slots = engine
        .state
        .slots
        .iter()
        .map(|slot| {
            Ok((
                block_keys[slot.block.0 as usize].clone(),
                u64::try_from(slot.offset).map_err(|_| too_large(slot.offset))?,
                u64::try_from(slot.len).map_err(|_| too_large(slot.len))?,
            ))
        })
        .collect::<Result<Vec<_>, EngineStateError>>()?;
    state_slots.sort_by(|left, right| left.0.cmp(&right.0));
    let external_inputs = engine
        .model
        .external_inputs
        .iter()
        .map(|id| connector_keys_by_dense[id.0 as usize].clone())
        .collect();
    let boundary_outputs = engine
        .model
        .boundary_outputs
        .iter()
        .map(|output| {
            (
                output.iri.to_string(),
                connector_keys_by_dense[output.source.0 as usize].clone(),
            )
        })
        .collect();
    let target_bound = engine
        .blocks
        .iter()
        .any(|block| requires_target(block.signature().class_path));
    let portability = if target_bound {
        Portability::TargetBound {
            arch: std::env::consts::ARCH.into(),
            os: std::env::consts::OS.into(),
        }
    } else {
        Portability::CrossPlatform
    };
    let manifest = ExecutionManifest {
        portability,
        enums,
        blocks,
        connectors,
        connections,
        schedule,
        connector_order,
        driver_of,
        state_slots,
        external_inputs,
        boundary_outputs,
    };
    let manifest_bytes = crate::state_manifest_codec::encode_manifest(&manifest, !durable)?;
    let fingerprint = fingerprint(crate::state::EXECUTION_ABI_REVISION, &manifest_bytes);
    Ok(BuiltManifest {
        manifest,
        fingerprint,
        connector_keys_by_dense,
        dense_by_sorted_key,
    })
}

pub(crate) fn compare_manifests(
    snapshot: &ExecutionManifest,
    target: &ExecutionManifest,
) -> Result<(), EngineStateError> {
    if snapshot == target {
        return Ok(());
    }
    if snapshot.portability != target.portability {
        return incompatible(
            "arithmetic portability",
            &snapshot.portability,
            &target.portability,
        );
    }
    compare_named("enum class", &snapshot.enums, &target.enums, |entry| {
        entry.class_path.clone()
    })?;
    compare_blocks(&snapshot.blocks, &target.blocks)?;
    compare_named(
        "connector",
        &snapshot.connectors,
        &target.connectors,
        |entry| {
            format!(
                "{}:{:?}:{}",
                entry.key.owner.subject(),
                entry.key.direction,
                entry.key.port_index
            )
        },
    )?;
    if snapshot.connections != target.connections {
        return incompatible("connections", &snapshot.connections, &target.connections);
    }
    if snapshot.schedule != target.schedule {
        return incompatible("block schedule", &snapshot.schedule, &target.schedule);
    }
    if snapshot.connector_order != target.connector_order {
        return incompatible(
            "connector schedule",
            &snapshot.connector_order,
            &target.connector_order,
        );
    }
    if snapshot.driver_of != target.driver_of {
        return incompatible("driver map", &snapshot.driver_of, &target.driver_of);
    }
    if snapshot.state_slots != target.state_slots {
        return incompatible("state slots", &snapshot.state_slots, &target.state_slots);
    }
    if snapshot.external_inputs != target.external_inputs {
        return incompatible(
            "external inputs",
            &snapshot.external_inputs,
            &target.external_inputs,
        );
    }
    incompatible(
        "boundary outputs",
        &snapshot.boundary_outputs,
        &target.boundary_outputs,
    )
}

fn compare_blocks(
    snapshot: &[BlockManifestEntry],
    target: &[BlockManifestEntry],
) -> Result<(), EngineStateError> {
    for (left, right) in snapshot.iter().zip(target) {
        if left.key != right.key {
            return incompatible("block key", &left.key, &right.key);
        }
        let block = left.key.subject();
        if left.class_path != right.class_path {
            return incompatible(
                &format!("block {block} class"),
                &left.class_path,
                &right.class_path,
            );
        }
        if left.kind != right.kind {
            return incompatible(&format!("block {block} kind"), &left.kind, &right.kind);
        }
        if left.state_revision != right.state_revision {
            return incompatible(
                &format!("block {block} state revision"),
                &left.state_revision,
                &right.state_revision,
            );
        }
        if left.state_len != right.state_len {
            return incompatible(
                &format!("block {block} state length"),
                &left.state_len,
                &right.state_len,
            );
        }
        for (left_param, right_param) in left.params.iter().zip(&right.params) {
            if left_param.0 != right_param.0 {
                return incompatible(
                    &format!("parameter {block} name"),
                    &left_param.0,
                    &right_param.0,
                );
            }
            if left_param.1 != right_param.1 {
                return incompatible(
                    &format!("parameter {block}.{}", left_param.0),
                    &left_param.1,
                    &right_param.1,
                );
            }
        }
        if left.params.len() != right.params.len() {
            return incompatible(
                &format!("block {block} parameter count"),
                &left.params.len(),
                &right.params.len(),
            );
        }
        compare_port_bindings(&block, "input", &left.inputs, &right.inputs)?;
        compare_port_bindings(&block, "output", &left.outputs, &right.outputs)?;
    }
    if snapshot.len() != target.len() {
        return incompatible("block count", &snapshot.len(), &target.len());
    }
    Ok(())
}

fn compare_port_bindings(
    block: &str,
    direction: &str,
    snapshot: &[ConnectorKey],
    target: &[ConnectorKey],
) -> Result<(), EngineStateError> {
    for (index, (left, right)) in snapshot.iter().zip(target).enumerate() {
        if left != right {
            return incompatible(
                &format!("block {block} {direction} binding {index}"),
                left,
                right,
            );
        }
    }
    if snapshot.len() != target.len() {
        return incompatible(
            &format!("block {block} {direction} count"),
            &snapshot.len(),
            &target.len(),
        );
    }
    Ok(())
}

pub(crate) fn block_subject<S: Store>(engine: &Engine<S>, block_index: usize) -> String {
    engine.model.blocks[block_index]
        .instance_iri
        .as_deref()
        .map(crate::state_diagnostics::bounded_text)
        .unwrap_or_else(|| {
            crate::state_diagnostics::bounded_format(format_args!(
                "{}#b{block_index}",
                engine.model.blocks[block_index].class_iri
            ))
        })
}

pub(crate) fn fingerprint(execution_revision: u32, manifest_bytes: &[u8]) -> u128 {
    let mut hash = StableHash::new();
    hash.write_u32(execution_revision);
    hash.write_bytes(manifest_bytes);
    hash.finish()
}

pub(crate) fn requires_target(class_path: &str) -> bool {
    TARGET_BOUND_CLASSES.binary_search(&class_path).is_ok()
}

fn block_key<S: Store>(
    engine: &Engine<S>,
    index: usize,
    durable: bool,
) -> Result<BlockKey, EngineStateError> {
    let block = &engine.model.blocks[index];
    if let Some(instance_iri) = &block.instance_iri {
        return Ok(BlockKey::Authored(instance_iri.to_string()));
    }
    if block.class_iri.starts_with(PASS_THROUGH_PREFIX)
        && block.inputs.len() == 1
        && block.outputs.len() == 1
    {
        let input = &engine.model.connectors[block.inputs[0].0 as usize];
        let output = &engine.model.connectors[block.outputs[0].0 as usize];
        if let (Some(input_path), Some(output_path)) = (&input.iri, &output.iri) {
            return Ok(BlockKey::PassThrough {
                input_path: input_path.to_string(),
                output_path: output_path.to_string(),
            });
        }
    }
    if durable {
        Err(EngineStateError::IneligibleModel {
            subject: block_subject(engine, index),
            detail: "durable block has no authored identity".into(),
        })
    } else {
        Ok(BlockKey::Dense(block.id.0))
    }
}

fn assign_connector_key(
    keys: &mut [Option<ConnectorKey>],
    connector_id: ConnectorId,
    key: ConnectorKey,
) -> Result<(), EngineStateError> {
    let dense = connector_id.0 as usize;
    let slot = keys
        .get_mut(dense)
        .ok_or_else(|| EngineStateError::IneligibleModel {
            subject: format!("connector#{dense}"),
            detail: "connector id is outside the arena".into(),
        })?;
    if slot.replace(key).is_some() {
        return Err(EngineStateError::IneligibleModel {
            subject: format!("connector#{dense}"),
            detail: "connector occurs in more than one ordered port list".into(),
        });
    }
    Ok(())
}

fn wire_value_type(
    value_type: ValueType,
    enums: &mut BTreeMap<String, Vec<String>>,
) -> Result<WireValueType, EngineStateError> {
    Ok(match value_type {
        ValueType::Real => WireValueType::Real,
        ValueType::Integer => WireValueType::Integer,
        ValueType::Boolean => WireValueType::Boolean,
        ValueType::String => WireValueType::String,
        ValueType::Enum(class) => {
            let descriptor =
                enum_descriptor(class).ok_or_else(|| EngineStateError::IneligibleModel {
                    subject: format!("enum-class#{}", class.0),
                    detail: "enum class has no canonical reverse descriptor".into(),
                })?;
            record_enum_descriptor(descriptor.class_path, descriptor.members, enums)?;
            WireValueType::Enum(descriptor.class_path.to_owned())
        }
    })
}

fn record_wire_value_enum(
    value: &WireValue,
    enums: &mut BTreeMap<String, Vec<String>>,
) -> Result<(), EngineStateError> {
    if let WireValue::Enum {
        class_path,
        ordinal,
    } = value
    {
        let descriptor = oce_model::enum_descriptor_by_path(class_path).ok_or_else(|| {
            EngineStateError::IneligibleModel {
                subject: class_path.clone(),
                detail: "enum value references an unknown canonical class".into(),
            }
        })?;
        if *ordinal == 0 || *ordinal as usize > descriptor.members.len() {
            return Err(EngineStateError::IneligibleModel {
                subject: class_path.clone(),
                detail: format!("enum ordinal {ordinal} is outside its canonical descriptor"),
            });
        }
        record_enum_descriptor(descriptor.class_path, descriptor.members, enums)?;
    }
    Ok(())
}

fn record_enum_descriptor(
    class_path: &str,
    members: &[&str],
    enums: &mut BTreeMap<String, Vec<String>>,
) -> Result<(), EngineStateError> {
    let members = members
        .iter()
        .map(|member| (*member).to_owned())
        .collect::<Vec<_>>();
    if let Some(existing) = enums.insert(class_path.to_owned(), members.clone())
        && existing != members
    {
        return Err(EngineStateError::IneligibleModel {
            subject: class_path.to_owned(),
            detail: "enum class resolves to inconsistent member lists".into(),
        });
    }
    Ok(())
}

fn compare_named<T: fmt::Debug + PartialEq>(
    kind: &str,
    snapshot: &[T],
    target: &[T],
    name: impl Fn(&T) -> String,
) -> Result<(), EngineStateError> {
    for (left, right) in snapshot.iter().zip(target) {
        if left != right {
            return incompatible(&format!("{kind} {}", name(left)), left, right);
        }
    }
    if snapshot.len() != target.len() {
        return incompatible(&format!("{kind} count"), &snapshot.len(), &target.len());
    }
    Ok(())
}

fn incompatible<T: fmt::Debug, U: fmt::Debug>(
    subject: &str,
    snapshot: &T,
    target: &U,
) -> Result<(), EngineStateError> {
    Err(crate::state_diagnostics::incompatible(
        subject, snapshot, target,
    ))
}

fn too_large(actual: usize) -> EngineStateError {
    EngineStateError::SnapshotTooLarge {
        actual_bytes: u64::try_from(actual).unwrap_or(u64::MAX),
        max_bytes: crate::state::MAX_SNAPSHOT_BYTES,
    }
}
