//! Materialization of represented top-composite boundary-input declarations.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use oce_diag::{DiagCode, Diagnostic};
use oce_model::{Attrs, BoundaryInput, ConnectorId, ModelGraph, NoAttrs, ValueType};

use crate::dto::{CxfDocument, Node};
use crate::export::EXPORT_ROOT_IRI;
use crate::export_defer::deferral_set;
use crate::export_pass_through::is_pass_through_class;

use super::attrs::connector_attrs;
use super::instance_params::Inst;

struct Assignment {
    source: String,
    target: String,
    count: usize,
    has_distinct_sources: bool,
}

/// Assignment cardinality for child inputs reached through top-composite boundary inputs.
#[derive(Default)]
pub(super) struct AssignmentTracker {
    by_target: HashMap<ConnectorId, Assignment>,
}

impl AssignmentTracker {
    /// Record an assignment and return whether it is the first assignment to the child input.
    pub(super) fn record(&mut self, target_id: ConnectorId, source: &str, target: &str) -> bool {
        match self.by_target.entry(target_id) {
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                let assignment = entry.get_mut();
                assignment.count += 1;
                assignment.has_distinct_sources |= assignment.source != source;
                false
            }
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(Assignment {
                    source: source.to_owned(),
                    target: target.to_owned(),
                    count: 1,
                    has_distinct_sources: false,
                });
                true
            }
        }
    }

    /// Emit one refusal for each child input reached more than once.
    pub(super) fn emit_diagnostics(self, diags: &mut Vec<Diagnostic>) {
        for assignment in self.by_target.into_values() {
            if assignment.count <= 1 {
                continue;
            }
            let message = if assignment.has_distinct_sources {
                "input is driven by distinct boundary inputs".to_owned()
            } else {
                format!("input is multiply driven (in-degree {})", assignment.count)
            };
            diags.push(
                Diagnostic::error(DiagCode::SingleAssignment, message)
                    .with_subject(assignment.target),
            );
        }
    }
}

/// Refuse a root input IRI that is also owned by an instance or one of its members.
///
/// Export emits root declarations, blocks, parameters, and child ports as separate nodes. Sharing
/// one authored IRI would therefore produce duplicate `@id` values even though import has only one
/// source node.
pub(super) fn refuse_role_aliases<'a>(
    top: &'a Node,
    boundary_in: &HashSet<&str>,
    conn_of_iri: &HashMap<&str, ConnectorId>,
    insts: &[Inst<'_>],
    diags: &mut Vec<Diagnostic>,
) -> HashSet<&'a str> {
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
    seen
}

