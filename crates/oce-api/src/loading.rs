//! Loading entry points beyond the primary `load_cxf` (which lives in `engine.rs`): the [`LoadReport`]
//! returned by every `load_*` call, the [`TemplateRef`] handle, and the two **deferred** load paths
//! (`load_from_semantic`, `load_modelica`). The deferred paths carry their **final frozen
//! signatures** now (R-PUB-5) but return a typed [`OcError::Load`] in M1 — never a panic (exit #6).

use oce_store::{SemanticQuery, Store};

use crate::engine::Engine;
use crate::error::OcError;
use crate::io::IoSummary;

/// A reference to a compiled CDL template for the Ch.13 reverse [`Engine::load_from_semantic`] flow
/// (`08` §3). doc-08 names this type but `oce-store` has none — it is an `oce-api`-level loader
/// concern. M1: an opaque, owned by-IRI handle carrying **no** store-backend type / `DomainKey` /
/// `PointHandle` (R-API-8). `#[non_exhaustive]` so M2 can add fields (version pin, param overrides)
/// additively.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct TemplateRef {
    iri: std::sync::Arc<str>,
}

impl TemplateRef {
    /// Construct a template reference from any string-like template IRI.
    #[must_use]
    pub fn new(iri: impl Into<std::sync::Arc<str>>) -> Self {
        Self { iri: iri.into() }
    }

    /// Borrow the compiled-template class IRI.
    #[must_use]
    pub fn iri(&self) -> &str {
        &self.iri
    }
}

/// The result of a successful load (`08` §3 R-PUB-5 / R-LOAD-1/2). Carries the `should`-level
/// diagnostics (the load succeeded; these are advisory), the model identity, the IO summary the host
/// needs immediately, and the model size signals. `#[non_exhaustive]`, `Clone` (a PyO3 binder owns
/// it), `Default` (the pre-load empty report).
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct LoadReport {
    /// Stable identity of the loaded model (re-exported `oce_store::DomainKey`; never a backend
    /// type, R-API-8). **M1: the empty `Default` key** — `ModelGraph` carries no model-level IRI
    /// yet; M2 derives it from the top-composite `@id`.
    pub model_id: oce_store::DomainKey,
    /// `should`-level diagnostics from ingest + validation (the shared `oce-diag` vocabulary, AD-4).
    pub warnings: Vec<oce_diag::Diagnostic>,
    /// §6 IO summary: counts by `IoClass` + point total (built at load from the model connectors).
    pub io: IoSummary,
    /// Number of (elementary) block instances in the loaded model.
    pub block_count: usize,
    /// Number of stateful `[S]` block instances — a state-footprint signal (`08` §3 / §5).
    pub stateful_blocks: usize,
}

impl<S: Store> Engine<S> {
    /// Ch.13 reverse flow (`08` §3): instantiate a compiled CDL template against equipment whose
    /// point signature matches a `SemanticStore` query. **Deferred to M2** — the
    /// `SemanticStore::match_template` binding is not yet wired; the frozen signature is surfaced
    /// now for stability.
    ///
    /// # Errors
    /// Always [`OcError::Load`] in M1 (deferred). Never panics (R-ERR-1).
    pub fn load_from_semantic(
        &mut self,
        template: &TemplateRef,
        query: &SemanticQuery,
    ) -> Result<LoadReport, OcError> {
        let _ = query; // the frozen signature is exercised; the binding is M2
        Err(OcError::Load {
            detail: format!(
                "load_from_semantic (Ch.13 reverse flow, 08 §3) is deferred to M2: template \
                 '{}' / SemanticStore::match_template binding not yet wired",
                template.iri()
            ),
        })
    }

    /// Deferred `.mo` path (FRAME §7 non-goal): in v1 this would delegate to upstream modelica-json
    /// to produce CXF, then call [`Engine::load_cxf`]. **Deferred to M2** — surfaced now so the
    /// signature is stable.
    ///
    /// # Errors
    /// Always [`OcError::Load`] in M1 (deferred). `Path::display` allocates only — no filesystem
    /// access — so this never panics (R-ERR-1).
    pub fn load_modelica(&mut self, path: &std::path::Path) -> Result<LoadReport, OcError> {
        Err(OcError::Load {
            detail: format!(
                "load_modelica (.mo path, 08 §3 / FRAME §7) is deferred to M2: upstream \
                 modelica-json -> CXF lowering not yet wired (path: {})",
                path.display()
            ),
        })
    }
}
