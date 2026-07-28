//! Materialization of native scalar boundary pass-through blocks.

use std::collections::HashMap;
use std::sync::Arc;

use oce_diag::{DiagCode, Diagnostic};
use oce_model::{BlockId, BlockInstance, Connector, ConnectorId, Dir, ParamTable, ValueType};

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
