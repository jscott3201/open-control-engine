//! Resolve Step 9: collect connections with boundary elision.
//!
//! Walks the lowered edge list — authored sources in `@graph` order, then node-less derived
//! sources in block-2 order, targets in array order within each source — screens inactive
//! endpoints, orients each edge on its driving end, and routes it to one of five elision arms or
//! the general connector-to-connector arm. Every endpoint is interned once
//! ([`connection_endpoints`](super::connection_endpoints)), so the pass costs time linear in the
//! bytes of the edge list, never fan-out × IRI length (GitHub issue #479).
//!
//! Diagnostics are pushed in edge order, exactly as before interning; the dedup sets key on
//! [`EndpointId`]s, which are equal exactly when the IRIs are byte-equal, so every "report once"
//! rule keeps its content semantics.

use std::collections::{HashMap, HashSet};

use oce_diag::{DiagCode, Diagnostic};
use oce_model::{Connection, Connector, ConnectorId, Dir};

use crate::dto::Node;

use super::boundary_inputs::AssignmentTracker;
use super::connection_endpoints::{EndpointId, EndpointLookups, EndpointTable};
use super::connection_orientation::orient_edge;

/// What Step 9 collected for the steps after it.
pub(super) struct CollectedConnections<'a> {
    /// Connector-to-connector wires, in edge order.
    pub(super) connections: Vec<Connection>,
    /// Child inputs reached from a boundary input, in first-assignment order (re-keyed later).
    pub(super) external_inputs: Vec<ConnectorId>,
    /// Assignment cardinality for those child inputs.
    pub(super) boundary_input_assignments: AssignmentTracker,
    /// Distinct boundary-input → boundary-output pass-through pairs, in first-seen order.
    pub(super) pass_through_pairs: Vec<(String, String)>,
    /// Distinct drivers per declared boundary output.
    pub(super) boundary_output_drivers: HashMap<String, HashSet<String>>,
    /// The first connector driving each declared boundary output.
    pub(super) boundary_output_sources: HashMap<String, ConnectorId>,
    /// Declared boundary inputs that appear as an endpoint of any edge, in either role.
    pub(super) boundary_inputs_read: HashSet<&'a str>,
}

fn is_array_node(node: &Node) -> bool {
    node.is_array == Some(true) || node.size_dims.is_some()
}

fn type_mismatch(from: oce_model::ValueType, to: oce_model::ValueType) -> String {
    format!("connected types differ: {:?} → {:?}", from, to)
}

