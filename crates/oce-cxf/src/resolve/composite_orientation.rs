//! Canonical connection orientation at nested-composite boundaries.
//!
//! CXF §8.2 permits either endpoint of a connection to carry `isConnectedTo`, so subject
//! position does not encode signal direction. Before the boundary walk, every authored edge with
//! at least one non-root boundary-port endpoint is classified by port role and strict transitive
//! containment, then kept, re-anchored, or left authored:
//!
//! - Polarity: a leaf output is a SOURCE and a leaf input a SINK; root boundary polarities are
//!   fixed (input SOURCE, output SINK); a non-root boundary input is a SOURCE toward its own
//!   composite interior (peer located in, or strictly inside, the owner) and a SINK outside it;
//!   a non-root boundary output is the mirror image.
//! - Verdicts: (SOURCE, SINK) keeps the authored direction; (SINK, SOURCE) swaps unless the
//!   canonical source has neither an `@graph` node nor a synthesized connector identity; same-
//!   polarity pairs are contradictory. Edges with an underivable endpoint (dangling or
//!   non-connector peer, conflicted ownership, non-tree containment) cannot be oriented. If
//!   lowering would erase an active unorientable edge, its diagnostic is deferred until after the
//!   bounded boundary walk. Active boundary-source edges to inactive targets similarly preserve
//!   the ordinary inactive-node refusal.
//! - Duplicate suppression: one forward and one reverse statement of the same relation collapse.
//!   Two authored copies in the same direction are not collapsed, so a genuine double-drive stays
//!   visible to the single-assignment check.
//!
//! Cost (GitHub issue #480): every per-edge question is asked of interned endpoints
//! ([`endpoints`]), and owners, composites and parents are `@graph` positions rather than copied
//! IRIs, so the pass costs time linear in the document's bytes — never fan-out × IRI length.

use std::collections::{HashMap, HashSet};

use crate::dto::{CxfDocument, Node};
use oce_diag::{DiagCode, Diagnostic};

use super::composite::is_runtime_composite;
use super::instance_interface::{
    contains_block_referents, is_derivation_shaped, orientation_port_members,
};
use super::specialize::Specialization;

mod diagnostics;
mod endpoints;

use endpoints::Endpoint;
pub(super) use endpoints::{CanonicalConnections, EndpointIx, LoweringEndpoints, SharedIris};

/// The `@graph` position that identifies one node `@id`: the last node carrying that `@id`, so
/// two indices are equal exactly when the `@id`s are byte-equal.
type NodeIx = usize;

