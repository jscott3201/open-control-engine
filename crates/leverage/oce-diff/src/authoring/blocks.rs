//! What the authoring level adds to the graph section once the blocks are matched: assignment
//! changes on blocks matched by name, authored order, and the comments on blocks, assignments and
//! connections (which are metadata).
//!
//! An assignment on both sides is `reparameterized` when its value differs. Two spellings of one
//! value are not a graph change but a `literal-respelled` metadata entry: an Integer literal
//! against the equal Real on a parameter the catalog types Real, or a changed unit suffix. An
//! assignment on one side only is `parameter-defaulted` when its literal equals the catalog's
//! literal default, and `reparameterized` with the other side empty otherwise: against a derived
//! default, a required parameter, an array element, a reference, or a class or parameter the
//! catalog does not know.
//!
//! A block the rename pass matched has, by the pass's gate, the same assignments on both sides;
//! it is reported as `renamed` and nothing here looks inside it. `reordered` is computed over
//! elements matched by name only, at most one entry per list.

use oce_author::{Assignment, Block, ParamValue, Sequence};

use crate::catalog;
use crate::changes::{GraphChange, MetadataChange};
use crate::data::{GraphList, Side};
use crate::pairing::{pair, reordered};
use crate::wiring::Matching;
use crate::work::visited;

use super::{comment, lower};

/// Add the assignment, order and comment entries of the blocks and connections.
pub(super) fn changes(
    base: &Sequence,
    head: &Sequence,
    matching: &Matching,
    graph: &mut Vec<GraphChange>,
    metadata: &mut Vec<MetadataChange>,
) {
    if reordered(&matching.by_name) {
        graph.push(GraphChange::Reordered {
            subject: "blocks".to_owned(),
            list: GraphList::Blocks,
        });
    }
    for &(b, h) in &matching.by_name {
        if let (Some(old), Some(new)) = (base.blocks.get(b), head.blocks.get(h)) {
            block(old, new, graph, metadata);
        }
    }

    let spell = |sequence: &Sequence| -> Vec<String> {
        sequence
            .connections
            .iter()
            .map(oce_author::Connection::spelling)
            .collect()
    };
    let (old_links, new_links) = (spell(base), spell(head));
    let links = pair(&old_links, &new_links);
    if reordered(&links.both) {
        graph.push(GraphChange::Reordered {
            subject: "connections".to_owned(),
            list: GraphList::Connections,
        });
    }
    for &(b, h) in &links.both {
        if let (Some(old), Some(new), Some(subject)) = (
            base.connections.get(b),
            head.connections.get(h),
            new_links.get(h),
        ) {
            commented(
                subject,
                &old.annotations.comments,
                &new.annotations.comments,
                metadata,
            );
        }
    }
}

/// Add a `comment-changed` entry when the comment lines differ.
pub(super) fn commented(
    subject: &str,
    old: &[String],
    new: &[String],
    metadata: &mut Vec<MetadataChange>,
) {
    if old != new {
        metadata.push(MetadataChange::CommentChanged {
            subject: subject.to_owned(),
            old: comment(old),
            new: comment(new),
        });
    }
}

/// Add a `literal-respelled` entry when the two spellings of one value differ.
pub(super) fn respelled(
    subject: &str,
    old: String,
    new: String,
    metadata: &mut Vec<MetadataChange>,
) {
    if old != new {
        metadata.push(MetadataChange::LiteralRespelled {
            subject: subject.to_owned(),
            old: Some(old),
            new: Some(new),
        });
    }
}

/// One block matched by name and class.
fn block(
    old: &Block,
    new: &Block,
    graph: &mut Vec<GraphChange>,
    metadata: &mut Vec<MetadataChange>,
) {
    visited(old.assignments.len() + new.assignments.len());
    let name = new.name.as_str();
    commented(
        name,
        &old.annotations.comments,
        &new.annotations.comments,
        metadata,
    );
    let names = |block: &Block| -> Vec<String> {
        block
            .assignments
            .iter()
            .map(|assignment| assignment.name.as_str().to_owned())
            .collect()
    };
    let pairs = pair(&names(old), &names(new));
    if reordered(&pairs.both) {
        graph.push(GraphChange::Reordered {
            subject: name.to_owned(),
            list: GraphList::Assignments,
        });
    }
    for &(b, h) in &pairs.both {
        if let (Some(was), Some(now)) = (old.assignments.get(b), new.assignments.get(h)) {
            assigned_on_both(&new.class_path, name, was, now, graph, metadata);
        }
    }
    for (positions, block, side) in [
        (&pairs.base_only, old, Side::Base),
        (&pairs.head_only, new, Side::Head),
    ] {
        for &position in positions {
            if let Some(assignment) = block.assignments.get(position) {
                graph.push(assigned_on_one(&new.class_path, name, assignment, side));
            }
        }
    }
}

fn spelled(assignment: &Assignment) -> String {
    lower::spelled(
        &lower::value(&assignment.value),
        assignment.scaled_from.as_ref(),
        assignment.suffix.as_deref(),
    )
}

fn assigned_on_both(
    class_path: &str,
    block: &str,
    old: &Assignment,
    new: &Assignment,
    graph: &mut Vec<GraphChange>,
    metadata: &mut Vec<MetadataChange>,
) {
    let subject = format!("{block}.{}", new.name);
    commented(
        &subject,
        &old.annotations.comments,
        &new.annotations.comments,
        metadata,
    );
    let one_real = match (&old.value, &new.value) {
        (ParamValue::Literal(was), ParamValue::Literal(now)) => {
            lower::same_real(was, now) && catalog::is_real(class_path, new.name.as_str())
        }
        _ => false,
    };
    if old.value == new.value || one_real {
        respelled(&subject, spelled(old), spelled(new), metadata);
    } else {
        graph.push(GraphChange::Reparameterized {
            subject,
            old: Some(lower::value(&old.value)),
            new: Some(lower::value(&new.value)),
        });
    }
}

fn assigned_on_one(
    class_path: &str,
    block: &str,
    assignment: &Assignment,
    side: Side,
) -> GraphChange {
    let subject = format!("{block}.{}", assignment.name);
    let value = lower::value(&assignment.value);
    let defaulted = match &assignment.value {
        ParamValue::Literal(literal) => {
            catalog::is_literal_default(class_path, assignment.name.as_str(), literal)
        }
        _ => false,
    };
    if defaulted {
        return GraphChange::ParameterDefaulted {
            subject,
            value,
            side,
        };
    }
    let (old, new) = match side {
        Side::Base => (Some(value), None),
        Side::Head => (None, Some(value)),
    };
    GraphChange::Reparameterized { subject, old, new }
}
