//! Document-wide subset checks for active array-valued connectors and block instances.

use std::collections::HashSet;

use oce_diag::{Diagnostic, Severity};

use crate::dto::CxfDocument;

use super::composite_rules::{ARRAY_CONNECTOR, ARRAY_INSTANCE};
use super::specialize::Specialization;
use super::value_types::{first_type, term_of};

/// Reject active array-valued connector and block-instance nodes before composite lowering.
///
/// Parameter nodes are deliberately excluded: preserved array parameters are expanded later in
/// the resolver. Nodes removed by specialization are invisible to this check.
pub(super) fn reject_unsupported(
    doc: &CxfDocument,
    specialization: &Specialization,
    diags: &mut Vec<Diagnostic>,
) {
    let mut connector_ids = HashSet::new();
    let mut instance_ids = HashSet::new();
    let mut parameter_ids = HashSet::new();

    for parent in &doc.graph {
        if specialization.is_inactive(&parent.id) {
            continue;
        }
        connector_ids.extend(
            parent
                .has_input
                .iter()
                .chain(parent.has_output.iter())
                .map(|reference| reference.id.as_str())
                .filter(|id| !specialization.is_inactive(id)),
        );
        instance_ids.extend(
            parent
                .contains_block
                .iter()
                .map(|reference| reference.id.as_str())
                .filter(|id| !specialization.is_inactive(id)),
        );
        parameter_ids.extend(
            parent
                .has_parameter
                .iter()
                .map(|reference| reference.id.as_str())
                .filter(|id| !specialization.is_inactive(id)),
        );
    }

    for node in &doc.graph {
        if specialization.is_inactive(&node.id)
            || (node.is_array != Some(true) && node.size_dims.is_none())
        {
            continue;
        }
        let is_parameter = parameter_ids.contains(node.id.as_str())
            || first_type(node).is_some_and(|type_iri| term_of(type_iri) == "Parameter");
        if connector_ids.contains(node.id.as_str()) && !is_parameter {
            diags.push(
                Diagnostic::new(
                    Severity::Error,
                    ARRAY_CONNECTOR.code,
                    ARRAY_CONNECTOR.message(
                        "array-valued connector nodes are not supported; use per-element \
                         connectors named `name_1` through `name_n`",
                    ),
                )
                .with_subject(node.id.clone()),
            );
        }
        if instance_ids.contains(node.id.as_str()) && !is_parameter {
            diags.push(
                Diagnostic::new(
                    Severity::Error,
                    ARRAY_INSTANCE.code,
                    ARRAY_INSTANCE.message(
                        "array-valued block-instance nodes are not supported; use per-element \
                         instances named `name_1` through `name_n`",
                    ),
                )
                .with_subject(node.id.clone()),
            );
        }
    }
}
