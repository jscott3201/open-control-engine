//! The typed refusals of the blob codec. Plain `Copy` enums carrying static labels, indices, byte
//! offsets and hashes, never formatted text; `Display` exists only under `std`, so a firmware
//! decode path need not link `core::fmt` for its errors.

use super::EngineVersion;

/// Why [`decode()`](super::decode()) refused a blob, or [`encode()`](super::encode()) refused a
/// source.
///
/// Variants are listed in the decoder's refusal order: a blob with several defects reports the
/// first one in this order. The one exception is [`Malformed`](Self::Malformed), listed once
/// although the decoder finds its faults at three points: a short input or a wrong magic before
/// the format, a total length that differs from the input after the engine version, and every
/// structural fault after the flags ([`decode()`](super::decode()) lists the steps). Indices are
/// positions in the blob's own lists (class, block, connection, input, output, schedule
/// position); offsets are absolute byte offsets into the blob.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BlobError {
    /// The input is larger than [`MAX_BLOB_BYTES`](super::MAX_BLOB_BYTES).
    TooLarge {
        /// The input length in bytes.
        length: u64,
        /// The limit in bytes.
        limit: u64,
    },
    /// The bytes do not have the blob's structure (see [`MalformedFault`]).
    Malformed {
        /// Byte offset of the field at which the structure failed.
        offset: u32,
        /// What was wrong at that offset.
        fault: MalformedFault,
    },
    /// The format version is not the one this build supports (older or newer alike).
    UnsupportedFormat {
        /// The version in the blob.
        found: u16,
        /// The only version this build accepts.
        supported: u16,
    },
    /// The encoder's engine version differs from this build's. Checked straight after the format,
    /// so a blob another engine version wrote reports this whatever its length, trailer or body
    /// hold; a corrupted version field reports the version it now spells.
    EngineVersionMismatch {
        /// The version in the blob.
        found: EngineVersion,
        /// This build's version.
        expected: EngineVersion,
    },
    /// The CRC-32C trailer does not match the bytes before it: corruption in flash, transfer or
    /// storage. Not tamper evidence; anyone can recompute it.
    IntegrityMismatch {
        /// The trailer value in the blob.
        stored: u32,
        /// The CRC-32C computed over the bytes before the trailer.
        computed: u32,
    },
    /// The execution profile is not the one this build runs.
    UnsupportedProfile {
        /// The profile in the blob.
        found: u16,
        /// The only profile this build accepts.
        supported: u16,
    },
    /// The reserved header flags are not zero.
    NonzeroFlags {
        /// The flags in the blob.
        flags: u16,
    },
    /// A class path is not registered in this build.
    UnknownClass {
        /// Index into the blob's class list.
        class: u32,
    },
    /// A class's signature hash differs from this build's for the same class path.
    ClassSignatureMismatch {
        /// Index into the blob's class list.
        class: u32,
        /// This build's hash for the class.
        expected: u64,
        /// The hash the blob carries.
        found: u64,
    },
    /// A class in the class list is used by no instance.
    UnusedClass {
        /// Index into the blob's class list.
        class: u32,
    },
    /// An instance carries two parameters with the same name.
    DuplicateParameter {
        /// The block id.
        block: u32,
        /// Index of the second occurrence in the block's parameter list.
        parameter: u32,
    },
    /// A Real parameter is NaN or infinite (load refuses those; a blob never carries one).
    NonFiniteParameter {
        /// The block id.
        block: u32,
        /// Index in the block's parameter list.
        parameter: u32,
    },
    /// An Enum value names a class path this build does not know.
    UnknownEnumClass {
        /// Byte offset of the value.
        offset: u32,
    },
    /// An Enum value's ordinal is outside `1..=members` of its class.
    EnumOrdinalOutOfRange {
        /// Byte offset of the value.
        offset: u32,
        /// The ordinal in the blob.
        ordinal: u32,
    },
    /// A constructed instance disagrees with what the blob carries for it.
    InstanceMismatch {
        /// The block id.
        block: u32,
        /// The field that disagrees.
        field: InstanceField,
    },
    /// A connector id in an instance's port lists is not one of `0..C` exactly once.
    InvalidConnector {
        /// The connector id.
        connector: u32,
        /// What was wrong with it.
        fault: ConnectorFault,
    },
    /// A boundary input record is inconsistent with the model.
    InvalidInput {
        /// Index in the input list.
        input: u32,
        /// What was wrong with it.
        fault: InputFault,
    },
    /// A connection is not a well-typed output-to-input edge between two blocks.
    InvalidConnection {
        /// Index in the connection list.
        connection: u32,
        /// What was wrong with it.
        fault: ConnectionFault,
    },
    /// An input connector has neither a connection driver nor a boundary input.
    UndrivenInput {
        /// The connector id.
        connector: u32,
    },
    /// A boundary output record is inconsistent with the model.
    InvalidOutput {
        /// Index in the output list.
        output: u32,
        /// What was wrong with it.
        fault: OutputFault,
    },
    /// The block order is not a permutation of the blocks, or it violates emit-before under this
    /// build's own `feeds_through`.
    InvalidSchedule {
        /// Position in the schedule list.
        position: u32,
        /// What was wrong at that position.
        fault: ScheduleFault,
    },
    /// The model's state regions exceed [`MAX_STATE_WORDS`](crate::MAX_STATE_WORDS) words, or the
    /// allocator refused the in-budget buffer.
    StateTooLarge {
        /// The first block (id order) at which the budget is crossed.
        block: u32,
        /// Words requested through that block, saturated at `u64::MAX`.
        requested: u64,
    },
    /// A stateful block's freshly initialized state words differ from the CRC the encoder carried.
    InitialStateMismatch {
        /// The block id.
        block: u32,
        /// The CRC-32C the blob carries.
        expected: u32,
        /// The CRC-32C of this build's initial state.
        found: u32,
    },
    /// The encoder cannot represent its source (encoder only).
    Unencodable {
        /// The block, connector, input or output index the fault names (0 when none applies).
        index: u32,
        /// What cannot be encoded.
        fault: EncodeFault,
    },
}

