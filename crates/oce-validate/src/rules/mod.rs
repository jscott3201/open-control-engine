//! Rule implementations and deterministic diagnostic ordering for the deep load gate.
//!
//! Every check here is **total and panic-free** on *any* [`ModelGraph`] — including a
//! structurally-malformed, hand-built one whose ids are out of range or whose [`Attrs`] variant
//! violates the R5 tag invariant (a public struct literal can do this, bypassing the checked
//! [`oce_model::Connector::with_attrs`] constructor). Every block/connector/connection index is
//! bounds-checked before use and every `Real`-attribute read goes through
//! [`oce_model::Attrs::as_real`] (which yields `None` on a mismatched tag) rather than an unwrap.
//! A malformed graph yields a [`DiagCode::MalformedDocument`] diagnostic, never an abort
//! (`08` R-ERR-1 / the safety-critical testing standard).

use std::collections::HashMap;
use std::sync::Arc;

use oce_diag::Diagnostic;
use oce_model::{Connector, ConnectorId, ModelGraph};

mod structural;
mod unify;

pub(crate) use structural::{
    check_arena_ids, check_connections, check_port_types, check_single_assignment,
};
pub(crate) use unify::unify_clusters;

// ---- shared helpers -------------------------------------------------------------------------

/// The diagnostic subject for a connector — its source IRI when known, else a synthetic
/// `connector#N` id. **Ported verbatim from `oce-cxf` `resolve.rs`** so resolve-time and
/// validate-time diagnostics share one subject convention; [`finalize_diags`] parses the synthetic
/// form back to a numeric id so IRI-less structural diagnostics still sort by ascending
/// `ConnectorId.0` (not lexicographically, where `connector#10` would precede `connector#3`).
pub(crate) fn subject_of(c: &Connector) -> Arc<str> {
    match &c.iri {
        Some(iri) => Arc::clone(iri),
        None => Arc::from(format!("connector#{}", c.id.0)),
    }
}

/// Build the lookup `source-IRI → ConnectorId` map [`finalize_diags`] uses to resolve a real
/// connector IRI back to its numeric id. Only connectors that carry an `iri` appear (hand-built
/// connectors typically do not — their diagnostics use the synthetic `connector#N` subject).
pub(crate) fn conn_of_iri(model: &ModelGraph) -> HashMap<&str, ConnectorId> {
    model
        .connectors
        .iter()
        .filter_map(|c| c.iri.as_deref().map(|iri| (iri, c.id)))
        .collect()
}

/// In-degree of every connector (count of connections whose `to` is that connector). Out-of-range
/// `to` endpoints are skipped here (they are reported as [`DiagCode::MalformedDocument`] by
/// [`structural::check_connections`]); the returned vector is dense over `connectors.len()`.
pub(super) fn in_degrees(model: &ModelGraph) -> Vec<u32> {
    let mut deg = vec![0u32; model.connectors.len()];
    for c in &model.connections {
        let to = c.to.0 as usize;
        if to < deg.len() {
            deg[to] += 1;
        }
    }
    deg
}

// ---- deterministic diagnostic ordering ------------------------------------------------------

/// Sort diagnostics into the pinned deterministic order: connector subjects by `ConnectorId.0`,
/// then block subjects by `BlockId.0`, then other subjects, followed by `DiagCode` string and
/// message tie-breakers. Synthetic `connector#N` and `block#N` subjects are parsed numerically so
/// IRI-less structural diagnostics never sort lexicographically (`#10` before `#3`). **Ported from
/// `oce-cxf` `resolve.rs`** for connector subjects so the resolver's and the validator's diagnostic
/// streams use one ordering discipline (`_spec/11-m1-cxf-plan.md` §2 determinism rule).
pub(crate) fn finalize_diags(
    mut diags: Vec<Diagnostic>,
    conn_of_iri: &HashMap<&str, ConnectorId>,
) -> Vec<Diagnostic> {
    // Resolve a subject IRI to its `ConnectorId.0`: either via a real source IRI (in `conn_of_iri`)
    // OR the synthetic `connector#N` form `subject_of` mints for an IRI-less connector. Both must
    // map to the numeric id so the structural diagnostics (single-assignment / direction / type) —
    // which are IRI-less in practice — sort by ascending `ConnectorId.0` per the pinned rule, not
    // by lexicographic string order (where `connector#10` would precede `connector#3`).
    let subject_key = |d: &Diagnostic| -> (u8, u32) {
        let Some(s) = d.subject.as_deref() else {
            return (2, u32::MAX);
        };
        if let Some(c) = conn_of_iri.get(s) {
            return (0, c.0);
        }
        if let Some(id) = s
            .strip_prefix("connector#")
            .and_then(|num| num.parse::<u32>().ok())
        {
            return (0, id);
        }
        if let Some(id) = s
            .strip_prefix("block#")
            .and_then(|num| num.parse::<u32>().ok())
        {
            return (1, id);
        }
        (2, u32::MAX)
    };
    diags.sort_by(|a, b| {
        subject_key(a)
            .cmp(&subject_key(b))
            .then_with(|| a.subject.as_deref().cmp(&b.subject.as_deref()))
            .then_with(|| a.code.as_str().cmp(b.code.as_str()))
            .then_with(|| a.message.cmp(&b.message))
    });
    diags
}
