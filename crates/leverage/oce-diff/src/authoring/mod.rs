//! The authoring-level diff (diff note, "What `oce-diff` is, and is not"; decisions 3, 4, 6, 10
//! and 14): two `oce_author::Sequence` values in, a [`Diff`] out.
//!
//! [`diff`] lowers each model to the level-neutral side the `wiring` module matches, then adds
//! what only this level has: assignment changes and catalog defaults, authored order, the
//! sequence parameters with what they affect, the boundary declarations, and the metadata
//! (names, provenance, semantic fields, comments and spellings).
//!
//! It never validates. Both inputs are parsed models and nothing else is asked of them: a draft
//! with a dangling connection, a repeated name or a class the catalog does not know is diffed as
//! it stands. Nothing is loaded, grounded, scheduled or run.
//!
//! One thing in a model is not compared: whether a blank line precedes a statement
//! (`Annotations::blank_line_before`), which is layout. Every other field of the model is, as a
//! field and never as its spelling (a link end as a boundary or as a block and a port, a written
//! number beside the value it stands for), so an empty diff means the two models are equal up to
//! blank lines.

mod blocks;
mod interface;
pub(crate) mod lower;
mod metadata;
mod parameters;

use oce_author::Sequence;

use crate::catalog;
use crate::changes::{same_but_for_order, sort_section};
use crate::data::{DIFF_SCHEMA, Diff, DiffOptions, SequenceRef, Source, Summary};
use crate::wiring;

/// Compare two authoring models.
///
/// `base` is the earlier side and `head` the later: an entry's `old` is base's and its `new`
/// head's. The result is data, sorted and tagged [`DIFF_SCHEMA`]; [`crate::render_text`] and
/// [`crate::render_markdown`] turn it into text.
///
/// Blocks are matched by authored name: the same name and class path is the same block, the same
/// name with another class path is `removed` plus `added`, and the blocks left over go through
/// the bounded rename pass unless `options` turns it off. `summary.graph_equal` is decided by the
/// first two steps alone.
///
/// Infallible, and it never validates: it takes parsed models as they are, reads the catalog only
/// for a parameter's literal default and type, and loads no engine. Its work is near-linear in
/// the size of the two models; the rename pass adds at most [`crate::MAX_RENAME_ROUNDS`] passes
/// over the connections of at most [`crate::MAX_RENAME_CANDIDATES`] unmatched blocks a side.
/// Never panics.
#[must_use]
pub fn diff(base: &Sequence, head: &Sequence, options: &DiffOptions) -> Diff {
    let base_side = lower::side(base);
    let head_side = lower::side(head);
    let outcome = wiring::compare(&base_side, &head_side, options);

    let mut graph = outcome.changes;
    let mut metadata = Vec::new();
    blocks::changes(base, head, &outcome.matching, &mut graph, &mut metadata);
    let mut parameters = parameters::changes(base, head, &mut metadata);
    let mut interface = interface::changes(base, head, &mut metadata);
    metadata::document(base, head, &mut metadata);

    sort_section(&mut parameters);
    sort_section(&mut interface);
    sort_section(&mut graph);
    sort_section(&mut metadata);

    // A section flag ignores `reordered` entries (decisions 5 and 16: order is not semantic);
    // `equal` does not. A block left unmatched by name and class is in the graph section as
    // `removed`, `added` or `renamed` whatever the rename pass did, so `graph_equal` never
    // depends on the pass.
    let summary = Summary {
        equal: parameters.is_empty()
            && interface.is_empty()
            && graph.is_empty()
            && metadata.is_empty(),
        graph_equal: same_but_for_order(&graph),
        parameters_equal: Some(same_but_for_order(&parameters)),
        interface_equal: same_but_for_order(&interface),
        metadata_equal: Some(metadata.is_empty()),
        blocks: outcome.counts,
    };
    Diff {
        schema: DIFF_SCHEMA.to_owned(),
        source: Source::Authoring,
        catalog: catalog::content_id().to_owned(),
        base: reference(base),
        head: reference(head),
        summary,
        notes: outcome.notes,
        parameters,
        interface,
        graph,
        metadata,
    }
}

fn reference(sequence: &Sequence) -> SequenceRef {
    SequenceRef {
        sequence: sequence.name.as_str().to_owned(),
        derived_from: sequence
            .derived_from
            .as_ref()
            .map(|id| id.as_str().to_owned()),
    }
}

/// The comment lines above a statement as one text, or `None` when there are none.
fn comment(lines: &[String]) -> Option<String> {
    (!lines.is_empty()).then(|| lines.join("\n"))
}
