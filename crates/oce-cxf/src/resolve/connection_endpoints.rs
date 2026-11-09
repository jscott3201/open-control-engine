//! Interned connection endpoints for resolve Step 9.
//!
//! Step 9 classifies both ends of every `isConnectedTo` edge against the resolver's lookup
//! tables: specialization inactivity, the top composite's boundary inputs and outputs, the
//! connector map, the `@id` index and the derived boundary types. Classifying by string content
//! hashes the endpoint IRI once per lookup. A re-anchored synthesized source (or any authored
//! source node) is one string shared by every edge of its fan-out, so per-edge classification
//! costs fan-out × source length: a 1 MiB source with 65,535 targets took about a minute of
//! release CPU before it was refused (GitHub issue #479).
//!
//! [`EndpointTable`] classifies each distinct endpoint IRI once and hands Step 9 a dense
//! [`EndpointId`]. The source of an edge group is interned once per group — every edge of a
//! group yields the same borrowed slice, so a pointer-identity memo skips the hash — and each
//! target once per edge, so Step 9 costs time linear in the bytes of the edge list. A diagnostic
//! subject is minted once per endpoint and shared by reference count, so a per-edge diagnostic
//! never copies a long IRI.
//!
//! Ids are assigned in first-seen edge order and are used only as set and map keys; no hash
//! iteration order reaches a diagnostic, a model id or a vector position.

use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use oce_model::{ConnectorId, Dir, ValueType};

use crate::dto::Node;

use super::specialize::Specialization;

/// Dense identity of one distinct endpoint IRI within one Step 9 pass.
///
/// Two endpoints have the same id exactly when their IRIs are byte-equal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) struct EndpointId(pub(super) usize);

/// Everything Step 9 asks about one endpoint IRI, computed once per distinct IRI.
pub(super) struct Endpoint<'a> {
    /// The endpoint IRI as authored or re-anchored.
    pub(super) iri: &'a str,
    /// Pruned by a false load-time conditional guard.
    pub(super) inactive: bool,
    /// A declared (active) boundary input of the top composite.
    pub(super) boundary_in: bool,
    /// A declared (active) boundary output of the top composite.
    pub(super) boundary_out: bool,
    /// The instance-port connector this IRI names, if any.
    pub(super) connector: Option<ConnectorId>,
    /// That connector's direction.
    pub(super) dir: Option<Dir>,
    /// The lowered `@graph` node with this `@id`, if any.
    pub(super) node: Option<&'a Node>,
    /// The derived boundary value type, when one was derived without error.
    pub(super) boundary_type: Option<ValueType>,
    subject: OnceCell<Arc<str>>,
}

/// The lookup tables an endpoint is classified against. All are read-only in Step 9.
pub(super) struct EndpointLookups<'a> {
    /// Load-time specialization (inactive conditional nodes).
    pub(super) specialization: &'a Specialization,
    /// Active declared boundary inputs of the top composite.
    pub(super) boundary_in: &'a HashSet<&'a str>,
    /// Active declared boundary outputs of the top composite.
    pub(super) boundary_out: &'a HashSet<&'a str>,
    /// Instance-port connector ids by IRI.
    pub(super) conn_of_iri: &'a HashMap<&'a str, ConnectorId>,
    /// Connector directions indexed by `ConnectorId`; Step 9 never changes a direction.
    pub(super) connector_dirs: &'a [Dir],
    /// The lowered `@graph` index.
    pub(super) by_id: &'a HashMap<&'a str, &'a Node>,
    /// Derived boundary types (`None` where the derivation already diagnosed the declaration).
    pub(super) boundary_types: &'a HashMap<&'a str, Option<ValueType>>,
}

/// Interning table from endpoint IRI to [`EndpointId`] and its classification.
pub(super) struct EndpointTable<'a> {
    lookups: EndpointLookups<'a>,
    ids: HashMap<&'a str, EndpointId>,
    endpoints: Vec<Endpoint<'a>>,
    last_source: Option<(&'a str, EndpointId)>,
}

impl<'a> EndpointTable<'a> {
    /// An empty table over `lookups`.
    pub(super) fn new(lookups: EndpointLookups<'a>) -> Self {
        Self {
            lookups,
            ids: HashMap::new(),
            endpoints: Vec::new(),
            last_source: None,
        }
    }

    /// Intern an edge's source. Consecutive edges of one group pass the same borrowed slice, so
    /// a pointer-and-length match on the previous source reuses its id without hashing the IRI.
    /// A miss falls back to [`Self::intern`], so the result never depends on the memo.
    pub(super) fn intern_source(&mut self, iri: &'a str) -> EndpointId {
        if let Some((last, id)) = self.last_source
            && std::ptr::eq(last, iri)
        {
            return id;
        }
        let id = self.intern(iri);
        self.last_source = Some((iri, id));
        id
    }

    /// Intern an endpoint IRI, classifying it on first sight.
    pub(super) fn intern(&mut self, iri: &'a str) -> EndpointId {
        if let Some(&id) = self.ids.get(iri) {
            return id;
        }
        // A `usize` index cannot overflow: every id is a distinct entry already held in memory.
        let id = EndpointId(self.endpoints.len());
        let endpoint = self.classify(iri);
        self.endpoints.push(endpoint);
        self.ids.insert(iri, id);
        id
    }

    /// The classification of an interned endpoint.
    pub(super) fn get(&self, id: EndpointId) -> &Endpoint<'a> {
        &self.endpoints[id.0]
    }

    /// The endpoint's IRI as a diagnostic subject, allocated once and shared by every diagnostic
    /// that names this endpoint.
    pub(super) fn subject(&self, id: EndpointId) -> Arc<str> {
        let endpoint = &self.endpoints[id.0];
        Arc::clone(endpoint.subject.get_or_init(|| Arc::from(endpoint.iri)))
    }

    /// Every interned endpoint that is a declared boundary input — the boundary inputs some edge
    /// reads, in either authored role.
    pub(super) fn boundary_inputs_read(&self) -> HashSet<&'a str> {
        self.endpoints
            .iter()
            .filter(|endpoint| endpoint.boundary_in)
            .map(|endpoint| endpoint.iri)
            .collect()
    }

    fn classify(&self, iri: &'a str) -> Endpoint<'a> {
        let lookups = &self.lookups;
        let connector = lookups.conn_of_iri.get(iri).copied();
        Endpoint {
            iri,
            inactive: lookups.specialization.is_inactive(iri),
            boundary_in: lookups.boundary_in.contains(iri),
            boundary_out: lookups.boundary_out.contains(iri),
            connector,
            dir: connector.map(|id| lookups.connector_dirs[id.0 as usize]),
            node: lookups.by_id.get(iri).copied(),
            boundary_type: lookups.boundary_types.get(iri).copied().flatten(),
            subject: OnceCell::new(),
        }
    }
}

#[cfg(test)]
#[path = "connection_endpoints_tests.rs"]
mod tests;
