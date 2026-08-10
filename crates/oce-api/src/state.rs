//! Process-local checkpoints and canonical durable engine-state snapshots.

use std::fmt;
use std::sync::Arc;

use oce_blocks::BlockKind;
use oce_model::{Dir, Value, ValueType, enum_descriptor, enum_descriptor_by_path};
use oce_store::Store;

use crate::engine::Engine;
use crate::error::OcError;

pub(crate) const FORMAT_REVISION: u32 = 1;
pub(crate) const EXECUTION_ABI_REVISION: u32 = 1;
pub(crate) const MAX_SNAPSHOT_BYTES: u64 = 64 * 1024 * 1024;

/// A process-local, codec-free engine-state image for deterministic branching.
#[derive(Clone)]
pub struct EngineCheckpoint {
    pub(crate) image: Arc<StateImage>,
}

impl fmt::Debug for EngineCheckpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EngineCheckpoint")
            .field("model_id_prefix", &model_id_prefix(&self.image.model_id))
            .field("model_id_bytes", &self.image.model_id.len())
            .field("connectors", &self.image.values.len())
            .field("state_words", &self.image.words.len())
            .field("time", &f64::from_bits(self.image.state_t))
            .finish_non_exhaustive()
    }
}

/// Canonical durable bytes for continuing a loaded model in another engine process.
#[derive(Clone)]
pub struct EngineStateSnapshot {
    pub(crate) bytes: Arc<[u8]>,
    pub(crate) image: Arc<StateImage>,
}

impl EngineStateSnapshot {
    /// Parse and fully validate one canonical state-snapshot byte stream.
    ///
    /// This validates the 64 MiB cap, header, format revision, integrity trailer, canonical
    /// ordering, manifest self-consistency, and execution fingerprint. Engine compatibility is
    /// checked later by [`Engine::restore_state`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, EngineStateError> {
        let image = crate::state_codec::decode_snapshot(bytes)?;
        Ok(Self {
            bytes: Arc::from(bytes),
            image: Arc::new(image),
        })
    }

    /// Borrow the canonical byte stream.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Return the canonical byte stream as an owned vector.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes.as_ref().to_vec()
    }
}

impl fmt::Debug for EngineStateSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EngineStateSnapshot")
            .field("format_revision", &FORMAT_REVISION)
            .field("model_id_prefix", &model_id_prefix(&self.image.model_id))
            .field("model_id_bytes", &self.image.model_id.len())
            .field("blocks", &self.image.manifest.blocks.len())
            .field("connectors", &self.image.values.len())
            .field("state_words", &self.image.words.len())
            .field("time", &f64::from_bits(self.image.state_t))
            .finish_non_exhaustive()
    }
}

fn model_id_prefix(model_id: &str) -> &str {
    let mut end = model_id.len().min(64);
    while !model_id.is_char_boundary(end) {
        end -= 1;
    }
    &model_id[..end]
}

