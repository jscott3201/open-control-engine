#![forbid(unsafe_code)]
//! `oce-store` — **THE SEAM** (FRAME D-OWNER-1). The database-free storage ports the Open Control
//! Engine reaches a durable/queryable backend through (`06-storage-abstraction-and-selene-adapter.md`
//! Part 1).
//!
//! This crate defines three object-safe port traits — [`ModelStore`], [`PointStore`],
//! [`SemanticStore`] — an umbrella [`Store`] trait, the plain-Rust DTOs they exchange, and a
//! typed [`StoreError`]. **No type defined here exposes, embeds, or refers to a database type**
//! (R-SEAM-1): no database type name appears in this crate. The execution core never calls
//! these traits on the tick; they are off-tick ports (except [`PointSnapshot::read_resolved`],
//! the single permitted hot-path read). The DTOs derive `serde::{Serialize, Deserialize}` so any
//! adapter may pick a serde codec; the runtime-value enum is kept small and `Copy`-friendly.
//!
//!
//! The trait surface, DTO shapes, and typed error seam are active. Durable/queryable backends remain
//! app-side adapters behind these ports.

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

/// The kind of element a [`DomainKey`] identifies. Drives node-label selection in adapters but is
/// itself DB-free.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum ElementKind {
    /// A CDL class definition.
    BlockClass,
    /// A CDL block instance (dotted `instance_path`).
    CdlBlock,
    /// One connector instance.
    Point,
    /// A physical/topological device.
    Equipment,
    /// A spatial entity.
    Zone,
    /// A namespace grouping.
    Package,
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

/// Quality/status flags travelling with a runtime point value. This is adapter-owned point state,
/// not the engine-owned continuation bytes returned by `Engine::state_snapshot`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum PointStatus {
    /// Value is current and valid.
    Ok,
    /// Value is stale.
    Stale,
    /// Value is operator-overridden.
    Override,
    /// Value is faulted.
    Fault,
    /// Value has never been written.
    Uninitialized,
}

/// A timestamped, status-tagged point sample written off-tick. The host/engine owns the clock;
/// the seam never invents time (resolves the D7 HLC-ownership item — Part 1 §2).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PointSample {
    /// The runtime value.
    pub value: OcValue,
    /// The quality/status flag.
    pub status: PointStatus,
    /// Wall-clock of the sample in UNIX nanoseconds.
    pub at_unix_nanos: u64,
}

/// Durability class for a batched point write — the Part 1 surface for the D4/D7 tiered policy.
/// The seam expresses *intent*; the adapter maps it to a concrete durability mechanism.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Durability {
    /// Safety-relevant (setpoint/override/command): must be durable-per-commit.
    Critical,
    /// Re-derivable telemetry: may group-commit (a bounded lost-write window is acceptable).
    Telemetry,
}

/// One entry in an off-tick point write batch.
#[derive(Clone, Debug)]
pub struct PointWrite {
    /// The point's domain key.
    pub key: DomainKey,
    /// The sample to write.
    pub sample: PointSample,
    /// The durability intent.
    pub durability: Durability,
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

/// An equipment (or, in v1, zone) subject node.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EquipmentDto {
    /// Stable tag/uuid identity.
    pub key: DomainKey,
    /// Subtype (AHU | VAV | Chiller | Zone | …).
    pub subtype: Option<String>,
    /// 223P/Brick/Haystack class tags as JSON.
    pub tags_json: String,
    /// Optional equipment embedding.
    pub embedding: Option<Vec<f32>>,
}

/// A directed semantic/topology relationship type (the seam carries the type as an enum so
/// adapters never see free-form edge labels).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum RelationKind {
    /// Equipment → Point.
    HasPoint,
    /// CdlBlock → Point.
    Exposes,
    /// CdlBlock → Equipment.
    Controls,
    /// Containment hierarchy.
    Contains,
    /// Equipment → Equipment|Zone media flow (223P).
    Feeds,
    /// Point → Point (CDL `connect()`).
    ConnectedTo,
}

