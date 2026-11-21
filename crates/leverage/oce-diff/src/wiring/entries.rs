//! The graph entries that follow from a matching alone: `added`, `removed`, `renamed` and
//! `rewired` (diff note, decision 12).
//!
//! Every link is reported once, at its target. A link into an unmatched block is one of that
//! block's `inputs`. Every other target (a port of a matched block, a boundary output, a port of
//! a block no statement declares) is compared by its drivers after renames are mapped, and a
//! difference is `rewired`.
//!
//! Two drivers are the same when they are the same boundary, the same port of one matched pair,
//! or, for a port of a block that is not matched, the same spelling. A target has one driver in a
//! valid model. With several, the drivers on both sides are set aside as a multiset and what is
//! left is paired in sorted order, a side that runs out reading as undriven, so every difference
//! is still one entry at its target.

use std::collections::BTreeMap;

use crate::changes::GraphChange;
use crate::data::Driver;
use crate::work::{Counted, compared, sort_counted, visited};

use super::{End, Index, Matching, Side};

/// The two sides with their lookups.
pub(super) struct Sides<'s, 'a> {
    pub(super) base: &'s Side<'a>,
    pub(super) head: &'s Side<'a>,
    pub(super) base_index: &'s Index<'a>,
    pub(super) head_index: &'s Index<'a>,
}

