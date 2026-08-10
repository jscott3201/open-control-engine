//! Revision-1 canonical state-snapshot encoder and bounded decoder.

use std::collections::{BTreeMap, BTreeSet};

use crate::stable_hash::StableHash;
use crate::state::{
    ConnectorKey, EngineStateError, FORMAT_REVISION, MAX_SNAPSHOT_BYTES, StateImage, WireValue,
};
use crate::state_manifest_codec::{
    decode_manifest, encode_manifest, read_connector_key, read_value, write_connector_key,
    write_value,
};
use crate::state_wire::{Reader, Writer};

const MAGIC: &[u8; 8] = b"OCESTAT\0";
const HEADER_BYTES: usize = 8 + 4 + 4 + 8;
const TRAILER_BYTES: usize = 16;

pub(crate) fn encode_snapshot(
    image: &StateImage,
    allow_dense: bool,
) -> Result<Vec<u8>, EngineStateError> {
    let manifest = encode_manifest(&image.manifest, allow_dense)?;
    let mut writer = Writer::with_capacity(
        HEADER_BYTES
            .saturating_add(manifest.len())
            .saturating_add(image.words.len().saturating_mul(8))
            .saturating_add(TRAILER_BYTES),
    );
    write_snapshot(&mut writer, image, allow_dense, &manifest)?;
    let mut bytes = writer.finish()?;
    let checksum = fnv(&bytes);
    bytes.extend_from_slice(&checksum.to_le_bytes());
    Ok(bytes)
}

pub(crate) fn encoded_snapshot_len(
    image: &StateImage,
    allow_dense: bool,
) -> Result<usize, EngineStateError> {
    let manifest = encode_manifest(&image.manifest, allow_dense)?;
    let mut writer = Writer::counting();
    write_snapshot(&mut writer, image, allow_dense, &manifest)?;
    Ok(writer.position().saturating_add(TRAILER_BYTES))
}

fn write_snapshot(
    writer: &mut Writer,
    image: &StateImage,
    allow_dense: bool,
    manifest: &[u8],
) -> Result<(), EngineStateError> {
    writer.bytes(MAGIC);
    writer.u32(FORMAT_REVISION);
    writer.u32(image.execution_revision);
    let body_length_offset = writer.position();
    writer.u64(0);
    let body_start = writer.position();
    writer.u128(image.fingerprint);
    writer.string(&image.model_id)?;
    writer.u64(image.state_t);
    match image.prev_t {
        None => writer.u8(0),
        Some(bits) => {
            writer.u8(1);
            writer.u64(bits);
        }
    }
    writer.u64(u64::try_from(manifest.len()).map_err(|_| too_large(manifest.len()))?);
    writer.bytes(manifest);
    writer.u32(u32::try_from(image.values.len()).map_err(|_| too_large(image.values.len()))?);
    let mut previous = None;
    for (key, value) in &image.values {
        if previous.as_ref().is_some_and(|prior| prior >= key) {
            return Err(EngineStateError::MalformedSnapshot {
                offset: u64::try_from(writer.position()).unwrap_or(u64::MAX),
                detail: "connector values are duplicate or out of order".into(),
            });
        }
        write_connector_key(writer, key, allow_dense)?;
        write_value(writer, value)?;
        previous = Some(key.clone());
    }
    writer.u64(u64::try_from(image.words.len()).map_err(|_| too_large(image.words.len()))?);
    for word in &image.words {
        writer.u64(*word);
    }
    writer.u32(0);
    let body_length = writer
        .position()
        .checked_sub(body_start)
        .ok_or_else(|| too_large(writer.position()))?;
    writer.patch_u64(
        body_length_offset,
        u64::try_from(body_length).map_err(|_| too_large(body_length))?,
    );
    crate::state::enforce_size(writer.position().saturating_add(TRAILER_BYTES))?;
    Ok(())
}

