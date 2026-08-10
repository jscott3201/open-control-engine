//! Register-wide identity, ordering, and lifecycle validation.

use std::collections::{BTreeMap, BTreeSet};

use super::{Party, Register, State, UpstreamStatus, ValidationCode, ValidationError};

pub(super) fn validate_collection(register: &Register) -> Result<(), ValidationError> {
    let mut unique_ids = BTreeSet::new();
    for (index, entry) in register.entries.iter().enumerate() {
        if !unique_ids.insert(entry.id.as_str()) {
            return Err(ValidationError::new(
                ValidationCode::IdDuplicate,
                Some(index),
                "duplicate id",
            ));
        }
    }
    let mut ids = BTreeMap::new();
    let mut subjects = BTreeSet::new();
    let mut producer_cases = BTreeSet::new();
    let mut previous_id: Option<&str> = None;
    for (index, entry) in register.entries.iter().enumerate() {
        if let Some(previous) = previous_id
            && entry.id.as_str() < previous
        {
            return Err(ValidationError::new(
                ValidationCode::EntryOrder,
                Some(index),
                "entries are not ordered by id",
            ));
        }
        previous_id = Some(&entry.id);
        ids.insert(entry.id.as_str(), index);
        if !subjects.insert(&entry.subject) {
            return Err(ValidationError::new(
                ValidationCode::SubjectDuplicate,
                Some(index),
                "duplicate subject",
            ));
        }
        for producer_case in &entry.producer_cases {
            if !producer_cases.insert(producer_case) {
                return Err(ValidationError::new(
                    ValidationCode::ProducerCaseGlobalDuplicate,
                    Some(index),
                    "producer case appears in multiple entries",
                ));
            }
        }
    }
    validate_supersession(register, &ids)?;
    for (index, entry) in register.entries.iter().enumerate() {
        let independent = entry
            .parties
            .iter()
            .filter(|party| {
                matches!(
                    party,
                    Party::Engine | Party::Dymola | Party::OpenModelica | Party::Spawn
                )
            })
            .count();
        if entry.state == State::Resolved
            && independent >= 3
            && entry.upstream_issue.status != UpstreamStatus::Filed
        {
            return Err(ValidationError::new(
                ValidationCode::ThreeWayIssue,
                Some(index),
                "resolved three-way implementation disagreement requires a filed issue",
            ));
        }
    }
    Ok(())
}

fn validate_supersession(
    register: &Register,
    ids: &BTreeMap<&str, usize>,
) -> Result<(), ValidationError> {
    for (index, entry) in register.entries.iter().enumerate() {
        let Some(target) = entry.superseded_by.as_deref() else {
            continue;
        };
        if target == entry.id {
            return Err(ValidationError::new(
                ValidationCode::SupersessionSelf,
                Some(index),
                "entry supersedes itself",
            ));
        }
        if !ids.contains_key(target) {
            return Err(ValidationError::new(
                ValidationCode::SupersessionTarget,
                Some(index),
                "supersession target does not exist",
            ));
        }
        let mut seen = BTreeSet::new();
        let mut cursor = entry.id.as_str();
        while let Some(next) = register.entries[ids[cursor]].superseded_by.as_deref() {
            if !seen.insert(cursor) || next == entry.id {
                return Err(ValidationError::new(
                    ValidationCode::SupersessionCycle,
                    Some(index),
                    "supersession cycle",
                ));
            }
            let Some(_) = ids.get(next) else {
                break;
            };
            cursor = next;
        }
    }
    Ok(())
}