/// A link target outside the unmatched blocks, in head terms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Target<'a> {
    /// A boundary, by name.
    Boundary(&'a str),
    /// A port of a matched block, by the head block's position.
    Port(usize, &'a str),
    /// A port of a block no statement declares, as spelled.
    Undeclared(&'a str, &'a str),
}

/// A driver, in head terms.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Source<'a> {
    /// A boundary, by name.
    Boundary(&'a str),
    /// A port of a matched block, by the head block's position.
    Port(usize, &'a str),
    /// A port of a block that is not matched, as spelled.
    Unmatched(String),
}

/// The drivers of one target: each with its spelling on its own side.
#[derive(Clone, Debug, Default)]
struct Drivers<'a> {
    old: Vec<(Source<'a>, String)>,
    new: Vec<(Source<'a>, String)>,
}

/// The `added`, `removed`, `renamed` and `rewired` entries of a matching, unsorted.
pub(super) fn changes(sides: &Sides<'_, '_>, matching: &Matching) -> Vec<GraphChange> {
    let mut changes = Vec::new();
    for &b in &matching.base_only {
        if let Some(node) = sides.base.nodes.get(b) {
            changes.push(GraphChange::Removed {
                subject: node.name.to_owned(),
                class: node.class.to_owned(),
                params: node.assignments.clone(),
                inputs: inputs(sides.base_index, b),
            });
        }
    }
    for &h in &matching.head_only {
        if let Some(node) = sides.head.nodes.get(h) {
            changes.push(GraphChange::Added {
                subject: node.name.to_owned(),
                class: node.class.to_owned(),
                params: node.assignments.clone(),
                inputs: inputs(sides.head_index, h),
            });
        }
    }
    for &(b, h, confidence) in &matching.renamed {
        if let (Some(old), Some(new)) = (sides.base.nodes.get(b), sides.head.nodes.get(h)) {
            changes.push(GraphChange::Renamed {
                subject: new.name.to_owned(),
                from: old.name.to_owned(),
                class: new.class.to_owned(),
                params: new.assignments.clone(),
                confidence,
            });
        }
    }
    rewired(sides, matching, &mut changes);
    changes
}

/// The driven input ports of the block at `position`, sorted by port, then source.
fn inputs(index: &Index<'_>, position: usize) -> Vec<Driver> {
    let mut inputs: Vec<Driver> = index
        .touching(position)
        .iter()
        .filter(|touch| !touch.output)
        .map(|touch| Driver {
            port: touch.port.to_owned(),
            source: touch.other.spelling(),
        })
        .collect();
    sort_counted(&mut inputs);
    inputs
}

fn rewired(sides: &Sides<'_, '_>, matching: &Matching, changes: &mut Vec<GraphChange>) {
    visited(sides.base.links.len() + sides.head.links.len());
    let base_partner = |position: usize| matching.partner.get(position).copied().flatten();
    let head_partner = |position: usize| {
        matches!(matching.matched_head.get(position), Some(true)).then_some(position)
    };
    let mut targets: BTreeMap<Counted<Target<'_>>, Drivers<'_>> = BTreeMap::new();
    for &(source, target) in &sides.base.links {
        if let Some(target) = place_target(target, sides.base_index, base_partner) {
            let driver = place_source(source, sides.base_index, base_partner);
            let drivers = targets.entry(Counted(target)).or_default();
            drivers.old.push((driver, source.spelling()));
        }
    }
    for &(source, target) in &sides.head.links {
        if let Some(target) = place_target(target, sides.head_index, head_partner) {
            let driver = place_source(source, sides.head_index, head_partner);
            let drivers = targets.entry(Counted(target)).or_default();
            drivers.new.push((driver, source.spelling()));
        }
    }
    for (Counted(target), drivers) in targets {
        let subject = match target {
            Target::Boundary(name) => name.to_owned(),
            Target::Port(position, port) => match sides.head.nodes.get(position) {
                Some(node) => format!("{}.{port}", node.name),
                None => continue,
            },
            Target::Undeclared(block, port) => format!("{block}.{port}"),
        };
        let (old, new) = unshared(drivers);
        for at in 0..old.len().max(new.len()) {
            changes.push(GraphChange::Rewired {
                subject: subject.clone(),
                old: old.get(at).cloned(),
                new: new.get(at).cloned(),
            });
        }
    }
}

/// Where a link's target falls: `None` for a port of an unmatched block, whose entry carries the
/// link as one of its inputs.
fn place_target<'a>(
    target: End<'a>,
    index: &Index<'a>,
    partner: impl Fn(usize) -> Option<usize>,
) -> Option<Target<'a>> {
    match target {
        End::Boundary(name) => Some(Target::Boundary(name)),
        End::Port { block, port } => match index.block(block) {
            Some(position) => partner(position).map(|head| Target::Port(head, port)),
            None => Some(Target::Undeclared(block, port)),
        },
    }
}

fn place_source<'a>(
    source: End<'a>,
    index: &Index<'a>,
    partner: impl Fn(usize) -> Option<usize>,
) -> Source<'a> {
    match source {
        End::Boundary(name) => Source::Boundary(name),
        End::Port { block, port } => match index.block(block).and_then(partner) {
            Some(head) => Source::Port(head, port),
            None => Source::Unmatched(source.spelling()),
        },
    }
}

/// The spellings of the drivers one side has and the other does not, each sorted.
fn unshared(mut drivers: Drivers<'_>) -> (Vec<String>, Vec<String>) {
    sort_counted(&mut drivers.old);
    sort_counted(&mut drivers.new);
    let (mut old_only, mut new_only) = (Vec::new(), Vec::new());
    let mut old = drivers.old.into_iter().peekable();
    let mut new = drivers.new.into_iter().peekable();
    loop {
        compared();
        match (old.peek(), new.peek()) {
            (Some(a), Some(b)) => match a.0.cmp(&b.0) {
                std::cmp::Ordering::Equal => {
                    old.next();
                    new.next();
                }
                std::cmp::Ordering::Less => old_only.extend(old.next().map(|driver| driver.1)),
                std::cmp::Ordering::Greater => new_only.extend(new.next().map(|driver| driver.1)),
            },
            (Some(_), None) => old_only.extend(old.next().map(|driver| driver.1)),
            (None, Some(_)) => new_only.extend(new.next().map(|driver| driver.1)),
            (None, None) => break,
        }
    }
    sort_counted(&mut old_only);
    sort_counted(&mut new_only);
    (old_only, new_only)
}