#[derive(Clone, Copy, Debug)]
enum Role {
    BoundaryInput(NodeIx),
    BoundaryOutput(NodeIx),
    LeafInput,
    LeafOutput,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum Polarity {
    Source,
    Sink,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Verdict {
    Keep,
    Swap,
    SwapBlocked,
    Contradictory,
    Unknown,
    Untouched,
}

/// Port-role and containment index for one document: composite membership, boundary-port sets,
/// per-port ownership claims, `@graph` positions, and the transitive `containsBlock` closure the
/// polarity test reads. Built once per import, before lowering.
#[derive(Clone, Debug, Default)]
pub(super) struct CompositeOrientation {
    pub(super) composites: HashSet<String>,
    pub(super) inputs: HashSet<String>,
    pub(super) outputs: HashSet<String>,
    pub(super) top_inputs: HashSet<String>,
    pub(super) top_outputs: HashSet<String>,
    root: Option<NodeIx>,
    composite_nodes: HashSet<NodeIx>,
    owners: HashMap<String, Option<(NodeIx, bool)>>,
    roles: HashMap<String, Option<Role>>,
    parents: HashMap<String, NodeIx>,
    node_parents: HashMap<NodeIx, NodeIx>,
    synthesized: HashSet<String>,
    synthesized_order: Vec<String>,
    tree: bool,
    positions: HashMap<String, usize>,
}

impl CompositeOrientation {
    /// Index active port ownership, composite boundaries, containment, and graph order.
    pub(super) fn new(
        doc: &CxfDocument,
        by_id: &HashMap<&str, &Node>,
        root: &str,
        specialization: &Specialization,
    ) -> Self {
        let mut index = Self {
            tree: true,
            ..Self::default()
        };
        for (position, node) in doc.graph.iter().enumerate() {
            index.positions.insert(node.id.clone(), position);
        }
        index.root = index.positions.get(root).copied();
        let contains_referents = contains_block_referents(doc);
        let reachable_members = reachable_containment_members(by_id, root, specialization);
        for node in &doc.graph {
            if specialization.is_inactive(&node.id) {
                continue;
            }
            if node.id != root && !reachable_members.contains(node.id.as_str()) {
                continue;
            }
            let Some(&owner) = index.positions.get(node.id.as_str()) else {
                continue;
            };
            let composite = is_runtime_composite(node);
            if composite {
                index.composites.insert(node.id.clone());
                index.composite_nodes.insert(owner);
            }
            for input in node.has_input.iter().map(|port| port.id.as_str()) {
                index.claim(input, owner, true);
                if composite {
                    index.inputs.insert(input.to_owned());
                    if node.id == root {
                        index.top_inputs.insert(input.to_owned());
                    }
                }
            }
            for output in node.has_output.iter().map(|port| port.id.as_str()) {
                index.claim(output, owner, false);
                if composite {
                    index.outputs.insert(output.to_owned());
                    if node.id == root {
                        index.top_outputs.insert(output.to_owned());
                    }
                }
            }
            // Listed and padded members claim the same leaf ownership used by later derivation.
            if is_derivation_shaped(node, &contains_referents) {
                for (member, input) in orientation_port_members(node) {
                    if !by_id.contains_key(member.as_str())
                        && index.synthesized.insert(member.clone())
                    {
                        index.synthesized_order.push(member.clone());
                    }
                    index.claim(&member, owner, input);
                }
            }
            if composite {
                for child in node.contains_block.iter().map(|child| child.id.as_str()) {
                    if specialization.is_inactive(child) {
                        continue;
                    }
                    if let Some(previous) = index.parents.insert(child.to_owned(), owner)
                        && previous != owner
                    {
                        index.tree = false;
                    }
                    if by_id
                        .get(child)
                        .is_some_and(|child_node| is_runtime_composite(child_node))
                    {
                        index.composites.insert(child.to_owned());
                        if let Some(&child_ix) = index.positions.get(child) {
                            index.composite_nodes.insert(child_ix);
                        }
                    }
                }
            }
        }
        // Only a node can be a location or a parent, so the containment walk runs over node
        // indices; a non-node child is never a parent and so never on a cycle.
        #[expect(
            clippy::iter_over_hash_type,
            reason = "builds the `node_parents` lookup map, one insert per distinct child; visit order is unobservable"
        )]
        for (child, parent) in &index.parents {
            if let Some(&child_ix) = index.positions.get(child.as_str()) {
                index.node_parents.insert(child_ix, *parent);
            }
        }
        index.tree &= !index.has_containment_cycle();
        #[expect(
            clippy::iter_over_hash_type,
            reason = "builds the `roles` lookup map, one insert per distinct port; visit order is unobservable"
        )]
        for (port, owner) in &index.owners {
            let role = owner.map(|(owner, input)| {
                if index.composite_nodes.contains(&owner) {
                    if input {
                        Role::BoundaryInput(owner)
                    } else {
                        Role::BoundaryOutput(owner)
                    }
                } else if input {
                    Role::LeafInput
                } else {
                    Role::LeafOutput
                }
            });
            index.roles.insert(port.clone(), role);
        }
        index
    }

    fn claim(&mut self, port: &str, owner: NodeIx, input: bool) {
        match self.owners.get_mut(port) {
            None => {
                self.owners.insert(port.to_owned(), Some((owner, input)));
            }
            Some(claim) => {
                if *claim != Some((owner, input)) {
                    *claim = None;
                }
            }
        }
    }

    fn has_containment_cycle(&self) -> bool {
        self.node_parents.keys().any(|&start| {
            let mut seen = HashSet::new();
            let mut cursor = start;
            while let Some(&parent) = self.node_parents.get(&cursor) {
                if !seen.insert(cursor) {
                    return true;
                }
                cursor = parent;
            }
            false
        })
    }

    /// Produce adjacency in authored `@graph` order with boundary-facing edges canonicalized,
    /// suppressing forward+reverse restatements of one relation (a repeated canonical pair where
    /// either sighting swapped) while keeping authored duplicate copies. Also records the
    /// canonical drivers whose edges touch a non-root boundary — the drivers whose lowered lists
    /// rule Gx orders by target `@graph` position.
    ///
    /// Each node's `@id` is interned once and each authored target once per edge; everything
    /// else is keyed on [`EndpointIx`], so the pass is linear in the edge list's bytes.
    pub(super) fn canonical_connections<'a>(
        &self,
        doc: &'a CxfDocument,
        by_id: &HashMap<&str, &Node>,
        specialization: &Specialization,
    ) -> CanonicalConnections<'a> {
        let mut endpoints = LoweringEndpoints::default();
        let mut authored = Vec::new();
        let mut crossed = HashSet::new();
        let mut records = Vec::new();
        let mut counts: HashMap<(EndpointIx, EndpointIx), (usize, usize)> = HashMap::new();
        for node in &doc.graph {
            if node.is_connected_to.is_empty() {
                continue;
            }
            let authored_source = endpoints.intern(&node.id, self, by_id, specialization);
            let source_crosses = self.is_non_root_boundary(endpoints.get(authored_source));
            for authored_target in node.is_connected_to.iter().map(|target| target.id.as_str()) {
                let authored_target =
                    endpoints.intern(authored_target, self, by_id, specialization);
                authored.push((authored_source, authored_target));
                let (source, target, verdict) =
                    self.canonical_pair(&endpoints, authored_source, authored_target);
                if source_crosses || self.is_non_root_boundary(endpoints.get(authored_target)) {
                    crossed.insert(source);
                }
                let count = counts.entry((source, target)).or_default();
                if verdict == Verdict::Swap {
                    count.1 += 1;
                } else {
                    count.0 += 1;
                }
                records.push((source, target));
            }
        }
        let mut adjacency = vec![Vec::new(); endpoints.len()];
        let mut emitted: HashMap<(EndpointIx, EndpointIx), usize> = HashMap::new();
        for (source, target) in records {
            let pair = (source, target);
            let desired = counts
                .get(&pair)
                .map(|(authored, swapped)| (*authored).max(*swapped))
                .unwrap_or(0);
            let emitted = emitted.entry(pair).or_default();
            if *emitted < desired {
                adjacency[source.0].push(target);
                *emitted += 1;
            }
        }
        CanonicalConnections::new(endpoints, authored, adjacency, crossed)
    }

    fn flat_polarity(&self, port: &Endpoint<'_>) -> Option<Polarity> {
        match port.role?? {
            Role::LeafOutput => Some(Polarity::Source),
            Role::LeafInput => Some(Polarity::Sink),
            Role::BoundaryInput(owner) if Some(owner) == self.root => Some(Polarity::Source),
            Role::BoundaryOutput(owner) if Some(owner) == self.root => Some(Polarity::Sink),
            Role::BoundaryInput(_) | Role::BoundaryOutput(_) => None,
        }
    }

    fn canonical_pair(
        &self,
        endpoints: &LoweringEndpoints<'_>,
        source: EndpointIx,
        target: EndpointIx,
    ) -> (EndpointIx, EndpointIx, Verdict) {
        let (source_port, target_port) = (endpoints.get(source), endpoints.get(target));
        if source_port.inactive || target_port.inactive {
            return (source, target, Verdict::Untouched);
        }
        if !source_port.boundary && !target_port.boundary {
            return (source, target, Verdict::Untouched);
        }
        let Some(source_polarity) = self.polarity(source_port, target_port) else {
            return (source, target, Verdict::Unknown);
        };
        let Some(target_polarity) = self.polarity(target_port, source_port) else {
            return (source, target, Verdict::Unknown);
        };
        match (source_polarity, target_polarity) {
            (Polarity::Source, Polarity::Sink) => (source, target, Verdict::Keep),
            (Polarity::Sink, Polarity::Source) if target_port.node || target_port.synthesized => {
                (target, source, Verdict::Swap)
            }
            (Polarity::Sink, Polarity::Source) => (source, target, Verdict::SwapBlocked),
            _ => (source, target, Verdict::Contradictory),
        }
    }

    fn polarity(&self, port: &Endpoint<'_>, peer: &Endpoint<'_>) -> Option<Polarity> {
        let role = port.role??;
        let peer_location = peer.location?;
        match role {
            Role::LeafOutput => Some(Polarity::Source),
            Role::LeafInput => Some(Polarity::Sink),
            Role::BoundaryInput(composite) if Some(composite) == self.root => {
                Some(Polarity::Source)
            }
            Role::BoundaryOutput(composite) if Some(composite) == self.root => Some(Polarity::Sink),
            Role::BoundaryInput(composite) => Some(
                if peer_location == composite || self.inside(peer_location, composite)? {
                    Polarity::Source
                } else {
                    Polarity::Sink
                },
            ),
            Role::BoundaryOutput(composite) => Some(
                if peer_location == composite || self.inside(peer_location, composite)? {
                    Polarity::Sink
                } else {
                    Polarity::Source
                },
            ),
        }
    }

    /// The node an endpoint sits in: its claimed owner, or the endpoint itself when it is a
    /// node no port claim names.
    fn location(&self, iri: &str) -> Option<NodeIx> {
        match self.owners.get(iri) {
            Some(Some((owner, _))) => Some(*owner),
            Some(None) => None,
            None => self.positions.get(iri).copied(),
        }
    }

    fn inside(&self, item: NodeIx, composite: NodeIx) -> Option<bool> {
        if !self.tree {
            return None;
        }
        let mut cursor = item;
        while let Some(&parent) = self.node_parents.get(&cursor) {
            if parent == composite {
                return Some(true);
            }
            cursor = parent;
        }
        Some(false)
    }

    fn is_non_root_boundary(&self, port: &Endpoint<'_>) -> bool {
        matches!(
            port.role,
            Some(Some(Role::BoundaryInput(owner) | Role::BoundaryOutput(owner)))
                if Some(owner) != self.root
        )
    }

    fn is_boundary(&self, iri: &str) -> bool {
        self.inputs.contains(iri) || self.outputs.contains(iri)
    }

    fn is_elided_boundary_source(&self, iri: &str) -> bool {
        (self.inputs.contains(iri) && !self.top_inputs.contains(iri))
            || (self.outputs.contains(iri) && !self.top_outputs.contains(iri))
    }

    /// Node-less derived connector sources in owner and class-signature order.
    pub(super) fn synthesized_sources(&self) -> impl Iterator<Item = &str> {
        self.synthesized_order.iter().map(String::as_str)
    }

    /// Whether `iri` is a node-less output identity synthesized from an instance interface.
    pub(super) fn is_synthesized_source(&self, iri: &str) -> bool {
        self.synthesized.contains(iri)
    }

    /// Original `@graph` position of a node, with unknown targets ordered last.
    pub(super) fn position(&self, iri: &str) -> usize {
        self.positions.get(iri).copied().unwrap_or(usize::MAX)
    }
}