/// The structural defects [`BlobError::Malformed`] distinguishes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MalformedFault {
    /// The input ends inside a field (or is shorter than a header and trailer).
    Truncated,
    /// The first four bytes are not `OCEB`.
    BadMagic,
    /// The header's total length differs from the input length.
    LengthMismatch,
    /// A section id is not the next one in the fixed order 1 to 5.
    SectionId,
    /// A section's declared payload length runs past the trailer.
    SectionLength,
    /// A count is larger than the remaining bytes could hold at the minimum record size.
    CountExceedsInput,
    /// A port list is longer than any registered class can resolve
    /// ([`oce_blocks::MAX_RESOLVED_ARITY`]).
    ArityTooLarge,
    /// A string is not valid UTF-8 (the offset is that of the first invalid byte).
    InvalidUtf8,
    /// A tag byte (value, value type, optional, Boolean, block kind) is not in its table.
    InvalidTag,
    /// A list is out of its canonical order or repeats an entry.
    NotCanonical,
    /// Bytes remain after a section's records, or between the last section and the trailer.
    TrailingBytes,
}

/// The instance field [`BlobError::InstanceMismatch`] names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum InstanceField {
    /// The class index is outside the class list.
    ClassIndex,
    /// The carried kind differs from the constructed block's.
    Kind,
    /// The carried state word count differs from the constructed block's.
    StateLen,
    /// A stateless instance carries a nonzero state CRC.
    StateCrc,
    /// The input arity differs from the constructed block's resolved signature.
    InputArity,
    /// The output arity differs from the constructed block's resolved signature.
    OutputArity,
}

/// What [`BlobError::InvalidConnector`] found wrong with a connector id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConnectorFault {
    /// The id is not below the carried connector count.
    OutOfRange,
    /// The id appears in a second port list.
    Duplicated,
    /// The id appears in no port list.
    Missing,
}

/// What [`BlobError::InvalidConnection`] found wrong with a connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConnectionFault {
    /// An endpoint id is not below the connector count.
    OutOfRange,
    /// The producer is not an output connector.
    ProducerNotOutput,
    /// The consumer is not an input connector.
    ConsumerNotInput,
    /// Producer and consumer carry different port kinds.
    KindMismatch,
    /// Producer and consumer belong to the same block.
    SameBlock,
    /// The consumer is also a boundary input target.
    ConsumerIsInputTarget,
    /// The consumer already has a driver (consumer ids repeat).
    MultipleDrivers,
}

/// What [`BlobError::InvalidInput`] found wrong with a boundary input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum InputFault {
    /// The input's type is Enum, which validation makes unreachable.
    EnumType,
    /// The input has no fan-out target.
    NoTargets,
    /// A target id is not below the connector count.
    TargetOutOfRange,
    /// A target is not an input connector.
    TargetNotInput,
    /// A target's port kind differs from the input's type.
    TargetTypeMismatch,
    /// A target is already the target of another boundary input.
    TargetShared,
    /// A bound's type differs from the input's type, or the type admits no bound.
    BoundTypeMismatch,
    /// A Real bound is NaN or infinite, or the lower bound exceeds the upper bound.
    InvalidBound,
}

/// What [`BlobError::InvalidOutput`] found wrong with a boundary output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum OutputFault {
    /// The source id is not below the connector count.
    SourceOutOfRange,
    /// The source is not an output connector.
    SourceNotOutput,
    /// Another output has the same path.
    DuplicatePath,
}

