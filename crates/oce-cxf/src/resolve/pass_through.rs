//! Materialization of native scalar boundary pass-through blocks.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use oce_diag::{DiagCode, Diagnostic};
use oce_model::{BlockId, BlockInstance, Connector, ConnectorId, Dir, ParamTable, ValueType};

use crate::dto::{CxfDocument, Node};

use super::value_types::try_derive_value_type;

/// Derive boundary types, retaining `String` only for endpoints of a native pass-through edge.
pub(super) fn derive_boundary_types<'a>(
    doc: &'a CxfDocument,
    boundary_in: &HashSet<&str>,
    boundary_out: &HashSet<&str>,
    diags: &mut Vec<Diagnostic>,
) -> HashMap<&'a str, Option<ValueType>> {
    let mut pass_endpoints = HashSet::new();
    for node in &doc.graph {
        let source = node.id.as_str();
        for target in node
            .is_connected_to
            .iter()
            .map(|reference| reference.id.as_str())
        {
            if (boundary_in.contains(source) && boundary_out.contains(target))
                || (boundary_out.contains(source) && boundary_in.contains(target))
            {
                pass_endpoints.insert(source);
                pass_endpoints.insert(target);
            }
        }
    }
    doc.graph
        .iter()
        .filter_map(|node| {
            let iri = node.id.as_str();
            (boundary_in.contains(iri) || boundary_out.contains(iri)).then(|| {
                let value_type = if pass_endpoints.contains(iri) {
                    try_derive_pass_through_value_type(node, diags)
                } else {
                    try_derive_value_type(node, diags)
                };
                (iri, value_type)
            })
        })
        .collect()
}

fn try_derive_pass_through_value_type(
    node: &Node,
    diags: &mut Vec<Diagnostic>,
) -> Option<ValueType> {
    if let Some(datatype) = &node.is_of_data_type {
        if datatype.id.rsplit([':', '#', '/']).next() == Some("String") {
            return Some(ValueType::String);
        }
        return try_derive_value_type(node, diags);
    }
    if node
        .r#type
        .as_ref()
        .and_then(|types| types.as_slice().first())
        .and_then(|iri| iri.rsplit([':', '#', '/']).next())
        .is_some_and(|term| term.starts_with("String"))
    {
        return Some(ValueType::String);
    }
    try_derive_value_type(node, diags)
}

/// Append reserved identity blocks after authored nodes have received their stable positions.
pub(super) fn materialize(
    pairs: Vec<(String, String)>,
    boundary_types: &HashMap<&str, Option<ValueType>>,
    blocks: &mut Vec<BlockInstance>,
    connectors: &mut Vec<Connector>,
    external_inputs: &mut Vec<ConnectorId>,
    diags: &mut Vec<Diagnostic>,
) {
    for (input_iri, output_iri) in pairs {
        let value_type = boundary_types
            .get(input_iri.as_str())
            .copied()
            .flatten()
            .or_else(|| boundary_types.get(output_iri.as_str()).copied().flatten())
            .unwrap_or(ValueType::Real);
        let class_path = match value_type {
            ValueType::Real => "urn:oce:lowering#PassThrough.Real",
            ValueType::Integer => "urn:oce:lowering#PassThrough.Integer",
            ValueType::Boolean => "urn:oce:lowering#PassThrough.Boolean",
            ValueType::Enum(_) | ValueType::String => {
                diags.push(
                    Diagnostic::error(
                        DiagCode::NonSubsetConstruct,
                        "non-scalar boundary pass-through type is unsupported",
                    )
                    .with_subject(output_iri),
                );
                continue;
            }
        };
        let block_id = BlockId(blocks.len() as u32);
        let input_id = ConnectorId(connectors.len() as u32);
        let output_id = ConnectorId(input_id.0 + 1);
        let mut input = Connector::new(input_id, block_id, Dir::In, value_type, input_id.0);
        input.iri = Some(Arc::from(input_iri));
        let mut output = Connector::new(output_id, block_id, Dir::Out, value_type, output_id.0);
        output.iri = Some(Arc::from(output_iri));
        connectors.push(input);
        connectors.push(output);
        external_inputs.push(input_id);
        blocks.push(BlockInstance {
            id: block_id,
            class_iri: Arc::from(class_path),
            inputs: vec![input_id],
            outputs: vec![output_id],
            params: ParamTable::default(),
            decl_order: block_id.0,
            instance_iri: None,
        });
    }
}