/// Run Step 9 over `edges`. `connectors` is mutated only to attach a boundary IRI to an elided
/// child input; directions are read from `lookups.connector_dirs`, which Step 9 never changes.
pub(super) fn collect<'a>(
    edges: impl Iterator<Item = (&'a str, &'a str)>,
    lookups: EndpointLookups<'a>,
    connectors: &mut [Connector],
    diags: &mut Vec<Diagnostic>,
) -> CollectedConnections<'a> {
    let mut table = EndpointTable::new(lookups);
    let mut connections: Vec<Connection> = Vec::new();
    let mut external_inputs: Vec<ConnectorId> = Vec::new();
    let mut boundary_input_assignments = AssignmentTracker::default();
    let mut pass_through_pairs: Vec<(String, String)> = Vec::new();
    let mut seen_pass_through_pairs: HashSet<(EndpointId, EndpointId)> = HashSet::new();
    let mut boundary_output_drivers: HashMap<String, HashSet<String>> = HashMap::new();
    let mut boundary_output_sources: HashMap<String, ConnectorId> = HashMap::new();
    let mut seen_output_drivers: HashSet<(EndpointId, ConnectorId)> = HashSet::new();
    let mut inactive_connection_sources: HashSet<EndpointId> = HashSet::new();
    let mut reported_missing_endpoints: HashSet<EndpointId> = HashSet::new();
    for (source, target) in edges {
        // Intern both ends of every edge, even one an inactivity screen drops below: the set of
        // boundary inputs read is taken over the whole lowered edge list.
        let source = table.intern_source(source);
        let target = table.intern(target);
        if table.get(source).inactive {
            if inactive_connection_sources.insert(source) {
                diags.push(
                    Diagnostic::error(
                        DiagCode::InactiveConditionalNode,
                        "inactive conditional node still carries active connections",
                    )
                    .with_subject(table.subject(source)),
                );
            }
            continue;
        }
        if table.get(target).inactive {
            diags.push(
                Diagnostic::error(
                    DiagCode::InactiveConditionalNode,
                    "connection targets an inactive conditional node",
                )
                .with_subject(table.subject(target)),
            );
            continue;
        }
        // Both endpoints have now been screened for inactivity in their authored roles; the
        // orientation swap below only relabels which is the driver.
        let (source, target) = orient_edge(source, target, &table);
        let (s, t) = (table.get(source), table.get(target));
        if s.boundary_in && t.boundary_in {
            diags.push(
                Diagnostic::error(
                    DiagCode::DirectionMismatch,
                    "connection joins two boundary inputs",
                )
                .with_subject(table.subject(target)),
            );
            continue;
        }
        if s.boundary_out && t.boundary_out {
            diags.push(
                Diagnostic::error(
                    DiagCode::DirectionMismatch,
                    "connection joins two boundary outputs",
                )
                .with_subject(table.subject(target)),
            );
            continue;
        }
        if s.boundary_in && t.boundary_out {
            if s.node.is_some_and(is_array_node) || t.node.is_some_and(is_array_node) {
                diags.push(
                    Diagnostic::error(
                        DiagCode::NonSubsetConstruct,
                        "array boundary pass-through endpoints are unsupported",
                    )
                    .with_subject(table.subject(target)),
                );
                continue;
            }
            if let (Some(source_type), Some(target_type)) = (s.boundary_type, t.boundary_type)
                && source_type != target_type
            {
                diags.push(
                    Diagnostic::error(
                        DiagCode::TypeMismatch,
                        type_mismatch(source_type, target_type),
                    )
                    .with_subject(table.subject(target)),
                );
                continue;
            }
            if seen_pass_through_pairs.insert((source, target)) {
                boundary_output_drivers
                    .entry(t.iri.to_owned())
                    .or_default()
                    .insert(format!("boundary-input:{}", s.iri));
                pass_through_pairs.push((s.iri.to_owned(), t.iri.to_owned()));
            }
            continue;
        }
        if s.boundary_in {
            // boundary input → child input: elide; record external + attach boundary IRI (AD-2).
            match t.connector {
                Some(_) if t.dir != Some(Dir::In) => {
                    // A composite boundary INPUT may only drive a child INPUT — `external_inputs`
                    // are inputs by contract (oce-model). Driving an output is a direction error,
                    // and this elision path bypasses Step 10, so it must be checked here.
                    diags.push(
                        Diagnostic::error(
                            DiagCode::DirectionMismatch,
                            "boundary input drives a non-input connector",
                        )
                        .with_subject(table.subject(target)),
                    );
                }
                Some(to) => {
                    if !boundary_input_assignments.record(to, source, t.iri) {
                        continue;
                    }
                    // This elision path bypasses Step 10, so it must also compare the boundary
                    // input's type with the child input's type here.
                    let child_type = connectors[to.0 as usize].value_type;
                    if let Some(boundary_type) = s.boundary_type
                        && boundary_type != child_type
                    {
                        diags.push(
                            Diagnostic::error(
                                DiagCode::TypeMismatch,
                                type_mismatch(boundary_type, child_type),
                            )
                            .with_subject(table.subject(target)),
                        );
                    }
                    // `record` answers true once per child input and nothing else appends to
                    // `external_inputs` before Step 9 ends, so `to` is new here.
                    external_inputs.push(to);
                    connectors[to.0 as usize].iri = Some(table.subject(source));
                }
                None if reported_missing_endpoints.insert(target) => diags.push(
                    Diagnostic::error(
                        DiagCode::UnresolvedReference,
                        "boundary-input target not found",
                    )
                    .with_subject(table.subject(target)),
                ),
                None => {}
            }
            continue;
        }
        if t.boundary_out {
            // child output → boundary output: elide (the child output IS the model output).
            // The driving end MUST be checked here for the same reason the boundary-input arm
            // checks its target: this path `continue`s past Step 10, so nothing downstream ever
            // looks at `source`. Without it, `<boundary output> isConnectedTo <anything>` — a
            // child input, a parameter node, or an `@id` in no node at all — elides to silence.
            match s.connector {
                Some(_) if s.dir != Some(Dir::Out) => diags.push(
                    Diagnostic::error(
                        DiagCode::DirectionMismatch,
                        "boundary output is driven by a non-output connector",
                    )
                    .with_subject(table.subject(source)),
                ),
                Some(from) => {
                    let child_type = connectors[from.0 as usize].value_type;
                    if let Some(boundary_type) = t.boundary_type
                        && child_type != boundary_type
                    {
                        diags.push(
                            Diagnostic::error(
                                DiagCode::TypeMismatch,
                                type_mismatch(child_type, boundary_type),
                            )
                            .with_subject(table.subject(source)),
                        );
                    }
                    if seen_output_drivers.insert((target, from)) {
                        boundary_output_drivers
                            .entry(t.iri.to_owned())
                            .or_default()
                            .insert(format!("connector:{}", from.0));
                        boundary_output_sources
                            .entry(t.iri.to_owned())
                            .or_insert(from);
                    }
                }
                None if reported_missing_endpoints.insert(source) => diags.push(
                    Diagnostic::error(
                        DiagCode::UnresolvedReference,
                        "boundary-output source not found",
                    )
                    .with_subject(table.subject(source)),
                ),
                None => {}
            }
            continue;
        }
        match (s.connector, t.connector) {
            (Some(from), Some(to)) => connections.push(Connection { from, to }),
            (from, to) => {
                if from.is_none() && reported_missing_endpoints.insert(source) {
                    diags.push(
                        Diagnostic::error(
                            DiagCode::UnresolvedReference,
                            "connection source not found",
                        )
                        .with_subject(table.subject(source)),
                    );
                }
                if to.is_none() && reported_missing_endpoints.insert(target) {
                    diags.push(
                        Diagnostic::error(
                            DiagCode::UnresolvedReference,
                            "connection target not found",
                        )
                        .with_subject(table.subject(target)),
                    );
                }
            }
        }
    }
    CollectedConnections {
        connections,
        external_inputs,
        boundary_input_assignments,
        pass_through_pairs,
        boundary_output_drivers,
        boundary_output_sources,
        boundary_inputs_read: table.boundary_inputs_read(),
    }
}
