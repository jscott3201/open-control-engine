//! Canonical encoding and bounded decoding for execution manifests.

use std::sync::Arc;

use crate::state::{
    BlockKey, BlockManifestEntry, ConnectorKey, ConnectorManifestEntry, EngineStateError,
    EnumManifestEntry, ExecutionManifest, Portability, WireDir, WireValue, WireValueType,
};
use crate::state_wire::{DecodeBudget, Reader, Writer};
use oce_blocks::BlockKind;

pub(crate) fn encode_manifest(
    manifest: &ExecutionManifest,
    allow_dense: bool,
) -> Result<Vec<u8>, EngineStateError> {
    let mut writer = Writer::new();
    match &manifest.portability {
        Portability::CrossPlatform => writer.u8(0),
        Portability::TargetBound { arch, os } => {
            writer.u8(1);
            writer.string(arch)?;
            writer.string(os)?;
        }
    }
    write_count(&mut writer, manifest.enums.len())?;
    for entry in &manifest.enums {
        writer.string(&entry.class_path)?;
        write_count(&mut writer, entry.members.len())?;
        for member in &entry.members {
            writer.string(member)?;
        }
    }
    write_count(&mut writer, manifest.blocks.len())?;
    for entry in &manifest.blocks {
        write_block_key(&mut writer, &entry.key, allow_dense)?;
        writer.string(&entry.class_path)?;
        writer.u8(match entry.kind {
            BlockKind::Algebraic => 0,
            BlockKind::Stateful => 1,
        });
        writer.u32(entry.state_revision);
        writer.u64(entry.state_len);
        write_count(&mut writer, entry.params.len())?;
        for (name, value) in &entry.params {
            writer.string(name)?;
            write_value(&mut writer, value)?;
        }
        write_connector_keys(&mut writer, &entry.inputs, allow_dense)?;
        write_connector_keys(&mut writer, &entry.outputs, allow_dense)?;
    }
    write_count(&mut writer, manifest.connectors.len())?;
    for entry in &manifest.connectors {
        write_connector_key(&mut writer, &entry.key, allow_dense)?;
        writer.string(&entry.path)?;
        writer.u32(entry.declaration_order);
        write_value_type(&mut writer, &entry.value_type)?;
    }
    write_count(&mut writer, manifest.connections.len())?;
    for (from, to) in &manifest.connections {
        write_connector_key(&mut writer, from, allow_dense)?;
        write_connector_key(&mut writer, to, allow_dense)?;
    }
    write_block_keys(&mut writer, &manifest.schedule, allow_dense)?;
    write_connector_keys(&mut writer, &manifest.connector_order, allow_dense)?;
    write_count(&mut writer, manifest.driver_of.len())?;
    for (connector, driver) in &manifest.driver_of {
        write_connector_key(&mut writer, connector, allow_dense)?;
        write_connector_key(&mut writer, driver, allow_dense)?;
    }
    write_count(&mut writer, manifest.state_slots.len())?;
    for (block, offset, length) in &manifest.state_slots {
        write_block_key(&mut writer, block, allow_dense)?;
        writer.u64(*offset);
        writer.u64(*length);
    }
    write_connector_keys(&mut writer, &manifest.external_inputs, allow_dense)?;
    write_count(&mut writer, manifest.boundary_outputs.len())?;
    for (iri, source) in &manifest.boundary_outputs {
        writer.string(iri)?;
        write_connector_key(&mut writer, source, allow_dense)?;
    }
    writer.finish()
}

