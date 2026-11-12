#![forbid(unsafe_code)]
//! `oce-api` — the primary embeddable host facade for the Open Control Engine
//! (`08-embeddable-api-and-performance.md`). [`catalog()`] and [`contract_descriptors`] expose
//! independent host metadata; [`class_ports()`] and [`class_metadata()`] add the resolved port names,
//! class descriptions and settable flags an authoring tool needs, and [`enumerations()`] the
//! enumeration and Integer constant-package table, outside the revision-1 catalog. The supported `oce-blocks::catalog()` remains a companion surface. The `open-control-engine` umbrella name is reserved for a
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
//! [`Engine::execute_frame`], or its slot-indexed twin [`Engine::execute_slots`] over a
//! [`FrameSlots`] map resolved once by [`Engine::frame_slots`]; both run the one core frame driver.
//! [`CompletedFrame`] is the immutable committed boundary-output receipt; [`SlotFrame`] borrows the
//! same outputs from the engine. The facade also provides latest-state `get_output` / `watch`, the live parameter table
//! (`get_param` / `set_param` / `halt` / `resume` / `mode`), and the typed IO inventory (`io` /
//! `io_summary` / `point_list`). Executable ingest is CXF only; there is no source Modelica or
//! semantic-template loader. Only `point_list(None)` is supported: device filtering is outside the
//! supported profile and is refused directly with [`OcError::Load`].

mod admission;
mod blob;
mod catalog;
mod catalog_adapter;
mod catalog_json;
mod catalog_rules;
mod class_ports;
mod compatibility;
mod content_id;
mod contracts;
mod diagnostics;
mod engine;
mod enumerations;
mod error;
mod export;
mod frame;
mod frame_inputs;
mod frame_slots;
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
pub use blob::CompiledBlob;
pub use catalog::{
    CATALOG_JSON, CATALOG_SCHEMA_REVISION, CatalogDefault, CatalogEntry, CatalogParamDefault,
    CatalogPort, CatalogPortKind, CatalogPortNaming, CatalogValueKind, catalog, catalog_content_id,
    catalog_to_json,
};
pub use catalog_rules::CatalogRule;
pub use class_ports::{
    ClassMetadata, ClassParameter, ClassPort, ClassPorts, ClassPortsError, PARAM_COL_PLACEHOLDER,
    PARAM_INDEX_PLACEHOLDER, PARAM_ROW_PLACEHOLDER, class_metadata, class_ports,
    param_element_name,
};
pub use compatibility::{CatalogContentId, CompatibilityDescriptor, CompatibilityMismatch};
pub use content_id::{ContentId, ContentIdError, ContentIdParseError};
pub use contracts::{ContractDescriptor, ContractDomain, contract_descriptors};
pub use diagnostics::{
    DIAGNOSTIC_SCHEMA_REVISION, DiagnosticKey, DiagnosticReceipt, DiagnosticRecord,
    DiagnosticSeverity, DiagnosticStage, DiagnosticSubject, ExportReceipt, LoadReceipt,
    OperationFailure,
};
pub use engine::Engine;
pub use enumerations::{
    EnumClassId, EnumerationClass, EnumerationKind, MemberValue, enumeration, enumeration_by_id,
    enumeration_class_path, enumeration_member_value, enumeration_types, enumerations,
};
pub use error::{LoadErrorContext, OcError, OcResult};
pub use export::{ExportIncomplete, ExportReport};
pub use frame::{CompletedFrame, PreparedInputFrame};
pub use frame_inputs::InputDefinition;
pub use frame_slots::{FrameSlots, SlotFrame};
// The HostTick v1 time limit and the non-finite input policy are the core frame driver's own
// (`oce_graph::frame`), re-exported so the facade names no second copy.
pub use io::{
    IoClass, IoInventory, IoSummary, PhysicalKind, PointDirection, PointInfo, PointValueType,
    TrendCfg, TrendInterval,
};
pub use loading::{LoadReport, LoadedModel};
pub use observations::{AssertEvent, AssertLevel};
/// This build's engine version: the major, minor and patch of the workspace version that the core
/// crates and `oce-api` share (`workspace.package.version`). A pre-release or build suffix is
/// dropped, so `0.5.0-rc.1` and `0.5.0` are the same engine version.
///
/// Every compiled-schedule blob ([`CompiledBlob`]) and state snapshot ([`EngineStateSnapshot`])
/// this build writes carries this value in its header. Each decoder accepts only this value: a blob
/// or snapshot with any other version is refused with [`BlobError::EngineVersionMismatch`] or
/// [`EngineStateError::EngineVersionMismatch`], whose `expected` field is this constant. A host
/// that stores either artefact can record this value's three fields beside it. That tells, without
/// parsing a header, an error or a version string, which stored bytes this build will refuse by
/// version. A match predicts only that check: the integrity, fingerprint and restore checks can
/// still refuse the bytes.
///
/// It is a release version, not a build identity: two builds of one version, from different
/// compilers, features or targets, carry the same value. It is also not the compatibility
/// descriptor's `oce-api-version` ([`CompatibilityDescriptor::oce_api_version`]), which is the
/// full package version string, suffix included.
pub use oce_graph::blob::ENGINE_VERSION;
/// Re-export of the compiled-schedule blob refusal type and its fault vocabularies, so a host
/// matches [`OcError::Blob`] without a direct `oce-graph` dependency (GitHub #400).
/// [`EngineVersion`] is also what [`EngineStateError::EngineVersionMismatch`] reports: a blob and a
/// state snapshot record the same version and each is accepted only by the engine that wrote it.
pub use oce_graph::blob::{
    BlobError, ConnectionFault, ConnectorFault, EncodeFault, EngineVersion, InputFault,
    InstanceField, MalformedFault, OutputFault, ScheduleFault,
};
pub use oce_graph::frame::{MAX_FRAME_TIME_MAGNITUDE, NonFiniteInputPolicy};
pub use params::{ParamAttrs, ParamTable, RunMode};
pub use replay::{MAX_REPLAY_BYTES, ReplayContentId, ReplayError, ReplayExactness, ReplayRecord};
pub use state::{EngineCheckpoint, EngineStateError, EngineStateSnapshot};
pub use state_snapshot::StatePortability;
pub use topology::{DeclaredOutput, PassThroughPair, Topology, TopologyBlock, TopologyConnection};

/// Re-export of the CXF reader's invisible-character class: whether a character is a Unicode
/// format (`Cf`) or control (`Cc`) character, which the loader refuses in a typing token and in a
/// `unit`, `quantity` or `displayUnit` term (GitHub #641, issue #629). It is here so a second CXF
/// reader above the facade, the authoring importer, refuses exactly the characters the loader
/// does, from the same table, and never a copy of it.
pub use oce_cxf::is_format_or_control;

/// Re-export of the diagnostic code vocabulary: the type of [`Diagnostic::code`], so a host
/// matches a refusal's code (for example [`ClassPortsError::InvalidParameters`]) as
/// `oce_api::DiagCode` without a direct `oce-diag` dependency. `DiagCode::as_str` is the stable
/// spelling.
pub use oce_diag::DiagCode;
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
