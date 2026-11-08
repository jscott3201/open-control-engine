//! The header, the five section frames, one parser per record, and the structural pass.
//!
//! The structural pass walks every section once with the record parsers and allocates nothing. It
//! proves that every count fits its section, every string is UTF-8, every tag is in its table,
//! every list is in canonical order and every section is consumed exactly, so that the building
//! pass in `decode.rs` can size its allocations from counts already proven against the input.

use oce_blocks::{BlockKind, MAX_RESOLVED_ARITY};

use super::error::{BlobError, MalformedFault};
use super::wire::{RawType, RawValue, Reader, U32List, crc32c};
use super::{
    ENGINE_VERSION, EXECUTION_PROFILE, EngineVersion, FORMAT_VERSION, HEADER_BYTES, MAGIC,
    MAX_BLOB_BYTES, TRAILER_BYTES,
};

/// Section ids, in their one fixed order.
pub(super) const CLASSES: u32 = 1;
pub(super) const INSTANCES: u32 = 2;
pub(super) const SCHEDULE: u32 = 3;
pub(super) const CONNECTIONS: u32 = 4;
pub(super) const IO: u32 = 5;

/// Minimum encoded bytes of one record, used to bound counts before anything is allocated.
const MIN_CLASS: usize = 4 + 8;
const MIN_INSTANCE: usize = 4 + 1 + 4 + 4 + 4 + 4 + 4;
const MIN_PARAMETER: usize = 4 + 2;
const MIN_CONNECTION: usize = 8;
const MIN_INPUT: usize = 4 + 1 + 1 + 1 + 4 + 4;
const MIN_OUTPUT: usize = 4 + 4 + 4;

/// One section's payload and the absolute offset of its first byte.
#[derive(Clone, Copy)]
pub(super) struct Section<'a> {
    pub(super) payload: &'a [u8],
    pub(super) offset: usize,
}

impl<'a> Section<'a> {
    pub(super) fn reader(&self) -> Reader<'a> {
        Reader::new(self.payload, self.offset)
    }
}

/// A blob whose header, integrity and section frames have been checked.
pub(super) struct Layout<'a> {
    pub(super) content_id: [u8; 32],
    pub(super) classes: Section<'a>,
    pub(super) instances: Section<'a>,
    pub(super) schedule: Section<'a>,
    pub(super) connections: Section<'a>,
    pub(super) io: Section<'a>,
}

/// Counts the structural pass proved, for sizing the building pass's allocations.
pub(super) struct Counts {
    pub(super) largest_parameter_list: usize,
}

/// Refusal steps 1 to 5 and the section frames: size, header, format, CRC, profile, engine
/// version and flags, then five sections in order whose lengths exactly fill the body.
pub(super) fn frame(bytes: &[u8]) -> Result<Layout<'_>, BlobError> {
    if bytes.len() > MAX_BLOB_BYTES {
        return Err(BlobError::TooLarge {
            length: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            limit: MAX_BLOB_BYTES as u64,
        });
    }
    let mut header = Reader::new(bytes, 0);
    if bytes.len() < HEADER_BYTES + TRAILER_BYTES {
        return Err(BlobError::Malformed {
            offset: u32::try_from(bytes.len()).unwrap_or(u32::MAX),
            fault: MalformedFault::Truncated,
        });
    }
    if header.take(4)? != MAGIC {
        return Err(BlobError::Malformed {
            offset: 0,
            fault: MalformedFault::BadMagic,
        });
    }
    let format = header.u16()?;
    let profile = header.u16()?;
    let found = EngineVersion {
        major: header.u16()?,
        minor: header.u16()?,
        patch: header.u16()?,
    };
    let flags = header.u16()?;
    let length_at = header.clone();
    let total = header.u32()?;
    if usize::try_from(total).ok() != Some(bytes.len()) {
        return length_at.malformed(MalformedFault::LengthMismatch);
    }
    if format != FORMAT_VERSION {
        return Err(BlobError::UnsupportedFormat {
            found: format,
            supported: FORMAT_VERSION,
        });
    }
    let body_end = bytes.len() - TRAILER_BYTES;
    let mut trailer = [0u8; 4];
    trailer.copy_from_slice(&bytes[body_end..]);
    let stored = u32::from_le_bytes(trailer);
    let computed = crc32c(&bytes[..body_end]);
    if stored != computed {
        return Err(BlobError::IntegrityMismatch { stored, computed });
    }
    if profile != EXECUTION_PROFILE {
        return Err(BlobError::UnsupportedProfile {
            found: profile,
            supported: EXECUTION_PROFILE,
        });
    }
    if found != ENGINE_VERSION {
        return Err(BlobError::EngineVersionMismatch {
            found,
            expected: ENGINE_VERSION,
        });
    }
    if flags != 0 {
        return Err(BlobError::NonzeroFlags { flags });
    }
    let mut content_id = [0u8; 32];
    content_id.copy_from_slice(header.take(32)?);

    let mut body = Reader::new(&bytes[HEADER_BYTES..body_end], HEADER_BYTES);
    let classes = section(&mut body, CLASSES)?;
    let instances = section(&mut body, INSTANCES)?;
    let schedule = section(&mut body, SCHEDULE)?;
    let connections = section(&mut body, CONNECTIONS)?;
    let io = section(&mut body, IO)?;
    if !body.is_empty() {
        return body.malformed(MalformedFault::TrailingBytes);
    }
    Ok(Layout {
        content_id,
        classes,
        instances,
        schedule,
        connections,
        io,
    })
}

