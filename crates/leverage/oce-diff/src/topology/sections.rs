//! What the CXF level adds once the blocks are matched: the ground parameters of the blocks
//! matched by name, and the interface section.
//!
//! **Ground parameters.** A loaded block's parameter table holds the values the document bound,
//! grounded: a sequence parameter it referenced is its value here, and a parameter the document
//! left out is absent. A value on both sides that differs is `reparameterized`, a Real by its
//! bits, and an Integer against the equal Real is a difference of value: the loaded graph holds
//! two different values, and the content id moves with them (issue #683 is the engine's ruling
//! that will make them one). A value on one side only is `parameter-defaulted` when it equals the
//! catalog's literal default, by the same rule as the authoring level, and `reparameterized`
//! with the other side empty otherwise. No `reordered` entry is made: order is not semantic, and
//! a table's order is the loader's.
//!
//! **Interface.** A boundary input is identified by its name and is in the topology only when
//! something reads it; a declared output only when it is driven. On a connector both sides have,
//! the type, `unit`, `quantity` and `displayUnit` are compared, and an input's `min` and `max`
//! are the effective bounds the engine enforces (the declaration's intersected with every port
//! it feeds). An output's bounds are not in the topology, the engine enforcing neither, so an
//! output entry carries none. No `reordered` entry is made.

use crate::catalog;
use crate::changes::{GraphChange, InterfaceChange};
use crate::data::{Direction, Literal, Side};
use crate::pairing::pair;
use crate::wiring::Matching;
use crate::work::visited;

use super::lower::{Lowered, NamedBlock, literal, type_name};

/// Add the parameter entries of the blocks matched by name and class.
pub(super) fn parameters(
    base: &Lowered<'_>,
    head: &Lowered<'_>,
    matching: &Matching,
    graph: &mut Vec<GraphChange>,
) {
    for &(b, h) in &matching.by_name {
        let (Some(old), Some(new), Some(class)) = (
            base.blocks.get(b),
            head.blocks.get(h),
            head.topology
                .blocks
                .get(h)
                .map(|block| block.class_iri.as_str()),
        ) else {
            continue;
        };
        block(class, old, new, graph);
    }
}

fn block(class: &str, old: &NamedBlock<'_>, new: &NamedBlock<'_>, graph: &mut Vec<GraphChange>) {
    visited(old.params.len() + new.params.len());
    let names = |block: &NamedBlock<'_>| -> Vec<String> {
        block
            .params
            .iter()
            .map(|(assignment, _)| assignment.name.clone())
            .collect()
    };
    let pairs = pair(&names(old), &names(new));
    for &(b, h) in &pairs.both {
        if let (Some((was, _)), Some((now, _))) = (old.params.get(b), new.params.get(h))
            && was.value != now.value
        {
            graph.push(GraphChange::Reparameterized {
                subject: format!("{}.{}", new.name, now.name),
                old: Some(was.value.clone()),
                new: Some(now.value.clone()),
            });
        }
    }
    for (positions, block, side) in [
        (&pairs.base_only, old, Side::Base),
        (&pairs.head_only, new, Side::Head),
    ] {
        for &position in positions {
            let Some((assignment, authored)) = block.params.get(position) else {
                continue;
            };
            // The subject names the block as head does: a block matched by name has one name.
            let subject = format!("{}.{}", new.name, assignment.name);
            let value = assignment.value.clone();
            graph.push(
                if catalog::is_literal_default(class, &assignment.name, authored) {
                    GraphChange::ParameterDefaulted {
                        subject,
                        value,
                        side,
                    }
                } else {
                    let (old, new) = match side {
                        Side::Base => (Some(value), None),
                        Side::Head => (None, Some(value)),
                    };
                    GraphChange::Reparameterized { subject, old, new }
                },
            );
        }
    }
}