pub(crate) fn decode_snapshot(bytes: &[u8]) -> Result<StateImage, EngineStateError> {
    crate::state::enforce_size(bytes.len())?;
    if bytes.len() < HEADER_BYTES + TRAILER_BYTES {
        return malformed(0, "snapshot is shorter than the fixed header and trailer");
    }
    if bytes.get(..MAGIC.len()) != Some(MAGIC) {
        return malformed(0, "invalid state-snapshot magic");
    }
    let mut header = Reader::new(&bytes[8..HEADER_BYTES], 8);
    let format_revision = header.u32()?;
    if format_revision != FORMAT_REVISION {
        return Err(EngineStateError::UnsupportedFormat {
            revision: format_revision,
        });
    }
    let execution_revision = header.u32()?;
    let body_length = header.u64()?;
    let body_length_usize =
        usize::try_from(body_length).map_err(|_| EngineStateError::MalformedSnapshot {
            offset: 16,
            detail: "body length does not fit usize".into(),
        })?;
    let expected_length = HEADER_BYTES
        .checked_add(body_length_usize)
        .and_then(|length| length.checked_add(TRAILER_BYTES))
        .ok_or_else(|| EngineStateError::MalformedSnapshot {
            offset: 16,
            detail: "total snapshot length overflows".into(),
        })?;
    if expected_length != bytes.len() {
        return malformed(
            16,
            format!(
                "body length names {expected_length} total bytes, found {}",
                bytes.len()
            ),
        );
    }
    let trailer_offset = bytes.len() - TRAILER_BYTES;
    let found = u128::from_le_bytes(
        bytes[trailer_offset..]
            .try_into()
            .expect("fixed-size trailer"),
    );
    let expected = fnv(&bytes[..trailer_offset]);
    if expected != found {
        return Err(EngineStateError::IntegrityMismatch { expected, found });
    }
    let body = &bytes[HEADER_BYTES..trailer_offset];
    let mut reader = Reader::new(body, HEADER_BYTES as u64);
    let fingerprint = reader.u128()?;
    let model_id = reader.string()?;
    let state_t_offset = reader.offset();
    let state_t = reader.u64()?;
    if !f64::from_bits(state_t).is_finite() {
        return malformed(state_t_offset, "model time is not finite");
    }
    let prev_t = match reader.u8()? {
        0 => None,
        1 => {
            let offset = reader.offset();
            let bits = reader.u64()?;
            if !f64::from_bits(bits).is_finite() || bits != state_t {
                return malformed(
                    offset,
                    "previous tick time must be finite and bit-equal to model time",
                );
            }
            Some(bits)
        }
        tag => return reader.malformed(format!("invalid previous-time tag {tag}")),
    };
    let manifest_length_offset = reader.offset();
    let manifest_length =
        usize::try_from(reader.u64()?).map_err(|_| EngineStateError::MalformedSnapshot {
            offset: manifest_length_offset,
            detail: "manifest length does not fit usize".into(),
        })?;
    let manifest_offset = reader.offset();
    let manifest_bytes = reader.slice(manifest_length)?;
    let manifest = decode_manifest(manifest_bytes, manifest_offset, execution_revision)?;
    let expected_fingerprint =
        crate::state_manifest::fingerprint(execution_revision, manifest_bytes);
    if fingerprint != expected_fingerprint {
        return malformed(
            HEADER_BYTES as u64,
            "execution fingerprint does not match its manifest",
        );
    }
    let value_count = reader.u32()?;
    let value_count = reader.bounded_count(u64::from(value_count), 2, "connector value count")?;
    let mut values = reader.bounded_vec(value_count, 12, "connector values")?;
    for _ in 0..value_count {
        let key = read_connector_key(&mut reader)?;
        let value = read_value(&mut reader)?;
        if values
            .last()
            .is_some_and(|(prior, _): &(ConnectorKey, WireValue)| prior >= &key)
        {
            return reader.malformed("connector values are duplicate or out of order");
        }
        reader.push(&mut values, (key, value), "connector values")?;
    }
    validate_values(&manifest, &values, reader.offset())?;
    let word_count_offset = reader.offset();
    let word_count = reader.u64()?;
    let word_count = reader.bounded_count(word_count, 8, "state word count")?;
    let mut words = reader.bounded_vec(word_count, 8, "state words")?;
    for _ in 0..word_count {
        let word = reader.u64()?;
        reader.push(&mut words, word, "state words")?;
    }
    let expected_words = manifest
        .state_slots
        .iter()
        .try_fold(0u64, |maximum, (_, offset, length)| {
            offset.checked_add(*length).map(|end| maximum.max(end))
        })
        .ok_or_else(|| EngineStateError::MalformedSnapshot {
            offset: word_count_offset,
            detail: "state-slot range overflows".into(),
        })?;
    if expected_words != u64::try_from(words.len()).unwrap_or(u64::MAX) {
        return malformed(
            word_count_offset,
            "state word count differs from the manifest slots",
        );
    }
    let flags_offset = reader.offset();
    let flags = reader.u32()?;
    if flags != 0 {
        return malformed(
            flags_offset,
            format!("reserved flags are nonzero: {flags:#x}"),
        );
    }
    if !reader.is_empty() {
        return reader.malformed("trailing bytes in snapshot body");
    }
    Ok(StateImage {
        execution_revision,
        fingerprint,
        model_id,
        state_t,
        prev_t,
        manifest,
        values,
        words,
    })
}