pub(crate) fn decode_manifest(
    bytes: &[u8],
    base_offset: u64,
    execution_revision: u32,
    budget: DecodeBudget,
) -> Result<ExecutionManifest, EngineStateError> {
    let mut reader = Reader::new(bytes, base_offset, budget);
    let portability = match reader.u8()? {
        0 => Portability::CrossPlatform,
        1 => Portability::TargetBound {
            arch: reader.string()?,
            os: reader.string()?,
        },
        tag => return reader.malformed(format!("invalid arithmetic-portability tag {tag}")),
    };
    let enum_count = read_count(&mut reader, "enum class count")?;
    let mut enums = reader.bounded_vec(enum_count, 8, "enum classes")?;
    for _ in 0..enum_count {
        let class_path = reader.string()?;
        let member_count = read_count(&mut reader, "enum member count")?;
        let index_storage = member_count
            .checked_mul(std::mem::size_of::<&str>())
            .ok_or_else(|| EngineStateError::MalformedSnapshot {
                offset: reader.offset(),
                detail: "enum member index allocation size overflows".into(),
            })?;
        let mut members = reader.bounded_vec(member_count, 4, "enum members")?;
        for _ in 0..member_count {
            let member = reader.string()?;
            reader.claim_allocation(member.len(), "enum member Arc allocation")?;
            let member = Arc::<str>::from(member);
            reader.push(&mut members, member, "enum members")?;
        }
        reader.claim_allocation(index_storage, "enum member index allocation")?;
        let mut member_names = Vec::new();
        member_names.try_reserve_exact(member_count).map_err(|_| {
            EngineStateError::MalformedSnapshot {
                offset: reader.offset(),
                detail: "enum member index allocation failed".into(),
            }
        })?;
        member_names.extend(members.iter().map(AsRef::as_ref));
        member_names.sort_unstable();
        if member_names.windows(2).any(|pair| pair[0] == pair[1]) {
            return reader.malformed("duplicate enum member name");
        }
        push_strict(
            &mut enums,
            EnumManifestEntry {
                class_path,
                members,
            },
            |entry| &entry.class_path,
            &reader,
            "enum classes",
        )?;
    }
    let block_count = read_count(&mut reader, "block count")?;
    let mut blocks = reader.bounded_vec(block_count, 34, "blocks")?;
    for _ in 0..block_count {
        let key = read_block_key(&mut reader)?;
        let class_path = reader.string()?;
        let kind = match reader.u8()? {
            0 => BlockKind::Algebraic,
            1 => BlockKind::Stateful,
            tag => return reader.malformed(format!("invalid block-kind tag {tag}")),
        };
        let state_revision = reader.u32()?;
        let state_len = reader.u64()?;
        let param_count = read_count(&mut reader, "parameter count")?;
        let mut params = reader.bounded_vec(param_count, 6, "parameters")?;
        for _ in 0..param_count {
            let name = reader.string()?;
            let value = read_value(&mut reader)?;
            push_strict(
                &mut params,
                (name, value),
                |entry| &entry.0,
                &reader,
                "parameter names",
            )?;
        }
        let inputs = read_connector_keys(&mut reader, "input connector count")?;
        let outputs = read_connector_keys(&mut reader, "output connector count")?;
        push_strict(
            &mut blocks,
            BlockManifestEntry {
                key,
                class_path,
                kind,
                state_revision,
                state_len,
                params,
                inputs,
                outputs,
            },
            |entry| &entry.key,
            &reader,
            "block keys",
        )?;
    }
    let connector_count = read_count(&mut reader, "connector count")?;
    let mut connectors = reader.bounded_vec(connector_count, 19, "connectors")?;
    for _ in 0..connector_count {
        let key = read_connector_key(&mut reader)?;
        let path = reader.string()?;
        let declaration_order = reader.u32()?;
        let value_type = read_value_type(&mut reader)?;
        push_strict(
            &mut connectors,
            ConnectorManifestEntry {
                key,
                path,
                declaration_order,
                value_type,
            },
            |entry| &entry.key,
            &reader,
            "connector keys",
        )?;
    }
    let connection_count = read_count(&mut reader, "connection count")?;
    let mut connections = reader.bounded_vec(connection_count, 20, "connections")?;
    for _ in 0..connection_count {
        let pair = (
            read_connector_key(&mut reader)?,
            read_connector_key(&mut reader)?,
        );
        push_strict(
            &mut connections,
            pair,
            |entry| entry,
            &reader,
            "connections",
        )?;
    }
    let schedule = read_block_keys(&mut reader, "block schedule count")?;
    let connector_order = read_connector_keys(&mut reader, "connector schedule count")?;
    let driver_count = read_count(&mut reader, "driver count")?;
    let mut driver_of = reader.bounded_vec(driver_count, 20, "driver entries")?;
    for _ in 0..driver_count {
        let pair = (
            read_connector_key(&mut reader)?,
            read_connector_key(&mut reader)?,
        );
        push_strict(
            &mut driver_of,
            pair,
            |entry| &entry.0,
            &reader,
            "driver keys",
        )?;
    }
    let slot_count = read_count(&mut reader, "state slot count")?;
    let mut state_slots = reader.bounded_vec(slot_count, 21, "state slots")?;
    for _ in 0..slot_count {
        let slot = (read_block_key(&mut reader)?, reader.u64()?, reader.u64()?);
        push_strict(
            &mut state_slots,
            slot,
            |entry| &entry.0,
            &reader,
            "state-slot block keys",
        )?;
    }
    let external_inputs = read_connector_keys(&mut reader, "external input count")?;
    let boundary_count = read_count(&mut reader, "boundary output count")?;
    let mut boundary_outputs = reader.bounded_vec(boundary_count, 14, "boundary outputs")?;
    for _ in 0..boundary_count {
        let output = (reader.string()?, read_connector_key(&mut reader)?);
        reader.push(&mut boundary_outputs, output, "boundary outputs")?;
    }
    if !reader.is_empty() {
        return reader.malformed("trailing bytes in execution manifest");
    }
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
    let validation_entries = manifest
        .enums
        .len()
        .checked_mul(2)
        .and_then(|count| {
            manifest
                .blocks
                .len()
                .checked_mul(4)
                .and_then(|blocks| count.checked_add(blocks))
        })
        .and_then(|count| {
            manifest
                .connectors
                .len()
                .checked_mul(4)
                .and_then(|connectors| count.checked_add(connectors))
        })
        .and_then(|count| count.checked_add(manifest.state_slots.len()))
        .and_then(|count| count.checked_add(manifest.external_inputs.len()))
        .and_then(|count| count.checked_add(manifest.boundary_outputs.len()))
        .ok_or_else(|| EngineStateError::MalformedSnapshot {
            offset: reader.offset(),
            detail: "manifest validation workspace size overflows".into(),
        })?;
    let validation_workspace =
        validation_entries
            .checked_mul(128)
            .ok_or_else(|| EngineStateError::MalformedSnapshot {
                offset: reader.offset(),
                detail: "manifest validation workspace size overflows".into(),
            })?;
    reader.claim_allocation(validation_workspace, "manifest validation workspace")?;
    crate::state_manifest_validation::validate_manifest(
        &manifest,
        base_offset.saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX)),
        execution_revision == crate::state::EXECUTION_ABI_REVISION,
    )?;
    reader.claim_allocation(bytes.len(), "canonical manifest encoding")?;
    let canonical = encode_manifest(&manifest, false)?;
    if canonical != bytes {
        let first = canonical
            .iter()
            .zip(bytes)
            .position(|(left, right)| left != right)
            .unwrap_or(canonical.len().min(bytes.len()));
        return Err(EngineStateError::MalformedSnapshot {
            offset: base_offset.saturating_add(first as u64),
            detail: "manifest has a non-canonical encoding".into(),
        });
    }
    Ok(manifest)
}

