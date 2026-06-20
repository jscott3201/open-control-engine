#![forbid(unsafe_code)]
//! `oce-api` — the embeddable host facade for the Open Control Engine
//! (`08-embeddable-api-and-performance.md`). The single public surface downstream projects link
//! against (published under the umbrella name `open-control-engine`).
//!
//! # Posture (binding, FRAME §6)
//!
//! Library-only, synchronous, in-process, `#![forbid(unsafe_code)]`, edition 2024, rust 1.95.0,
//! **no async runtime, no server**. The host owns transport, TLS, authN/Z, multi-tenancy,
//! off-host durability, and metrics export.
//!
//! # The store seam
//!
//! [`Engine`] is generic over a `Store`, defaulting to `oce_store_mem::MemStore` so the **default
//! (and only) build has no database** (D-OWNER-1). The library ships no first-party database and no
//! DB-gated feature; a durable/queryable backend is an app-side adapter the host wires behind the
//! `oce-store` port. No store-backend-specific type ever escapes this facade (R-API-8).
//!
//! # Module layout
//!
//! The public surface is split across internal modules and re-exported **flat** here, so every path
//! stays `oce_api::Foo` (R-PUB-1/4; the `cargo public-api` baseline): `engine` (the
//! [`Engine`] handle + load/tick core), `error` ([`OcError`]), `loading` ([`LoadReport`],
//! [`TemplateRef`], deferred loaders), `params` (the live parameter table), `sim` (execution modes
//! + [`Outputs`]), `io` (the typed IO inventory).
//!
//! The full load → tick → simulate loop works; [`Engine::load_cxf`] runs the end-to-end CXF ingest
//! pipeline (resolve → flatten → validate → BUILD). The frozen public surface (`08` §11.1 R-PUB-5/6)
//! includes `simulate` / `step_realtime`, `set_input` / `get_output`, the live parameter table
//! (`get_param` / `set_param` / `halt` / `resume` / `mode`), and the typed IO inventory (`io` /
//! `io_summary` / `point_list`) with final signatures and non-panicking bodies. Semantic/modelica
//! loaders and device-filtered point lists remain deferred behind typed errors.

mod engine;
mod error;
/// Compile-time PyO3 binding-shape guards (R-API-PY-1..8). A non-test module so a frozen surface
/// drift fails the normal `cargo build`, not only the release-gate test run.
mod guards;
mod io;
mod loading;
mod params;
mod sim;

pub use engine::Engine;
pub use error::{OcError, OcResult};
pub use io::{
    IoClass, IoInventory, IoSummary, PhysicalKind, PointDirection, PointInfo, PointValueType,
    TrendCfg, TrendInterval,
};
pub use loading::{LoadReport, TemplateRef};
pub use params::{ParamAttrs, ParamTable, RunMode};
pub use sim::{
    AssertEvent, AssertLevel, CollectSpec, InputSource, OutputTrace, Outputs, SimMetrics, SimSpec,
    StepReport,
};

/// Re-export of the shared diagnostic type: the element type of [`LoadReport::warnings`], so a
/// binder owns it as `oce_api::Diagnostic`.
pub use oce_diag::Diagnostic;
/// Re-export of the `oce-model` value/IO types the frozen surface is typed in (R-PUB-1: `oce-api`
/// is the single public surface). A binder names `oce_api::Value` / `oce_api::ConnectorId` /
/// `oce_api::ValueType` — never a second direct `oce-model` dependency. These are the engine's value
/// types (`01` §3), explicitly whitelisted by R-PUB-1; no database type is ever re-exported (R-API-8).
pub use oce_model::{ConnectorId, Value, ValueType};
/// Re-export of the store seam DTOs/traits (`08` §11 R-PUB-1). No store-backend-specific type is
/// ever re-exported here.
pub use oce_store;
/// The Ch.13 reverse-flow query type, re-exported from the store seam (R-PUB-1).
pub use oce_store::SemanticQuery;

// Keep a reference to a DomainKey-using path so the re-exported store types are linked even when no
// caller names one (documents that oce-api re-exports the oce-store seam, never store-backend types).
#[doc(hidden)]
pub fn _doc_link_domain_key(k: oce_store::DomainKey) -> oce_store::DomainKey {
    k
}

#[cfg(test)]
mod assert_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod validation_bypass_tests;
