//! The `OCEB` compiled-schedule blob, format version 2 (`docs/design/compiled-schedule-blob.md`,
//! GitHub #396/#400): the byte form of a loaded executable that a `no_std + alloc` target decodes
//! and runs without the CXF importer, the expression evaluator or the validator.
//!
//! A `std` host loads CXF, validates and schedules it, then encodes the result with [`encode()`]:
//! every parameter ground, the block evaluation order frozen and connector numbering unchanged. A
//! target decodes the bytes with [`decode()`], re-instantiates every block from its own registry
//! after checking each class against the host's class signature hash
//! ([`oce_blocks::class_signature_hash`]), validates the schedule under its own `feeds_through`,
//! and allocates the same [`RunState`] the host allocates, word for word.
//!
//! [`Executable`] wraps [`decode()`] and the HostTick v1 frame driver ([`crate::frame`]) into the
//! `no_std` execution entry: construct from bytes, pass one value per input slot, execute one
//! frame, read outputs by slot and the state as raw words.
//!
//! This one module owns the format in both directions (the tag tables, the section layout, the
//! checks), so the encoder and decoder cannot drift. The user-facing contract is
//! `docs/compiled-schedule-blob.md`.
//!
//! # Trust model
//!
//! For **semantics** a blob is trusted: a well-formed but wrong parameter executes as written,
//! because `oce-validate` and its parameter rules are not linked on a target. For **memory safety**
//! a blob is hostile: [`decode()`] never panics, refuses malformed bytes with a typed
//! [`BlobError`], and checks every count against the bytes that remain before allocating for it.
//!
//! # Allocation
//!
//! [`decode()`] allocates what it returns plus bounded scratch that it frees before returning, and
//! nothing per tick. Exactly (pinned for every golden by `oce-api`'s `blob_allocations` test):
//!
//! * returned: the `Model` block, connector and connection arenas; one `Arc<str>` per class (shared
//!   by its instances); per block its input, output and parameter vectors (each only when
//!   non-empty), one `Arc<str>` per parameter name and one per String parameter value; the block
//!   vector and one `Box<dyn Block>` per non-zero-sized block plus whatever its constructor
//!   allocates (as on the host); `Schedule::order` and `driver_of`; the [`RunState`] arrays that
//!   [`allocate_state`](crate::allocate_state) builds; the input and output tables and one target
//!   vector per input (a bound is never a String: those are refused);
//! * scratch, freed before return: one default-parameter block per class, built and dropped by
//!   [`oce_blocks::class_signature_hash`]; the class table; one parameter-name sort buffer sized
//!   once to the largest parameter list; one 8-byte-per-connector ownership table; one
//!   schedule-position table.
//!
//! "When non-empty" applies throughout: an empty vector allocates nothing. `RunState::slots` is
//! pushed one slot at a time by [`allocate_state`](crate::allocate_state), so it grows by the
//! standard library's amortized doubling rather than in one allocation.
//!
//! I/O paths and `SemanticPoint` payloads are borrowed from the input slice, never copied.
//!
//! ## Peak memory against blob length
//!
//! Every allocation is sized from bytes already in hand, so peak memory is linear in the blob
//! length. The structural pass refuses a port list longer than
//! [`oce_blocks::MAX_RESOLVED_ARITY`] before anything is sized by it, and connector ownership is
//! tracked in 8 bytes per id; the 144-byte `Connector` arena is built only after every port list
//! has matched its constructed block. Measured peaks, as multiples of the blob length (pinned by
//! `oce-api`'s `blob_peak_allocation` test, with the bound it asserts):
//!
//! | Shape | Outcome | Peak | Bound |
//! | --- | --- | --- | --- |
//! | One block listing 10⁶ input ids it does not have | refused at its arity check | 2.0× | 8× |
//! | A port list longer than `MAX_RESOLVED_ARITY` | refused by the structural pass | ≈0× | 0.01× |
//! | 10⁵ minimal instances, schedule missing | refused at the schedule | 8.4× | 12× |
//! | One 200,000-input block fed by one boundary input | accepted | 26.6× | 32× |
//!
//! The last row is the densest accepted shape: a connector costs eight encoded bytes there and
//! about 210 bytes decoded (its `Connector`, its value, its driver and its share of the port and
//! target vectors), which is what the decoded executable needs to run, as it does on the host.

