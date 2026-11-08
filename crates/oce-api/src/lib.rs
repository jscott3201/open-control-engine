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
//! # Pure engine, host-owned durability
//!
//! [`Engine`] is a plain struct: plain data in, plain data out. It has no storage backend, no type
//! parameter and no dependency on the storage port, so the **only build has no database**
//! (D-OWNER-1). [`Engine::load_cxf`] returns a [`LoadedModel`] beside the [`LoadReport`]: the
//! resolved-model projection and the IO inventory as owned data. A host that wants durability
//! persists that value itself, through an app-side adapter behind the `oce-store` port; the engine
//! never calls it. No backend-specific type ever escapes this facade (R-API-8).
//!
//! # Module layout
//!
//! The public surface is split across internal modules and re-exported **flat** here, so every path
//! stays `oce_api::Foo` (R-PUB-1/4): `engine` (the
//! [`Engine`] handle + load/core), `error` ([`OcError`]), `loading` ([`LoadReport`],
//! [`LoadedModel`]),
//! `params` (the live parameter table), `frame` (complete typed execution), `io` (the typed IO
//! inventory), and `observations` / `watch` (latest-state, non-receipt output inspection).
//!
//! [`Engine::load_cxf`] runs the end-to-end CXF ingest pipeline (resolve → validate → BUILD).
//! Execution is exclusively [`Engine::prepare_frame`] followed by consuming
//! [`Engine::execute_frame`]. [`CompletedFrame`] is the immutable committed boundary-output
//! receipt. The facade also provides latest-state `get_output` / `watch`, the live parameter table
//! (`get_param` / `set_param` / `halt` / `resume` / `mode`), and the typed IO inventory (`io` /
//! `io_summary` / `point_list`). Executable ingest is CXF only; there is no source Modelica or
//! semantic-template loader. Only `point_list(None)` is supported: device filtering is outside the
//! supported profile and is refused directly with [`OcError::Load`].

mod admission;
mod catalog;
mod catalog_adapter;
mod catalog_json;
mod catalog_rules;
mod compatibility;
mod contracts;
mod diagnostics;
mod engine;
mod error;
mod export;
mod frame;
mod frame_inputs;
/// Compile-time PyO3 binding-shape guards (R-API-PY-1..8). A non-test module so a frozen surface
/// drift fails the normal `cargo build`, not only the release-gate test run.
mod guards;
mod input_policy;
mod io;
mod loading;
mod observations;
mod params;
mod projection;
mod replay;
mod replay_codec;
mod replay_descriptor;
mod replay_wire;
mod stable_hash;
mod state;
mod state_codec;
mod state_diagnostics;
mod state_io;
mod state_key_order;
mod state_manifest;
mod state_manifest_codec;
mod state_manifest_validation;
mod state_snapshot;
mod state_wire;
mod topology;
mod watch;

pub use admission::MAX_CXF_BYTES;
pub use catalog::{
    CATALOG_JSON, CATALOG_SCHEMA_REVISION, CatalogDefault, CatalogEntry, CatalogParamDefault,
    CatalogPort, CatalogPortKind, CatalogPortNaming, CatalogValueKind, catalog, catalog_content_id,
    catalog_to_json,
};
pub use catalog_rules::CatalogRule;
pub use compatibility::{
    CatalogContentId, CompatibilityDescriptor, CompatibilityMismatch, CompleteExportContentId,
};
pub use contracts::{ContractDescriptor, ContractDomain, contract_descriptors};
pub use diagnostics::{
    DIAGNOSTIC_SCHEMA_REVISION, DiagnosticKey, DiagnosticReceipt, DiagnosticRecord,
    DiagnosticSeverity, DiagnosticStage, DiagnosticSubject, ExportReceipt, LoadReceipt,
    OperationFailure,
};
pub use engine::Engine;
pub use error::{LoadErrorContext, OcError, OcResult};
pub use export::{ContentIdError, ExportReport};
pub use frame::{CompletedFrame, MAX_FRAME_TIME_MAGNITUDE, PreparedInputFrame};
pub use frame_inputs::InputDefinition;
pub use input_policy::NonFiniteInputPolicy;
pub use io::{
    IoClass, IoInventory, IoSummary, PhysicalKind, PointDirection, PointInfo, PointValueType,
    TrendCfg, TrendInterval,
};
pub use loading::{LoadReport, LoadedModel};
pub use observations::{AssertEvent, AssertLevel};
pub use params::{ParamAttrs, ParamTable, RunMode};
pub use replay::{MAX_REPLAY_BYTES, ReplayContentId, ReplayError, ReplayExactness, ReplayRecord};
pub use state::{EngineCheckpoint, EngineStateError, EngineStateSnapshot};
pub use state_snapshot::StatePortability;
pub use topology::{DeclaredOutput, PassThroughPair, Topology, TopologyBlock, TopologyConnection};

/// Re-export of the shared diagnostic type: the element type of [`LoadReport::warnings`], so a
/// binder owns it as `oce_api::Diagnostic`.
pub use oce_diag::Diagnostic;
/// Re-export of the plain-data resolved-model DTOs carried by [`LoadedModel`] (`08` §11 R-PUB-1).
/// They are defined in `oce-model` so that this facade and the storage port share them without
/// depending on each other; no storage trait or backend type is ever re-exported here.
pub use oce_model::resolved_model;
/// Flat re-exports of the two resolved-model names a host meets first: the model identity carried
/// by [`LoadReport::model_id`] and the projection carried by [`LoadedModel::model`].
pub use oce_model::resolved_model::{DomainKey, ResolvedModel};
/// Re-export of the `oce-model` value/IO types the frozen surface is typed in (R-PUB-1: `oce-api`
/// is the single public surface). A binder names `oce_api::Value` / `oce_api::ConnectorId` /
/// `oce_api::ValueType` — never a second direct `oce-model` dependency. These are the engine's value
/// types (`01` §3), explicitly whitelisted by R-PUB-1; no database type is ever re-exported (R-API-8).
pub use oce_model::{ConnectorId, Value, ValueType};

#[cfg(test)]
mod assert_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod validation_bypass_tests;
