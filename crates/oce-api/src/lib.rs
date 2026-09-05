#![forbid(unsafe_code)]
//! `oce-api` — the primary embeddable host facade for the Open Control Engine
//! (`08-embeddable-api-and-performance.md`). [`catalog()`] and [`contract_descriptors`] expose
//! independent host metadata. The supported `oce-blocks::catalog()` remains a companion surface. The `open-control-engine` umbrella name is reserved for a
//! future release; nothing is published to crates.io yet.
//!
//! # Posture (binding, FRAME §6)
//!
//! Library-only, synchronous, in-process, `#![forbid(unsafe_code)]`, edition 2024, rust 1.97.1,
//! **no async runtime, no server**. The host owns transport, TLS, authN/Z, multi-tenancy,
//! off-host durability, and metrics export.
//!
//! # The store seam
//!
//! [`Engine`] is generic over an `oce_store::Store`, defaulting to `oce_store_mem::MemStore` so the **default
//! (and only) build has no database** (D-OWNER-1). The library ships no first-party database and no
//! DB-gated feature; a durable/queryable backend is an app-side adapter the host wires behind the
//! `oce-store` port. No store-backend-specific type ever escapes this facade (R-API-8).
//!
//! # Module layout
//!
//! The public surface is split across internal modules and re-exported **flat** here, so every path
//! stays `oce_api::Foo` (R-PUB-1/4; the `cargo public-api` baseline): `engine` (the
//! [`Engine`] handle + load/tick core), `error` ([`OcError`]), `loading` ([`LoadReport`],
//! the successful ingest report), `params` (the live parameter table), `sim` (execution modes
//! + [`Outputs`]), `io` (the typed IO inventory), `watch` (key-selected output reads).
//!
//! The full load → tick → simulate loop works; [`Engine::load_cxf`] runs the end-to-end CXF ingest
//! pipeline (resolve → flatten → validate → BUILD). The frozen public surface (`08` §11.1 R-PUB-5/6)
//! includes `simulate` / `step_realtime`, `set_input` / `get_output` / `watch`, the live parameter table
//! (`get_param` / `set_param` / `halt` / `resume` / `mode`), and the typed IO inventory (`io` /
//! `io_summary` / `point_list`). Executable ingest is CXF only; there is no source Modelica or
//! semantic-template loader. Only `point_list(None)` is supported: device filtering is outside the
//! supported profile and is refused directly with [`OcError::Load`], even with a custom store.

mod catalog;
mod catalog_adapter;
mod catalog_json;
mod catalog_rules;
mod contracts;
mod diagnostics;
mod engine;
mod error;
mod export;
/// Compile-time PyO3 binding-shape guards (R-API-PY-1..8). A non-test module so a frozen surface
/// drift fails the normal `cargo build`, not only the release-gate test run.
mod guards;
mod io;
mod loading;
mod params;
mod projection;
mod sim;
mod stable_hash;
mod state;
mod state_codec;
mod state_diagnostics;
mod state_key_order;
mod state_manifest;
mod state_manifest_codec;
mod state_manifest_validation;
mod state_wire;
mod topology;
mod watch;

pub use catalog::{
    CATALOG_JSON, CATALOG_SCHEMA_REVISION, CatalogDefault, CatalogEntry, CatalogParamDefault,
    CatalogPort, CatalogPortKind, CatalogPortNaming, CatalogValueKind, catalog, catalog_content_id,
    catalog_to_json,
};
pub use catalog_rules::CatalogRule;
pub use contracts::{ContractDescriptor, ContractDomain, contract_descriptors};
pub use diagnostics::{
    DIAGNOSTIC_SCHEMA_REVISION, DiagnosticKey, DiagnosticReceipt, DiagnosticRecord,
    DiagnosticSeverity, DiagnosticStage, DiagnosticSubject, ExportReceipt, LoadReceipt,
    OperationFailure,
};
pub use engine::Engine;
pub use error::{LoadErrorContext, OcError, OcResult};
pub use export::{ContentIdError, ExportReport};
pub use io::{
    IoClass, IoInventory, IoSummary, PhysicalKind, PointDirection, PointInfo, PointValueType,
    TrendCfg, TrendInterval,
};
pub use loading::LoadReport;
pub use params::{ParamAttrs, ParamTable, RunMode};
pub use sim::{
    AssertEvent, AssertLevel, CollectSpec, InputSource, OutputTrace, Outputs, SimMetrics, SimSpec,
    StepReport,
};
pub use state::{EngineCheckpoint, EngineStateError, EngineStateSnapshot};
pub use topology::{DeclaredOutput, PassThroughPair, Topology, TopologyBlock, TopologyConnection};

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
