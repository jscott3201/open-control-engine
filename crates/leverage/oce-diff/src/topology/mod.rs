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
//! canonical content id encodes, and nothing else:
//!
//! - sequence parameters, which are grounded into the leaves that reference them: a retune is
//!   `reparameterized` on each affected leaf, so `parameters_equal` is `None`;
//! - a reference against a literal of the same value, and an Integer literal the emitter widened
//!   to the Real of a Real parameter;
//! - comments, semantic fields other than `unit`, `derived_from` and authored order, so
//!   `metadata_equal` is `None`, `derived_from` is `None` and no entry is `reordered`;
//! - a boundary input nothing reads, an output nothing drives, and an output's bounds.
//!
//! The sequence name of each side is the root id's name after its namespace (the IRI after its
//! last `#`, `/` or `:`, as the content id names an IRI), so a renamed root shows in the two
//! names and in no section: every name under the root is stripped of it.

mod lower;
mod sections;

use oce_api::{ContentId, Topology};

use crate::catalog;
use crate::changes::{same_but_for_order, sort_section};
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
///   retune is one entry per affected leaf, and `parameters_equal` and `metadata_equal` are
///   `None`;
/// - order is never compared and no entry is `reordered`.
///
/// Equal canonical content ids prove equal loaded graphs, and a topology holds every fact the id
/// encodes, so two documents with equal ids give empty graph and interface sections. The
/// converse does not hold: the id also covers each name's root, which this level strips, so a
/// renamed root moves the id and leaves every section empty.
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

    let summary = Summary {
        equal: graph.is_empty() && interface.is_empty(),
        graph_equal: same_but_for_order(&graph),
        parameters_equal: None,
        interface_equal: same_but_for_order(&interface),
        metadata_equal: None,
        blocks: outcome.counts,
    };
    Ok(Diff {
        schema: DIFF_SCHEMA.to_owned(),
        source: Source::Cxf,
        catalog: catalog::content_id().to_owned(),
        base: reference(base.topology),
        head: reference(head.topology),
        summary,
        notes,
        parameters: Vec::new(),
        interface,
        graph,
        metadata: Vec::new(),
    })
}

/// [`diff_topology`] for a caller that holds the two documents' canonical content ids
/// (`oce_api::Engine::content_id`), the form of decision 8's fast path.
///
/// Equal ids prove equal loaded graphs, so every block matches by name and class and the rename
/// pass has nothing to do: that is the whole fast path, and [`diff_topology`] takes it whenever
/// no block is left unmatched on a side. The ids are recorded as
/// [`DiffOptions::canonical_ids_equal`] and are a statement only: the result is
/// [`diff_topology`]'s for the same topologies and options, whatever ids are given, so a wrong
/// pair costs nothing.
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
