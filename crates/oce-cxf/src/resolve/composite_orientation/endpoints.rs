//! Interned connection endpoints for composite-boundary lowering.
//!
//! Lowering classifies both ends of every authored `isConnectedTo` edge (inactivity, boundary
//! membership, port role, location, `@graph` position) and keys its duplicate counts, crossed
//! drivers and adjacency on the canonical pair. Asking those questions of the IRI string hashes
//! it once per lookup, and one authored source node is shared by its whole fan-out, so a 1 MiB
//! source wired to 65,535 targets cost fan-out × source length: more than three minutes of
//! release CPU before any refusal (GitHub issue #480).
//!
//! [`LoweringEndpoints`] classifies each distinct IRI once and hands lowering a dense
//! [`EndpointIx`]. Each authored source is interned once per node and each authored target once
//! per edge, so the per-edge work is linear in the bytes of the edge list. Ids are assigned in
//! first-seen authored order and are used only as vector indices and set or map keys; no hash
//! iteration order reaches a diagnostic, the canonical adjacency or a lowered edge.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::dto::Node;

use super::super::specialize::Specialization;
use super::{CompositeOrientation, NodeIx, Role};

/// Dense identity of one distinct endpoint IRI within one document's lowering.
///
/// Two endpoints have the same id exactly when their IRIs are byte-equal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(in crate::resolve) struct EndpointIx(pub(in crate::resolve) usize);

/// Everything lowering asks about one endpoint IRI, computed once per distinct IRI.
#[derive(Debug)]
pub(in crate::resolve) struct Endpoint<'a> {
    /// The endpoint IRI as authored.
    pub(in crate::resolve) iri: &'a str,
    /// Pruned by a false load-time conditional guard.
    pub(in crate::resolve) inactive: bool,
    /// A boundary port of any active runtime composite, the top one included.
    pub(super) boundary: bool,
    /// A boundary port of a non-top composite: lowering erases it and re-anchors its edges.
    pub(in crate::resolve) elided: bool,
    /// The port role, `Some(None)` for a conflicted ownership claim, `None` for no claim.
    pub(super) role: Option<Option<Role>>,
    /// The node the endpoint sits in (see `CompositeOrientation::location`).
    pub(super) location: Option<NodeIx>,
    /// An `@graph` node carries this `@id`.
    pub(in crate::resolve) node: bool,
    /// A node-less connector identity synthesized from an instance interface.
    pub(in crate::resolve) synthesized: bool,
    /// Original `@graph` position, `usize::MAX` for a non-node.
    pub(in crate::resolve) position: usize,
}

/// Interning table from endpoint IRI to [`EndpointIx`] and its classification.
#[derive(Debug, Default)]
pub(in crate::resolve) struct LoweringEndpoints<'a> {
    ids: HashMap<&'a str, EndpointIx>,
    endpoints: Vec<Endpoint<'a>>,
}

impl<'a> LoweringEndpoints<'a> {
    /// Intern an endpoint IRI, classifying it against `orientation` on first sight.
    pub(in crate::resolve) fn intern(
        &mut self,
        iri: &'a str,
        orientation: &CompositeOrientation,
        by_id: &HashMap<&str, &Node>,
        specialization: &Specialization,
    ) -> EndpointIx {
        if let Some(&id) = self.ids.get(iri) {
            return id;
        }
        // A `usize` index cannot overflow: every id is a distinct entry already held in memory.
        let id = EndpointIx(self.endpoints.len());
        self.endpoints.push(Endpoint {
            iri,
            inactive: specialization.is_inactive(iri),
            boundary: orientation.is_boundary(iri),
            elided: orientation.is_elided_boundary_source(iri),
            role: orientation.roles.get(iri).copied(),
            location: orientation.location(iri),
            node: by_id.contains_key(iri),
            synthesized: orientation.is_synthesized_source(iri),
            position: orientation.position(iri),
        });
        self.ids.insert(iri, id);
        id
    }

