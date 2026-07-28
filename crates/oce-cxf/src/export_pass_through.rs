//! Export-side validation helpers for reserved pass-through blocks.

use oce_model::{Dir, ModelGraph};

/// Whether a reserved block has the resolver-produced single typed external In/Out shape.
pub(crate) fn has_valid_shape(graph: &ModelGraph, block_position: usize) -> bool {
    let Some(block) = graph.blocks.get(block_position) else {
        return false;
    };
    match (block.inputs.as_slice(), block.outputs.as_slice()) {
        ([input_id], [output_id]) => {
            let input = graph.connectors.get(input_id.0 as usize);
            let output = graph.connectors.get(output_id.0 as usize);
            input.is_some_and(|connector| {
                connector.block.0 as usize == block_position
                    && connector.dir == Dir::In
                    && graph.external_inputs.contains(input_id)
            }) && output.is_some_and(|connector| {
                connector.block.0 as usize == block_position
                    && connector.dir == Dir::Out
                    && input.is_some_and(|input| input.value_type == connector.value_type)
            })
        }
        _ => false,
    }
}