/// What [`BlobError::InvalidSchedule`] found wrong with the block order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ScheduleFault {
    /// The schedule's length differs from the block count.
    CountMismatch,
    /// A scheduled id is not a block.
    BlockOutOfRange,
    /// A block is scheduled twice.
    Duplicate,
    /// A block is scheduled before the producer of an input it reads at emit time.
    EmitBefore,
}

/// What [`BlobError::Unencodable`] found the encoder cannot represent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum EncodeFault {
    /// A count or length does not fit a `u32`.
    CountOverflow,
    /// The block list and block instances differ in length.
    BlockCountMismatch,
    /// A block or connector id differs from its arena position.
    IdNotDense,
    /// A port names a connector outside the arena.
    ConnectorOutOfRange,
    /// A connector's value type differs from its block's port kind.
    ConnectorTypeMismatch,
    /// An Enum value's class has no canonical descriptor.
    UnknownEnumClass,
    /// A Real parameter is NaN or infinite.
    NonFiniteParameter,
    /// Two inputs share a path.
    DuplicateInputPath,
    /// An input bound is a non-finite Real, or its lower bound exceeds its upper bound.
    InvalidInputBound,
}

#[cfg(feature = "std")]
impl core::fmt::Display for BlobError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TooLarge { length, limit } => {
                write!(f, "blob of {length} bytes exceeds the {limit}-byte limit")
            }
            Self::Malformed { offset, fault } => {
                write!(f, "malformed blob at byte {offset}: {fault:?}")
            }
            Self::UnsupportedFormat { found, supported } => write!(
                f,
                "unsupported blob format version {found}; this build supports only {supported}"
            ),
            Self::EngineVersionMismatch { found, expected } => write!(
                f,
                "blob encoded by engine {}.{}.{}; this build is {}.{}.{}",
                found.major,
                found.minor,
                found.patch,
                expected.major,
                expected.minor,
                expected.patch
            ),
            Self::IntegrityMismatch { stored, computed } => write!(
                f,
                "blob CRC-32C mismatch: trailer {stored:#010x}, computed {computed:#010x}"
            ),
            Self::UnsupportedProfile { found, supported } => write!(
                f,
                "unsupported execution profile {found}; this build runs only {supported}"
            ),
            Self::NonzeroFlags { flags } => {
                write!(f, "reserved blob flags {flags:#06x} are not zero")
            }
            Self::UnknownClass { class } => write!(f, "class {class} is not registered"),
            Self::ClassSignatureMismatch {
                class,
                expected,
                found,
            } => write!(
                f,
                "class {class} signature hash {found:#018x} differs from this build's {expected:#018x}"
            ),
            Self::UnusedClass { class } => write!(f, "class {class} is used by no instance"),
            Self::DuplicateParameter { block, parameter } => {
                write!(f, "block {block} parameter {parameter} repeats a name")
            }
            Self::NonFiniteParameter { block, parameter } => {
                write!(f, "block {block} parameter {parameter} is not finite")
            }
            Self::UnknownEnumClass { offset } => {
                write!(f, "enum value at byte {offset} names an unknown class")
            }
            Self::EnumOrdinalOutOfRange { offset, ordinal } => write!(
                f,
                "enum value at byte {offset} has ordinal {ordinal} outside its class"
            ),
            Self::InstanceMismatch { block, field } => {
                write!(
                    f,
                    "block {block} disagrees with its constructed instance: {field:?}"
                )
            }
            Self::InvalidConnector { connector, fault } => {
                write!(f, "connector {connector}: {fault:?}")
            }
            Self::InvalidInput { input, fault } => write!(f, "input {input}: {fault:?}"),
            Self::InvalidConnection { connection, fault } => {
                write!(f, "connection {connection}: {fault:?}")
            }
            Self::UndrivenInput { connector } => {
                write!(
                    f,
                    "input connector {connector} has no driver and no boundary input"
                )
            }
            Self::InvalidOutput { output, fault } => write!(f, "output {output}: {fault:?}"),
            Self::InvalidSchedule { position, fault } => {
                write!(f, "schedule position {position}: {fault:?}")
            }
            Self::StateTooLarge { block, requested } => write!(
                f,
                "run state of {requested} words through block {block} exceeds the supported maximum"
            ),
            Self::InitialStateMismatch {
                block,
                expected,
                found,
            } => write!(
                f,
                "block {block} initial state CRC {found:#010x} differs from the carried {expected:#010x}"
            ),
            Self::Unencodable { index, fault } => {
                write!(f, "cannot encode (index {index}): {fault:?}")
            }
        }
    }
}

#[cfg(feature = "std")]
impl core::error::Error for BlobError {}
