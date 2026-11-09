//! Boundary-aware single-assignment validation for resolved connector wiring.

use std::collections::HashMap;

use oce_diag::{DiagCode, Diagnostic};
use oce_model::{Connection, Connector, ConnectorId, Dir};

use super::diags::subject_of;

/// Diagnose input connectors whose resolved wire in-degree is not exactly one.
///
/// A connector in `external_inputs` is driven by the frame, so its wire in-degree must be zero:
/// any wire into it is a second driver. Every such connector a root boundary input reaches was
/// already refused by `boundary_inputs::AssignmentTracker::emit_diagnostics` (the mixed
/// block-output-and-boundary-input shape), which names the authored input IRI — this check would
/// name the boundary IRI the elision copied onto the connector — so it is skipped here. The only
/// other members, the pass-through inputs minted after Step 9, are fresh connectors no wire can
/// reach.
pub(super) fn check(
    connectors: &[Connector],
    connections: &[Connection],
    external_inputs: &[ConnectorId],
    diags: &mut Vec<Diagnostic>,
) {
    let mut in_degree: HashMap<ConnectorId, u32> = HashMap::new();
    for connection in connections {
        *in_degree.entry(connection.to).or_insert(0) += 1;
    }
    for connector in connectors {
        if connector.dir != Dir::In {
            continue;
        }
        let degree = in_degree.get(&connector.id).copied().unwrap_or(0);
        if external_inputs.contains(&connector.id) {
            // Frame-driven: in-degree 0 is the legal shape, and any wire was refused by the
            // boundary-input pass with the authored subject (see the function docs).
            continue;
        }
        match degree {
            1 => {}
            0 => diags.push(
                Diagnostic::error(
                    DiagCode::SingleAssignment,
                    "input is undriven (in-degree 0)",
                )
                .with_subject(subject_of(connector)),
            ),
            degree => diags.push(
                Diagnostic::error(
                    DiagCode::SingleAssignment,
                    format!("input is multiply driven (in-degree {degree})"),
                )
                .with_subject(subject_of(connector)),
            ),
        }
    }
}

#[cfg(test)]
#[path = "single_assignment_tests.rs"]
mod tests;
