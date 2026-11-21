//! The rename pass (diff note, "Identity"; decisions 2 and 11): deterministic, bounded, and only
//! ever a way to present blocks that steps 1 and 2 left unmatched.
//!
//! **Hard gates.** A base block and a head block are a candidate pair only when they have the
//! same class path and the same assignments, order ignored: same names, same values (a Real by
//! its bits, a reference by the parameter it names, a member by its spelling).
//!
//! **Neighbourhood.** Each port of a block is read through the current matching. An end of a
//! link is an *anchor* when it is a boundary declared on both sides or a port of a matched block.
//! For a pair, a port *agrees* when the ends on both sides are the same anchors, *disagrees* when
//! an anchor on one side has no equal on the other, and is *unknown* when what is left on both
//! sides are ports of unmatched blocks, the same number of them. Three further cases are
//! disagreements too, because nothing a later round does can make them agree: a port
//! linked on one side only, a different number of ends, and an end that is neither an anchor nor
//! an unmatched block (a boundary declared on one side only, a block no statement declares).
//!
//! **Rounds.** A round collects the pairs with no disagreement and at least one agreement and
//! accepts a pair only when each of its two blocks has exactly one such candidate. Accepted pairs
//! are anchors for the next round. The pass stops when a round accepts nothing, or after
//! [`MAX_RENAME_ROUNDS`].
//!
//! **How it is computed.** Whether a pair has a disagreement depends only on each block's own
//! *profile*: its gate and, per port, the anchors and the number of unmatched ends. Two blocks
//! are a candidate pair exactly when their profiles are equal, and a profile says by itself
//! whether a port agrees. So a round groups the unmatched blocks of both sides by profile and
//! accepts the groups that hold one base block and one head block. Its work is the links at the
//! unmatched blocks, never the number of pairs: a bucket of thousands of identical blocks costs
//! what its links cost. Grouping makes the result independent of the order the blocks or the
//! links are written in.
//!
//! **Bounds.** With more than [`MAX_RENAME_CANDIDATES`] unmatched blocks on a side, or more than
//! [`MAX_RENAME_PAIRS`] gated pairs, the pass does not run.

use std::collections::BTreeMap;

use crate::data::{
    Assignment, Confidence, MAX_RENAME_CANDIDATES, MAX_RENAME_PAIRS, MAX_RENAME_ROUNDS,
};
use crate::work::{Counted, sort_counted, visited};

use super::{End, Index, Matching, Side, set};

/// The two sides with their lookups.
pub(super) struct Sides<'s, 'a> {
    pub(super) base: &'s Side<'a>,
    pub(super) head: &'s Side<'a>,
    pub(super) base_index: &'s Index<'a>,
    pub(super) head_index: &'s Index<'a>,
}

/// What a link end is, read through the current matching and placed in head terms so that the
/// two sides compare.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Anchor<'a> {
    /// A boundary declared on both sides.
    Boundary(&'a str),
    /// A port of a matched block, by the head block's position.
    Port(usize, &'a str),
}

/// One port of an unmatched block, read through the current matching.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct PortProfile<'a> {
    output: bool,
    port: &'a str,
    /// The anchors at the port, sorted (a multiset: a repeated link counts twice).
    anchors: Vec<Counted<Anchor<'a>>>,
    /// The ends at the port that are ports of unmatched blocks.
    open: usize,
}

/// An unmatched block's gate and ports. Two blocks are a candidate pair exactly when their
/// profiles are equal.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Profile<'a> {
    gate: usize,
    ports: Vec<PortProfile<'a>>,
}

impl Profile<'_> {
    /// Whether at least one port agrees: every end at it is an anchor.
    fn has_agreement(&self) -> bool {
        self.ports.iter().any(|port| port.open == 0)
    }

    /// Whether every port agrees.
    fn all_agree(&self) -> bool {
        self.ports.iter().all(|port| port.open == 0)
    }
}

/// The blocks of one profile seen so far in a round: how many on each side, and the first.
#[derive(Clone, Copy, Debug, Default)]
struct Group {
    base: (usize, usize),
    head: (usize, usize),
}

/// Run the pass over the unmatched blocks, recording every accepted pair in `matching`. Returns
/// the accepted pairs in base order with their confidence, or `None` when a cap is passed and
/// the pass did not run.
pub(super) fn run(
    sides: &Sides<'_, '_>,
    matching: &mut Matching,
    loose_base: &[usize],
    loose_head: &[usize],
) -> Option<Vec<(usize, usize, Confidence)>> {
    if loose_base.len() > MAX_RENAME_CANDIDATES || loose_head.len() > MAX_RENAME_CANDIDATES {
        return None;
    }
    let gates = Gates::new(sides, loose_base, loose_head)?;

    let mut live_base = loose_base.to_vec();
    let mut live_head = loose_head.to_vec();
    let mut accepted: Vec<(usize, usize)> = Vec::new();
    for _ in 0..MAX_RENAME_ROUNDS {
        let round = round(sides, matching, &gates, &live_base, &live_head);
        if round.is_empty() {
            break;
        }
        for &(b, h) in &round {
            set(&mut matching.partner, b, Some(h));
            set(&mut matching.matched_head, h, true);
        }
        live_base.retain(|&b| matches!(matching.partner.get(b), Some(None)));
        live_head.retain(|&h| matches!(matching.matched_head.get(h), Some(false)));
        accepted.extend(round);
    }

    accepted.sort_unstable();
    Some(
        accepted
            .into_iter()
            .map(|(b, h)| (b, h, confidence(sides, matching, &gates, b, h)))
            .collect(),
    )
}

