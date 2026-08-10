//! Shared enum-reference and permutation checks for decoded execution manifests.

use std::collections::{BTreeMap, BTreeSet};

use oce_blocks::BlockKind;
use oce_model::enum_descriptor_by_path;

use crate::state::{
    EngineStateError, ExecutionManifest, Portability, WireDir, WireValue, WireValueType,
};

pub(crate) fn validate_value_enum<'a>(
    value: &'a WireValue,
    descriptors: &BTreeMap<&str, usize>,
    referenced: &mut BTreeSet<&'a str>,
    malformed: &impl Fn(String) -> EngineStateError,
) -> Result<(), EngineStateError> {
    if let WireValue::Enum {
        class_path,
        ordinal,
    } = value
    {
        let member_count = descriptors.get(class_path.as_str()).ok_or_else(|| {
            malformed(format!(
                "unknown enum value class '{}'",
                crate::state_diagnostics::bounded_text(class_path)
            ))
        })?;
        if *ordinal == 0 || *ordinal as usize > *member_count {
            return Err(malformed(format!(
                "enum value {}#{ordinal} is outside its descriptor",
                crate::state_diagnostics::bounded_text(class_path)
            )));
        }
        referenced.insert(class_path);
    }
    Ok(())
}

pub(crate) fn require_exact_set<'a, T: Ord + Clone + 'a>(
    values: &[T],
    expected: impl Iterator<Item = &'a T>,
    detail: &str,
    malformed: &impl Fn(String) -> EngineStateError,
) -> Result<(), EngineStateError> {
    let set = values.iter().cloned().collect::<BTreeSet<_>>();
    if set.len() != values.len() || set != expected.cloned().collect() {
        Err(malformed(format!(
            "{detail} is not a permutation of its manifest section"
        )))
    } else {
        Ok(())
    }
}