/// A directed relationship DTO.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelationDto {
    /// Source element.
    pub src: DomainKey,
    /// Destination element.
    pub dst: DomainKey,
    /// Relationship type.
    pub kind: RelationKind,
}

/// A raw semantic payload kept for lossless re-emit (§7.17.2).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SemanticPayloadDto {
    /// The subject element.
    pub subject: DomainKey,
    /// The semantic language, e.g. `"Brick 1.3"`.
    pub language: String,
    /// MIME type (turtle / JSON-LD / plain).
    pub mime: String,
    /// Optional version string.
    pub version: Option<String>,
    /// Verbatim payload.
    pub payload: String,
}

/// A point-list row — the result of the §7.7.5 traversal.
#[derive(Clone, Debug)]
pub struct PointListRow {
    /// System/Equipment.
    pub controlled_device: Option<String>,
    /// The point's domain key.
    pub point: DomainKey,
    /// The point name.
    pub name: String,
    /// AI/AO/DI/DO/Mode.
    pub point_type: PointType,
    /// Hardwired flag.
    pub hardwired: bool,
    /// Trend interval.
    pub trend_interval_s: TrendInterval,
    /// Description.
    pub description: Option<String>,
}

/// A scored retrieval hit (`score` is always higher-is-better at the seam).
#[derive(Clone, Debug)]
pub struct RetrievalHit {
    /// The matched element.
    pub key: DomainKey,
    /// Normalized higher-is-better score.
    pub score: f64,
}

/// A required-point signature element for the Ch.13 reverse `match_template` flow.
#[derive(Clone, Debug)]
pub struct TemplatePointReq {
    /// Required physical quantity.
    pub quantity: Option<String>,
    /// Required value type.
    pub value_type: PointValueType,
    /// Required direction.
    pub direction: PointDirection,
}

/// Query selectors for off-tick retrieval (DB-free).
#[derive(Clone, Debug)]
pub enum SemanticQuery {
    /// Exact tag containment (Brick/Haystack JSON fragment).
    TagContains {
        /// JSON containment fragment.
        containment_json: String,
    },
    /// Fuzzy point match by free text (BM25 over name + search_text).
    FuzzyText {
        /// Query text.
        query: String,
        /// Top-k.
        k: usize,
    },
    /// Embedding nearest-neighbour discovery.
    Embedding {
        /// Query embedding.
        query: Vec<f32>,
        /// Top-k.
        k: usize,
    },
}

/// An opaque, adapter-defined fast handle for a pre-resolved point. Carries no DB type inside.
///
/// The public scalar lets an out-of-crate adapter mint and consume its own handles. Validity is
/// limited to that adapter's mapping from resolution through compatible snapshot reads; no durable,
/// global, cross-adapter, cross-reload, or host-control identity is implied.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PointHandle(pub u64);

