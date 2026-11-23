//! Charging the identities a document holds under its blocks' instance IRIs to the document's
//! [`InstancePathBudget`] (issue #807): the padded outputs of `hasInstance` instances, before
//! lowering builds any of them, and the inputs a boundary drives, once Step 9 has found them. The
//! third charge, each listing of a scalar parameter, is made where Step 7 reads the listing
//! (`instance_params`). The budget and what it leaves to other bounds are in
//! `crate::instance_path_budget`.

use std::collections::{HashMap, HashSet};

use oce_diag::DiagnosticList;
use oce_model::{ConnectorId, ModelGraph};

use crate::dto::{CxfDocument, Node};
use crate::instance_path_budget::{InstancePath, InstancePathBudget, path_bytes};

use super::instance_interface::{contains_block_referents, is_derivation_shaped};
use super::specialize::Specialization;
use super::value_types::first_type;

/// Charge the padded output identities of every active derivation-shaped node, in `@graph`
/// order, the outputs of one node together; `false` when one node's did not fit, which refuses
/// the document before lowering builds any identity (nothing after that node is charged).
///
/// A node's padded outputs are the outputs its class publishes that no **direct** member names
/// (the node's `@id`, `.`, the output's name): the identities `<IRI>.<name>` the derivation and
/// the composite orientation index build for it. The derivation pads an output for every name no
/// member it admits lists, and the orientation index pads fewer, so this counts every padded
/// identity either builds. A member that is itself a block instance names the output it lists,
/// and is refused for it; the identity built for it is the member's own IRI, which the document
/// writes.
pub(super) fn charge_padded_outputs(
    doc: &CxfDocument,
    specialization: &Specialization,
    budget: &mut InstancePathBudget,
    diags: &mut DiagnosticList,
) -> bool {
    let referents = contains_block_referents(doc);
    for node in &doc.graph {
        if specialization.is_inactive(&node.id) || !is_derivation_shaped(node, &referents) {
            continue;
        }
        let Some(bytes) = padded_bytes(node) else {
            continue;
        };
        if let Err(refusal) = budget.take(InstancePath::PaddedOutputs, bytes) {
            diags.raise(|| refusal.diagnostic(&node.id, &node.id));
            return false;
        }
    }
    true
}

/// The bytes of `node`'s padded output identities together, `Some(None)` when they overflow a
/// `usize`, `None` when its class publishes no port names (a class whose port count comes from a
/// parameter, which the derivation refuses whole and the orientation index pads nothing for).
fn padded_bytes(node: &Node) -> Option<Option<usize>> {
    let names = first_type(node)
        .map(crate::bridge::class_path_of)
        .and_then(oce_blocks::port_names::port_names)?;
    let mut listed = vec![false; names.outputs.len()];
    for member in node.has_instance.iter() {
        let Some(name) = member
            .id
            .strip_prefix(node.id.as_str())
            .and_then(|rest| rest.strip_prefix('.'))
        else {
            continue;
        };
        if let Some(slot) = names.outputs.iter().position(|output| *output == name) {
            listed[slot] = true;
        }
    }
    let total = names
        .outputs
        .iter()
        .zip(&listed)
        .filter(|(_, listed)| !**listed)
        .try_fold(0_usize, |total, (name, _)| {
            total.checked_add(path_bytes(node.id.len(), name.len())?)
        });
    Some(total)
}

/// Charge each input a boundary drives once, as the export spells it: its block's instance IRI,
/// `.in` and its position among the block's inputs. Charged in `external_inputs` order; one that
/// does not fit is refused on its boundary input and reserves nothing. Inputs of blocks the
/// export defers or passes through are charged too, as an upper bound of what it writes.
pub(super) fn charge_boundary_inputs(
    graph: &ModelGraph,
    budget: &mut InstancePathBudget,
    diags: &mut DiagnosticList,
) {
    if graph.external_inputs.is_empty() {
        return;
    }
    // Each input's first position among its block's inputs, found in one pass.
    let mut positions: HashMap<(usize, ConnectorId), usize> = HashMap::new();
    for (block, instance) in graph.blocks.iter().enumerate() {
        for (position, &input) in instance.inputs.iter().enumerate() {
            positions.entry((block, input)).or_insert(position);
        }
    }
    let mut charged: HashSet<ConnectorId> = HashSet::new();
    for &id in &graph.external_inputs {
        let Some(connector) = graph.connectors.get(id.0 as usize) else {
            continue;
        };
        let owner = connector.block.0 as usize;
        let Some(instance) = graph
            .blocks
            .get(owner)
            .and_then(|b| b.instance_iri.as_deref())
        else {
            continue;
        };
        let Some(&position) = positions.get(&(owner, id)) else {
            continue;
        };
        if !charged.insert(id) {
            continue;
        }
        let suffix = "in".len() + decimal_digits(position);
        if let Err(refusal) = budget.take_path(InstancePath::BoundaryInput, instance.len(), suffix)
        {
            let subject = connector.iri.as_deref().unwrap_or(instance);
            diags.raise(|| refusal.diagnostic(subject, instance));
        }
    }
}

/// How many decimal digits `value` is written with.
fn decimal_digits(mut value: usize) -> usize {
    let mut digits = 1;
    while value >= 10 {
        value /= 10;
        digits += 1;
    }
    digits
}

#[cfg(test)]
#[path = "instance_paths_tests.rs"]
mod tests;
