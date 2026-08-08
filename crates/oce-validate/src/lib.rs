#![forbid(unsafe_code)]
// Bit-exact is the law here: §7.10 numeric-bound agreement compares `f64` by `to_bits`,
// never by `==`/ε. `clippy::float_cmp` is pedantic-tier (not in the workspace `-D warnings` set), so
// without this deny a regression from `to_bits` to `==` would silently flip `-0.0`/`+0.0` to
// "agree" and `NaN`/`NaN` to "conflict" — and pass every test + the default clippy gate. Deny it so
// any raw `f64` comparison in this gate is a build error, not just a missed test.
#![deny(clippy::float_cmp)]
//! `oce-validate` — the authoritative **deep load-conformance gate** for the Open Control Engine.
//!
//! After the `oce-cxf` resolver's minimal structural fail-fast (duplicate `@id`, unresolved
//! endpoints, CXF interface arity) and `oce-flatten`'s array normalization, this crate is the gate
//! that decides whether a flattened [`ModelGraph`] may be built and ticked. It enforces:
//!
//! 1. **Arena id integrity**: every `BlockId` is dense/unique, every connector's owning block id is
//!    in range, and every block input/output connector id is in range before `oce-graph` indexes
//!    arenas by raw ids.
//! 2. **Boundary-output integrity**: every declared output aliases an in-range `Out` connector and
//!    carries the attribute variant for that connector's value type.
//! 3. **Boundary-aware single assignment** (§7.10 / §9.1.5): every `In` connector has in-degree
//!    exactly 1, except a declared external boundary input (`ModelGraph::external_inputs`, the AD-2
//!    elision), which legally has in-degree 0.
//! 4. **Per-connection direction + value type** (§9.1.6): `from` is an output, `to` an input, and
//!    the two share a [`oce_model::ValueType`] (CDL forbids implicit coercion).
//! 5. **Block interface arity + connector type ↔ block-signature port-kind** agreement (AD-8, §7.8):
//!    a block's port counts and each connector's declared value type must match the native block
//!    class signature — the reason this crate depends on `oce-blocks`. Without it a malformed graph
//!    could reach the hot-path readers/emitters and panic or silently coerce to a type-zero.
//! 6. **§7.10 attribute unification** (doc 02 §9 R13.1–R13.4): the unified attributes
//!    (`quantity`, `unit`, `min`, `max`) must agree across a connected cluster, including declared
//!    boundary outputs that alias a source connector (`shall`-error on conflict —
//!    [`oce_diag::DiagCode::UnitQuantityMismatch`] / [`oce_diag::DiagCode::BoundMismatch`] — and
//!    propagation when one-sided, R13.1/R13.2); divergent `displayUnit` is an advisory
//!    `should`-warning (R13.3); `nominal`/`unbounded` are the only attributes not unified (R13.4).
//!
//! It reports problems in the **shared `oce-diag` vocabulary** (AD-4) — the same
//! [`oce_diag::Diagnostic`] the resolver emits — so `oce-api`'s `LoadReport.warnings` is one
//! uniform stream with no dialect translation. It is **Group A** (no store, no database) and reads
//! the `oce-blocks` registry only to classify connectors, never to compute signals.
//!
//! # API shape
//!
//! - [`validate`] — a **pure** checker: reads the graph, returns `should`-warnings on success or a
//!   [`ValidationError`] of `shall`-errors on failure. Never mutates.
//! - [`unify_attributes`] — runs §7.10 unification, which **mutates** the graph (R13.2 propagates
//!   one-sided unified attributes to unset connector and boundary-alias peers). Returns warnings /
//!   [`ValidationError`] likewise. A failing call leaves all attributes unchanged.
//! - [`unify_and_validate`] — the `oce-api` entry point: unify (so propagation lands before the
//!   structural checks read the graph), then validate, concatenating the warning streams. Any
//!   failure leaves all attributes unchanged.
//!
//! The deep gate is implemented against hand-built and CXF-resolved graphs. Attribute extraction
//! from CXF feeds this gate, and hand-built tests exercise the unification rules directly.

use oce_diag::Diagnostic;
use oce_model::ModelGraph;

mod rules;

#[cfg(test)]
mod tests;

