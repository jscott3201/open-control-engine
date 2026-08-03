//! Materialization and interface checks for authored top-composite boundary outputs.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use oce_diag::{DiagCode, Diagnostic};
use oce_model::{BoundaryOutput, ConnectorId};

use crate::dto::{CxfDocument, Node};

/// Materialize outputs in their boundary nodes' source-document positions.
pub(super) fn materialize(
    doc: &CxfDocument,
    sources: &HashMap<String, ConnectorId>,
) -> Vec<BoundaryOutput> {
    doc.graph
        .iter()
        .filter_map(|node| {
            sources
                .get(node.id.as_str())
                .copied()
                .map(|source| BoundaryOutput {
                    iri: Arc::from(node.id.as_str()),
                    source,
                })
        })
        .collect()
}

/// Run the three declared-interface checks in one pass over the top composite's `hasOutput`
/// contract: refuse shadowed identities, refuse multiply driven outputs, and warn for undriven
/// declarations. `drivers` is Step 9's per-declared-output distinct-driver map.
pub(super) fn check_declared_interface(
    top: &Node,
    boundary_in: &HashSet<&str>,
    boundary_out: &HashSet<&str>,
    conn_of_iri: &HashMap<&str, ConnectorId>,
    by_id: &HashMap<&str, &Node>,
    drivers: &HashMap<String, HashSet<String>>,
    diags: &mut Vec<Diagnostic>,
) {
    let shadowed = refuse_shadowed(top, boundary_in, boundary_out, conn_of_iri, diags);
    refuse_multiply_driven(drivers, diags);
    warn_undriven(top, boundary_out, by_id, drivers, &shadowed, diags);
}

/// Refuse every declared boundary output whose IRI is also an existing connector identity.
///
/// Such a name would answer as two different points: the connector's own slot on the
/// connector-path surfaces and a different driver through the declared-output alias — and in the
/// hasInput∩hasOutput form the "declared output" is an INPUT path `set_input` accepts, breaking
/// the output-only alias contract by construction. The refusal is the identity-level analogue of
/// the `SingleAssignment` value-level refusal. Checked in `hasOutput` array order; the returned
/// set lets [`warn_undriven`] skip refused IRIs — the refusal owns the class.
fn refuse_shadowed<'doc>(
    top: &'doc Node,
    boundary_in: &HashSet<&str>,
    boundary_out: &HashSet<&str>,
    conn_of_iri: &HashMap<&str, ConnectorId>,
    diags: &mut Vec<Diagnostic>,
) -> HashSet<&'doc str> {
    let mut shadowed: HashSet<&str> = HashSet::new();
    for iri in top
        .has_output
        .iter()
        .map(|r| r.id.as_str())
        .filter(|iri| boundary_out.contains(iri))
    {
        let message = if boundary_in.contains(iri) {
            "boundary output IRI is also a boundary input"
        } else if conn_of_iri.contains_key(iri) {
            "boundary output shadows an instance port connector"
        } else {
            continue;
        };
        if shadowed.insert(iri) {
            diags.push(
                Diagnostic::error(DiagCode::BoundaryOutputShadowsConnector, message)
                    .with_subject(iri.to_owned()),
            );
        }
    }
    shadowed
}

/// Refuse every boundary output with more than one distinct driver (§7.10 single assignment):
/// post-specialization multiplicity means two surviving values claim one declared name.
fn refuse_multiply_driven(drivers: &HashMap<String, HashSet<String>>, diags: &mut Vec<Diagnostic>) {
    for (output, distinct) in drivers {
        if distinct.len() > 1 {
            diags.push(
                Diagnostic::error(
                    DiagCode::SingleAssignment,
                    format!(
                        "boundary output is multiply driven (distinct drivers {})",
                        distinct.len()
                    ),
                )
                .with_subject(output.clone()),
            );
        }
    }
}

/// Warn for every declared boundary output whose node exists but that nothing drives.
///
/// Such a declaration previously imported with zero diagnostics and vanished from re-export.
/// Advisory only: no CDL sentence requires a top composite's declared output to be internally
/// driven, and refusal would be an acceptance change. The undriven output enters neither
/// `boundary_outputs` nor any point surface, so this warning is its only representation. A
/// missing node already carries its `UnresolvedReference` error, and a shadowed IRI its refusal
/// — neither warns again here.
fn warn_undriven(
    top: &Node,
    boundary_out: &HashSet<&str>,
    by_id: &HashMap<&str, &Node>,
    drivers: &HashMap<String, HashSet<String>>,
    shadowed: &HashSet<&str>,
    diags: &mut Vec<Diagnostic>,
) {
    let mut seen: HashSet<&str> = HashSet::new();
    for iri in top.has_output.iter().map(|r| r.id.as_str()) {
        if boundary_out.contains(iri)
            && by_id.contains_key(iri)
            && !drivers.contains_key(iri)
            && !shadowed.contains(iri)
            && seen.insert(iri)
        {
            diags.push(
                Diagnostic::warning(
                    DiagCode::UndrivenBoundaryOutput,
                    "declared boundary output has no internal driver",
                )
                .with_subject(iri.to_owned()),
            );
        }
    }
}