/// One boundary as the interface section compares it.
#[derive(Clone, Debug)]
struct Connector<'t> {
    name: &'t str,
    value_type: String,
    unit: Option<String>,
    quantity: Option<String>,
    display_unit: Option<String>,
    min: Option<Literal>,
    max: Option<Literal>,
}

fn connectors<'t>(lowered: &Lowered<'t>, direction: Direction) -> Vec<Connector<'t>> {
    let topology = lowered.topology;
    match direction {
        Direction::In => topology
            .boundary_inputs
            .iter()
            .zip(&lowered.inputs)
            .map(|(input, &name)| Connector {
                name,
                value_type: type_name(input.value_type),
                unit: input.unit.clone(),
                quantity: input.quantity.clone(),
                display_unit: input.display_unit.clone(),
                min: input.min.as_ref().map(literal),
                max: input.max.as_ref().map(literal),
            })
            .collect(),
        Direction::Out => topology
            .boundary_outputs
            .iter()
            .zip(&lowered.outputs)
            .map(|(output, &name)| Connector {
                name,
                value_type: type_name(output.value_type),
                unit: output.unit.clone(),
                quantity: output.quantity.clone(),
                display_unit: output.display_unit.clone(),
                min: None,
                max: None,
            })
            .collect(),
    }
}

/// The interface section, unsorted.
pub(super) fn interface(base: &Lowered<'_>, head: &Lowered<'_>) -> Vec<InterfaceChange> {
    let mut changes = Vec::new();
    for direction in [Direction::In, Direction::Out] {
        let (old, new) = (connectors(base, direction), connectors(head, direction));
        visited(old.len() + new.len());
        let names = |list: &[Connector<'_>]| -> Vec<String> {
            list.iter()
                .map(|connector| connector.name.to_owned())
                .collect()
        };
        let pairs = pair(&names(&old), &names(&new));
        for &position in &pairs.base_only {
            if let Some(connector) = old.get(position) {
                changes.push(declared(connector, direction, false));
            }
        }
        for &position in &pairs.head_only {
            if let Some(connector) = new.get(position) {
                changes.push(declared(connector, direction, true));
            }
        }
        for &(b, h) in &pairs.both {
            if let (Some(was), Some(now)) = (old.get(b), new.get(h)) {
                compared(was, now, direction, &mut changes);
            }
        }
    }
    changes
}

fn declared(connector: &Connector<'_>, direction: Direction, added: bool) -> InterfaceChange {
    let connector = connector.clone();
    let subject = connector.name.to_owned();
    if added {
        InterfaceChange::Added {
            subject,
            direction,
            value_type: connector.value_type,
            unit: connector.unit,
            quantity: connector.quantity,
            display_unit: connector.display_unit,
            min: connector.min,
            max: connector.max,
        }
    } else {
        InterfaceChange::Removed {
            subject,
            direction,
            value_type: connector.value_type,
            unit: connector.unit,
            quantity: connector.quantity,
            display_unit: connector.display_unit,
            min: connector.min,
            max: connector.max,
        }
    }
}

fn compared(
    old: &Connector<'_>,
    new: &Connector<'_>,
    direction: Direction,
    changes: &mut Vec<InterfaceChange>,
) {
    let subject = || new.name.to_owned();
    if old.value_type != new.value_type {
        changes.push(InterfaceChange::TypeChanged {
            subject: subject(),
            direction,
            old: old.value_type.clone(),
            new: new.value_type.clone(),
        });
    }
    if old.unit != new.unit {
        changes.push(InterfaceChange::UnitChanged {
            subject: subject(),
            direction,
            old: old.unit.clone(),
            new: new.unit.clone(),
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
    if old.min != new.min {
        changes.push(InterfaceChange::MinChanged {
            subject: subject(),
            direction,
            old: old.min.clone(),
            new: new.min.clone(),
        });
    }
    if old.max != new.max {
        changes.push(InterfaceChange::MaxChanged {
            subject: subject(),
            direction,
            old: old.max.clone(),
            new: new.max.clone(),
        });
    }
}
