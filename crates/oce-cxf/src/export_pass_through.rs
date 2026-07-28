//! Export-side validation helpers for reserved pass-through blocks.

use oce_model::ModelGraph;

/// Whether a reserved block has the resolver-produced arity and external-input membership.
///
/// Phase 6 owns connector existence, ownership, direction, and pair-type validation while it
/// plans the external input and reserved output; duplicating those checks here adds no distinct
/// rejection. This earlier shape gate owns only block arity and membership.
pub(crate) fn has_valid_shape(graph: &ModelGraph, block_position: usize) -> bool {
    let Some(block) = graph.blocks.get(block_position) else {
        return false;
    };
    match (block.inputs.as_slice(), block.outputs.as_slice()) {
        ([input_id], [_output_id]) => graph.external_inputs.contains(input_id),
        _ => false,
    }
}
