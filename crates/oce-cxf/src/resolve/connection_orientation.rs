//! Canonical orientation of authored CXF connections.

use std::collections::{HashMap, HashSet};

use oce_model::{Connector, ConnectorId, Dir};

/// Re-anchor one `isConnectedTo` edge on its driving end.
///
/// CXF permits either connector to carry the relationship, while the flat model requires a
/// driver-to-driven ordering. Invalid same-direction pairs remain unchanged for later diagnostics.
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