fn section<'a>(body: &mut Reader<'a>, expected: u32) -> Result<Section<'a>, BlobError> {
    let id_at = body.clone();
    if body.u32()? != expected {
        return id_at.malformed(MalformedFault::SectionId);
    }
    let length_at = body.clone();
    let length = usize::try_from(body.u32()?).unwrap_or(usize::MAX);
    if length > body.remaining() {
        return length_at.malformed(MalformedFault::SectionLength);
    }
    let offset = usize::try_from(body.offset()).unwrap_or(usize::MAX);
    Ok(Section {
        payload: body.take(length)?,
        offset,
    })
}

/// One class record: the canonical class path and the encoder's class signature hash.
pub(super) struct ClassRecord<'a> {
    pub(super) path: &'a str,
    pub(super) hash: u64,
}

pub(super) fn class_record<'a>(r: &mut Reader<'a>) -> Result<ClassRecord<'a>, BlobError> {
    Ok(ClassRecord {
        path: r.str()?,
        hash: r.u64()?,
    })
}

/// One instance record. Parameters stay encoded; [`ParamList::reader`] re-walks them.
pub(super) struct InstanceRecord<'a> {
    pub(super) class: u32,
    pub(super) kind: BlockKind,
    pub(super) state_len: u32,
    pub(super) state_crc: u32,
    pub(super) inputs: U32List<'a>,
    pub(super) outputs: U32List<'a>,
    pub(super) params: ParamList<'a>,
}

/// An instance's encoded parameter table: `count` (name, value) pairs.
pub(super) struct ParamList<'a> {
    pub(super) count: usize,
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> ParamList<'a> {
    /// A reader positioned at the first parameter.
    pub(super) fn reader(&self) -> Reader<'a> {
        Reader::new(self.bytes, self.offset)
    }
}

/// One parameter: its name, its raw value and the value's absolute offset.
pub(super) fn parameter<'a>(r: &mut Reader<'a>) -> Result<(&'a str, RawValue<'a>, u32), BlobError> {
    let name = r.str()?;
    let at = r.offset();
    Ok((name, r.value()?, at))
}

pub(super) fn instance_record<'a>(r: &mut Reader<'a>) -> Result<InstanceRecord<'a>, BlobError> {
    let class = r.u32()?;
    let kind_at = r.clone();
    let kind = match r.u8()? {
        0 => BlockKind::Algebraic,
        1 => BlockKind::Stateful,
        _ => return kind_at.malformed(MalformedFault::InvalidTag),
    };
    let state_len = r.u32()?;
    let state_crc = r.u32()?;
    // No registered class resolves a longer port list, so a longer one is refused here, before the
    // decoder sizes anything by the connector count.
    let inputs = r.u32_list_at_most(MAX_RESOLVED_ARITY)?;
    let outputs = r.u32_list_at_most(MAX_RESOLVED_ARITY)?;
    let count = r.count(MIN_PARAMETER)?;
    let start = r.clone();
    for _ in 0..count {
        parameter(r)?;
    }
    let consumed = start.remaining() - r.remaining();
    let mut params = start;
    let offset = usize::try_from(params.offset()).unwrap_or(usize::MAX);
    Ok(InstanceRecord {
        class,
        kind,
        state_len,
        state_crc,
        inputs,
        outputs,
        params: ParamList {
            count,
            bytes: params.take(consumed)?,
            offset,
        },
    })
}

/// One boundary input record.
pub(super) struct InputRecord<'a> {
    pub(super) path: &'a str,
    pub(super) value_type: RawType,
    pub(super) min: Option<(RawValue<'a>, u32)>,
    pub(super) max: Option<(RawValue<'a>, u32)>,
    pub(super) targets: U32List<'a>,
    pub(super) semantic_point: &'a [u8],
}

pub(super) fn input_record<'a>(r: &mut Reader<'a>) -> Result<InputRecord<'a>, BlobError> {
    let path = r.str()?;
    let value_type = r.value_type()?;
    let min_at = r.offset();
    let min = r.optional_value()?.map(|value| (value, min_at + 1));
    let max_at = r.offset();
    let max = r.optional_value()?.map(|value| (value, max_at + 1));
    Ok(InputRecord {
        path,
        value_type,
        min,
        max,
        targets: r.u32_list()?,
        semantic_point: r.bytes_field()?,
    })
}

