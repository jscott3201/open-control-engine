//! Canonical orientation of authored CXF connections.

use std::collections::{HashMap, HashSet};

use oce_model::{Connector, ConnectorId, Dir};

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