use alloc::{boxed::Box, vec::Vec};

use oce_blocks::Block;
use oce_model::{ConnectorId, Model, Value, ValueType};

use crate::{RunState, Schedule};

mod connectors;
mod decode;
mod encode;
mod error;
mod executable;
mod sections;
mod wire;

pub use decode::decode;
pub use encode::encode;
pub use error::{
    BlobError, ConnectionFault, ConnectorFault, EncodeFault, InputFault, InstanceField,
    MalformedFault, OutputFault, ScheduleFault,
};
pub use executable::{Executable, StateWordsRefusal};
pub use wire::crc32c;

/// The four magic bytes every blob starts with: ASCII `OCEB`.
pub const MAGIC: [u8; 4] = *b"OCEB";

/// The one format version this build encodes and decodes. Any change to the layout, a tag table,
/// the class signature record, the CRC or the content-id meaning bumps it; a decoder accepts
/// exactly its own version ([`BlobError::UnsupportedFormat`]).
pub const FORMAT_VERSION: u16 = 2;

/// The execution profile the blob is encoded for: `1` is HostTick v1 (`docs/execution-profile.md`).
pub const EXECUTION_PROFILE: u16 = 1;

/// The largest blob in bytes, header and trailer included: 64 MiB, so every section length and
/// every count fits a `u32`. Equal to the facade's snapshot cap, declared here because that cap is
/// private to `oce-api`.
pub const MAX_BLOB_BYTES: usize = 64 * 1024 * 1024;

/// Bytes in the fixed header: magic, format, profile, engine version, flags, total length and the
/// 32-byte content id.
pub const HEADER_BYTES: usize = 52;

/// Bytes in the CRC-32C trailer.
pub const TRAILER_BYTES: usize = 4;

/// An engine release version, `major.minor.patch`. A blob records its encoder's version and a
/// decoder accepts exactly its own ([`BlobError::EngineVersionMismatch`]): between release tags
/// every build shares one version string, so a blob and the target that runs it come from the
/// same build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EngineVersion {
    /// Major version.
    pub major: u16,
    /// Minor version.
    pub minor: u16,
    /// Patch version.
    pub patch: u16,
}

/// This build's engine version, from the crate's own package version.
pub const ENGINE_VERSION: EngineVersion = EngineVersion {
    major: parse_u16(env!("CARGO_PKG_VERSION_MAJOR")),
    minor: parse_u16(env!("CARGO_PKG_VERSION_MINOR")),
    patch: parse_u16(env!("CARGO_PKG_VERSION_PATCH")),
};

/// Parse a decimal `u16` at compile time; a version component that is not one fails the build.
const fn parse_u16(text: &str) -> u16 {
    let bytes = text.as_bytes();
    assert!(!bytes.is_empty(), "empty version component");
    let mut value: u32 = 0;
    let mut index = 0;
    while index < bytes.len() {
        let digit = bytes[index];
        assert!(digit.is_ascii_digit(), "non-decimal version component");
        value = value * 10 + (digit - b'0') as u32;
        assert!(value <= u16::MAX as u32, "version component exceeds u16");
        index += 1;
    }
    value as u16
}

/// The 32-byte content-id field for a canonical content id: the 32 SHA-256 digest bytes of the
/// facade's `oce:canon1:` id in digest output order, with no padding (format version 2,
/// `docs/design/content-id.md`). `None` (a hand-built model, which has no id) is 32 zero bytes.
/// The decoder carries the field verbatim and never interprets it, so no hash is linked on a
/// `no_std` target. The field's meaning is "the facade's canonical content id at encode time";
/// changing it bumps [`FORMAT_VERSION`] (format 1 held an FNV-1a-128 export tag here).
#[must_use]
pub fn content_id_field(digest: Option<[u8; 32]>) -> [u8; 32] {
    digest.unwrap_or([0u8; 32])
}