pub(crate) fn write_block_key(
    writer: &mut Writer,
    key: &BlockKey,
    allow_dense: bool,
) -> Result<(), EngineStateError> {
    match key {
        BlockKey::Authored(iri) => {
            writer.u8(0);
            writer.string(iri)?;
        }
        BlockKey::PassThrough {
            input_path,
            output_path,
        } => {
            writer.u8(1);
            writer.string(input_path)?;
            writer.string(output_path)?;
        }
        BlockKey::Dense(id) if allow_dense => {
            writer.u8(2);
            writer.u32(*id);
        }
        BlockKey::Dense(_) => {
            return Err(EngineStateError::IneligibleModel {
                subject: key.subject(),
                detail: "dense block identity is checkpoint-only".into(),
            });
        }
    }
    Ok(())
}

pub(crate) fn write_connector_key(
    writer: &mut Writer,
    key: &ConnectorKey,
    allow_dense: bool,
) -> Result<(), EngineStateError> {
    write_block_key(writer, &key.owner, allow_dense)?;
    writer.u8(match key.direction {
        WireDir::In => 0,
        WireDir::Out => 1,
    });
    writer.u32(key.port_index);
    Ok(())
}

pub(crate) fn read_connector_key(
    reader: &mut Reader<'_>,
) -> Result<ConnectorKey, EngineStateError> {
    let owner = read_block_key(reader)?;
    let direction = match reader.u8()? {
        0 => WireDir::In,
        1 => WireDir::Out,
        tag => return reader.malformed(format!("invalid connector-direction tag {tag}")),
    };
    Ok(ConnectorKey {
        owner,
        direction,
        port_index: reader.u32()?,
    })
}

pub(crate) fn write_value(writer: &mut Writer, value: &WireValue) -> Result<(), EngineStateError> {
    match value {
        WireValue::Real(bits) => {
            writer.u8(0);
            writer.u64(*bits);
        }
        WireValue::Integer(value) => {
            writer.u8(1);
            writer.i64(*value);
        }
        WireValue::Boolean(value) => {
            writer.u8(2);
            writer.u8(u8::from(*value));
        }
        WireValue::String(value) => {
            writer.u8(3);
            writer.string(value)?;
        }
        WireValue::Enum {
            class_path,
            ordinal,
        } => {
            writer.u8(4);
            writer.string(class_path)?;
            writer.u32(*ordinal);
        }
    }
    Ok(())
}

