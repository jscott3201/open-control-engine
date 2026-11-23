//! The lookups the export's plan makes once for each boundary-driven input, each at a cost that
//! does not grow with the number of such inputs (issue #811).
//!
//! The plan used to search `external_inputs` for every block input it planned, search the
//! boundary-driven inputs again, comparing IRIs, for every declared boundary input, and search the
//! boundary nodes it had planned, comparing IRIs, for every boundary-driven input. Each was
//! quadratic in a boundary's fan-out, and the comparisons also ran the length of the IRI. Each is
//! now a set or a map built in one pass, answering exactly what the search answered.
//!
//! Every input a boundary input drives holds the boundary's IRI as one shared allocation, so the
//! passes below recognise a repeat of an allocation by its address and length before they read
//! its text: equal address and length is the same text. Unequal addresses imply nothing, and the
//! text is then compared, so a hand-built graph with separate allocations is answered alike.

use std::collections::{BTreeMap, BTreeSet};

use oce_model::ModelGraph;

/// The address and length of a string's bytes: equal exactly when two `&str`s view the same
/// bytes, which implies equal text.
fn view(text: &str) -> (*const u8, usize) {
    (text.as_ptr(), text.len())
}

/// The dense ids of the connectors `external_inputs` lists, as a set: what
/// `external_inputs.contains` answered for each block input.
pub(super) fn boundary_driven(g: &ModelGraph) -> BTreeSet<u32> {
    g.external_inputs.iter().map(|id| id.0).collect()
}

/// The IRIs the connectors `external_inputs` lists carry (an id past the arena, or a connector
/// with no IRI, adds none): what searching those connectors for a declared boundary input's IRI
/// answered. A run of entries holding one allocation is read once.
pub(super) fn targeted_iris(g: &ModelGraph) -> BTreeSet<&str> {
    let mut iris = BTreeSet::new();
    let mut last = None;
    for id in &g.external_inputs {
        let Some(iri) = g
            .connectors
            .get(id.0 as usize)
            .and_then(|connector| connector.iri.as_deref())
        else {
            continue;
        };
        if last != Some(view(iri)) {
            iris.insert(iri);
            last = Some(view(iri));
        }
    }
    iris
}

/// The planned boundary nodes by IRI: what searching them for the first with an IRI answered. The
/// plan adds a node only when no node has its IRI, so the first is the only one.
#[derive(Default)]
pub(super) struct BoundaryNodes<'g> {
    by_iri: BTreeMap<&'g str, usize>,
    /// The view and position of the last IRI asked for: the entries of one boundary's fan-out
    /// come together and share one allocation, so each but the first is answered here.
    last: Option<((*const u8, usize), usize)>,
}

impl<'g> BoundaryNodes<'g> {
    /// The position of the node with `iri`, if one was added.
    pub(super) fn find(&mut self, iri: &'g str) -> Option<usize> {
        if let Some((seen, at)) = self.last
            && seen == view(iri)
        {
            return Some(at);
        }
        let at = self.by_iri.get(iri).copied()?;
        self.last = Some((view(iri), at));
        Some(at)
    }

    /// Record that the node with `iri` is at `at`.
    pub(super) fn add(&mut self, iri: &'g str, at: usize) {
        self.by_iri.insert(iri, at);
        self.last = Some((view(iri), at));
    }
}

#[cfg(test)]
#[path = "export_boundary_lookups_tests.rs"]
mod tests;