/// One executable boundary input: the record the encoder writes and the decoder returns.
///
/// The slot is the record's position in its list; the list is sorted by UTF-8 path bytes.
#[derive(Clone, Debug)]
pub struct BlobInput<'a> {
    /// Canonical absolute identity (`InputDefinition::path` on the host).
    pub path: &'a str,
    /// Exact native type. Never Enum: the validator makes an Enum input unreachable and the
    /// decoder refuses one.
    pub value_type: ValueType,
    /// Effective inclusive lower bound of the acceptance domain, if any.
    pub min: Option<Value>,
    /// Effective inclusive upper bound of the acceptance domain, if any.
    pub max: Option<Value>,
    /// Fan-out target input connectors, at least one. The decoder returns them strictly
    /// increasing; the encoder sorts them.
    pub targets: Vec<ConnectorId>,
    /// The non-computational `SemanticPoint` payload (CDL §7.17), opaque: the decoder bounds it by
    /// its length and never interprets it. The v1 facade encoder writes it empty.
    pub semantic_point: &'a [u8],
}

/// One executable boundary output: the record the encoder writes and the decoder returns.
#[derive(Clone, Debug)]
pub struct BlobOutput<'a> {
    /// The executable boundary output identity.
    pub path: &'a str,
    /// The output connector whose value the boundary exposes.
    pub source: ConnectorId,
    /// The non-computational `SemanticPoint` payload, opaque (see [`BlobInput::semantic_point`]).
    pub semantic_point: &'a [u8],
}

/// Everything [`encode()`] reads: a loaded executable plus the facade-only facts (content id and
/// the boundary I/O table). Every borrowed list must describe the same model.
#[derive(Clone, Copy)]
pub struct BlobSource<'a> {
    /// The flattened model the schedule was compiled from, with dense block and connector ids.
    pub model: &'a Model,
    /// The instantiated blocks, indexed by `BlockId.0`.
    pub blocks: &'a [Box<dyn Block>],
    /// The frozen schedule; only [`Schedule::order`] is carried.
    pub schedule: &'a Schedule,
    /// The 32-byte content-id field ([`content_id_field`]), or `None` for "no content id".
    pub content_id: Option<[u8; 32]>,
    /// The boundary inputs, in any order (the encoder sorts them by path).
    pub inputs: &'a [BlobInput<'a>],
    /// The boundary outputs, in any order (the encoder sorts them by path).
    pub outputs: &'a [BlobOutput<'a>],
}

/// A decoded, checked executable, ready for [`eval_tick`](crate::eval_tick).
///
/// Block and connector ids are the encoder's own, so the [`RunState`] compares word for word with
/// the host's. Fields the execution path never reads hold fixed values: every block's and
/// connector's `decl_order` equals its id, no instance or connector carries an IRI, connectors
/// carry the default attributes of their value type, the model's boundary declarations are empty,
/// and `schedule.connector_order` is empty (only trace tooling reads it).
pub struct Decoded<'a> {
    /// The model arena.
    pub model: Model,
    /// The schedule: the carried block order and the rebuilt `driver_of`.
    pub schedule: Schedule,
    /// One instantiated block per instance, indexed by `BlockId.0`.
    pub blocks: Vec<Box<dyn Block>>,
    /// The freshly allocated run state, at `t = 0`, equal to the host's after load.
    pub state: RunState,
    /// The boundary inputs in slot (path) order.
    pub inputs: Vec<BlobInput<'a>>,
    /// The boundary outputs in slot (path) order.
    pub outputs: Vec<BlobOutput<'a>>,
    /// The content-id field verbatim, or `None` when it is 32 zero bytes (no content id).
    pub content_id: Option<[u8; 32]>,
    /// Whether any instance's class is target-bound
    /// ([`oce_blocks::class_is_target_bound`]): the outputs may depend on the platform's
    /// transcendental math library.
    pub target_bound: bool,
}

impl Decoded<'_> {
    /// Borrow this executable as an encoder source, for a byte-exact re-encode.
    #[must_use]
    pub fn source(&self) -> BlobSource<'_> {
        BlobSource {
            model: &self.model,
            blocks: &self.blocks,
            schedule: &self.schedule,
            content_id: self.content_id,
            inputs: &self.inputs,
            outputs: &self.outputs,
        }
    }
}
