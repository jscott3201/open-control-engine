//! The canonical content id (GitHub #470, `docs/design/content-id.md`): one identity for a loaded
//! sequence, `oce:canon1:` and the SHA-256 of the `OCECANON` v1 encoding of the loaded flat graph.
//!
//! [`graph`] reduces the model to sorted, name-keyed records, [`encode`] writes them as canonical
//! bytes and [`hash`] names the bytes. The bytes carry no `BlockId`, `ConnectorId`, declaration
//! order, schedule or instance namespace, so a re-serialized, reordered or re-namespaced document
//! keeps its id, while any parameter bit, name, wiring, class or boundary I/O change moves it.

mod encode;
mod graph;
mod hash;

pub use hash::{ContentId, ContentIdParseError};

use crate::{Engine, OcError};

/// Why [`Engine::content_id`] could not mint an id for a loaded model.
///
/// Every variant describes a hand-built model that no public load path produces; a CXF document
/// that loads always has an id. The preconditions (a model loaded, no pending parameter edits)
/// are [`OcError::State`], as for [`Engine::compile_blob`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ContentIdError {
    /// A non-reserved block has no instance IRI, so nothing names it.
    #[error("block {block} has no instance IRI to name it in the canonical graph")]
    #[non_exhaustive]
    UnnamedBlock {
        /// The block's dense index.
        block: u32,
    },
    /// A block's class has no class signature hash in this build's registry.
    #[error("class `{class_path}` is not registered, so it has no class signature hash")]
    #[non_exhaustive]
    UnregisteredClass {
        /// The unregistered class path.
        class_path: String,
    },
    /// An Enum type or value names an enumeration with no descriptor.
    #[error("an enumeration type or value has no descriptor")]
    UnknownEnumClass,
    /// A connector, port or boundary reference does not resolve inside the model, or a reserved
    /// lowering block is wired through an ordinary connection.
    #[error("the model's references are inconsistent")]
    MalformedModel,
    /// A count or length does not fit the encoding's `u32`.
    #[error("the canonical encoding exceeds a u32 count or length")]
    TooLarge,
}

impl Engine {
    /// The canonical content id of the loaded model: `oce:canon1:` and the 64 lowercase hex
    /// digits of the SHA-256 of [`Self::canonical_content_bytes`].
    ///
    /// This is the only place an id is minted. It names the loaded flat graph after flattening,
    /// port binding, grounding and §7.10 unification: boundary inputs and outputs (name, type,
    /// `quantity`, `unit`, `displayUnit`, each input's effective acceptance domain and fan-out),
    /// non-reserved blocks (name, class path, class signature hash, bound parameters sorted by
    /// name) and connections by port index. Instance IRIs enter without their namespace (the IRI
    /// up to its last `#`, `/` or `:`). Authored order, `SemanticPoint` records, `extends`,
    /// comments, the root `@type` and label, internal connector IRIs and attributes, output
    /// bounds, `nominal`, `unbounded`, dense ids and the schedule never enter it.
    ///
    /// Equal ids from one engine build promise the same outputs and per-block state for every
    /// successful frame given the same named inputs, and the same refusals; they do not promise
    /// byte-identical blobs, equal point inventories or interchangeable state snapshots
    /// (`docs/design/content-id.md`, "What equal ids promise"). A pure read: no engine state
    /// changes. Frames executed do not move the id; parameter edits applied by a resume do.
    ///
    /// # Errors
    /// [`OcError::State`] when no model is loaded or parameter edits are pending (the same
    /// preconditions as [`Self::compile_blob`]); [`OcError::ContentId`] for a hand-built model
    /// with an unnamed non-reserved block or another inconsistency no public load produces.
    pub fn content_id(&self) -> Result<ContentId, OcError> {
        let bytes = self.canonical_content_bytes()?;
        Ok(ContentId::from_digest(hash::sha256(&bytes)))
    }

    /// The `OCECANON` v1 canonical bytes [`Self::content_id`] hashes, for a host that recomputes
    /// the SHA-256 itself or inspects what the id covers. The layout is the design note's
    /// encoding table (`docs/design/content-id.md`, "Canonical bytes"); it is versioned with the
    /// id text (`canon1`), so the bytes for one model change only with a `canon2`. Allocates one
    /// `Vec<u8>` for the bytes (plus the sorted records it writes them from), bounded by the loaded
    /// model's size; no engine state changes.
    ///
    /// # Errors
    /// As [`Self::content_id`].
    pub fn canonical_content_bytes(&self) -> Result<Vec<u8>, OcError> {
        self.capture_preconditions()?;
        let graph = graph::canonical_graph(&self.model, &self.io)?;
        Ok(encode::encode(&graph)?)
    }
}
