//! Canonical orientation of authored CXF connections.

use oce_model::Dir;

use super::connection_endpoints::{EndpointId, EndpointTable};

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
/// target slot is what routes it to that check. Endpoints arrive interned, so orienting an edge
/// never re-hashes either IRI.
pub(super) fn orient_edge(
    source: EndpointId,
    target: EndpointId,
    endpoints: &EndpointTable<'_>,
) -> (EndpointId, EndpointId) {
    let (s, t) = (endpoints.get(source), endpoints.get(target));
    if s.boundary_in || t.boundary_out {
        return (source, target);
    }
    if t.boundary_in || s.boundary_out {
        return (target, source);
    }
    match (s.dir, t.dir) {
        (Some(Dir::In), Some(Dir::Out)) => (target, source),
        _ => (source, target),
    }
}
