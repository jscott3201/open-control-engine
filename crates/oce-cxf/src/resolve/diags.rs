//! Deterministic resolver diagnostic subject and sort helpers.

use std::collections::HashMap;
use std::sync::Arc;

use oce_diag::Diagnostic;
use oce_model::{Connector, ConnectorId};

/// The diagnostic subject for a connector — its source IRI when known, else a synthetic id.
pub(super) fn subject_of(c: &Connector) -> Arc<str> {
    match &c.iri {
        Some(iri) => Arc::clone(iri),
        None => Arc::from(format!("connector#{}", c.id.0)),
    }
}

/// Sort diagnostics into the pinned deterministic order: by the subject's `ConnectorId.0` ascending
/// (connector subjects first), then by `DiagCode` string, then message. Non-connector subjects sort
/// after all connectors (`u32::MAX`) by their raw subject string — total and panic-free.
pub(super) fn finalize_diags(
    mut diags: Vec<Diagnostic>,
    conn_of_iri: &HashMap<&str, ConnectorId>,
) -> Vec<Diagnostic> {
    // Resolve a subject IRI to its `ConnectorId.0`: either via a real source IRI (in `conn_of_iri`)
    // OR the synthetic `connector#N` form `subject_of` mints for an IRI-less connector. Both must
    // map to the numeric id so the structural diagnostics (single-assignment / direction / type) —
    // which are IRI-less in practice — sort by ascending `ConnectorId.0` per the pinned rule, not
    // by lexicographic string order (where `connector#10` would precede `connector#3`).
    let key_cid = |d: &Diagnostic| -> u32 {
        d.subject
            .as_deref()
            .and_then(|s| {
                conn_of_iri.get(s).map(|c| c.0).or_else(|| {
                    s.strip_prefix("connector#")
                        .and_then(|n| n.parse::<u32>().ok())
                })
            })
            .unwrap_or(u32::MAX)
    };
    diags.sort_by(|a, b| {
        if matches!((&a.subject, &b.subject), (Some(a), Some(b)) if Arc::ptr_eq(a, b)) {
            return a
                .code
                .as_str()
                .cmp(b.code.as_str())
                .then_with(|| a.message.cmp(&b.message));
        }
        key_cid(a)
            .cmp(&key_cid(b))
            .then_with(|| a.subject.as_deref().cmp(&b.subject.as_deref()))
            .then_with(|| a.code.as_str().cmp(b.code.as_str()))
            .then_with(|| a.message.cmp(&b.message))
    });
    diags
}
