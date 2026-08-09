//! Boundary-aware single-assignment validation for resolved connector wiring.

use std::collections::HashMap;

use oce_diag::{DiagCode, Diagnostic};
use oce_model::{Connection, Connector, ConnectorId, Dir};

use super::diags::subject_of;

/// Diagnose input connectors whose resolved in-degree is not exactly one.
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
        // Boundary inputs are elided into `external_inputs`. A connector that is both external
        // and the target of one wire therefore remains valid; boundary membership is not folded
        // into this wire count.
        let degree = in_degree.get(&connector.id).copied().unwrap_or(0);
        match degree {
            1 => {}
            0 if external_inputs.contains(&connector.id) => {}
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
