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
//! - Duplicate suppression: forward+reverse restatements of ONE relation collapse (a canonical
//!   pair repeats and either sighting swapped); two authored copies of the same edge are NOT
//!   collapsed — a genuine double-drive stays visible to the single-assignment check.

use std::collections::{HashMap, HashSet};

use crate::dto::{CxfDocument, Node};
use oce_diag::{DiagCode, Diagnostic};

use super::composite::is_runtime_composite;
use super::instance_interface::{
    contains_block_referents, is_derivation_shaped, orientation_port_members,
};
use super::specialize::Specialization;

#[derive(Clone, Debug)]
enum Role {
    BoundaryInput(String),
    BoundaryOutput(String),
    LeafInput,
    LeafOutput,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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

type CanonicalConnections<'a> = (HashMap<&'a str, Vec<&'a str>>, HashSet<&'a str>);

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
    owners: HashMap<String, Option<(String, bool)>>,
    roles: HashMap<String, Option<Role>>,
    parents: HashMap<String, String>,
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
        let contains_referents = contains_block_referents(doc);
        for (position, node) in doc.graph.iter().enumerate() {
            index.positions.insert(node.id.clone(), position);
            if specialization.is_inactive(&node.id) {
                continue;
            }
            let composite = is_runtime_composite(node);
            if composite {
                index.composites.insert(node.id.clone());
            }
            for input in node.has_input.iter().map(|port| port.id.as_str()) {
                index.claim(input, &node.id, true);
                if composite {
                    index.inputs.insert(input.to_owned());
                    if node.id == root {
                        index.top_inputs.insert(input.to_owned());
                    }
                }
            }
            for output in node.has_output.iter().map(|port| port.id.as_str()) {
                index.claim(output, &node.id, false);
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
                    index.claim(&member, &node.id, input);
                }
            }
            for child in node.contains_block.iter().map(|child| child.id.as_str()) {
                if specialization.is_inactive(child) {
                    continue;
                }
                if let Some(previous) = index.parents.insert(child.to_owned(), node.id.clone())
                    && previous != node.id
                {
                    index.tree = false;
                }
                if by_id
                    .get(child)
                    .is_some_and(|child_node| is_runtime_composite(child_node))
                {
                    index.composites.insert(child.to_owned());
                }
            }
        }
        index.tree &= !index.has_containment_cycle();
        for (port, owner) in &index.owners {
            let role = owner.as_ref().map(|(owner, input)| {
                if index.composites.contains(owner) {
                    if *input {
                        Role::BoundaryInput(owner.clone())
                    } else {
                        Role::BoundaryOutput(owner.clone())
                    }
                } else if *input {
                    Role::LeafInput
                } else {
                    Role::LeafOutput
                }
            });
            index.roles.insert(port.clone(), role);
        }
        index
    }

    fn claim(&mut self, port: &str, owner: &str, input: bool) {
        match self.owners.entry(port.to_owned()) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(Some((owner.to_owned(), input)));
            }
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                if entry.get().as_ref() != Some(&(owner.to_owned(), input)) {
                    entry.insert(None);
                }
            }
        }
    }

    fn has_containment_cycle(&self) -> bool {
        self.parents.keys().any(|start| {
            let mut seen = HashSet::new();
            let mut cursor = start.as_str();
            while let Some(parent) = self.parents.get(cursor) {
                if !seen.insert(cursor.to_owned()) {
                    return true;
                }
                cursor = parent;
            }
            false
        })
    }

    /// Produce adjacency in authored `@graph` order with boundary-facing edges canonicalized,
    /// suppressing forward+reverse restatements of one relation (a repeated canonical pair where
    /// either sighting swapped) while keeping authored duplicate copies. Also returns the set of
    /// canonical drivers whose edges touch a non-root boundary — the drivers whose lowered lists
    /// rule Gx orders by target `@graph` position.
    ///
    pub(super) fn canonical_connections<'a>(
        &self,
        doc: &'a CxfDocument,
        by_id: &HashMap<&str, &Node>,
        root: &str,
        specialization: &Specialization,
    ) -> CanonicalConnections<'a> {
        let mut out: HashMap<&str, Vec<&str>> = HashMap::new();
        let mut crossed = HashSet::new();
        let mut stored: HashMap<(&str, &str), Verdict> = HashMap::new();
        for node in &doc.graph {
            for authored_target in node.is_connected_to.iter().map(|target| target.id.as_str()) {
                let (source, target, verdict) =
                    self.canonical_pair(&node.id, authored_target, by_id, root, specialization);
                if self.is_non_root_boundary(&node.id, root)
                    || self.is_non_root_boundary(authored_target, root)
                {
                    crossed.insert(source);
                }
                let pair = (source, target);
                if stored
                    .get(&pair)
                    .is_some_and(|previous| verdict == Verdict::Swap || *previous == Verdict::Swap)
                {
                    continue;
                }
                out.entry(source).or_default().push(target);
                stored.insert(pair, verdict);
            }
        }
        (out, crossed)
    }

    /// First active relation that lowering erased without a later generic diagnostic.
    pub(super) fn erased_relation_diagnostic(
        &self,
        doc: &CxfDocument,
        by_id: &HashMap<&str, &Node>,
        root: &str,
        specialization: &Specialization,
        reached_boundaries: &HashSet<&str>,
    ) -> Option<Diagnostic> {
        for node in &doc.graph {
            if specialization.is_inactive(&node.id) {
                continue;
            }
            for authored_target in node.is_connected_to.iter().map(|target| target.id.as_str()) {
                let source_is_erased = self.is_elided_boundary_source(&node.id);
                let target_is_inactive = specialization.is_inactive(authored_target);
                let relation_is_erased = source_is_erased
                    || (!target_is_inactive
                        && self.is_elided_boundary_source(authored_target)
                        && by_id.contains_key(authored_target));
                if !relation_is_erased {
                    continue;
                }
                if target_is_inactive {
                    if source_is_erased && !reached_boundaries.contains(node.id.as_str()) {
                        return Some(Diagnostic::error(
                            DiagCode::InactiveConditionalNode,
                            "connection targets an inactive conditional node",
                        ));
                    }
                    continue;
                }
                let (source, target, verdict) =
                    self.canonical_pair(&node.id, authored_target, by_id, root, specialization);
                if matches!(verdict, Verdict::Keep | Verdict::Swap | Verdict::Untouched)
                    && source_is_erased
                    && !reached_boundaries.contains(node.id.as_str())
                {
                    if !by_id.contains_key(source) && !self.synthesized.contains(source) {
                        return Some(Diagnostic::error(
                            DiagCode::UnresolvedReference,
                            "connection source not found",
                        ));
                    }
                    if !by_id.contains_key(target) && !self.synthesized.contains(target) {
                        return Some(Diagnostic::error(
                            DiagCode::UnresolvedReference,
                            "connection target not found",
                        ));
                    }
                }
                match verdict {
                    Verdict::Contradictory => {
                        return Some(Diagnostic::error(
                            DiagCode::DirectionMismatch,
                            "boundary connection has contradictory endpoint directions",
                        ));
                    }
                    Verdict::SwapBlocked | Verdict::Unknown => {
                        return Some(Diagnostic::error(
                            DiagCode::DirectionMismatch,
                            "boundary connection direction cannot be derived",
                        ));
                    }
                    Verdict::Keep | Verdict::Swap | Verdict::Untouched => {}
                }
            }
        }
        None
    }

    fn canonical_pair<'a>(
        &self,
        source: &'a str,
        target: &'a str,
        by_id: &HashMap<&str, &Node>,
        root: &str,
        specialization: &Specialization,
    ) -> (&'a str, &'a str, Verdict) {
        if specialization.is_inactive(source) || specialization.is_inactive(target) {
            return authored(source, target, Verdict::Untouched);
        }
        let scoped =
            self.is_non_root_boundary(source, root) || self.is_non_root_boundary(target, root);
        if !scoped {
            return authored(source, target, Verdict::Untouched);
        }
        let Some(source_polarity) = self.polarity(source, target, root) else {
            return authored(source, target, Verdict::Unknown);
        };
        let Some(target_polarity) = self.polarity(target, source, root) else {
            return authored(source, target, Verdict::Unknown);
        };
        match (source_polarity, target_polarity) {
            (Polarity::Source, Polarity::Sink) => authored(source, target, Verdict::Keep),
            (Polarity::Sink, Polarity::Source)
                if by_id.contains_key(target) || self.synthesized.contains(target) =>
            {
                (target, source, Verdict::Swap)
            }
            (Polarity::Sink, Polarity::Source) => authored(source, target, Verdict::SwapBlocked),
            _ => authored(source, target, Verdict::Contradictory),
        }
    }

    fn polarity(&self, port: &str, peer: &str, root: &str) -> Option<Polarity> {
        let role = self.roles.get(port)?.as_ref()?;
        let peer_location = self.location(peer)?;
        match role {
            Role::LeafOutput => Some(Polarity::Source),
            Role::LeafInput => Some(Polarity::Sink),
            Role::BoundaryInput(composite) if composite == root => Some(Polarity::Source),
            Role::BoundaryOutput(composite) if composite == root => Some(Polarity::Sink),
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

    fn location<'a>(&'a self, iri: &'a str) -> Option<&'a str> {
        match self.owners.get(iri) {
            Some(Some((owner, _))) => Some(owner),
            Some(None) => None,
            None if self.positions.contains_key(iri) => Some(iri),
            None => None,
        }
    }

    fn inside(&self, item: &str, composite: &str) -> Option<bool> {
        if !self.tree {
            return None;
        }
        let mut cursor = item;
        while let Some(parent) = self.parents.get(cursor) {
            if parent == composite {
                return Some(true);
            }
            cursor = parent;
        }
        Some(false)
    }

    fn is_non_root_boundary(&self, iri: &str, root: &str) -> bool {
        self.roles.get(iri).is_some_and(|role| {
            matches!(
                role,
                Some(Role::BoundaryInput(owner) | Role::BoundaryOutput(owner)) if owner != root
            )
        })
    }

    fn is_elided_boundary_source(&self, iri: &str) -> bool {
        (self.inputs.contains(iri) && !self.top_inputs.contains(iri))
            || (self.outputs.contains(iri) && !self.top_outputs.contains(iri))
    }

    /// Node-less derived connector sources in owner and class-signature order.
    pub(super) fn synthesized_sources(&self) -> impl Iterator<Item = &str> {
        self.synthesized_order.iter().map(String::as_str)
    }

    /// Original `@graph` position of a node, with unknown targets ordered last.
    pub(super) fn position(&self, iri: &str) -> usize {
        self.positions.get(iri).copied().unwrap_or(usize::MAX)
    }
}

fn authored<'a>(source: &'a str, target: &'a str, verdict: Verdict) -> (&'a str, &'a str, Verdict) {
    (source, target, verdict)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn adjacency(forward: &[&str], reverse: &[&str]) -> HashMap<String, Vec<String>> {
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
        let doc: CxfDocument = serde_json::from_value(value).expect("document");
        let by_id: HashMap<&str, &Node> = doc
            .graph
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect();
        let specialization = Specialization::default();
        let index = CompositeOrientation::new(&doc, &by_id, "root", &specialization);
        index
            .canonical_connections(&doc, &by_id, "root", &specialization)
            .0
            .into_iter()
            .map(|(source, targets)| {
                (
                    source.to_owned(),
                    targets.into_iter().map(str::to_owned).collect(),
                )
            })
            .collect()
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
    fn two_reverse_spellings_collapse_to_one() {
        assert_eq!(pair_count(&adjacency(&[], &["sub.u", "sub.u"])), 1);
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
        let (adjacency, _) = index.canonical_connections(&doc, &by_id, "root", &specialization);
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
        let (first, _) =
            first_index.canonical_connections(&first_doc, &first_by_id, "root", &specialization);
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
        let (second, _) =
            second_index.canonical_connections(&second_doc, &second_by_id, "root", &specialization);
        assert_eq!(second, first);
    }
}
