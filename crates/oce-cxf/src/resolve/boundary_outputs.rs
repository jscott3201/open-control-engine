//! Materialization of authored top-composite boundary outputs.

use std::collections::HashMap;
use std::sync::Arc;

use oce_model::{BoundaryOutput, ConnectorId};

use crate::dto::CxfDocument;

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
