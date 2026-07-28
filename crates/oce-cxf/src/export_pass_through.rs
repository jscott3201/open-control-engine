//! Export-side validation helpers for reserved pass-through blocks.

use oce_model::ModelGraph;

/// Whether a reserved block has the resolver-produced arity and external-input membership.
///
/// Phase 6 owns connector existence, direction, output ownership, and pair-type validation while
/// it plans the external input and reserved output. It keys that plan from the input connector's
/// owner, so this earlier gate uniquely owns the input-to-reserved-block relationship in addition
/// to block arity and external-input membership.
pub(crate) fn has_valid_shape(graph: &ModelGraph, block_position: usize) -> bool {
    let Some(block) = graph.blocks.get(block_position) else {
        return false;
    };
    match (block.inputs.as_slice(), block.outputs.as_slice()) {
        ([input_id], [_output_id]) => {
            graph
                .connectors
                .get(input_id.0 as usize)
                .is_some_and(|connector| connector.block.0 as usize == block_position)
                && graph.external_inputs.contains(input_id)
        }
        _ => false,
    }
}