/// Typed failures from checkpoint capture, snapshot decoding, and state restore.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum EngineStateError {
    /// Capture or restore was requested before a model loaded successfully.
    #[error("no model is loaded")]
    NoLoadedModel,
    /// Capture or restore was requested while tune-at-rest edits were pending.
    #[error("engine has pending parameter edits")]
    PendingParameterEdits,
    /// The loaded model lacks stable state identity or a registered state contract.
    #[error("model is ineligible for state capture at '{subject}': {detail}")]
    IneligibleModel {
        /// First block, connector, enum, or parameter that makes capture ineligible.
        subject: String,
        /// Human-readable reason.
        detail: String,
    },
    /// The encoded snapshot exceeds the fixed resource cap.
    #[error("state snapshot is {actual_bytes} bytes; maximum is {max_bytes}")]
    SnapshotTooLarge {
        /// Actual encoded or supplied byte count.
        actual_bytes: u64,
        /// Maximum accepted byte count.
        max_bytes: u64,
    },
    /// The fixed header names an unsupported wire-format revision.
    #[error("unsupported state-snapshot format revision {revision}")]
    UnsupportedFormat {
        /// Unsupported revision from the byte stream.
        revision: u32,
    },
    /// Bytes violate the structural or canonical encoding contract.
    #[error("malformed or non-canonical state snapshot at byte {offset}: {detail}")]
    MalformedSnapshot {
        /// Byte offset where validation first proves the problem.
        offset: u64,
        /// Human-readable reason.
        detail: String,
    },
    /// The integrity trailer differs from the checksum of the preceding bytes.
    #[error("state-snapshot integrity mismatch: expected {expected}, found {found}")]
    IntegrityMismatch {
        /// Checksum recomputed from the stream.
        expected: u128,
        /// Checksum carried in the trailer.
        found: u128,
    },
    /// The snapshot and target engine describe different executable models.
    #[error("incompatible execution at '{subject}': snapshot={snapshot}, target={target}")]
    IncompatibleExecution {
        /// First named manifest subject that differs.
        subject: String,
        /// Snapshot-side value summary.
        snapshot: String,
        /// Target-side value summary.
        target: String,
    },
    /// A target-bound snapshot was presented on another architecture or operating system.
    #[error(
        "target-bound state snapshot requires {snapshot_arch}/{snapshot_os}, target is {target_arch}/{target_os}"
    )]
    TargetDomainMismatch {
        /// Architecture encoded by the snapshot.
        snapshot_arch: String,
        /// Operating system encoded by the snapshot.
        snapshot_os: String,
        /// Architecture of this build.
        target_arch: String,
        /// Operating system of this build.
        target_os: String,
    },
    /// Durable restore was attempted after the target crossed a mutation boundary.
    #[error("durable restore target has already crossed a mutation boundary")]
    DurableTargetAdvanced,
    /// A block state region violates its class-specific invariant.
    #[error("invalid state for block '{block}': {detail}")]
    InvalidBlockState {
        /// Stable block key rendered for diagnostics.
        block: String,
        /// Human-readable invariant failure.
        detail: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BlockKey {
    Authored(String),
    PassThrough {
        input_path: String,
        output_path: String,
    },
    Dense(u32),
}

impl BlockKey {
    pub(crate) fn subject(&self) -> String {
        match self {
            Self::Authored(value) => value.clone(),
            Self::PassThrough {
                input_path,
                output_path,
            } => format!("{input_path} -> {output_path}"),
            Self::Dense(id) => format!("block#{id}"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum WireDir {
    In,
    Out,
}

impl From<Dir> for WireDir {
    fn from(value: Dir) -> Self {
        match value {
            Dir::In => Self::In,
            Dir::Out => Self::Out,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConnectorKey {
    pub(crate) owner: BlockKey,
    pub(crate) direction: WireDir,
    pub(crate) port_index: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WireValueType {
    Real,
    Integer,
    Boolean,
    String,
    Enum(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WireValue {
    Real(u64),
    Integer(i64),
    Boolean(bool),
    String(String),
    Enum { class_path: String, ordinal: u32 },
}

impl WireValue {
    pub(crate) fn from_value(value: &Value) -> Result<Self, EngineStateError> {
        Ok(match value {
            Value::Real(value) => Self::Real(value.to_bits()),
            Value::Integer(value) => Self::Integer(*value),
            Value::Boolean(value) => Self::Boolean(*value),
            Value::String(value) => Self::String(value.to_string()),
            Value::Enum { class, ordinal } => {
                let descriptor =
                    enum_descriptor(*class).ok_or_else(|| EngineStateError::IneligibleModel {
                        subject: format!("enum-class#{}", class.0),
                        detail: "enum class has no canonical reverse descriptor".into(),
                    })?;
                Self::Enum {
                    class_path: descriptor.class_path.to_owned(),
                    ordinal: *ordinal,
                }
            }
        })
    }

    pub(crate) fn value_type(&self) -> WireValueType {
        match self {
            Self::Real(_) => WireValueType::Real,
            Self::Integer(_) => WireValueType::Integer,
            Self::Boolean(_) => WireValueType::Boolean,
            Self::String(_) => WireValueType::String,
            Self::Enum { class_path, .. } => WireValueType::Enum(class_path.clone()),
        }
    }

    fn to_target_value(&self, target: ValueType) -> Result<Value, String> {
        match (self, target) {
            (Self::Real(bits), ValueType::Real) => Ok(Value::Real(f64::from_bits(*bits))),
            (Self::Integer(value), ValueType::Integer) => Ok(Value::Integer(*value)),
            (Self::Boolean(value), ValueType::Boolean) => Ok(Value::Boolean(*value)),
            (Self::String(value), ValueType::String) => {
                Ok(Value::String(Arc::from(value.as_str())))
            }
            (
                Self::Enum {
                    class_path,
                    ordinal,
                },
                ValueType::Enum(class),
            ) => {
                let descriptor = enum_descriptor_by_path(class_path)
                    .ok_or_else(|| format!("unknown enum class '{class_path}'"))?;
                if descriptor.id != class
                    || *ordinal == 0
                    || *ordinal as usize > descriptor.members.len()
                {
                    return Err(format!(
                        "enum value {class_path}#{ordinal} does not fit target class"
                    ));
                }
                Ok(Value::Enum {
                    class,
                    ordinal: *ordinal,
                })
            }
            _ => Err(format!(
                "snapshot value type {:?} does not match target {target:?}",
                self.value_type()
            )),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EnumManifestEntry {
    pub(crate) class_path: String,
    pub(crate) members: Vec<Arc<str>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BlockManifestEntry {
    pub(crate) key: BlockKey,
    pub(crate) class_path: String,
    pub(crate) kind: BlockKind,
    pub(crate) state_revision: u32,
    pub(crate) state_len: u64,
    pub(crate) params: Vec<(String, WireValue)>,
    pub(crate) inputs: Vec<ConnectorKey>,
    pub(crate) outputs: Vec<ConnectorKey>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConnectorManifestEntry {
    pub(crate) key: ConnectorKey,
    pub(crate) path: String,
    pub(crate) declaration_order: u32,
    pub(crate) value_type: WireValueType,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Portability {
    CrossPlatform,
    TargetBound { arch: String, os: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExecutionManifest {
    pub(crate) portability: Portability,
    pub(crate) enums: Vec<EnumManifestEntry>,
    pub(crate) blocks: Vec<BlockManifestEntry>,
    pub(crate) connectors: Vec<ConnectorManifestEntry>,
    pub(crate) connections: Vec<(ConnectorKey, ConnectorKey)>,
    pub(crate) schedule: Vec<BlockKey>,
    pub(crate) connector_order: Vec<ConnectorKey>,
    pub(crate) driver_of: Vec<(ConnectorKey, ConnectorKey)>,
    pub(crate) state_slots: Vec<(BlockKey, u64, u64)>,
    pub(crate) external_inputs: Vec<ConnectorKey>,
    pub(crate) boundary_outputs: Vec<(String, ConnectorKey)>,
}

#[derive(Clone, Debug)]
pub(crate) struct StateImage {
    pub(crate) execution_revision: u32,
    pub(crate) fingerprint: u128,
    pub(crate) model_id: String,
    pub(crate) state_t: u64,
    pub(crate) prev_t: Option<u64>,
    pub(crate) manifest: ExecutionManifest,
    pub(crate) values: Vec<(ConnectorKey, WireValue)>,
    pub(crate) words: Vec<u64>,
}

impl<S: Store> Engine<S> {
    /// Capture an opaque process-local image that may later rewind a compatible engine.
    ///
    /// # Errors
    /// Returns [`OcError::State`] when no model is loaded, parameter edits are pending, the loaded
    /// state contract is incomplete, the current state is invalid, or the bounded image is too
    /// large. The operation does not call the store or mutate the engine.
    pub fn checkpoint(&self) -> Result<EngineCheckpoint, OcError> {
        self.capture_preconditions()?;
        let built = crate::state_manifest::build_manifest(self, false)?;
        let image = self.capture_image(built)?;
        crate::state_codec::encoded_snapshot_len(&image, true)?;
        Ok(EngineCheckpoint {
            image: Arc::new(image),
        })
    }

    /// Restore a compatible process-local checkpoint, including its absolute model time.
    ///
    /// Unlike durable restore, this operation may rewind a running engine. Validation is complete
    /// before any field changes; a refusal leaves engine and store state untouched.
    pub fn restore_checkpoint(&mut self, checkpoint: &EngineCheckpoint) -> Result<(), OcError> {
        self.restore_preconditions(false)?;
        let prepared = self.prepare_restore(&checkpoint.image, false)?;
        self.commit_restore(prepared);
        Ok(())
    }

    /// Capture canonical durable continuation bytes for the loaded executable model.
    ///
    /// Persistence, authentication, generation fencing, and actuator ownership remain host-owned;
    /// this method performs no store call.
    pub fn state_snapshot(&self) -> Result<EngineStateSnapshot, OcError> {
        self.capture_preconditions()?;
        let built = crate::state_manifest::build_manifest(self, true)?;
        let image = self.capture_image(built)?;
        let bytes = crate::state_codec::encode_snapshot(&image, false)?;
        enforce_size(bytes.len())?;
        Ok(EngineStateSnapshot {
            bytes: Arc::from(bytes),
            image: Arc::new(image),
        })
    }

    /// Continue a durable snapshot in a freshly loaded compatible engine.
    ///
    /// The target must not have crossed an input, tick, simulation, resume, or restore mutation
    /// boundary since its successful load. Validation is atomic and calls no store method.
    pub fn restore_state(&mut self, snapshot: &EngineStateSnapshot) -> Result<(), OcError> {
        self.restore_preconditions(true)?;
        let prepared = self.prepare_restore(&snapshot.image, true)?;
        self.commit_restore(prepared);
        Ok(())
    }

    fn capture_preconditions(&self) -> Result<(), EngineStateError> {
        if !self.loaded {
            return Err(EngineStateError::NoLoadedModel);
        }
        if self.params_dirty {
            return Err(EngineStateError::PendingParameterEdits);
        }
        Ok(())
    }

    fn restore_preconditions(&self, durable: bool) -> Result<(), EngineStateError> {
        self.capture_preconditions()?;
        if durable && !self.durable_restore_ready {
            return Err(EngineStateError::DurableTargetAdvanced);
        }
        Ok(())
    }

    fn capture_image(
        &self,
        built: crate::state_manifest::BuiltManifest,
    ) -> Result<StateImage, EngineStateError> {
        validate_payload(
            self,
            &self.state.values,
            &self.state.words,
            self.state.t,
            self.prev_t,
        )?;
        let mut values = Vec::with_capacity(self.state.values.len());
        for (dense, key) in built.connector_keys_by_dense.iter().enumerate() {
            values.push((
                key.clone(),
                WireValue::from_value(&self.state.values[dense])?,
            ));
        }
        values.sort_by(|left, right| left.0.cmp(&right.0));
        Ok(StateImage {
            execution_revision: EXECUTION_ABI_REVISION,
            fingerprint: built.fingerprint,
            model_id: self.model_id.as_str().to_owned(),
            state_t: self.state.t.to_bits(),
            prev_t: self.prev_t.map(f64::to_bits),
            manifest: built.manifest,
            values,
            words: self.state.words.clone(),
        })
    }

    fn prepare_restore(
        &self,
        image: &StateImage,
        durable: bool,
    ) -> Result<PreparedRestore, EngineStateError> {
        if durable {
            check_target_domain(&image.manifest.portability)?;
        }
        if image.execution_revision != EXECUTION_ABI_REVISION {
            return Err(crate::state_diagnostics::incompatible(
                "execution-state ABI revision",
                &image.execution_revision,
                &EXECUTION_ABI_REVISION,
            ));
        }
        let target = crate::state_manifest::build_manifest(self, durable)?;
        crate::state_manifest::compare_manifests(&image.manifest, &target.manifest)?;
        if image.fingerprint != target.fingerprint {
            return Err(crate::state_diagnostics::incompatible(
                "execution fingerprint",
                &image.fingerprint,
                &target.fingerprint,
            ));
        }
        if image.values.len() != target.connector_keys_by_dense.len() {
            return Err(crate::state_diagnostics::incompatible(
                "connector value count",
                &image.values.len(),
                &target.connector_keys_by_dense.len(),
            ));
        }
        let mut values: Vec<Option<Value>> = vec![None; target.connector_keys_by_dense.len()];
        for (key, wire_value) in &image.values {
            let dense = target
                .dense_by_sorted_key
                .get(key)
                .copied()
                .ok_or_else(|| {
                    crate::state_diagnostics::incompatible_text(
                        &format!("connector {}", key.owner.subject()),
                        "present",
                        "missing",
                    )
                })?;
            let want = self.model.connectors[dense].value_type;
            let value = wire_value.to_target_value(want).map_err(|detail| {
                crate::state_diagnostics::incompatible_text(
                    &format!("connector {}", key.owner.subject()),
                    &detail,
                    &format!("{want:?}"),
                )
            })?;
            if values[dense].replace(value).is_some() {
                return Err(crate::state_diagnostics::incompatible_text(
                    &format!("connector {}", key.owner.subject()),
                    "duplicate value",
                    "one value",
                ));
            }
        }
        let values = values
            .into_iter()
            .enumerate()
            .map(|(dense, value)| {
                value.ok_or_else(|| {
                    crate::state_diagnostics::incompatible_text(
                        &format!("connector#{dense}"),
                        "missing",
                        "present",
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let state_t = f64::from_bits(image.state_t);
        let prev_t = image.prev_t.map(f64::from_bits);
        validate_payload(self, &values, &image.words, state_t, prev_t)?;
        Ok(PreparedRestore {
            values,
            words: image.words.clone(),
            state_t,
            prev_t,
        })
    }

    fn commit_restore(&mut self, prepared: PreparedRestore) {
        self.state.values = prepared.values;
        self.state.words = prepared.words;
        self.state.t = prepared.state_t;
        self.prev_t = prepared.prev_t;
        self.outputs.refresh_from(&self.state);
        self.durable_restore_ready = false;
    }
}

struct PreparedRestore {
    values: Vec<Value>,
    words: Vec<u64>,
    state_t: f64,
    prev_t: Option<f64>,
}

fn validate_payload<S: Store>(
    engine: &Engine<S>,
    values: &[Value],
    words: &[u64],
    state_t: f64,
    prev_t: Option<f64>,
) -> Result<(), EngineStateError> {
    if values.len() != engine.model.connectors.len() {
        return invalid_global("connector value count differs from loaded model");
    }
    for (connector, value) in engine.model.connectors.iter().zip(values) {
        if connector.value_type != value.value_type() {
            return invalid_global("connector value type differs from loaded model");
        }
        if let Value::Enum { class, ordinal } = value {
            let descriptor =
                enum_descriptor(*class).ok_or_else(|| EngineStateError::InvalidBlockState {
                    block: "<connector arena>".into(),
                    detail: format!("enum class {} has no descriptor", class.0),
                })?;
            if *ordinal == 0 || *ordinal as usize > descriptor.members.len() {
                return invalid_global("enum ordinal is outside its declared class");
            }
        }
    }
    if words.len() != engine.state.words.len() {
        return invalid_global("state word count differs from loaded model");
    }
    if !state_t.is_finite() {
        return invalid_global("model time is not finite");
    }
    match prev_t {
        None => {
            let mut expected = vec![0; words.len()];
            for slot in &engine.state.slots {
                let block_index = slot.block.0 as usize;
                engine.blocks[block_index].init_state(
                    &mut expected[slot.offset..slot.offset + slot.len],
                    &engine.model.blocks[block_index].params,
                );
            }
            if expected != words {
                return invalid_global("pre-first-tick words differ from block init_state output");
            }
        }
        Some(prev_t) => {
            if !prev_t.is_finite() || prev_t.to_bits() != state_t.to_bits() {
                return invalid_global(
                    "previous tick time must be finite and bit-equal to model time",
                );
            }
            for slot in &engine.state.slots {
                let block_index = slot.block.0 as usize;
                let block = &engine.blocks[block_index];
                if block.kind() != BlockKind::Stateful || block.state_contract_revision() == 0 {
                    return Err(EngineStateError::IneligibleModel {
                        subject: crate::state_manifest::block_subject(engine, block_index),
                        detail: "stateful block has no registered state-contract revision".into(),
                    });
                }
                let end = slot.offset.checked_add(slot.len).ok_or_else(|| {
                    EngineStateError::InvalidBlockState {
                        block: crate::state_manifest::block_subject(engine, block_index),
                        detail: "state slot offset overflow".into(),
                    }
                })?;
                let region = words.get(slot.offset..end).ok_or_else(|| {
                    EngineStateError::InvalidBlockState {
                        block: crate::state_manifest::block_subject(engine, block_index),
                        detail: "state slot exceeds the word arena".into(),
                    }
                })?;
                block
                    .validate_state(region, state_t, prev_t)
                    .map_err(|detail| EngineStateError::InvalidBlockState {
                        block: crate::state_manifest::block_subject(engine, block_index),
                        detail,
                    })?;
            }
        }
    }
    Ok(())
}

fn invalid_global<T>(detail: &str) -> Result<T, EngineStateError> {
    Err(EngineStateError::InvalidBlockState {
        block: "<engine>".into(),
        detail: detail.into(),
    })
}

fn check_target_domain(portability: &Portability) -> Result<(), EngineStateError> {
    let Portability::TargetBound { arch, os } = portability else {
        return Ok(());
    };
    let target_arch = std::env::consts::ARCH;
    let target_os = std::env::consts::OS;
    if arch == target_arch && os == target_os {
        Ok(())
    } else {
        Err(EngineStateError::TargetDomainMismatch {
            snapshot_arch: crate::state_diagnostics::bounded_text(arch),
            snapshot_os: crate::state_diagnostics::bounded_text(os),
            target_arch: target_arch.into(),
            target_os: target_os.into(),
        })
    }
}

pub(crate) fn enforce_size(actual: usize) -> Result<(), EngineStateError> {
    let actual_bytes = u64::try_from(actual).unwrap_or(u64::MAX);
    if actual_bytes > MAX_SNAPSHOT_BYTES {
        Err(EngineStateError::SnapshotTooLarge {
            actual_bytes,
            max_bytes: MAX_SNAPSHOT_BYTES,
        })
    } else {
        Ok(())
    }
}
