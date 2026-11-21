//! The CXF-level diff (diff note, "What `oce-diff` is, and is not"; decisions 3, 8 and 15; issue
//! #666): two loaded documents, compared through the facade's [`Topology`] views keyed by IRI
//! with the root prefix stripped.
//!
//! [`diff_topology`] names each side's blocks and boundaries under its root, names each block's
//! ports from its class and ground parameters, and lowers the result to the level-neutral side
//! the `wiring` module matches, so a block keeps its identity, is renamed, and is wired exactly
//! as at the authoring level. What this level adds is the ground parameters of the blocks matched
//! by name and the interface section.
//!
//! **What this level cannot see.** A topology holds the loaded flat graph, the facts the
//! canonical content id encodes, the root id and nothing else:
//!
//! - sequence parameters, which are grounded into the leaves that reference them: a retune is
//!   `reparameterized` on each affected leaf, so `parameters_equal` is `None`;
//! - a reference against a literal of the same value, an Integer literal the emitter widened to
//!   the Real of a Real parameter, an Integer-spelled bound of a Real boundary (held as the
//!   Real), and a member of an Integer constant package (held as its Integer);
//! - comments, semantic fields other than `unit`, `derived_from` and authored order, so
//!   `derived_from` is `None` and no entry is `reordered`;
//! - a `unit`, `quantity` or `displayUnit` on a boundary that is not a Real;
//! - a boundary input nothing reads, an output nothing drives, and an output's bounds.
//!
//! The sequence name of each side is the root id's name after its namespace (the IRI after its
//! last `#`, `/` or `:`, as the content id names an IRI). Names under the root are stripped of
//! it, so a renamed root is no graph entry: it is the one metadata entry this level makes,
//! `sequence-renamed` (owner ruling of 2026-11-21), and `metadata_equal` is `Some(false)` beside
//! it and `None` otherwise, since nothing else of the metadata is seen.

mod lower;
mod sections;

use oce_api::{ContentId, Topology};

use crate::catalog;
use crate::changes::{MetadataChange, same_but_for_order, sort_section};
use crate::data::{DIFF_SCHEMA, Diff, DiffOptions, Note, SequenceRef, Side, Source, Summary};
use crate::error::DiffError;
use crate::wiring;

