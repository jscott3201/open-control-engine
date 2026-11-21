//! The level-neutral graph comparison: block identity, the rename pass and the connection
//! changes (diff note, "Identity"; decisions 2, 4, 11 and 12).
//!
//! A [`Side`] is the part of a model that identity and wiring are decided on: named blocks with a
//! class and assignments, declared boundary names and directed links. It borrows from whatever it
//! was lowered from and knows nothing of the authoring model, so an adapter over another level
//! (the CXF adapter over the facade's topology view) lowers into the same shape and gets the same
//! matching, the same rename pass and the same entries.
//!
//! [`compare`] matches the blocks in the note's three steps and returns the [`Matching`] with the
//! graph entries that follow from it alone: `added`, `removed`, `renamed` and `rewired`. What a
//! level adds on top (assignment changes, reorderings, the other sections) it computes from the
//! matching.
//!
//! The diff never validates, so a side may hold what a validator refuses, and every case has one
//! fixed reading:
//!
//! - A block name used more than once on a side is matched occurrence by occurrence; a link that
//!   names it means the first.
//! - A link end that names no declared block is kept as spelled and never matched.
//! - A target with several drivers is compared as a multiset of drivers.

mod entries;
mod index;
mod rename;

use crate::changes::GraphChange;
use crate::data::{Assignment, BlockCounts, Confidence, DiffOptions, Note};
use crate::pairing::pair;

pub(crate) use index::Index;

/// One end of a link.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum End<'a> {
    /// A boundary input or output, by name.
    Boundary(&'a str),
    /// A block port.
    Port {
        /// The block name.
        block: &'a str,
        /// The port name.
        port: &'a str,
    },
}

impl End<'_> {
    /// The `name` or `block.port` spelling.
    pub(crate) fn spelling(&self) -> String {
        match self {
            Self::Boundary(name) => (*name).to_owned(),
            Self::Port { block, port } => format!("{block}.{port}"),
        }
    }
}

/// One block of a side.
#[derive(Clone, Debug)]
pub(crate) struct Node<'a> {
    /// The block name.
    pub(crate) name: &'a str,
    /// The class path.
    pub(crate) class: &'a str,
    /// The assignments, in authored order.
    pub(crate) assignments: Vec<Assignment>,
}

/// The part of one model that identity and wiring are decided on.
#[derive(Clone, Debug, Default)]
pub(crate) struct Side<'a> {
    /// The blocks, in authored order.
    pub(crate) nodes: Vec<Node<'a>>,
    /// The declared boundary names, inputs and outputs alike.
    pub(crate) boundaries: Vec<&'a str>,
    /// The links, `(source, target)`, in authored order.
    pub(crate) links: Vec<(End<'a>, End<'a>)>,
}

/// How the blocks of two sides matched.
#[derive(Clone, Debug, Default)]
pub(crate) struct Matching {
    /// `(base, head)` positions of the blocks with the same name and class, in base order.
    pub(crate) by_name: Vec<(usize, usize)>,
    /// `(base, head, confidence)` of the pairs the rename pass matched, in base order.
    pub(crate) renamed: Vec<(usize, usize, Confidence)>,
    /// Base blocks matched to nothing, in base order.
    pub(crate) base_only: Vec<usize>,
    /// Head blocks matched to nothing, in head order.
    pub(crate) head_only: Vec<usize>,
    /// For each base block, the head block it is matched to by either step.
    pub(crate) partner: Vec<Option<usize>>,
    /// For each head block, whether it is matched by either step.
    pub(crate) matched_head: Vec<bool>,
}

/// What [`compare`] found.
#[derive(Clone, Debug, Default)]
pub(crate) struct Outcome {
    /// The `added`, `removed`, `renamed` and `rewired` entries, unsorted.
    pub(crate) changes: Vec<GraphChange>,
    /// The matching the entries follow from.
    pub(crate) matching: Matching,
    /// The notes about the rename pass and the names.
    pub(crate) notes: Vec<Note>,
    /// The block counts.
    pub(crate) counts: BlockCounts,
}

/// Match the blocks of two sides and report the entries that follow from the matching.
///
/// Step 1 pairs blocks of the same name and class. Step 2 leaves a pair of the same name and
/// different classes unmatched on both sides. Step 3, the rename pass, runs over what is left
/// unless `options.renames` is off, a side has nothing left, or a cap is passed.
pub(crate) fn compare(base: &Side<'_>, head: &Side<'_>, options: &DiffOptions) -> Outcome {
    let base_index = Index::new(base);
    let head_index = Index::new(head);
    let base_names: Vec<&str> = base.nodes.iter().map(|node| node.name).collect();
    let head_names: Vec<&str> = head.nodes.iter().map(|node| node.name).collect();
    let named = pair(&base_names, &head_names);

    let mut matching = Matching {
        partner: vec![None; base.nodes.len()],
        matched_head: vec![false; head.nodes.len()],
        ..Matching::default()
    };
    let mut loose_base = named.base_only;
    let mut loose_head = named.head_only;
    for (b, h) in named.both {
        let same_class = matches!(
            (base.nodes.get(b), head.nodes.get(h)),
            (Some(old), Some(new)) if old.class == new.class
        );
        if same_class {
            matching.by_name.push((b, h));
            set(&mut matching.partner, b, Some(h));
            set(&mut matching.matched_head, h, true);
        } else {
            loose_base.push(b);
            loose_head.push(h);
        }
    }
    loose_base.sort_unstable();
    loose_head.sort_unstable();

    let mut notes = Vec::new();
    let nothing_to_pair = loose_base.is_empty() || loose_head.is_empty();
    // `options.canonical_ids_equal` is a hint and is not read: with equal ids nothing is left
    // to pair, which is the fast path whoever says so, and a wrong hint must not cost a rename.
    if !options.renames {
        notes.push(Note::RenamePassOff);
    } else if !nothing_to_pair {
        let sides = rename::Sides {
            base,
            head,
            base_index: &base_index,
            head_index: &head_index,
        };
        match rename::run(&sides, &mut matching, &loose_base, &loose_head) {
            Some(pairs) => matching.renamed = pairs,
            None => notes.push(Note::RenamePassSkipped),
        }
    }
    matching.base_only = loose_base
        .into_iter()
        .filter(|&b| matches!(matching.partner.get(b), Some(None)))
        .collect();
    matching.head_only = loose_head
        .into_iter()
        .filter(|&h| matches!(matching.matched_head.get(h), Some(false)))
        .collect();

    let counts = BlockCounts {
        by_name: matching.by_name.len(),
        renamed: matching.renamed.len(),
        base_only: matching.base_only.len(),
        head_only: matching.head_only.len(),
    };
    if counts.by_name == 0 && !base.nodes.is_empty() && !head.nodes.is_empty() {
        notes.push(Note::NamesDisjoint);
    }
    notes.sort_unstable();
    notes.dedup();

    let sides = entries::Sides {
        base,
        head,
        base_index: &base_index,
        head_index: &head_index,
    };
    Outcome {
        changes: entries::changes(&sides, &matching),
        matching,
        notes,
        counts,
    }
}

/// Store `value` at `position`; a position outside the list is ignored, which cannot happen for
/// the positions this module computes.
fn set<T>(list: &mut [T], position: usize, value: T) {
    if let Some(slot) = list.get_mut(position) {
        *slot = value;
    }
}
