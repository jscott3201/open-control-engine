#![forbid(unsafe_code)]
//! `oce-validate` — loader conformance for the Open Control Engine.
//!
//! Enforces the CDL subset at load: subset-restriction rejection, single-assignment /
//! in-degree-1 on input connectors (§7.10), structural Real/Integer/Boolean type matching, the
//! §7.10 attribute-unification rule, and the two-tier `shall`(error)/`should`(warning)
//! diagnostic model (CDL Ch. 2). It is **Group A** (no store, no database) and reads the
//! `oce-blocks` feedthrough oracle only to classify connectors, never to compute signals.
//!
//! Status: **M0 scaffold.** The checks land in M1.

use oce_model::ModelGraph;

/// Diagnostic severity — the two-tier `shall`/`should` model (CDL Ch. 2).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Severity {
    /// `shall`-level: a hard load error.
    Shall,
    /// `should`-level: a non-fatal warning.
    Should,
}

/// A single validation diagnostic.
#[derive(Clone, Debug)]
pub struct Diagnostic {
    /// Severity (error vs warning).
    pub severity: Severity,
    /// Human-readable message; cites the governing CDL section where one applies.
    pub message: String,
}

/// A validation failure carrying the `shall`-level diagnostics that blocked the load.
#[derive(Clone, Debug, thiserror::Error)]
#[error("validation failed with {} shall-level diagnostic(s)", .diagnostics.len())]
pub struct ValidationError {
    /// The `shall`-level diagnostics.
    pub diagnostics: Vec<Diagnostic>,
}

/// Validate a flattened model. Returns `should`-level warnings on success; a [`ValidationError`]
/// carrying `shall`-level diagnostics on failure.
///
/// # Errors
/// Returns [`ValidationError`] if any `shall`-level rule is violated.
pub fn validate(_model: &ModelGraph) -> Result<Vec<Diagnostic>, ValidationError> {
    unimplemented!("oce-validate::validate — M0 scaffold (conformance checks land in M1)")
}