/// The typed store error model. `Backend`/`Durability`/`Validation` carry a `String` only — a
/// database error type may **never** appear in an `oce-store` signature (Part 1 §3).
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum StoreError {
    /// The requested element was not found.
    #[error("element not found: {0:?}")]
    NotFound(DomainKey),
    /// The requested model is not loaded.
    #[error("model not loaded: {0:?}")]
    ModelNotLoaded(DomainKey),
    /// A durable write failed; a bounded window may be lost.
    #[error("durable write failed (window may be lost): {0}")]
    Durability(String),
    /// A schema/validation check rejected a write at commit time.
    #[error("schema/validation rejected at commit: {0}")]
    Validation(String),
    /// A backend error, flattened to a string (never a DB type).
    #[error("backend error: {0}")]
    Backend(String),
    /// The backend does not support this store operation.
    #[error("operation unsupported by this backend: {0}")]
    Unsupported(&'static str),
    /// The backend does not support the requested retrieval.
    #[error("retrieval unsupported by this backend: {0}")]
    RetrievalUnsupported(&'static str),
}

/// Convenience result alias for store operations.
pub type StoreResult<T> = Result<T, StoreError>;

/// Persists and loads RESOLVED sequence models (D1: the flat `oce-model` is the tick truth; this
/// is the durable projection). Off-tick only.
pub trait ModelStore: Send + Sync {
    /// Persist a fully resolved model as a self-contained snapshot artifact.
    ///
    /// # Errors
    /// Returns [`StoreError`] if the write fails or is rejected at commit.
    fn save_model(&self, model: &ResolvedModel) -> StoreResult<()>;

    /// Load a previously saved model by id (e.g. on engine recovery).
    ///
    /// # Errors
    /// Returns [`StoreError::ModelNotLoaded`] / [`StoreError::Backend`] on failure.
    fn load_model(&self, model_id: &DomainKey) -> StoreResult<ResolvedModel>;

    /// List the ids of all persisted models.
    ///
    /// # Errors
    /// Returns [`StoreError`] on backend failure.
    fn list_models(&self) -> StoreResult<Vec<DomainKey>>;

    /// Drop a model and all its projected elements.
    ///
    /// # Errors
    /// Returns [`StoreError`] on backend failure.
    fn delete_model(&self, model_id: &DomainKey) -> StoreResult<()>;
}

/// Reads/writes runtime point values + status. Resolution and writes are off-tick; a store-backed
/// control tick calls [`PointStore::snapshot`] once and then [`PointSnapshot::read_resolved`] for
/// its resolved inputs. This typed point channel does not carry engine continuation snapshot bytes.
pub trait PointStore: Send + Sync {
    /// Pre-resolve a set of domain keys into opaque fast handles for the hot read path. Called
    /// once at model load.
    ///
    /// # Errors
    /// Returns [`StoreError`] on backend failure.
    fn resolve_points(&self, keys: &[DomainKey]) -> StoreResult<Vec<PointHandle>>;

    /// Acquire a consistent, cheap-to-read snapshot of current point state — one per tick.
    ///
    /// # Errors
    /// Returns [`StoreError`] on backend failure.
    fn snapshot(&self) -> StoreResult<Box<dyn PointSnapshot>>;

    /// Batch-write runtime point samples off-tick, honoring [`Durability`]. Returns the count of
    /// samples accepted into the durable path — for [`Durability::Critical`], the count made durable
    /// on return; for [`Durability::Telemetry`], the count accepted pending the next `flush`/`commit`
    /// (§4.1 "Durability ordering"). The in-memory default accepts all and durabilizes none (§5 R-6),
    /// so it returns the full batch length.
    ///
    /// # Errors
    /// Returns [`StoreError::Durability`] if a critical write is not durably committed.
    fn write_points(&self, batch: &[PointWrite]) -> StoreResult<usize>;
}

/// A consistent, immutable view of point state for the duration of one tick. Read-only, lock-free.
pub trait PointSnapshot: Send {
    /// O(1) read of a pre-resolved point handle — the only call the hot path makes.
    fn read_resolved(&self, handle: PointHandle) -> Option<PointSample>;
    /// Fallback read by key (slower; off-tick / diagnostics).
    fn read_by_key(&self, key: &DomainKey) -> Option<PointSample>;
}

/// Equipment/point/tag graph queries + retrieval (D5). Entirely off-tick.
pub trait SemanticStore: Send + Sync {
    /// Upsert an equipment/zone subject node.
    ///
    /// # Errors
    /// Returns [`StoreError::Unsupported`] if the backend cannot store semantic graph nodes.
    fn upsert_equipment(&self, eq: &EquipmentDto) -> StoreResult<()>;

    /// Add a directed relationship.
    ///
    /// # Errors
    /// Returns [`StoreError::Unsupported`] if the backend cannot store semantic graph edges.
    fn add_relation(&self, rel: &RelationDto) -> StoreResult<()>;

    /// Store a raw semantic payload for lossless re-emit.
    ///
    /// # Errors
    /// Returns [`StoreError::Unsupported`] if the backend cannot store semantic payloads.
    fn put_semantic_payload(&self, p: &SemanticPayloadDto) -> StoreResult<()>;

    /// Fetch all semantic payloads for a subject.
    ///
    /// # Errors
    /// Returns [`StoreError::Unsupported`] if the backend cannot query semantic payloads.
    fn get_semantic_payloads(&self, subject: &DomainKey) -> StoreResult<Vec<SemanticPayloadDto>>;

    /// §7.7.5 point list — a graph traversal (Equipment -HAS_POINT-> Point WHERE in_pointlist).
    ///
    /// # Errors
    /// Returns [`StoreError::Unsupported`] if the backend cannot serve the point-list traversal.
    fn point_list(&self, controlled_device: Option<&str>) -> StoreResult<Vec<PointListRow>>;

    /// Off-tick retrieval (tag containment / fuzzy text / vector).
    ///
    /// # Errors
    /// Returns [`StoreError::RetrievalUnsupported`] if the backend cannot serve the query.
    fn retrieve(&self, q: &SemanticQuery) -> StoreResult<Vec<RetrievalHit>>;

    /// Ch.13 reverse flow: find equipment whose point signature matches a template signature.
    ///
    /// # Errors
    /// Returns [`StoreError::Unsupported`] if the backend cannot serve template matching.
    fn match_template(&self, required_points: &[TemplatePointReq]) -> StoreResult<Vec<DomainKey>>;
}

/// Crash-safety hooks a durable adapter implements; the in-memory default no-ops them (`06` §4.1,
/// D4 / D7). All hooks are **off-tick** — the control tick never calls them — and DB-free (no
/// backend type appears). The library prescribes only this contract + the per-write [`Durability`]
/// intent; *how* durability is achieved (WAL, snapshot cadence, fsync, lost-write window) is wholly
/// the adapter's concern.
///
/// **Durability ordering (normative, `06` §4.1):** `write_points` with [`Durability::Critical`] is
/// self-durabilizing (durable the instant it returns `Ok`); [`Durability::Telemetry`] samples and
/// all model/semantic mutations are accepted on the call but become durable only at the next
/// `flush` (telemetry tier) or `commit` (the full set + atomic-visibility barrier).
pub trait Durable: Send + Sync {
    /// Make every write accepted since the last `commit` durable and atomically visible (the model
    /// writes, the semantic mutations, and any not-yet-flushed `Telemetry` samples). `Critical`
    /// samples are already durable from `write_points`. The in-memory default returns `Ok(())`.
    ///
    /// # Errors
    /// Returns [`StoreError::Durability`] / [`StoreError::Backend`] if the durable commit fails.
    fn commit(&self) -> StoreResult<()>;

    /// Flush only the pending group-committed (`Telemetry`) point writes to stable storage now.
    /// `Critical` writes are already durable on return from `write_points`. The in-memory default
    /// no-ops.
    ///
    /// # Errors
    /// Returns [`StoreError::Durability`] / [`StoreError::Backend`] if the flush fails.
    fn flush(&self) -> StoreResult<()>;

    /// Recover durable state on (re)open: rehydrate persisted models, the `domain_key → handle`
    /// map, and the last durable runtime point state, then truncate/replay any uncommitted tail per
    /// the adapter's policy. Runs once before the first tick. The in-memory default returns `Ok(())`
    /// (there is no persistent state — the live maps are the only state).
    ///
    /// # Errors
    /// Returns [`StoreError::Backend`] if recovery fails.
    fn recover(&self) -> StoreResult<()>;
}

/// The umbrella the engine is generic over. A backend implements all three ports + the durability
/// hooks; the blanket impl makes any type satisfying the parts a `Store` automatically.
pub trait Store: ModelStore + PointStore + SemanticStore + Durable + Send + Sync {}
impl<T> Store for T where T: ModelStore + PointStore + SemanticStore + Durable + Send + Sync {}