fn reachable_containment_members<'a>(
    by_id: &HashMap<&'a str, &'a Node>,
    root: &'a str,
    specialization: &Specialization,
) -> HashSet<&'a str> {
    let mut reachable = HashSet::new();
    let mut pending = vec![root];
    while let Some(parent) = pending.pop() {
        let Some(node) = by_id.get(parent) else {
            continue;
        };
        for child in node.contains_block.iter().map(|child| child.id.as_str()) {
            if specialization.is_inactive(child) || !reachable.insert(child) {
                continue;
            }
            if by_id
                .get(child)
                .is_some_and(|node| is_runtime_composite(node))
            {
                pending.push(child);
            }
        }
    }
    reachable
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn adjacency_in_order(
        forward: &[&str],
        reverse: &[&str],
        reverse_first: bool,
    ) -> HashMap<String, Vec<String>> {
        let mut value = serde_json::json!({
            "@context": { "S231": "http://data.ashrae.org/S231P#" },
            "@graph": [
                { "@id": "root", "@type": "S231:Block",
                  "S231:containsBlock": { "@id": "sub" } },
                { "@id": "sub", "@type": "S231:Block",
                  "S231:containsBlock": { "@id": "leaf" },
                  "S231:hasInput": { "@id": "sub.u" } },
                { "@id": "leaf",
                  "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.MultiplyByParameter",
                  "S231:hasInput": { "@id": "leaf.u" } },
                { "@id": "sub.u", "@type": "S231:RealInput" },
                { "@id": "leaf.u", "@type": "S231:RealInput" }
            ]
        });
        let targets = |ids: &[&str]| {
            Value::Array(
                ids.iter()
                    .map(|id| serde_json::json!({ "@id": id }))
                    .collect(),
            )
        };
        value["@graph"][3]["S231:isConnectedTo"] = targets(forward);
        value["@graph"][4]["S231:isConnectedTo"] = targets(reverse);
        if reverse_first {
            value["@graph"].as_array_mut().expect("@graph").swap(3, 4);
        }
        let doc: CxfDocument = serde_json::from_value(value).expect("document");
        let by_id: HashMap<&str, &Node> = doc
            .graph
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect();
        let specialization = Specialization::default();
        let index = CompositeOrientation::new(&doc, &by_id, "root", &specialization);
        index
            .canonical_connections(&doc, &by_id, &specialization)
            .by_iri()
            .into_iter()
            .map(|(source, targets)| {
                (
                    source.to_owned(),
                    targets.into_iter().map(str::to_owned).collect(),
                )
            })
            .collect()
    }

    fn adjacency(forward: &[&str], reverse: &[&str]) -> HashMap<String, Vec<String>> {
        adjacency_in_order(forward, reverse, false)
    }

    fn pair_count(adjacency: &HashMap<String, Vec<String>>) -> usize {
        adjacency
            .get("sub.u")
            .into_iter()
            .flatten()
            .filter(|target| target.as_str() == "leaf.u")
            .count()
    }

    #[test]
    fn two_forward_spellings_are_both_preserved() {
        assert_eq!(pair_count(&adjacency(&["leaf.u", "leaf.u"], &[])), 2);
    }

    #[test]
    fn two_reverse_spellings_are_both_preserved() {
        assert_eq!(pair_count(&adjacency(&[], &["sub.u", "sub.u"])), 2);
    }

    #[test]
    fn forward_then_reverse_collapses_to_one() {
        assert_eq!(pair_count(&adjacency(&["leaf.u"], &["sub.u"])), 1);
    }

    #[test]
    fn two_forward_then_reverse_preserves_the_forward_pair() {
        assert_eq!(pair_count(&adjacency(&["leaf.u", "leaf.u"], &["sub.u"])), 2);
    }

    #[test]
    fn one_forward_then_two_reverse_preserves_two_relations() {
        assert_eq!(pair_count(&adjacency(&["leaf.u"], &["sub.u", "sub.u"])), 2);
    }

    #[test]
    fn one_reverse_then_two_forward_preserves_two_relations() {
        assert_eq!(
            pair_count(&adjacency_in_order(&["leaf.u", "leaf.u"], &["sub.u"], true,)),
            2
        );
    }

    #[test]
    fn authored_and_reanchored_targets_follow_authoring_node_order() {
        let value = serde_json::json!({
            "@context": { "S231": "http://data.ashrae.org/S231P#" },
            "@graph": [
                { "@id": "root", "@type": "S231:Block",
                  "S231:containsBlock": { "@id": "sub" } },
                { "@id": "sub", "@type": "S231:Block",
                  "S231:containsBlock": [ { "@id": "first" }, { "@id": "second" } ],
                  "S231:hasInput": { "@id": "sub.u" } },
                { "@id": "first",
                  "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.MultiplyByParameter",
                  "S231:hasInput": { "@id": "first.u" } },
                { "@id": "sub.u", "@type": "S231:RealInput",
                  "S231:isConnectedTo": { "@id": "first.u" } },
                { "@id": "first.u", "@type": "S231:RealInput" },
                { "@id": "second",
                  "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.MultiplyByParameter",
                  "S231:hasInput": { "@id": "second.u" } },
                { "@id": "second.u", "@type": "S231:RealInput",
                  "S231:isConnectedTo": { "@id": "sub.u" } }
            ]
        });
        let doc: CxfDocument = serde_json::from_value(value).expect("document");
        let by_id: HashMap<&str, &Node> = doc
            .graph
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect();
        let specialization = Specialization::default();
        let index = CompositeOrientation::new(&doc, &by_id, "root", &specialization);
        let adjacency = index
            .canonical_connections(&doc, &by_id, &specialization)
            .by_iri();
        assert_eq!(adjacency.get("sub.u"), Some(&vec!["first.u", "second.u"]));
    }

    #[test]
    fn canonicalizing_canonical_adjacency_is_idempotent() {
        let mut value = serde_json::json!({
            "@context": { "S231": "http://data.ashrae.org/S231P#" },
            "@graph": [
                { "@id": "root", "@type": "S231:Block",
                  "S231:containsBlock": { "@id": "sub" } },
                { "@id": "sub", "@type": "S231:Block",
                  "S231:containsBlock": { "@id": "leaf" },
                  "S231:hasInput": { "@id": "sub.u" } },
                { "@id": "leaf",
                  "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.MultiplyByParameter",
                  "S231:hasInput": { "@id": "leaf.u" } },
                { "@id": "sub.u", "@type": "S231:RealInput" },
                { "@id": "leaf.u", "@type": "S231:RealInput",
                  "S231:isConnectedTo": { "@id": "sub.u" } }
            ]
        });
        let first_doc: CxfDocument = serde_json::from_value(value.clone()).expect("document");
        let first_by_id: HashMap<&str, &Node> = first_doc
            .graph
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect();
        let specialization = Specialization::default();
        let first_index =
            CompositeOrientation::new(&first_doc, &first_by_id, "root", &specialization);
        let first = first_index
            .canonical_connections(&first_doc, &first_by_id, &specialization)
            .by_iri();
        for node in value["@graph"].as_array_mut().expect("@graph") {
            let id = node["@id"].as_str().expect("@id").to_owned();
            node.as_object_mut()
                .expect("node")
                .remove("S231:isConnectedTo");
            if let Some(targets) = first.get(id.as_str()) {
                node["S231:isConnectedTo"] = Value::Array(
                    targets
                        .iter()
                        .map(|target| serde_json::json!({ "@id": target }))
                        .collect(),
                );
            }
        }
        let second_doc: CxfDocument = serde_json::from_value(value).expect("canonical document");
        let second_by_id: HashMap<&str, &Node> = second_doc
            .graph
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect();
        let second_index =
            CompositeOrientation::new(&second_doc, &second_by_id, "root", &specialization);
        let second = second_index
            .canonical_connections(&second_doc, &second_by_id, &specialization)
            .by_iri();
        assert_eq!(second, first);
    }
}
