//! The parameters section: sequence-level `param` changes, each with the assignments it affects
//! (diff note, "Site parameters and the graph"; decision 6).
//!
//! A change to a sequence parameter is reported here and only here. The block assignments that
//! reference the parameter are listed as `affects` (`block.param`, sorted, each once) and are not
//! repeated as `reparameterized` graph entries: the assignment still says the same thing, a
//! reference to the same name. The list is read from head, and from base for a removed parameter.
//!
//! Two spellings of one value are metadata, not a parameter change: an Integer literal against
//! the equal Real on a parameter both sides declare `Real`, or a changed unit suffix.

use std::collections::BTreeMap;

use oce_author::{ParamValue, ParameterType, Sequence, SequenceParameter};

use crate::changes::{MetadataChange, ParameterChange};
use crate::pairing::{pair, reordered};
use crate::work::{Counted, sort_counted, visited};

use super::blocks::{commented, respelled};
use super::lower;

/// For each sequence parameter name, the `block.param` assignments of `sequence` that reference
/// it, sorted, each once.
fn references(sequence: &Sequence) -> BTreeMap<Counted<&str>, Vec<String>> {
    let mut references: BTreeMap<Counted<&str>, Vec<String>> = BTreeMap::new();
    for block in &sequence.blocks {
        visited(block.assignments.len());
        for assignment in &block.assignments {
            if let ParamValue::Ref(name) = &assignment.value {
                references
                    .entry(Counted(name.as_str()))
                    .or_default()
                    .push(format!("{}.{}", block.name, assignment.name));
            }
        }
    }
    for list in references.values_mut() {
        sort_counted(list);
        list.dedup();
    }
    references
}

fn spelled(parameter: &SequenceParameter) -> String {
    lower::spelled(
        &lower::literal(&parameter.value),
        parameter.scaled_from.as_ref(),
        parameter.suffix.as_deref(),
    )
}

/// The parameters section, unsorted; spellings and comments go to `metadata`.
pub(super) fn changes(
    base: &Sequence,
    head: &Sequence,
    metadata: &mut Vec<MetadataChange>,
) -> Vec<ParameterChange> {
    let names = |sequence: &Sequence| -> Vec<String> {
        sequence
            .params
            .iter()
            .map(|parameter| parameter.name.as_str().to_owned())
            .collect()
    };
    let pairs = pair(&names(base), &names(head));
    let base_references = references(base);
    let head_references = references(head);
    let affects = |references: &BTreeMap<Counted<&str>, Vec<String>>, name: &str| {
        references.get(&Counted(name)).cloned().unwrap_or_default()
    };

    let mut changes = Vec::new();
    if reordered(&pairs.both) {
        changes.push(ParameterChange::Reordered {
            subject: "params".to_owned(),
        });
    }
    for &position in &pairs.base_only {
        if let Some(parameter) = base.params.get(position) {
            changes.push(ParameterChange::Removed {
                subject: parameter.name.as_str().to_owned(),
                value_type: parameter.value_type.as_str().to_owned(),
                value: lower::literal(&parameter.value),
                affects: affects(&base_references, parameter.name.as_str()),
            });
        }
    }
    for &position in &pairs.head_only {
        if let Some(parameter) = head.params.get(position) {
            changes.push(ParameterChange::Added {
                subject: parameter.name.as_str().to_owned(),
                value_type: parameter.value_type.as_str().to_owned(),
                value: lower::literal(&parameter.value),
                affects: affects(&head_references, parameter.name.as_str()),
            });
        }
    }
    for &(b, h) in &pairs.both {
        let (Some(old), Some(new)) = (base.params.get(b), head.params.get(h)) else {
            continue;
        };
        let subject = new.name.as_str();
        commented(
            subject,
            &old.annotations.comments,
            &new.annotations.comments,
            metadata,
        );
        if old.value_type != new.value_type {
            changes.push(ParameterChange::TypeChanged {
                subject: subject.to_owned(),
                old_type: old.value_type.as_str().to_owned(),
                new_type: new.value_type.as_str().to_owned(),
                old: lower::literal(&old.value),
                new: lower::literal(&new.value),
                affects: affects(&head_references, subject),
            });
            continue;
        }
        let one_real =
            new.value_type == ParameterType::Real && lower::same_real(&old.value, &new.value);
        if old.value == new.value || one_real {
            respelled(subject, spelled(old), spelled(new), metadata);
        } else {
            changes.push(ParameterChange::Changed {
                subject: subject.to_owned(),
                value_type: new.value_type.as_str().to_owned(),
                old: lower::literal(&old.value),
                new: lower::literal(&new.value),
                affects: affects(&head_references, subject),
            });
        }
    }
    changes
}