/// Warn for every declared root boundary input whose node exists but that no connection reads.
///
/// `edges` is the lowered connection list, after nested-composite boundary hops are rewritten to
/// their leaf endpoints, so an input reaching a block only through a composite boundary, or
/// passing through to a boundary output, appears as an endpoint and stays silent. An input whose
/// only route ends at a nested composite boundary nothing reads loses that edge in lowering and
/// warns: no block, and no boundary output, observes its value.
///
/// Advisory only: an unread top-composite input is legal CDL, and listing it in the frame
/// contract would require every frame to supply a value the model never reads. It therefore
/// enters neither `external_inputs` nor `boundary_inputs`, and this warning is its only
/// representation. Declarations other checks already refuse stay silent here: a missing node
/// (`UnresolvedReference`), an IRI also declared as a boundary output (the shadowing refusal),
/// and a role alias (`MalformedDocument`). An inactive conditional input is absent from
/// `boundary_in`. An input whose only edge is itself refused (direction, type, missing target)
/// counts as read and stays silent: the load refuses anyway. Emitted once per IRI in `hasInput` array order; the resolver's final sort
/// fixes the reported order.
pub(super) fn warn_unread<'e>(
    top: &Node,
    boundary_in: &HashSet<&str>,
    boundary_out: &HashSet<&str>,
    by_id: &HashMap<&str, &Node>,
    role_aliases: &HashSet<&str>,
    edges: impl Iterator<Item = (&'e str, &'e str)>,
    diags: &mut Vec<Diagnostic>,
) {
    let read = edges
        .flat_map(|(source, target)| [source, target])
        .filter(|endpoint| boundary_in.contains(endpoint))
        .collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    for iri in top.has_input.iter().map(|reference| reference.id.as_str()) {
        if boundary_in.contains(iri)
            && !boundary_out.contains(iri)
            && by_id.contains_key(iri)
            && !role_aliases.contains(iri)
            && !read.contains(iri)
            && seen.insert(iri)
        {
            diags.push(
                Diagnostic::warning(
                    DiagCode::UnusedBoundaryInput,
                    "declared boundary input is read by no connection",
                )
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
    graph: &ModelGraph,
    role_aliases: &HashSet<&str>,
    boundary_types: &HashMap<&str, Option<ValueType>>,
    diags: &mut Vec<Diagnostic>,
) -> Vec<BoundaryInput> {
    let represented = graph
        .external_inputs
        .iter()
        .filter_map(|id| graph.connectors.get(id.0 as usize))
        .filter_map(|connector| connector.iri.as_deref())
        .collect::<HashSet<_>>();
    let (deferred, _) = deferral_set(graph);
    refuse_minted_aliases(doc, &represented, graph, role_aliases, &deferred, diags);

    doc.graph
        .iter()
        .filter(|node| {
            represented.contains(node.id.as_str()) && !role_aliases.contains(node.id.as_str())
        })
        .map(|node| BoundaryInput {
            iri: Arc::from(node.id.as_str()),
            attrs: declared_attrs(node, boundary_types, diags),
        })
        .collect()
}

/// Refuse emitted declaration IRIs that collide with identities synthesized for the survivor cone.
fn refuse_minted_aliases(
    doc: &CxfDocument,
    represented: &HashSet<&str>,
    graph: &ModelGraph,
    role_aliases: &HashSet<&str>,
    deferred: &BTreeSet<usize>,
    diags: &mut Vec<Diagnostic>,
) {
    let mut minted = HashSet::from([EXPORT_ROOT_IRI.to_owned()]);
    for (position, block) in graph.blocks.iter().enumerate() {
        if deferred.contains(&position) || is_pass_through_class(&block.class_iri) {
            continue;
        }
        let Some(instance) = block.instance_iri.as_deref() else {
            continue;
        };
        for (name, _) in &block.params.values {
            if !name.is_empty() && !name.contains('.') {
                minted.insert(format!("{instance}.{name}"));
            }
        }
    }
    for connector_id in &graph.external_inputs {
        let Some(connector) = graph.connectors.get(connector_id.0 as usize) else {
            continue;
        };
        let Some(block) = graph.blocks.get(connector.block.0 as usize) else {
            continue;
        };
        if deferred.contains(&(connector.block.0 as usize))
            || is_pass_through_class(&block.class_iri)
        {
            continue;
        }
        let Some(instance) = block.instance_iri.as_deref() else {
            continue;
        };
        let Some(position) = block.inputs.iter().position(|input| input == connector_id) else {
            continue;
        };
        minted.insert(format!("{instance}.in{position}"));
    }

    let emitted_boundaries = graph
        .external_inputs
        .iter()
        .filter_map(|id| graph.connectors.get(id.0 as usize))
        .filter(|connector| !deferred.contains(&(connector.block.0 as usize)))
        .filter_map(|connector| connector.iri.as_deref())
        .collect::<HashSet<_>>();

    for node in &doc.graph {
        if represented.contains(node.id.as_str())
            && emitted_boundaries.contains(node.id.as_str())
            && !role_aliases.contains(node.id.as_str())
            && minted.contains(&node.id)
        {
            diags.push(
                Diagnostic::error(
                    DiagCode::MalformedDocument,
                    "boundary input collides with a canonical export node identity",
                )
                .with_subject(node.id.clone()),
            );
        }
    }
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
