//! The interface section: boundary inputs and outputs (diff note, decision 14).
//!
//! A connector is identified by its direction and its name; the rename pass never renames a
//! boundary, because a renamed input or output breaks every binding to it, so a rename reads as
//! `removed` plus `added`. On a connector both sides declare, the type, `unit`, `quantity`,
//! `displayUnit` and the `min` and `max` bounds are interface; the other semantic fields and the
//! comments are metadata. A bound written as an Integer on one side and as the equal Real on the
//! other, on a connector both sides type `Real`, is one value spelled two ways and so metadata
//! as well.

use oce_author::{Bound, Boundary, Sequence, SignalType};

use crate::changes::{InterfaceChange, MetadataChange};
use crate::data::{Direction, Literal};
use crate::pairing::{pair, reordered};
use crate::work::visited;

use super::blocks::{commented, respelled};
use super::lower;

/// The interface section, unsorted; semantic fields, spellings and comments go to `metadata`.
pub(super) fn changes(
    base: &Sequence,
    head: &Sequence,
    metadata: &mut Vec<MetadataChange>,
) -> Vec<InterfaceChange> {
    let mut changes = Vec::new();
    for (direction, list, old, new) in [
        (Direction::In, "inputs", &base.inputs, &head.inputs),
        (Direction::Out, "outputs", &base.outputs, &head.outputs),
    ] {
        visited(old.len() + new.len());
        let names = |boundaries: &[Boundary]| -> Vec<String> {
            boundaries
                .iter()
                .map(|boundary| boundary.name.as_str().to_owned())
                .collect()
        };
        let pairs = pair(&names(old), &names(new));
        if reordered(&pairs.both) {
            changes.push(InterfaceChange::Reordered {
                subject: list.to_owned(),
            });
        }
        for &position in &pairs.base_only {
            if let Some(boundary) = old.get(position) {
                changes.push(declared(boundary, direction, false));
            }
        }
        for &position in &pairs.head_only {
            if let Some(boundary) = new.get(position) {
                changes.push(declared(boundary, direction, true));
            }
        }
        for &(b, h) in &pairs.both {
            if let (Some(was), Some(now)) = (old.get(b), new.get(h)) {
                connector(was, now, direction, &mut changes, metadata);
            }
        }
    }
    changes
}

/// An `added` or `removed` entry carrying the connector's declaration.
fn declared(boundary: &Boundary, direction: Direction, added: bool) -> InterfaceChange {
    let subject = boundary.name.as_str().to_owned();
    let value_type = boundary.value_type.as_str().to_owned();
    let unit = boundary.semantic.unit.clone();
    let quantity = boundary.quantity.clone();
    let display_unit = boundary.display_unit.clone();
    let min = boundary.min.map(lower::bound);
    let max = boundary.max.map(lower::bound);
    if added {
        InterfaceChange::Added {
            subject,
            direction,
            value_type,
            unit,
            quantity,
            display_unit,
            min,
            max,
        }
    } else {
        InterfaceChange::Removed {
            subject,
            direction,
            value_type,
            unit,
            quantity,
            display_unit,
            min,
            max,
        }
    }
}

/// Whether two bounds are an Integer and the Real it means.
fn same_real(a: Bound, b: Bound) -> bool {
    let (integer, real) = match (a, b) {
        (Bound::Integer(integer), Bound::Real(real))
        | (Bound::Real(real), Bound::Integer(integer)) => (integer, real),
        _ => return false,
    };
    lower::exact_real(integer).is_some_and(|exact| exact.to_bits() == real.to_bits())
}

/// One connector declared on both sides.
fn connector(
    old: &Boundary,
    new: &Boundary,
    direction: Direction,
    changes: &mut Vec<InterfaceChange>,
    metadata: &mut Vec<MetadataChange>,
) {
    let name = new.name.as_str();
    let subject = || name.to_owned();
    commented(
        name,
        &old.annotations.comments,
        &new.annotations.comments,
        metadata,
    );
    if old.value_type != new.value_type {
        changes.push(InterfaceChange::TypeChanged {
            subject: subject(),
            direction,
            old: old.value_type.as_str().to_owned(),
            new: new.value_type.as_str().to_owned(),
        });
    }
    if old.semantic.unit != new.semantic.unit {
        changes.push(InterfaceChange::UnitChanged {
            subject: subject(),
            direction,
            old: old.semantic.unit.clone(),
            new: new.semantic.unit.clone(),
        });
    }
    if old.quantity != new.quantity {
        changes.push(InterfaceChange::QuantityChanged {
            subject: subject(),
            direction,
            old: old.quantity.clone(),
            new: new.quantity.clone(),
        });
    }
    if old.display_unit != new.display_unit {
        changes.push(InterfaceChange::DisplayUnitChanged {
            subject: subject(),
            direction,
            old: old.display_unit.clone(),
            new: new.display_unit.clone(),
        });
    }
    let real = old.value_type == SignalType::Real && new.value_type == SignalType::Real;
    for (tag, was, now) in [("min", old.min, new.min), ("max", old.max, new.max)] {
        if was == now {
            continue;
        }
        let (old, new) = (was.map(lower::bound), now.map(lower::bound));
        let one_value = real && matches!((was, now), (Some(a), Some(b)) if same_real(a, b));
        if one_value {
            let text = |bound: Option<&Literal>| {
                bound.map(crate::render::literal_text).unwrap_or_default()
            };
            respelled(
                &format!("{name}.{tag}"),
                text(old.as_ref()),
                text(new.as_ref()),
                metadata,
            );
        } else if tag == "min" {
            changes.push(InterfaceChange::MinChanged {
                subject: subject(),
                direction,
                old,
                new,
            });
        } else {
            changes.push(InterfaceChange::MaxChanged {
                subject: subject(),
                direction,
                old,
                new,
            });
        }
    }
    semantic(old, new, metadata);
}

/// The semantic fields other than `unit`, each a `semantic-changed` entry under
/// `boundary.field`. The Haystack tags are compared as the list they are and reported joined by
/// one space, in authored order.
fn semantic(old: &Boundary, new: &Boundary, metadata: &mut Vec<MetadataChange>) {
    let boundary = new.name.as_str();
    let (was, now) = (&old.semantic, &new.semantic);
    let tags = |tags: &[String]| (!tags.is_empty()).then(|| tags.join(" "));
    let flag = |flag: bool| Some(flag.to_string());
    for (field, differs, old, new) in [
        (
            "name",
            was.name != now.name,
            was.name.clone(),
            now.name.clone(),
        ),
        (
            "label",
            was.label != now.label,
            was.label.clone(),
            now.label.clone(),
        ),
        (
            "brick",
            was.brick != now.brick,
            was.brick.clone(),
            now.brick.clone(),
        ),
        (
            "s223",
            was.s223 != now.s223,
            was.s223.clone(),
            now.s223.clone(),
        ),
        (
            "qudt",
            was.qudt != now.qudt,
            was.qudt.clone(),
            now.qudt.clone(),
        ),
        (
            "haystack",
            was.haystack != now.haystack,
            tags(&was.haystack),
            tags(&now.haystack),
        ),
        (
            "provisional",
            was.provisional != now.provisional,
            flag(was.provisional),
            flag(now.provisional),
        ),
    ] {
        if differs {
            metadata.push(MetadataChange::SemanticChanged {
                subject: format!("{boundary}.{field}"),
                old,
                new,
            });
        }
    }
}