pub(crate) fn validate_manifest(
    manifest: &ExecutionManifest,
    validation_offset: u64,
    current_execution_revision: bool,
) -> Result<(), EngineStateError> {
    let malformed = |detail: String| EngineStateError::MalformedSnapshot {
        offset: validation_offset,
        detail,
    };
    let mut enum_paths = BTreeMap::new();
    for entry in &manifest.enums {
        if entry.members.is_empty() {
            return Err(malformed(format!(
                "enum descriptor '{}' has no members",
                crate::state_diagnostics::bounded_text(&entry.class_path)
            )));
        }
        if current_execution_revision {
            let descriptor = enum_descriptor_by_path(&entry.class_path).ok_or_else(|| {
                malformed(format!(
                    "unknown enum descriptor '{}'",
                    crate::state_diagnostics::bounded_text(&entry.class_path)
                ))
            })?;
            if !entry
                .members
                .iter()
                .map(AsRef::as_ref)
                .eq(descriptor.members.iter().copied())
            {
                return Err(malformed(format!(
                    "enum descriptor '{}' has the wrong member order",
                    crate::state_diagnostics::bounded_text(&entry.class_path)
                )));
            }
        }
        enum_paths.insert(entry.class_path.as_str(), entry.members.len());
    }
    let blocks = manifest
        .blocks
        .iter()
        .map(|entry| (&entry.key, entry))
        .collect::<BTreeMap<_, _>>();
    let connectors = manifest
        .connectors
        .iter()
        .map(|entry| (&entry.key, entry))
        .collect::<BTreeMap<_, _>>();
    let mut referenced_enums = BTreeSet::new();
    let mut expected_connectors = BTreeSet::new();
    let mut stateful = BTreeMap::new();
    for entry in &manifest.blocks {
        match entry.kind {
            BlockKind::Algebraic if entry.state_revision != 0 || entry.state_len != 0 => {
                return Err(malformed(format!(
                    "algebraic block '{}' carries a state contract",
                    crate::state_diagnostics::bounded_block_subject(&entry.key)
                )));
            }
            BlockKind::Stateful if entry.state_revision == 0 || entry.state_len == 0 => {
                return Err(malformed(format!(
                    "stateful block '{}' has no state contract",
                    crate::state_diagnostics::bounded_block_subject(&entry.key)
                )));
            }
            BlockKind::Stateful => {
                stateful.insert(&entry.key, entry.state_len);
            }
            BlockKind::Algebraic => {}
        }
        for (_, value) in &entry.params {
            validate_value_enum(value, &enum_paths, &mut referenced_enums, &malformed)?;
        }
        for (index, key) in entry.inputs.iter().enumerate() {
            if key.owner != entry.key
                || key.direction != WireDir::In
                || key.port_index as usize != index
            {
                return Err(malformed(
                    "block input key disagrees with ordered port position".into(),
                ));
            }
            if !expected_connectors.insert(key) {
                return Err(malformed(
                    "connector occurs in multiple block port lists".into(),
                ));
            }
        }
        for (index, key) in entry.outputs.iter().enumerate() {
            if key.owner != entry.key
                || key.direction != WireDir::Out
                || key.port_index as usize != index
            {
                return Err(malformed(
                    "block output key disagrees with ordered port position".into(),
                ));
            }
            if !expected_connectors.insert(key) {
                return Err(malformed(
                    "connector occurs in multiple block port lists".into(),
                ));
            }
        }
    }
    if expected_connectors != connectors.keys().copied().collect() {
        return Err(malformed(
            "block port keys and connector section differ".into(),
        ));
    }
    for entry in &manifest.connectors {
        if let WireValueType::Enum(class_path) = &entry.value_type {
            referenced_enums.insert(class_path.as_str());
        }
    }
    if referenced_enums != enum_paths.keys().copied().collect() {
        return Err(malformed(
            "enum descriptor set does not exactly match referenced classes".into(),
        ));
    }
    for (from, to) in &manifest.connections {
        if from.direction != WireDir::Out
            || to.direction != WireDir::In
            || !connectors.contains_key(from)
            || !connectors.contains_key(to)
        {
            return Err(malformed(
                "connection endpoint is missing or has the wrong direction".into(),
            ));
        }
    }
    require_exact_set(
        &manifest.schedule,
        blocks.keys().copied(),
        "block schedule",
        &malformed,
    )?;
    require_exact_set(
        &manifest.connector_order,
        connectors.keys().copied(),
        "connector schedule",
        &malformed,
    )?;
    if manifest.driver_of.len() != connectors.len() {
        return Err(malformed(
            "driver map count differs from connector count".into(),
        ));
    }
    for (key, driver) in &manifest.driver_of {
        if !connectors.contains_key(key) || !connectors.contains_key(driver) {
            return Err(malformed(
                "driver map references an unknown connector".into(),
            ));
        }
    }
    let slot_keys = manifest
        .state_slots
        .iter()
        .map(|(key, _, _)| key)
        .collect::<BTreeSet<_>>();
    if slot_keys != stateful.keys().copied().collect() {
        return Err(malformed(
            "state-slot set differs from stateful blocks".into(),
        ));
    }
    let mut ranges = Vec::with_capacity(manifest.state_slots.len());
    for (key, offset, length) in &manifest.state_slots {
        if stateful.get(key) != Some(length) {
            return Err(malformed(
                "state-slot length differs from its block descriptor".into(),
            ));
        }
        let end = offset
            .checked_add(*length)
            .ok_or_else(|| malformed("state-slot range overflows".into()))?;
        ranges.push((*offset, end));
    }
    ranges.sort_unstable();
    let mut expected_offset = 0;
    for (offset, end) in ranges {
        if offset != expected_offset {
            return Err(malformed("state-slot ranges are not contiguous".into()));
        }
        expected_offset = end;
    }
    let mut external = BTreeSet::new();
    for key in &manifest.external_inputs {
        if key.direction != WireDir::In || !connectors.contains_key(key) || !external.insert(key) {
            return Err(malformed(
                "external inputs contain an invalid or duplicate key".into(),
            ));
        }
    }
    let mut boundary_names = BTreeSet::new();
    for (iri, source) in &manifest.boundary_outputs {
        if source.direction != WireDir::Out
            || !connectors.contains_key(source)
            || !boundary_names.insert(iri)
        {
            return Err(malformed(
                "boundary outputs contain an invalid source or duplicate IRI".into(),
            ));
        }
    }
    if current_execution_revision {
        let target_bound = manifest
            .blocks
            .iter()
            .any(|block| crate::state_manifest::requires_target(&block.class_path));
        if target_bound != matches!(manifest.portability, Portability::TargetBound { .. }) {
            return Err(malformed(
                "arithmetic-portability tag disagrees with manifest class set".into(),
            ));
        }
    }
    if let Portability::TargetBound { arch, os } = &manifest.portability
        && (arch.is_empty() || os.is_empty())
    {
        return Err(malformed(
            "target-bound descriptor contains an empty target string".into(),
        ));
    }
    Ok(())
}
