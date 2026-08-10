//! Materialization of represented top-composite boundary-input declarations.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use oce_diag::{DiagCode, Diagnostic};
use oce_model::{Attrs, BoundaryInput, Connector, ConnectorId, NoAttrs, ValueType};

use crate::dto::{CxfDocument, Node};

use super::attrs::connector_attrs;
use super::instance_params::Inst;

/// Refuse a root input IRI that is also owned by an instance or one of its members.
///
/// Export emits root declarations, blocks, parameters, and child ports as separate nodes. Sharing
/// one authored IRI would therefore produce duplicate `@id` values even though import has only one
/// source node.
pub(super) fn refuse_role_aliases(
    top: &Node,
    boundary_in: &HashSet<&str>,
    conn_of_iri: &HashMap<&str, ConnectorId>,
    insts: &[Inst<'_>],
    diags: &mut Vec<Diagnostic>,
) {
    let block_iris = insts
        .iter()
        .map(|inst| inst.node.id.as_str())
        .collect::<HashSet<_>>();
    let instance_member_iris = insts
        .iter()
        .flat_map(|inst| {
            inst.node
                .has_parameter
                .iter()
                .chain(inst.node.has_constant.iter())
                .chain(inst.node.has_instance.iter())
        })
        .map(|reference| reference.id.as_str())
        .collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    for iri in top
        .has_input
        .iter()
        .map(|reference| reference.id.as_str())
        .filter(|iri| boundary_in.contains(iri))
    {
        let message = if conn_of_iri.contains_key(iri) {
            "boundary input shadows an instance port connector"
        } else if block_iris.contains(iri) {
            "boundary input shadows a contained block"
        } else if instance_member_iris.contains(iri) {
            "boundary input shadows an instance member"
        } else {
            continue;
        };
        if seen.insert(iri) {
            diags.push(
                Diagnostic::error(DiagCode::MalformedDocument, message)
                    .with_subject(iri.to_owned()),
            );
        }
    }
}

/// Materialize one declaration per boundary IRI represented by `external_inputs`.
///
/// Source-document iteration fixes declaration order to boundary-node `@graph` position. Target
/// order and fan-out cardinality remain independent in `external_inputs`.
pub(super) fn materialize(
    doc: &CxfDocument,
    external_inputs: &[ConnectorId],
    connectors: &[Connector],
    boundary_types: &HashMap<&str, Option<ValueType>>,
    diags: &mut Vec<Diagnostic>,
) -> Vec<BoundaryInput> {
    let represented = external_inputs
        .iter()
        .filter_map(|id| connectors.get(id.0 as usize))
        .filter_map(|connector| connector.iri.as_deref())
        .collect::<HashSet<_>>();

    doc.graph
        .iter()
        .filter(|node| represented.contains(node.id.as_str()))
        .map(|node| BoundaryInput {
            iri: Arc::from(node.id.as_str()),
            attrs: declared_attrs(node, boundary_types, diags),
        })
        .collect()
}

/// Parse the declaration under its derived boundary type. A failed type derivation already carries
/// an error, so the placeholder never escapes the resolver's final error gate.
fn declared_attrs(
    node: &Node,
    boundary_types: &HashMap<&str, Option<ValueType>>,
    diags: &mut Vec<Diagnostic>,
) -> Attrs {
    match boundary_types.get(node.id.as_str()).copied().flatten() {
        Some(value_type) => connector_attrs(node, value_type, diags),
        None => Attrs::Boolean(NoAttrs),
    }
}