pub(crate) fn read_value(reader: &mut Reader<'_>) -> Result<WireValue, EngineStateError> {
    Ok(match reader.u8()? {
        0 => WireValue::Real(reader.u64()?),
        1 => WireValue::Integer(reader.i64()?),
        2 => WireValue::Boolean(match reader.u8()? {
            0 => false,
            1 => true,
            value => return reader.malformed(format!("invalid Boolean value {value}")),
        }),
        3 => WireValue::String(reader.string()?),
        4 => WireValue::Enum {
            class_path: reader.string()?,
            ordinal: reader.u32()?,
        },
        tag => return reader.malformed(format!("invalid value tag {tag}")),
    })
}

fn write_value_type(
    writer: &mut Writer,
    value_type: &WireValueType,
) -> Result<(), EngineStateError> {
    match value_type {
        WireValueType::Real => writer.u8(0),
        WireValueType::Integer => writer.u8(1),
        WireValueType::Boolean => writer.u8(2),
        WireValueType::String => writer.u8(3),
        WireValueType::Enum(class_path) => {
            writer.u8(4);
            writer.string(class_path)?;
        }
    }
    Ok(())
}

fn read_value_type(reader: &mut Reader<'_>) -> Result<WireValueType, EngineStateError> {
    Ok(match reader.u8()? {
        0 => WireValueType::Real,
        1 => WireValueType::Integer,
        2 => WireValueType::Boolean,
        3 => WireValueType::String,
        4 => WireValueType::Enum(reader.string()?),
        tag => return reader.malformed(format!("invalid value-type tag {tag}")),
    })
}

fn read_block_key(reader: &mut Reader<'_>) -> Result<BlockKey, EngineStateError> {
    Ok(match reader.u8()? {
        0 => BlockKey::Authored(reader.string()?),
        1 => BlockKey::PassThrough {
            input_path: reader.string()?,
            output_path: reader.string()?,
        },
        2 => return reader.malformed("dense block key is not valid in durable bytes"),
        tag => return reader.malformed(format!("invalid block-key tag {tag}")),
    })
}

fn write_count(writer: &mut Writer, count: usize) -> Result<(), EngineStateError> {
    writer.u32(
        u32::try_from(count).map_err(|_| EngineStateError::SnapshotTooLarge {
            actual_bytes: u64::try_from(count).unwrap_or(u64::MAX),
            max_bytes: crate::state::MAX_SNAPSHOT_BYTES,
        })?,
    );
    Ok(())
}

fn read_count(reader: &mut Reader<'_>, detail: &str) -> Result<usize, EngineStateError> {
    let count = reader.u32()?;
    reader.bounded_count(u64::from(count), 1, detail)
}

fn write_connector_keys(
    writer: &mut Writer,
    keys: &[ConnectorKey],
    allow_dense: bool,
) -> Result<(), EngineStateError> {
    write_count(writer, keys.len())?;
    for key in keys {
        write_connector_key(writer, key, allow_dense)?;
    }
    Ok(())
}

fn read_connector_keys(
    reader: &mut Reader<'_>,
    detail: &str,
) -> Result<Vec<ConnectorKey>, EngineStateError> {
    let count = read_count(reader, detail)?;
    let mut keys = reader.bounded_vec(count, 10, detail)?;
    for _ in 0..count {
        let key = read_connector_key(reader)?;
        reader.push(&mut keys, key, detail)?;
    }
    Ok(keys)
}

fn write_block_keys(
    writer: &mut Writer,
    keys: &[BlockKey],
    allow_dense: bool,
) -> Result<(), EngineStateError> {
    write_count(writer, keys.len())?;
    for key in keys {
        write_block_key(writer, key, allow_dense)?;
    }
    Ok(())
}

fn read_block_keys(
    reader: &mut Reader<'_>,
    detail: &str,
) -> Result<Vec<BlockKey>, EngineStateError> {
    let count = read_count(reader, detail)?;
    let mut keys = reader.bounded_vec(count, 5, detail)?;
    for _ in 0..count {
        let key = read_block_key(reader)?;
        reader.push(&mut keys, key, detail)?;
    }
    Ok(keys)
}

fn push_strict<T, K: Ord + ?Sized>(
    values: &mut Vec<T>,
    value: T,
    key: impl Fn(&T) -> &K,
    reader: &Reader<'_>,
    detail: &str,
) -> Result<(), EngineStateError> {
    if values.last().is_some_and(|prior| key(prior) >= key(&value)) {
        return reader.malformed(format!("{detail} are duplicate or out of order"));
    }
    reader.push(values, value, detail)
}