/// Compare two loaded documents through their topology views.
///
/// `base` is the earlier side and `head` the later, each the [`Topology`] of a document the
/// caller loaded (`oce_api::Engine::topology`); this crate loads nothing. The result is the same
/// [`Diff`] data as [`crate::diff`] gives, with `source` [`Source::Cxf`]:
///
/// - a block's identity is its instance IRI with the root prefix (`model_id` and a `.`)
///   stripped; a block with no instance IRI (`b<id>`) is matched by the rename pass only and
///   the diff carries [`Note::UnnamedBlocks`];
/// - blocks are matched, renamed and rewired as at the authoring level, ports named by index;
/// - a changed ground parameter is `reparameterized` on its block, so a sequence-parameter
///   retune is one entry per affected leaf, and `parameters_equal` is `None`;
/// - two sequence names that differ are a `sequence-renamed` metadata entry, with
///   `metadata_equal` `Some(false)`; otherwise `metadata_equal` is `None`;
/// - order is never compared and no entry is `reordered`.
///
/// **Content ids.** Equal canonical content ids and equal sequence names never coexist with a
/// graph or interface entry. The diff reads only facts the id encodes, plus the root, and a
/// name here is what follows the root and a `.`, with no `#`, `/` or `:`, so the id names the
/// same element `{sequence}.{name}`; with equal sequence names the root adds nothing. The sequence names are needed: the id never encodes the root id, so a
/// document whose root `@id` alone moved to a shorter dot-prefix of its instance IRIs keeps its
/// id and names every element differently here. The converse does not hold either: a renamed
/// root moves the id and is only a `sequence-renamed` entry, and an interface-only change moves
/// it with the graph section empty.
///
/// # Errors
///
/// [`DiffError::OutsideRoot`] when a block or boundary path of either side is not under that
/// side's root, and [`DiffError::UnresolvedPorts`] when a block's ports cannot be named from
/// its class and ground parameters (no loaded document has one). Base is checked before head.
///
/// # Cost
///
/// Near-linear in the two topologies: each block, port, parameter, connection and boundary is
/// visited a fixed number of times and looked up through ordered maps, the facade names each
/// block's ports once, and the rename pass is the authoring level's, with its caps. Never panics.
pub fn diff_topology(
    base: &Topology,
    head: &Topology,
    options: &DiffOptions,
) -> Result<Diff, DiffError> {
    let base = lower::lower(base, Side::Base)?;
    let head = lower::lower(head, Side::Head)?;
    let base_side = base.side();
    let head_side = head.side();
    let outcome = wiring::compare(&base_side, &head_side, options);

    let mut graph = outcome.changes;
    sections::parameters(&base, &head, &outcome.matching, &mut graph);
    let mut interface = sections::interface(&base, &head);
    sort_section(&mut graph);
    sort_section(&mut interface);

    let mut notes = outcome.notes;
    if base.has_unnamed() || head.has_unnamed() {
        notes.push(Note::UnnamedBlocks);
    }
    notes.sort_unstable();
    notes.dedup();

    let (base_ref, head_ref) = (reference(base.topology), reference(head.topology));
    // The one metadata fact a topology holds is its root (owner ruling of 2026-11-21): a renamed
    // root is reported as the authoring level reports it, and nothing else of the metadata is
    // seen, so the flag is `false` beside the entry and unknown without it.
    let mut metadata = Vec::new();
    if base_ref.sequence != head_ref.sequence {
        metadata.push(MetadataChange::SequenceRenamed {
            subject: "sequence".to_owned(),
            old: Some(base_ref.sequence.clone()),
            new: Some(head_ref.sequence.clone()),
        });
    }
    let summary = Summary {
        equal: graph.is_empty() && interface.is_empty() && metadata.is_empty(),
        graph_equal: same_but_for_order(&graph),
        parameters_equal: None,
        interface_equal: same_but_for_order(&interface),
        metadata_equal: (!metadata.is_empty()).then_some(false),
        blocks: outcome.counts,
    };
    Ok(Diff {
        schema: DIFF_SCHEMA.to_owned(),
        source: Source::Cxf,
        catalog: catalog::content_id().to_owned(),
        base: base_ref,
        head: head_ref,
        summary,
        notes,
        parameters: Vec::new(),
        interface,
        graph,
        metadata,
    })
}

/// [`diff_topology`] for a caller that holds the two documents' canonical content ids
/// (`oce_api::Engine::content_id`).
///
/// It sets [`DiffOptions::canonical_ids_equal`] to whether the two ids are equal and calls
/// [`diff_topology`]. That option is a hint that never changes a result: the return value is
/// exactly [`diff_topology`]'s for the same topologies and options, whatever ids are given, true
/// or false. Nothing is skipped on the ids' word.
///
/// # Errors
///
/// As [`diff_topology`].
pub fn diff_topology_with_ids(
    base: &Topology,
    base_id: &ContentId,
    head: &Topology,
    head_id: &ContentId,
    options: &DiffOptions,
) -> Result<Diff, DiffError> {
    let mut options = *options;
    options.canonical_ids_equal = base_id == head_id;
    diff_topology(base, head, &options)
}

/// A side's sequence: the root id's name after its namespace, or the whole id when nothing
/// follows the namespace. A topology carries no provenance, so `derived_from` is `None`.
fn reference(topology: &Topology) -> SequenceRef {
    let root = topology.model_id.as_str();
    let start = root.rfind(['#', '/', ':']).map_or(0, |at| at + 1);
    let name = root
        .get(start..)
        .filter(|name| !name.is_empty())
        .unwrap_or(root);
    SequenceRef {
        sequence: name.to_owned(),
        derived_from: None,
    }
}

#[cfg(test)]
#[path = "topology_tests.rs"]
mod tests;