/// One round: the pairs whose profile is held by exactly one live base block and one live head
/// block and has an agreeing port.
fn round(
    sides: &Sides<'_, '_>,
    matching: &Matching,
    gates: &Gates,
    live_base: &[usize],
    live_head: &[usize],
) -> Vec<(usize, usize)> {
    let mut groups: BTreeMap<Counted<Profile<'_>>, Group> = BTreeMap::new();
    for &b in live_base {
        if let Some(profile) = base_profile(sides, matching, gates, b) {
            let group = groups.entry(Counted(profile)).or_default();
            group.base = (
                group.base.0 + 1,
                if group.base.0 == 0 { b } else { group.base.1 },
            );
        }
    }
    for &h in live_head {
        if let Some(profile) = head_profile(sides, matching, gates, h) {
            let group = groups.entry(Counted(profile)).or_default();
            group.head = (
                group.head.0 + 1,
                if group.head.0 == 0 { h } else { group.head.1 },
            );
        }
    }
    groups
        .into_iter()
        .filter(|(profile, group)| {
            group.base.0 == 1 && group.head.0 == 1 && profile.0.has_agreement()
        })
        .map(|(_, group)| (group.base.1, group.head.1))
        .collect()
}

/// `exact` when the two blocks' final profiles are equal and every port agrees; `high` otherwise.
fn confidence(
    sides: &Sides<'_, '_>,
    matching: &Matching,
    gates: &Gates,
    b: usize,
    h: usize,
) -> Confidence {
    let old = base_profile(sides, matching, gates, b);
    let new = head_profile(sides, matching, gates, h);
    match (old, new) {
        (Some(old), Some(new)) if old == new && old.all_agree() => Confidence::Exact,
        _ => Confidence::High,
    }
}

fn base_profile<'a>(
    sides: &Sides<'_, 'a>,
    matching: &Matching,
    gates: &Gates,
    b: usize,
) -> Option<Profile<'a>> {
    let gate = gates.base.get(&b).copied()?;
    profile(gate, sides.base_index, sides.head_index, b, |position| {
        matching.partner.get(position).copied().flatten()
    })
}

fn head_profile<'a>(
    sides: &Sides<'_, 'a>,
    matching: &Matching,
    gates: &Gates,
    h: usize,
) -> Option<Profile<'a>> {
    let gate = gates.head.get(&h).copied()?;
    profile(gate, sides.head_index, sides.base_index, h, |position| {
        matches!(matching.matched_head.get(position), Some(true)).then_some(position)
    })
}

/// The profile of the block at `position` of the side `own` indexes, or `None` when one of its
/// link ends can never be matched (so the block can never be renamed). `matched` gives the head
/// position of a matched block of that side.
fn profile<'a>(
    gate: usize,
    own: &Index<'a>,
    other: &Index<'a>,
    position: usize,
    matched: impl Fn(usize) -> Option<usize>,
) -> Option<Profile<'a>> {
    let touches = own.touching(position);
    visited(touches.len());
    let mut ports: Vec<PortProfile<'a>> = Vec::new();
    for touch in touches {
        let same_port = ports
            .last()
            .is_some_and(|last| last.output == touch.output && last.port == touch.port);
        if !same_port {
            ports.push(PortProfile {
                output: touch.output,
                port: touch.port,
                anchors: Vec::new(),
                open: 0,
            });
        }
        let port = ports.last_mut()?;
        match touch.other {
            End::Boundary(name) => {
                if !(own.declares(name) && other.declares(name)) {
                    return None;
                }
                port.anchors.push(Counted(Anchor::Boundary(name)));
            }
            End::Port { block, port: far } => match matched(own.block(block)?) {
                Some(head) => port.anchors.push(Counted(Anchor::Port(head, far))),
                None => port.open += 1,
            },
        }
    }
    for port in &mut ports {
        port.anchors.sort();
    }
    Some(Profile { gate, ports })
}

/// The gate of each unmatched block: a number shared by the blocks with the same class path and
/// the same assignments.
struct Gates {
    base: BTreeMap<usize, usize>,
    head: BTreeMap<usize, usize>,
}

impl Gates {
    /// Number the gates, or `None` when the gated pairs pass [`MAX_RENAME_PAIRS`].
    fn new(sides: &Sides<'_, '_>, loose_base: &[usize], loose_head: &[usize]) -> Option<Self> {
        type Key<'k> = Counted<(&'k str, Vec<&'k Assignment>)>;
        fn key<'k>(side: &'k Side<'_>, position: usize) -> Option<Key<'k>> {
            let node = side.nodes.get(position)?;
            visited(node.assignments.len());
            let mut assignments: Vec<&Assignment> = node.assignments.iter().collect();
            sort_counted(&mut assignments);
            Some(Counted((node.class, assignments)))
        }

        let mut numbers: BTreeMap<Key<'_>, (usize, usize, usize)> = BTreeMap::new();
        let mut gates = Self {
            base: BTreeMap::new(),
            head: BTreeMap::new(),
        };
        for &b in loose_base {
            let next = numbers.len();
            let entry = numbers.entry(key(sides.base, b)?).or_insert((next, 0, 0));
            entry.1 += 1;
            gates.base.insert(b, entry.0);
        }
        for &h in loose_head {
            let next = numbers.len();
            let entry = numbers.entry(key(sides.head, h)?).or_insert((next, 0, 0));
            entry.2 += 1;
            gates.head.insert(h, entry.0);
        }
        let pairs = numbers.values().fold(0_usize, |sum, &(_, base, head)| {
            sum.saturating_add(base.saturating_mul(head))
        });
        (pairs <= MAX_RENAME_PAIRS).then_some(gates)
    }
}
