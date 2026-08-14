//! Canonical orientation of authored CXF connections.

use std::collections::{HashMap, HashSet};

use oce_model::{Connector, ConnectorId, Dir};

use super::specialize::Specialization;

/// One-for-one collapse of opposite endpoint spellings after driver orientation.
pub(super) struct RelationMultiplicity<'a> {
    desired: HashMap<(&'a str, &'a str), usize>,
    emitted: HashMap<(&'a str, &'a str), usize>,
}

impl<'a> RelationMultiplicity<'a> {
    /// Count active forward and reverse spellings without collapsing same-direction copies.
    pub(super) fn new(
        edges: impl Iterator<Item = (&'a str, &'a str)>,
        boundary_in: &HashSet<&str>,
        boundary_out: &HashSet<&str>,
        conn_of_iri: &HashMap<&str, ConnectorId>,
        connectors: &[Connector],
        specialization: &Specialization,
    ) -> Self {
        let mut counts: HashMap<(&str, &str), (usize, usize)> = HashMap::new();
        for (source, target) in edges {
            if specialization.is_inactive(source) || specialization.is_inactive(target) {
                continue;
            }
            if !boundary_in.contains(source)
                && !boundary_in.contains(target)
                && !boundary_out.contains(source)
                && !boundary_out.contains(target)
            {
                continue;
            }
            let oriented = orient_edge(
                source,
                target,
                boundary_in,
                boundary_out,
                conn_of_iri,
                connectors,
            );
            let count = counts.entry(oriented).or_default();
            if oriented == (target, source) && source != target {
                count.1 += 1;
            } else {
                count.0 += 1;
            }
        }
        Self {
            desired: counts
                .into_iter()
                .map(|(pair, (forward, reverse))| (pair, forward.max(reverse)))
                .collect(),
            emitted: HashMap::new(),
        }
    }

    /// Retain the earliest occurrences up to the canonical pair's required multiplicity.
    pub(super) fn retain(&mut self, source: &'a str, target: &'a str) -> bool {
        let pair = (source, target);
        let Some(desired) = self.desired.get(&pair).copied() else {
            return true;
        };
        let emitted = self.emitted.entry(pair).or_default();
        if *emitted >= desired {
            return false;
        }
        *emitted += 1;
        true
    }
}

/// Re-anchor one `isConnectedTo` edge on its driving end.
///
/// CXF §8.2 gives `connectedTo` the domain *(OutputConnector, InputConnector)* and the range
/// *(InputConnector, OutputConnector)*, so either endpoint may carry the relationship. CDL also
/// makes `connect` argument order immaterial; modelica-json preserves that authored order (11 of
/// 16 edges in `Economizers.Subsequences.Modulations.Reliefs` are input-subject). The flat model,
/// however, requires driver-to-driven ordering.
///
/// Invalid same-direction or unresolved pairs remain unchanged for later diagnostics. Step 9 has
/// five arms that continue before the general edge checks, and Step 10 only sees edges that reach
/// it, so each early arm must validate its own counterpart; orienting a boundary output into the
/// target slot is what routes it to that check.
pub(super) fn orient_edge<'a>(
    source: &'a str,
    target: &'a str,
    boundary_in: &HashSet<&str>,
    boundary_out: &HashSet<&str>,
    conn_of_iri: &HashMap<&str, ConnectorId>,
    connectors: &[Connector],
) -> (&'a str, &'a str) {
    if boundary_in.contains(source) || boundary_out.contains(target) {
        return (source, target);
    }
    if boundary_in.contains(target) || boundary_out.contains(source) {
        return (target, source);
    }
    let dir = |iri: &str| conn_of_iri.get(iri).map(|id| connectors[id.0 as usize].dir);
    match (dir(source), dir(target)) {
        (Some(Dir::In), Some(Dir::Out)) => (target, source),
        _ => (source, target),
    }
}
