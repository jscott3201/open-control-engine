//! Export-side validation helpers for reserved pass-through blocks.

use std::collections::BTreeSet;

use oce_model::{Attrs, Connector, ModelGraph, ValueType};

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

/// Whether a reserved block declares one input/output pair whose input belongs to the block and is
/// listed as external. This first structural check avoids choosing an endpoint diagnostic subject
/// from malformed membership data.
fn has_valid_wiring_shape(graph: &ModelGraph, block_position: usize) -> bool {
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

/// Whether the reserved input/output pair exists, belongs to the block, and has the required
/// directions. Phase 3 runs this before Phase 6 can skip a cascade-deferred reserved block.
fn has_valid_endpoint_shape(graph: &ModelGraph, block_position: usize) -> bool {
    let Some(block) = graph.blocks.get(block_position) else {
        return false;
    };
    match (block.inputs.as_slice(), block.outputs.as_slice()) {
        ([input_id], [output_id]) => match (
            graph.connectors.get(input_id.0 as usize),
            graph.connectors.get(output_id.0 as usize),
        ) {
            (Some(input), Some(output)) => {
                input.block.0 as usize == block_position
                    && output.block.0 as usize == block_position
                    && input.dir == oce_model::Dir::In
                    && output.dir == oce_model::Dir::Out
            }
            _ => false,
        },
        _ => false,
    }
}

/// Whether both reserved endpoints carry the boundary identities Phase 6 emits.
fn has_boundary_iris(graph: &ModelGraph, block_position: usize) -> bool {
    let Some(block) = graph.blocks.get(block_position) else {
        return false;
    };
    match (block.inputs.as_slice(), block.outputs.as_slice()) {
        ([input_id], [output_id]) => graph
            .connectors
            .get(input_id.0 as usize)
            .zip(graph.connectors.get(output_id.0 as usize))
            .is_some_and(|(input, output)| input.iri.is_some() && output.iri.is_some()),
        _ => false,
    }
}

fn has_elided_input_attrs(input: &Connector) -> bool {
    if !input.attrs.matches(input.value_type) {
        return false;
    }
    match &input.attrs {
        Attrs::Real(attrs) => {
            attrs.quantity.is_some()
                || attrs.unit.is_some()
                || attrs.display_unit.is_some()
                || attrs.min.is_some_and(f64::is_finite)
                || attrs.max.is_some_and(f64::is_finite)
        }
        Attrs::Integer(attrs) => attrs.min.is_some() || attrs.max.is_some(),
        Attrs::Boolean(_) | Attrs::String(_) | Attrs::Enum(_) => false,
    }
}

/// Whether elision would discard an authored identity, parameter, or input attribute, or would
/// change the reserved scalar identity. A deferred reserved block checks every non-default input
/// attribute here because Phase 4 intentionally suppresses its ordinary connector diagnostics.
fn has_unrepresentable_state(graph: &ModelGraph, block_position: usize, deferred: bool) -> bool {
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
                input.value_type != value_type
                    || output.value_type != value_type
                    || has_elided_input_attrs(input)
                    || (deferred && input.attrs != Attrs::default_for(input.value_type))
            }
            _ => false,
        },
        _ => false,
    }
}

/// A reserved-shape rejection category with its stable diagnostic subject.
pub(crate) enum ReservedShapeFailure {
    /// Block/connector cross-references disagree.
    Structure(String),
    /// The reserved input or output lacks the boundary identity needed for emission.
    BoundaryIri(String),
    /// Elision would discard state or change the reserved scalar identity.
    HiddenState(String),
}

fn block_subject(graph: &ModelGraph, block_position: usize) -> String {
    graph
        .blocks
        .get(block_position)
        .and_then(|block| block.instance_iri.as_deref())
        .map_or_else(|| format!("block#{block_position}"), str::to_owned)
}

fn endpoint_subject(graph: &ModelGraph, block_position: usize, fallback: &str) -> String {
    let Some(input_id) = graph
        .blocks
        .get(block_position)
        .and_then(|block| block.inputs.first())
    else {
        return fallback.to_owned();
    };
    graph
        .connectors
        .get(input_id.0 as usize)
        .and_then(|input| {
            graph
                .blocks
                .get(input.block.0 as usize)
                .and_then(|owner| owner.instance_iri.as_deref())
        })
        .map_or_else(|| format!("connector#{}", input_id.0), str::to_owned)
}

/// Validate every reserved block before deferred-owner skips can suppress Phase 6 checks.
pub(crate) fn reserved_shape_failures(
    graph: &ModelGraph,
    deferred: &BTreeSet<usize>,
) -> Vec<ReservedShapeFailure> {
    let mut failures = Vec::new();
    for (block_position, block) in graph.blocks.iter().enumerate() {
        if !is_pass_through_class(&block.class_iri) {
            continue;
        }
        let block_subject = block_subject(graph, block_position);
        if !has_valid_wiring_shape(graph, block_position) {
            failures.push(ReservedShapeFailure::Structure(block_subject.clone()));
        } else {
            let endpoint_subject = endpoint_subject(graph, block_position, &block_subject);
            if !has_valid_endpoint_shape(graph, block_position) {
                failures.push(ReservedShapeFailure::Structure(endpoint_subject));
            } else if !has_boundary_iris(graph, block_position) {
                failures.push(ReservedShapeFailure::BoundaryIri(endpoint_subject));
            }
        }
        if has_unrepresentable_state(graph, block_position, deferred.contains(&block_position)) {
            failures.push(ReservedShapeFailure::HiddenState(block_subject));
        }
    }
    failures
}

/// Whether Phase 6 can plan this reserved block without repeating a Phase 3 diagnostic.
pub(crate) fn has_plannable_shape(graph: &ModelGraph, block_position: usize) -> bool {
    has_valid_wiring_shape(graph, block_position)
        && has_valid_endpoint_shape(graph, block_position)
        && has_boundary_iris(graph, block_position)
}
