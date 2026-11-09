//! The encoder: a loaded executable and its boundary I/O table to canonical `OCEB` bytes.
//!
//! Canonical means one byte sequence per executable on every architecture: fixed-width
//! little-endian fields, no `usize` on the wire, Reals as their bits, and one order per list —
//! classes and I/O by UTF-8 bytes, instances by `BlockId`, connections by consumer id, parameters
//! in table order, and the schedule as carried.

use alloc::{vec, vec::Vec};

use oce_blocks::{BlockKind, PortKind, class_signature_hash};
use oce_model::{Dir, Value, ValueType};

use super::error::{BlobError, EncodeFault, OutputFault};
use super::sections::{CLASSES, CONNECTIONS, INSTANCES, IO, SCHEDULE};
use super::wire::{Writer, crc32c, state_crc, unencodable};
use super::{
    BlobInput, BlobOutput, BlobSource, ENGINE_VERSION, EXECUTION_PROFILE, FORMAT_VERSION, MAGIC,
    MAX_BLOB_BYTES,
};
use crate::MAX_STATE_WORDS;

/// Encode `source` as one canonical `OCEB` blob.
///
/// The encoder computes each class's signature hash from this build's registry and each stateful
/// block's initial-state CRC by running `init_state` on a fresh region; it carries no live state.
/// It checks what it needs to write well-formed bytes, not everything the decoder checks: a caller
/// should [`decode()`](super::decode()) the result before trusting it, as
/// `oce_api::Engine::compile_blob` does.
///
/// # Errors
/// [`BlobError::Unencodable`] for a non-dense id, a port outside the connector arena, a connector
/// whose type differs from its port kind, an Enum without a canonical descriptor, a non-finite Real
/// parameter, a duplicate input path, a non-finite or crossed input bound, or a count beyond
/// `u32`; [`BlobError::InvalidOutput`] with
/// [`OutputFault::DuplicatePath`] for two outputs with one path; [`BlobError::UnknownClass`] for an
/// unregistered class; [`BlobError::StateTooLarge`] for a state region beyond
/// [`MAX_STATE_WORDS`]; [`BlobError::TooLarge`] for a result beyond [`MAX_BLOB_BYTES`].
pub fn encode(source: &BlobSource<'_>) -> Result<Vec<u8>, BlobError> {
    let model = source.model;
    if source.blocks.len() != model.blocks.len() {
        return Err(unencodable(EncodeFault::BlockCountMismatch, 0));
    }
    for (index, block) in model.blocks.iter().enumerate() {
        if block.id.0 as usize != index {
            return Err(unencodable(EncodeFault::IdNotDense, index_u32(index)));
        }
    }
    for (index, connector) in model.connectors.iter().enumerate() {
        if connector.id.0 as usize != index {
            return Err(unencodable(EncodeFault::IdNotDense, index_u32(index)));
        }
    }

    // Classes: distinct class paths, strictly increasing by UTF-8 bytes.
    let mut classes: Vec<&str> = model.blocks.iter().map(|b| b.class_iri.as_ref()).collect();
    classes.sort_unstable_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    classes.dedup();

    let mut w = Writer::new();
    w.raw(&MAGIC);
    w.u16(FORMAT_VERSION);
    w.u16(EXECUTION_PROFILE);
    w.u16(ENGINE_VERSION.major);
    w.u16(ENGINE_VERSION.minor);
    w.u16(ENGINE_VERSION.patch);
    w.u16(0);
    let total_at = w.len();
    w.u32(0);
    w.raw(&source.content_id.unwrap_or([0u8; 32]));

    let start = open_section(&mut w, CLASSES);
    w.count(classes.len())?;
    for (index, path) in classes.iter().enumerate() {
        let hash = class_signature_hash(path).ok_or(BlobError::UnknownClass {
            class: index_u32(index),
        })?;
        w.str(path)?;
        w.u64(hash);
    }
    close_section(&mut w, start)?;

    let start = open_section(&mut w, INSTANCES);
    instances(&mut w, source, &classes)?;
    close_section(&mut w, start)?;

    let start = open_section(&mut w, SCHEDULE);
    w.count(source.schedule.order.len())?;
    for block in &source.schedule.order {
        w.u32(block.0);
    }
    close_section(&mut w, start)?;

    let start = open_section(&mut w, CONNECTIONS);
    let mut connections: Vec<(u32, u32)> = model
        .connections
        .iter()
        .map(|c| (c.to.0, c.from.0))
        .collect();
    connections.sort_unstable();
    w.count(connections.len())?;
    for (to, from) in connections {
        w.u32(from);
        w.u32(to);
    }
    close_section(&mut w, start)?;

    let start = open_section(&mut w, IO);
    inputs(&mut w, source.inputs)?;
    outputs(&mut w, source.outputs)?;
    close_section(&mut w, start)?;

    let total = w.len() + 4;
    if total > MAX_BLOB_BYTES {
        return Err(BlobError::TooLarge {
            length: u64::try_from(total).unwrap_or(u64::MAX),
            limit: MAX_BLOB_BYTES as u64,
        });
    }
    w.patch_u32(total_at, index_u32(total));
    let mut bytes = w.into_bytes();
    let crc = crc32c(&bytes);
    bytes.extend_from_slice(&crc.to_le_bytes());
    Ok(bytes)
}

