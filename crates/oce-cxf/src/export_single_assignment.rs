//! CDL §7.10 single-assignment refusals over the export survivor cone.
//!
//! Every edge the exporter counts is one the emitted document carries, so an input with two
//! drivers re-imports with a `SingleAssignment` error and the bytes do not load at all. That is
//! the outcome class the exporter refuses rather than emits: `MSG_DUPLICATE_EXTERNAL_INPUT`
//! already rejects bytes that come back one entry short, and these would come back as nothing.
//! Rejecting at plan time also names the offender better than re-import can — the owning block's
//! `instance_iri`, against re-import's positional `connector#N`.

use oce_diag::Diagnostic;
use oce_model::ModelGraph;

use super::{owner_subject, reject};

/// A surviving input connector is driven by more than one **surviving** output. The emitted
/// document carries every one of those `isConnectedTo` targets, so re-import counts an in-degree
/// above 1 and fails the §7.10 single-assignment law — the export would be `Ok` with bytes that do
/// not load at all, a strictly worse silent loss than `MSG_DUPLICATE_EXTERNAL_INPUT`'s one
/// dropped entry. Deliberately not a reuse of `MSG_STRUCTURE`: a multiply-driven input is
/// internally *consistent* (ids in range, owners correct, every edge output→input) and needs a
/// different fix from the host than a cross-reference disagreement does. Hand-built graphs only —
/// `resolve`'s single-assignment pre-check rejects in-degree ≥ 2 on the way in, so no imported
/// graph can reach this.
const MSG_MULTIPLY_DRIVEN: &str = "export subset: input connector is driven by more than one surviving output, and the emitted \
     document fails re-import with a single-assignment error (§7.10)";

/// A surviving `external_inputs` connector is also the target of a surviving output→input wire.
/// The emitted document carries both the boundary node's `isConnectedTo` entry and the driving
/// port's, so re-import refuses the input as driven by a block output and a boundary input
/// (issue #472) — the same "bytes that do not load" class as [`MSG_MULTIPLY_DRIVEN`]. Hand-built
/// graphs only: `resolve` refuses the shape on the way in, so no imported graph can reach this.
const MSG_DRIVEN_EXTERNAL_INPUT: &str = "export subset: input connector is listed in external_inputs and is also driven by a \
     surviving output, and the emitted document fails re-import with a single-assignment error \
     (§7.10)";

/// Push one [`MSG_MULTIPLY_DRIVEN`] refusal per flagged connector.
///
/// `multiply_driven[i]` is set by Phase 5 the second time a surviving output→input edge reaches
/// connector `i`. One diagnostic per offending connector, in connector-position order (a post-loop
/// scan, not a reject at the push site), so a triple-driven input yields one entry rather than two
/// and the ordering matches the Phase 3 orphan scan. No `Dir::In` test is needed: Phase 5's
/// `output_to_input` guard is the only path that sets the flag, so it is false for every output.
pub(super) fn refuse_multiply_driven(
    g: &ModelGraph,
    multiply_driven: &[bool],
    diags: &mut Vec<Diagnostic>,
) {
    for (i, c) in g.connectors.iter().enumerate() {
        if multiply_driven.get(i).copied().unwrap_or(false) {
            diags.push(reject(MSG_MULTIPLY_DRIVEN, &owner_subject(g, c, i)));
        }
    }
}

/// Push a [`MSG_DRIVEN_EXTERNAL_INPUT`] refusal naming `subject` when the surviving
/// `external_inputs` connector it owns is also `wired` (driven by a surviving output→input edge),
/// and return whether it did, so Phase 6 drops the entry instead of planning its boundary node.
pub(super) fn refuse_driven_external(
    wired: bool,
    subject: &str,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    if wired {
        diags.push(reject(MSG_DRIVEN_EXTERNAL_INPUT, subject));
    }
    wired
}
