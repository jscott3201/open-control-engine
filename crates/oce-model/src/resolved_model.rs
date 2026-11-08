//! Plain-data projection of a loaded model: the resolved-model DTOs a host persists.
//!
//! These types are the durable, serde-serializable view of the flat [`crate::ModelGraph`] that the
//! engine facade computes at load and returns to the host. They live in the core model crate,
//! not in the storage port or the facade, because of the dependency direction: the facade
//! (`oce-api`) must never depend on the storage port (`oce-store`), and the storage port must
//! never depend on the facade. Both depend on this crate, so `oce-api` returns these values from
//! its load entry points and `oce-store` re-exports them unchanged as the DTOs its ports take.
//! Nothing here refers to a database type, an adapter handle or an `Arc<dyn …>`; the engine's
//! tick never reads them.

use alloc::{boxed::Box, string::String, vec::Vec};
use serde::{Deserialize, Serialize};

/// A stable semantic domain key for any storable element — the IRI/dotted-path identity from
/// CDL/CXF (FRAME §2). It is database-free and **never** carries an adapter handle (Part 1 §1).
#[derive(Clone, Default, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct DomainKey(pub Box<str>);

impl DomainKey {
    /// Construct a domain key from any string-like value.
    pub fn new(s: impl Into<Box<str>>) -> Self {
        Self(s.into())
    }
    /// Borrow the key as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The runtime value of a point or the ground value of a parameter — the store-facing form of the
/// `oce-model` value lattice. DB-free; adapters map it onto their own value union (Part 1 §2).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum OcValue {
    /// Boolean.
    Bool(bool),
    /// Integer.
    Int(i64),
    /// Real (IEEE-754 double).
    Real(f64),
    /// Exact-decimal carrier for setpoints that must not suffer binary-float rounding (stored as
    /// a normalized decimal string at the seam).
    Decimal(String),
    /// An enumeration literal, by class IRI + literal name.
    Enum {
        /// The enumeration class IRI.
        type_iri: String,
        /// The literal name.
        literal: String,
    },
    /// A string value.
    String(String),
}

/// Direction of a point.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum PointDirection {
    /// Input.
    In,
    /// Output.
    Out,
}

/// The signal value type of a point.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum PointValueType {
    /// Real.
    Real,
    /// Integer.
    Int,
    /// Boolean.
    Bool,
}

/// The point type (§7.7.5; `Mode` is an open-control extension).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum PointType {
    /// Analog input.
    Ai,
    /// Analog output.
    Ao,
    /// Digital input.
    Di,
    /// Digital output.
    Do,
    /// Operating-mode signal.
    Mode,
}

/// Trend interval (§7.11). `OnChange` is the `interval=0` COV sentinel — never coerced to a
/// periodic interval (Part 1 §2.1).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum TrendInterval {
    /// Record on every change (COV).
    OnChange,
    /// Record every N seconds.
    EverySeconds(u32),
}

/// The kind of a resolved block class.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum BlockKind {
    /// Built-in elementary block.
    Elementary,
    /// User-defined composite block.
    Composite,
    /// Extension / FMI block (surfaced as an unresolved external in v1).
    Extension,
}

/// A resolved block-class DTO.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockClassDto {
    /// The class IRI (UNIQUE identity).
    pub class_iri: String,
    /// Elementary / composite / extension.
    pub kind: BlockKind,
    /// Whether this is a library class.
    pub is_library: bool,
    /// Opaque class shape (JSON); adapters may index it.
    pub shape_json: String,
}

/// A resolved block-instance DTO. Parameters are typed properties on the block (D5).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockInstanceDto {
    /// The `instance_path` identity.
    pub key: DomainKey,
    /// The instantiated class IRI.
    pub class_iri: String,
    /// Ground parameter values.
    pub params: Vec<(String, OcValue)>,
    /// Long-tail CDL config blob (JSON).
    pub config_json: String,
}

/// A resolved point (connector) DTO carrying effective (post-propagation) metadata, resolved
/// once at ingest (§7.7.5/§7.11/§7.17.1).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PointDto {
    /// The connector dotted-path identity.
    pub key: DomainKey,
    /// The owning block (EXPOSES owner).
    pub owner_block: DomainKey,
    /// Input or output.
    pub direction: PointDirection,
    /// Real/Int/Bool.
    pub value_type: PointValueType,
    /// AI/AO/DI/DO/Mode.
    pub point_type: PointType,
    /// §7.11 `connection(hardwired)`.
    pub hardwired: bool,
    /// §7.11 trend interval (0 = on-change sentinel).
    pub trend_interval_s: TrendInterval,
    /// §7.11 trend enable.
    pub trend_enable: bool,
    /// §7.17.1 physical quantity.
    pub quantity: Option<String>,
    /// SI computation unit.
    pub unit: Option<String>,
    /// Presentation-only display unit (never affects computation).
    pub display_unit: Option<String>,
    /// Human-readable description.
    pub description: Option<String>,
    /// Effective §7.7.5 point-list membership.
    pub in_pointlist: bool,
    /// §7.7.5 controlled device.
    pub controlled_device: Option<String>,
    /// Write-time alias-normalized text for fuzzy match.
    pub search_text: Option<String>,
    /// Brick/Haystack marker-tag set as JSON.
    pub tags_json: Option<String>,
    /// Optional point-description embedding.
    pub embedding: Option<Vec<f32>>,
}

/// A connection (output→input) DTO.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConnectionDto {
    /// The driving output connector.
    pub from_point: DomainKey,
    /// The driven input connector.
    pub to_point: DomainKey,
}

/// A fully resolved sequence model ready to schedule — the durable projection of the flat
/// `oce-model` graph (D1). Self-contained and serde-serializable so any adapter can persist it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResolvedModel {
    /// Identity of this loaded sequence model.
    pub model_id: DomainKey,
    /// open-control's own model-format version.
    pub schema_rev: u32,
    /// Block classes.
    pub classes: Vec<BlockClassDto>,
    /// Block instances.
    pub blocks: Vec<BlockInstanceDto>,
    /// Points (connectors).
    pub points: Vec<PointDto>,
    /// Connections.
    pub connections: Vec<ConnectionDto>,
    /// `(parent CdlBlock, child CdlBlock)` containment pairs.
    pub containment: Vec<(DomainKey, DomainKey)>,
}
