//! Export-side validation helpers for reserved pass-through blocks.

use oce_model::{Connector, ModelGraph};

/// Whether `class_path` names one of the reserved scalar pass-through identities.
pub(crate) fn is_pass_through_class(class_path: &str) -> bool {
    matches!(
        class_path,
        "urn:oce:lowering#PassThrough.Real"
            | "urn:oce:lowering#PassThrough.Integer"
            | "urn:oce:lowering#PassThrough.Boolean"
    )
}

/// Return the first source-to-target connector owned by a reserved pass-through block.
pub(crate) fn reserved_connection_endpoint(
    graph: &ModelGraph,
    source_position: usize,
    target_position: usize,
) -> Option<(&Connector, usize)> {
    [source_position, target_position]
        .into_iter()
        .find_map(|position| {
            let connector = graph.connectors.get(position)?;
            graph
                .blocks
                .get(connector.block.0 as usize)
                .is_some_and(|block| is_pass_through_class(&block.class_iri))
                .then_some((connector, position))
        })
}

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