/// One boundary output record.
pub(super) struct OutputRecord<'a> {
    pub(super) path: &'a str,
    pub(super) source: u32,
    pub(super) semantic_point: &'a [u8],
}

pub(super) fn output_record<'a>(r: &mut Reader<'a>) -> Result<OutputRecord<'a>, BlobError> {
    Ok(OutputRecord {
        path: r.str()?,
        source: r.u32()?,
        semantic_point: r.bytes_field()?,
    })
}

/// Read the instance section's two leading counts: blocks (bounded by the minimum instance record)
/// and connectors (each appears in exactly one 4-byte port slot, so bounded by a quarter of what
/// remains).
pub(super) fn instance_counts(r: &mut Reader<'_>) -> Result<(usize, usize), BlobError> {
    let blocks_at = r.clone();
    let blocks = usize::try_from(r.u32()?).unwrap_or(usize::MAX);
    let connectors = r.count(4)?;
    let fits = blocks
        .checked_mul(MIN_INSTANCE)
        .is_some_and(|needed| needed <= r.remaining());
    if !fits {
        return blocks_at.malformed(MalformedFault::CountExceedsInput);
    }
    Ok((blocks, connectors))
}

/// Count prefixes, for the building pass (which re-reads them from proven sections).
pub(super) fn class_count(r: &mut Reader<'_>) -> Result<usize, BlobError> {
    r.count(MIN_CLASS)
}

pub(super) fn connection_count(r: &mut Reader<'_>) -> Result<usize, BlobError> {
    r.count(MIN_CONNECTION)
}

pub(super) fn input_count(r: &mut Reader<'_>) -> Result<usize, BlobError> {
    r.count(MIN_INPUT)
}

pub(super) fn output_count(r: &mut Reader<'_>) -> Result<usize, BlobError> {
    r.count(MIN_OUTPUT)
}

/// Refusal step 6: walk every section, allocating nothing.
pub(super) fn check_structure(layout: &Layout<'_>) -> Result<Counts, BlobError> {
    // 1 Classes: strictly increasing by UTF-8 path bytes.
    let mut r = layout.classes.reader();
    let classes = class_count(&mut r)?;
    let mut previous: Option<&str> = None;
    for _ in 0..classes {
        let at = r.clone();
        let record = class_record(&mut r)?;
        if previous.is_some_and(|p| p.as_bytes() >= record.path.as_bytes()) {
            return at.malformed(MalformedFault::NotCanonical);
        }
        previous = Some(record.path);
    }
    exhausted(&r)?;

    // 2 Instances, in BlockId order.
    let mut r = layout.instances.reader();
    let (blocks, _) = instance_counts(&mut r)?;
    let mut largest_parameter_list = 0;
    for _ in 0..blocks {
        let record = instance_record(&mut r)?;
        largest_parameter_list = largest_parameter_list.max(record.params.count);
    }
    exhausted(&r)?;

    // 3 Schedule.
    let mut r = layout.schedule.reader();
    r.u32_list()?;
    exhausted(&r)?;

    // 4 Connections: non-decreasing by consumer id (a repeat is "one driver per input", step 8).
    let mut r = layout.connections.reader();
    let count = connection_count(&mut r)?;
    let mut previous = None;
    for _ in 0..count {
        let at = r.clone();
        r.u32()?;
        let consumer = r.u32()?;
        if previous.is_some_and(|p| p > consumer) {
            return at.malformed(MalformedFault::NotCanonical);
        }
        previous = Some(consumer);
    }
    exhausted(&r)?;

    // 5 I/O: inputs strictly increasing by path with strictly increasing targets, then outputs
    // non-decreasing by path (a repeated path is a duplicate output, step 8).
    let mut r = layout.io.reader();
    let count = input_count(&mut r)?;
    let mut previous: Option<&str> = None;
    for _ in 0..count {
        let at = r.clone();
        let record = input_record(&mut r)?;
        if previous.is_some_and(|p| p.as_bytes() >= record.path.as_bytes()) {
            return at.malformed(MalformedFault::NotCanonical);
        }
        let mut last = None;
        for (index, target) in record.targets.iter().enumerate() {
            if last.is_some_and(|l| l >= target) {
                return Err(BlobError::Malformed {
                    offset: record.targets.offset_of(index),
                    fault: MalformedFault::NotCanonical,
                });
            }
            last = Some(target);
        }
        previous = Some(record.path);
    }
    let count = output_count(&mut r)?;
    let mut previous: Option<&str> = None;
    for _ in 0..count {
        let at = r.clone();
        let record = output_record(&mut r)?;
        if previous.is_some_and(|p| p.as_bytes() > record.path.as_bytes()) {
            return at.malformed(MalformedFault::NotCanonical);
        }
        previous = Some(record.path);
    }
    exhausted(&r)?;

    Ok(Counts {
        largest_parameter_list,
    })
}

fn exhausted(r: &Reader<'_>) -> Result<(), BlobError> {
    if r.is_empty() {
        Ok(())
    } else {
        r.malformed(MalformedFault::TrailingBytes)
    }
}