fn open_section(w: &mut Writer, id: u32) -> usize {
    w.u32(id);
    let at = w.len();
    w.u32(0);
    at
}

fn close_section(w: &mut Writer, length_at: usize) -> Result<(), BlobError> {
    let length = w.len() - length_at - 4;
    if length > MAX_BLOB_BYTES {
        return Err(BlobError::TooLarge {
            length: u64::try_from(w.len()).unwrap_or(u64::MAX),
            limit: MAX_BLOB_BYTES as u64,
        });
    }
    w.patch_u32(length_at, index_u32(length));
    Ok(())
}

fn instances(w: &mut Writer, source: &BlobSource<'_>, classes: &[&str]) -> Result<(), BlobError> {
    let model = source.model;
    w.count(model.blocks.len())?;
    w.count(model.connectors.len())?;
    for (index, (block, instance)) in model.blocks.iter().zip(source.blocks).enumerate() {
        let block_index = index_u32(index);
        let class = classes
            .binary_search_by(|probe| probe.as_bytes().cmp(block.class_iri.as_bytes()))
            .unwrap_or(0);
        w.count(class)?;
        let (kind, state_len, crc) = match instance.kind() {
            BlockKind::Algebraic => (0, 0, 0),
            BlockKind::Stateful => {
                let len = instance.state_len();
                if len > MAX_STATE_WORDS {
                    return Err(BlobError::StateTooLarge {
                        block: block_index,
                        requested: u64::try_from(len).unwrap_or(u64::MAX),
                    });
                }
                let mut region = vec![0u64; len];
                instance.init_state(&mut region, &block.params);
                (1, len, state_crc(&region))
            }
        };
        w.u8(kind);
        w.count(state_len)?;
        w.u32(crc);
        let signature = instance.resolved_signature();
        for (ports, kinds, dir) in [
            (&block.inputs, &signature.inputs, Dir::In),
            (&block.outputs, &signature.outputs, Dir::Out),
        ] {
            w.count(ports.len())?;
            for (port, id) in ports.iter().enumerate() {
                let connector = model
                    .connectors
                    .get(id.0 as usize)
                    .ok_or(unencodable(EncodeFault::ConnectorOutOfRange, id.0))?;
                let typed = kinds
                    .get(port)
                    .is_some_and(|kind| port_value_type(*kind) == connector.value_type);
                if !typed || connector.dir != dir || connector.block != block.id {
                    return Err(unencodable(EncodeFault::ConnectorTypeMismatch, id.0));
                }
                w.u32(id.0);
            }
        }
        w.count(block.params.values.len())?;
        for (name, value) in &block.params.values {
            if let Value::Real(x) = value
                && !x.is_finite()
            {
                return Err(unencodable(EncodeFault::NonFiniteParameter, block_index));
            }
            w.str(name)?;
            w.value(value, block_index)?;
        }
    }
    Ok(())
}

fn port_value_type(kind: PortKind) -> ValueType {
    match kind {
        PortKind::Real => ValueType::Real,
        PortKind::Integer => ValueType::Integer,
        PortKind::Boolean => ValueType::Boolean,
    }
}

fn inputs(w: &mut Writer, inputs: &[BlobInput<'_>]) -> Result<(), BlobError> {
    let mut sorted: Vec<&BlobInput<'_>> = inputs.iter().collect();
    sorted.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
    if let Some(index) = sorted
        .windows(2)
        .position(|pair| pair[0].path == pair[1].path)
    {
        return Err(unencodable(
            EncodeFault::DuplicateInputPath,
            index_u32(index + 1),
        ));
    }
    w.count(sorted.len())?;
    for (index, input) in sorted.iter().enumerate() {
        let index = index_u32(index);
        if !super::decode::bounds_are_valid(input.min.as_ref(), input.max.as_ref()) {
            return Err(unencodable(EncodeFault::InvalidInputBound, index));
        }
        w.str(input.path)?;
        w.value_type(input.value_type, index)?;
        w.optional_value(input.min.as_ref(), index)?;
        w.optional_value(input.max.as_ref(), index)?;
        let mut targets: Vec<u32> = input.targets.iter().map(|t| t.0).collect();
        targets.sort_unstable();
        w.count(targets.len())?;
        for target in targets {
            w.u32(target);
        }
        w.bytes_field(input.semantic_point)?;
    }
    Ok(())
}

fn outputs(w: &mut Writer, outputs: &[BlobOutput<'_>]) -> Result<(), BlobError> {
    let mut sorted: Vec<&BlobOutput<'_>> = outputs.iter().collect();
    sorted.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
    if let Some(index) = sorted
        .windows(2)
        .position(|pair| pair[0].path == pair[1].path)
    {
        return Err(BlobError::InvalidOutput {
            output: index_u32(index + 1),
            fault: OutputFault::DuplicatePath,
        });
    }
    w.count(sorted.len())?;
    for output in sorted {
        w.str(output.path)?;
        w.u32(output.source.0);
        w.bytes_field(output.semantic_point)?;
    }
    Ok(())
}

fn index_u32(index: usize) -> u32 {
    u32::try_from(index).unwrap_or(u32::MAX)
}