/// A validation failure carrying the `shall`-level (error-severity) [`oce_diag::Diagnostic`]s that
/// blocked the load. Warnings, when present, are returned on the success path instead.
#[derive(Clone, Debug, thiserror::Error)]
#[error("validation failed with {} shall-level diagnostic(s)", .diagnostics.len())]
pub struct ValidationError {
    /// The `shall`-level (error) diagnostics, in the pinned deterministic order.
    pub diagnostics: Vec<Diagnostic>,
}

/// Validate a flattened model **without mutating it** (the structural, type, and parameter gates
/// above).
///
/// Returns the `should`-level warnings on success, or a [`ValidationError`] of the `shall`-level
/// diagnostics on failure. Total and panic-free on any graph (`08` R-ERR-1).
///
/// # Errors
/// Returns [`ValidationError`] if any `shall`-level rule is violated.
pub fn validate(model: &ModelGraph) -> Result<Vec<Diagnostic>, ValidationError> {
    let mut diags = Vec::new();
    rules::check_arena_ids(model, &mut diags);
    rules::check_boundary_outputs(model, &mut diags);
    rules::check_single_assignment(model, &mut diags);
    rules::check_connections(model, &mut diags);
    rules::check_port_types(model, &mut diags);
    rules::check_block_params(model, &mut diags);
    finish(model, diags)
}

/// Run §7.10 attribute unification (doc 02 §9 R13.1–R13.3), **mutating** the graph to propagate a
/// one-sided unit, quantity, or bound to its unset connector and boundary-output peers (R13.2).
///
/// Returns the `should`-level warnings (e.g. divergent `displayUnit`, R13.3) on success, or a
/// [`ValidationError`] of unit, quantity, or bound conflicts (R13.1) on failure. Order-independent (a
/// gather-then-decide cluster algorithm) and panic-free on any graph. Failure is transactional: no
/// connector or boundary-output attribute propagation remains applied.
///
/// # Errors
/// Returns [`ValidationError`] if any cluster declares conflicting unit, quantity, or bound values
/// (R13.1).
pub fn unify_attributes(model: &mut ModelGraph) -> Result<Vec<Diagnostic>, ValidationError> {
    let mut diags = Vec::new();
    rules::unify_clusters(model, &mut diags);
    finish(model, diags)
}

/// The `oce-api` load entry point: [`unify_attributes`] (propagation lands first), then
/// [`validate`], concatenating the warning streams (unification warnings first, then structural).
/// Short-circuits to the first failing stage's [`ValidationError`]. Failure is transactional: no
/// connector or boundary-output attribute propagation remains applied.
///
/// # Errors
/// Returns [`ValidationError`] from whichever stage first finds a `shall`-level violation.
pub fn unify_and_validate(model: &mut ModelGraph) -> Result<Vec<Diagnostic>, ValidationError> {
    let connector_attrs = model
        .connectors
        .iter()
        .map(|connector| connector.attrs.clone())
        .collect::<Vec<_>>();
    let boundary_attrs = model
        .boundary_outputs
        .iter()
        .map(|output| output.attrs.clone())
        .collect::<Vec<_>>();
    let result = (|| {
        let mut warnings = unify_attributes(model)?;
        let validate_warnings = validate(model)?;
        warnings.extend(validate_warnings);
        Ok(warnings)
    })();
    if result.is_err() {
        for (connector, attrs) in model.connectors.iter_mut().zip(connector_attrs) {
            connector.attrs = attrs;
        }
        for (output, attrs) in model.boundary_outputs.iter_mut().zip(boundary_attrs) {
            output.attrs = attrs;
        }
    }
    result
}

/// Sort `diags` into the pinned deterministic order, then partition: any error fails the load
/// (returning only the errors, for a correct `shall`-count), otherwise the (all-warning) stream is
/// the success value.
fn finish(model: &ModelGraph, diags: Vec<Diagnostic>) -> Result<Vec<Diagnostic>, ValidationError> {
    let conn_of_iri = rules::conn_of_iri(model);
    let diags = rules::finalize_diags(diags, &conn_of_iri);
    if oce_diag::has_errors(&diags) {
        let diagnostics = diags.into_iter().filter(Diagnostic::is_error).collect();
        Err(ValidationError { diagnostics })
    } else {
        Ok(diags)
    }
}
