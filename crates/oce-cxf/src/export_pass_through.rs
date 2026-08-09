//! Export-side validation helpers for reserved pass-through blocks.

use oce_model::{Connector, ModelGraph, ValueType};

fn pass_through_value_type(class_path: &str) -> Option<ValueType> {
    match class_path {
        "urn:oce:lowering#PassThrough.Real" => Some(ValueType::Real),
        "urn:oce:lowering#PassThrough.Integer" => Some(ValueType::Integer),
        "urn:oce:lowering#PassThrough.Boolean" => Some(ValueType::Boolean),
        _ => None,
    }
}

/// Whether `class_path` names one of the reserved scalar pass-through identities.
pub(crate) fn is_pass_through_class(class_path: &str) -> bool {
    pass_through_value_type(class_path).is_some()
}

/// Whether `connector` belongs to the declared input/output pair of a reserved pass-through block.
pub(crate) fn is_declared_pass_through_connector(
    graph: &ModelGraph,
    connector: &Connector,
) -> bool {
    graph
        .blocks
        .get(connector.block.0 as usize)
        .filter(|block| is_pass_through_class(&block.class_iri))
        .is_some_and(|block| {
            block.inputs.contains(&connector.id) || block.outputs.contains(&connector.id)
        })
}

/// Return the reserved-owned endpoint of a connection, source first, with its connector position.
/// Returns `None` when neither endpoint belongs to a reserved pass-through block.
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

/// Whether Phase 3's pre-existing wiring checks accept a reserved block.
pub(crate) fn has_valid_wiring_shape(graph: &ModelGraph, block_position: usize) -> bool {
    let Some(block) = graph.blocks.get(block_position) else {
        return false;
    };
    match (block.inputs.as_slice(), block.outputs.as_slice()) {
        ([input_id], [_output_id]) => {
            graph
                .connectors
                .get(input_id.0 as usize)
                .is_some_and(|input| input.block.0 as usize == block_position)
                && graph.external_inputs.contains(input_id)
        }
        _ => false,
    }
}

/// Whether elision would discard authored state or change the reserved scalar identity.
pub(crate) fn has_unrepresentable_state(graph: &ModelGraph, block_position: usize) -> bool {
    let Some(block) = graph.blocks.get(block_position) else {
        return false;
    };
    let Some(value_type) = pass_through_value_type(&block.class_iri) else {
        return false;
    };
    if block.instance_iri.is_some() || !block.params.values.is_empty() {
        return true;
    }
    match (block.inputs.as_slice(), block.outputs.as_slice()) {
        ([input_id], [output_id]) => match (
            graph.connectors.get(input_id.0 as usize),
            graph.connectors.get(output_id.0 as usize),
        ) {
            (Some(input), Some(output)) => {
                input.value_type != value_type || output.value_type != value_type
            }
            _ => false,
        },
        _ => false,
    }
}
