//! Exact complete-frame acceptance domains in the execution-state manifest.

use std::collections::BTreeMap;

use crate::state::{EngineStateError, ExecutionManifest, WireValue, WireValueType};
use crate::state_manifest_codec::{read_value, read_value_type, write_value, write_value_type};
use crate::state_wire::{Reader, Writer};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InputManifestEntry {
    pub(crate) path: String,
    pub(crate) value_type: WireValueType,
    pub(crate) min: Option<WireValue>,
    pub(crate) max: Option<WireValue>,
}

pub(crate) fn build_inputs(
    definitions: &[crate::InputDefinition],
) -> Result<Vec<InputManifestEntry>, EngineStateError> {
    definitions
        .iter()
        .map(|entry| {
            let value_type = match entry.value_type {
                oce_model::ValueType::Real => WireValueType::Real,
                oce_model::ValueType::Integer => WireValueType::Integer,
                oce_model::ValueType::Boolean => WireValueType::Boolean,
                oce_model::ValueType::String => WireValueType::String,
                oce_model::ValueType::Enum(class) => WireValueType::Enum(
                    oce_model::enum_descriptor(class)
                        .ok_or_else(|| EngineStateError::IneligibleModel {
                            subject: entry.path.clone(),
                            detail: "input enum has no canonical descriptor".into(),
                        })?
                        .class_path
                        .into(),
                ),
            };
            Ok(InputManifestEntry {
                path: entry.path.clone(),
                value_type,
                min: entry.min.as_ref().map(WireValue::from_value).transpose()?,
                max: entry.max.as_ref().map(WireValue::from_value).transpose()?,
            })
        })
        .collect()
}

pub(crate) fn write_optional_text(
    writer: &mut Writer,
    value: &Option<String>,
) -> Result<(), EngineStateError> {
    match value {
        None => writer.u8(0),
        Some(value) => {
            writer.u8(1);
            writer.string(value)?;
        }
    }
    Ok(())
}

pub(crate) fn read_optional_text(
    reader: &mut Reader<'_>,
) -> Result<Option<String>, EngineStateError> {
    match reader.u8()? {
        0 => Ok(None),
        1 => Ok(Some(reader.string()?)),
        tag => reader.malformed(format!("invalid optional text tag {tag}")),
    }
}

pub(crate) fn write_inputs(
    writer: &mut Writer,
    entries: &[InputManifestEntry],
) -> Result<(), EngineStateError> {
    writer.u32(
        u32::try_from(entries.len()).map_err(|_| EngineStateError::SnapshotTooLarge {
            actual_bytes: u64::try_from(entries.len()).unwrap_or(u64::MAX),
            max_bytes: crate::state::MAX_SNAPSHOT_BYTES,
        })?,
    );
    for entry in entries {
        writer.string(&entry.path)?;
        write_value_type(writer, &entry.value_type)?;
        for bound in [&entry.min, &entry.max] {
            match bound {
                None => writer.u8(0),
                Some(value) => {
                    writer.u8(1);
                    write_value(writer, value)?;
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn read_inputs(
    reader: &mut Reader<'_>,
) -> Result<Vec<InputManifestEntry>, EngineStateError> {
    let count = reader.u32()?;
    let count = reader.bounded_count(u64::from(count), 7, "input definition count")?;
    let mut entries = reader.bounded_vec::<InputManifestEntry>(count, 7, "input definitions")?;
    for _ in 0..count {
        let path = reader.string()?;
        if entries.last().is_some_and(|entry| entry.path >= path) {
            return reader.malformed("input definitions are duplicate or out of order");
        }
        let value_type = read_value_type(reader)?;
        let mut bound = || match reader.u8()? {
            0 => Ok(None),
            1 => Ok(Some(read_value(reader)?)),
            tag => reader.malformed(format!("invalid input bound tag {tag}")),
        };
        let entry = InputManifestEntry {
            path,
            value_type,
            min: bound()?,
            max: bound()?,
        };
        reader.push(&mut entries, entry, "input definitions")?;
    }
    Ok(entries)
}

pub(crate) fn validate_inputs(
    manifest: &ExecutionManifest,
    malformed: &impl Fn(String) -> EngineStateError,
) -> Result<(), EngineStateError> {
    let connectors: BTreeMap<_, _> = manifest.connectors.iter().map(|c| (&c.key, c)).collect();
    let enums: BTreeMap<_, _> = manifest
        .enums
        .iter()
        .map(|e| (&e.class_path, e.members.len()))
        .collect();
    let mut expected = BTreeMap::new();
    for key in &manifest.external_inputs {
        let connector = connectors
            .get(key)
            .ok_or_else(|| malformed("unknown external input".into()))?;
        if expected
            .insert(connector.path.as_str(), &connector.value_type)
            .is_some_and(|old| old != &connector.value_type)
        {
            return Err(malformed("input path has inconsistent types".into()));
        }
    }
    if expected.len() != manifest.input_definitions.len() {
        return Err(malformed(
            "input definitions differ from external input paths".into(),
        ));
    }
    for entry in &manifest.input_definitions {
        if expected.get(entry.path.as_str()) != Some(&&entry.value_type) {
            return Err(malformed(
                "input definition path or type differs from connectors".into(),
            ));
        }
        for bound in [&entry.min, &entry.max].into_iter().flatten() {
            if bound.value_type() != entry.value_type
                || matches!(bound, WireValue::Boolean(_) | WireValue::String(_))
            {
                return Err(malformed("input bound type differs from its domain".into()));
            }
            if let WireValue::Enum {
                class_path,
                ordinal,
            } = bound
                && enums
                    .get(class_path)
                    .is_none_or(|count| *ordinal == 0 || *ordinal as usize > *count)
            {
                return Err(malformed(
                    "input enum bound is outside its descriptor".into(),
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests/state_io_tests.rs"]
mod tests;