fn validate_values(
    manifest: &crate::state::ExecutionManifest,
    values: &[(ConnectorKey, WireValue)],
    offset: u64,
) -> Result<(), EngineStateError> {
    let connectors = manifest
        .connectors
        .iter()
        .map(|entry| (&entry.key, &entry.value_type))
        .collect::<BTreeMap<_, _>>();
    let enums = manifest
        .enums
        .iter()
        .map(|entry| (entry.class_path.as_str(), entry.members.len()))
        .collect::<BTreeMap<_, _>>();
    if values.len() != connectors.len() {
        return malformed(offset, "connector value count differs from the manifest");
    }
    let mut seen = BTreeSet::new();
    for (key, value) in values {
        let expected = connectors
            .get(key)
            .ok_or_else(|| EngineStateError::MalformedSnapshot {
                offset,
                detail: "connector value references a key absent from the manifest".into(),
            })?;
        if &value.value_type() != *expected {
            return malformed(offset, "connector value tag differs from its manifest type");
        }
        if let WireValue::Enum {
            class_path,
            ordinal,
        } = value
        {
            let member_count = enums.get(class_path.as_str()).ok_or_else(|| {
                EngineStateError::MalformedSnapshot {
                    offset,
                    detail: format!(
                        "enum class '{}' is absent from the manifest",
                        crate::state_diagnostics::bounded_text(class_path)
                    ),
                }
            })?;
            if *ordinal == 0 || *ordinal as usize > *member_count {
                return malformed(offset, "connector enum ordinal is outside its class");
            }
        }
        seen.insert(key);
    }
    if seen.len() != connectors.len() {
        return malformed(offset, "connector value key set differs from the manifest");
    }
    Ok(())
}

fn fnv(bytes: &[u8]) -> u128 {
    let mut hash = StableHash::new();
    hash.write_bytes(bytes);
    hash.finish()
}

fn malformed<T>(offset: u64, detail: impl Into<String>) -> Result<T, EngineStateError> {
    Err(EngineStateError::MalformedSnapshot {
        offset,
        detail: detail.into(),
    })
}

fn too_large(actual: usize) -> EngineStateError {
    EngineStateError::SnapshotTooLarge {
        actual_bytes: u64::try_from(actual).unwrap_or(u64::MAX),
        max_bytes: MAX_SNAPSHOT_BYTES,
    }
}
