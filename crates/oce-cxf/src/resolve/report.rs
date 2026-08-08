//! The resolver's public diagnostic report surface.

use oce_diag::{Diagnostic, has_errors};

/// The resolver's diagnostics in deterministic order. On the `Ok` path it carries `Warning`/`Info`
/// only — any `Error` is returned inside [`CxfError::Validation`](crate::CxfError::Validation)
/// instead, with the graph withheld (it may be structurally unsound). Invariant enforced by
/// construction in the resolver. The report also carries the model identity side-channel for
/// consumers that need durable identity without polluting [`oce_model::ModelGraph`] execution
/// state.
#[derive(Clone, Debug, Default)]
pub struct ValidationReport {
    /// The top-composite `@id` that names the CXF model.
    ///
    /// This is the raw DTO [`Node::id`](crate::dto::Node::id) value as authored in the document.
    /// The resolver currently carries context entries losslessly but does not perform general
    /// JSON-LD `@id` expansion, so callers that need a durable model key should treat this as
    /// the source CXF model IRI for M3. It is `Some` on every successful resolver-owned import
    /// path and `None` only for manually default-constructed reports.
    pub model_iri: Option<String>,
    /// The (sorted, error-free on the `Ok` path) diagnostics.
    pub diagnostics: Vec<Diagnostic>,
}

impl ValidationReport {
    /// Whether the report carries no diagnostics at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// Whether any diagnostic is an error (always `false` on the `Ok` path).
    #[must_use]
    pub fn has_errors(&self) -> bool {
        has_errors(&self.diagnostics)
    }
}