    /// The id of an IRI some authored edge names, if any does.
    pub(in crate::resolve) fn lookup(&self, iri: &str) -> Option<EndpointIx> {
        self.ids.get(iri).copied()
    }

    /// The classification of an interned endpoint.
    pub(in crate::resolve) fn get(&self, id: EndpointIx) -> &Endpoint<'a> {
        &self.endpoints[id.0]
    }

    /// The number of distinct endpoints interned.
    pub(super) fn len(&self) -> usize {
        self.endpoints.len()
    }
}

/// The canonical adjacency of one document, over interned endpoints.
#[derive(Debug)]
pub(in crate::resolve) struct CanonicalConnections<'a> {
    /// The endpoint table every id below indexes.
    pub(in crate::resolve) endpoints: LoweringEndpoints<'a>,
    /// Every authored edge as `(authored source, authored target)`, in authored order.
    authored: Vec<(EndpointIx, EndpointIx)>,
    /// Canonical targets per canonical source, indexed by [`EndpointIx`].
    adjacency: Vec<Vec<EndpointIx>>,
    /// Canonical drivers with an edge touching a non-root boundary.
    crossed: HashSet<EndpointIx>,
}

impl<'a> CanonicalConnections<'a> {
    /// Assemble the canonical connections from their parts; `adjacency` is indexed by id.
    pub(in crate::resolve) fn new(
        endpoints: LoweringEndpoints<'a>,
        authored: Vec<(EndpointIx, EndpointIx)>,
        adjacency: Vec<Vec<EndpointIx>>,
        crossed: HashSet<EndpointIx>,
    ) -> Self {
        Self {
            endpoints,
            authored,
            adjacency,
            crossed,
        }
    }

    /// The canonical targets of `source`, in canonical order; empty when it drives nothing.
    pub(in crate::resolve) fn targets(&self, source: EndpointIx) -> &[EndpointIx] {
        self.adjacency.get(source.0).map_or(&[], Vec::as_slice)
    }

    /// Whether `source` drives an edge that touches a non-root boundary.
    pub(in crate::resolve) fn is_crossed(&self, source: EndpointIx) -> bool {
        self.crossed.contains(&source)
    }

    /// The authored edges, in authored order.
    pub(super) fn authored(&self) -> &[(EndpointIx, EndpointIx)] {
        &self.authored
    }

    /// The canonical adjacency keyed by IRI, for tests that compare it with an expected map.
    #[cfg(test)]
    pub(in crate::resolve) fn by_iri(&self) -> HashMap<&'a str, Vec<&'a str>> {
        self.adjacency
            .iter()
            .enumerate()
            .filter(|(_, targets)| !targets.is_empty())
            .map(|(source, targets)| {
                (
                    self.endpoints.endpoints[source].iri,
                    targets
                        .iter()
                        .map(|target| self.endpoints.get(*target).iri)
                        .collect(),
                )
            })
            .collect()
    }
}

/// One shared allocation per distinct endpoint IRI, minted on first use, so a lowered edge list
/// that names one long IRI many times (a swapped fan-out) holds it once.
pub(in crate::resolve) struct SharedIris<'e, 'a> {
    endpoints: &'e LoweringEndpoints<'a>,
    shared: Vec<Option<Arc<str>>>,
}

impl<'e, 'a> SharedIris<'e, 'a> {
    /// An empty set of shared IRIs over `endpoints`.
    pub(in crate::resolve) fn new(endpoints: &'e LoweringEndpoints<'a>) -> Self {
        Self {
            endpoints,
            shared: vec![None; endpoints.len()],
        }
    }

    /// The shared allocation of `id`'s IRI.
    pub(in crate::resolve) fn get(&mut self, id: EndpointIx) -> Arc<str> {
        let endpoints = self.endpoints;
        Arc::clone(self.shared[id.0].get_or_insert_with(|| Arc::from(endpoints.get(id).iri)))
    }
}

#[cfg(test)]
#[path = "endpoints_tests.rs"]
mod tests;
