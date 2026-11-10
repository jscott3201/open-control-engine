//! Encoding the loaded executable as an `OCEB` compiled-schedule blob (GitHub #400).
//!
//! The byte format, its tag tables and the decoder live in `oce_graph::blob`; this module adds the
//! facade-only facts the format carries — the content id, the boundary input definitions with their
//! fan-out targets, and the executable boundary output identities — and checks its own output by
//! decoding it before returning. See `docs/compiled-schedule-blob.md`.

use oce_graph::blob::{self, BlobInput, BlobOutput, BlobSource, content_id_field};

use crate::{ContentId, ContentIdError, Engine, OcError};

/// A compiled-schedule blob for the loaded executable, and the content id it records.
///
/// `bytes` is the canonical `OCEB` encoding: identical for one loaded executable on every
/// architecture and codegen profile, decoded by `oce_graph::blob::decode` on a `no_std + alloc`
/// target built from the same engine version. It carries no run state, no host or architecture
/// field and no signature; authenticating and transporting it is the host's job.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CompiledBlob {
    /// The canonical blob bytes, header to CRC-32C trailer.
    pub bytes: Vec<u8>,
    /// The canonical content id the blob records, exactly as [`Engine::content_id`] mints it: its
    /// 32 SHA-256 digest bytes fill the header's content-id field. Present for every loaded CXF
    /// document, partial export or not; `None` only for a hand-built model with no id, which the
    /// blob records as 32 zero bytes. Any other failure to mint the id refuses the blob.
    pub content_id: Option<ContentId>,
}

impl Engine {
    /// Encode the loaded executable as a compiled-schedule blob.
    ///
    /// The blob carries every class with its signature hash, every block instance with its ground
    /// parameters in table order, the frozen block order, the connections and the boundary I/O
    /// table, in one canonical byte order. Run state is not carried: the decoder allocates the same
    /// initial state and checks it against per-block CRCs. Frames already executed do not change
    /// the bytes; parameter edits applied by a resume do.
    ///
    /// Before returning, the bytes are decoded with `oce_graph::blob::decode`, so every success is
    /// a blob this build accepts. No engine state changes and no storage is touched.
    ///
    /// # Errors
    /// [`OcError::State`] when no model is loaded or parameter edits are pending;
    /// [`OcError::ContentId`] when the canonical content id cannot be minted for any reason other
    /// than an unnamed block of a hand-built model (which no CXF load produces, and which records
    /// "no content id"): a blob from CXF always carries its id;
    /// [`OcError::Blob`] when the executable cannot be encoded (for example two boundary outputs
    /// with one path, or a non-finite parameter) or its encoding does not decode.
    pub fn compile_blob(&self) -> Result<CompiledBlob, OcError> {
        self.capture_preconditions()?;
        // Only a hand-built model, with an unnamed block, has no id; no CXF load produces one. Any
        // other minting failure is an encoder defect and refuses the blob, so a blob loaded from
        // CXF never ships without its id (the registry fetches by it).
        let content_id = match self.content_id() {
            Ok(id) => Some(id),
            Err(OcError::ContentId(ContentIdError::UnnamedBlock { .. })) => None,
            Err(error) => return Err(error),
        };

        let inputs: Vec<BlobInput<'_>> = self
            .io
            .frame_definitions
            .iter()
            .map(|definition| BlobInput {
                path: &definition.path,
                value_type: definition.value_type,
                min: definition.min.clone(),
                max: definition.max.clone(),
                targets: self
                    .io
                    .input_targets(&definition.path)
                    .map(<[_]>::to_vec)
                    .unwrap_or_default(),
                semantic_point: &[],
            })
            .collect();
        let outputs: Vec<BlobOutput<'_>> = self
            .io
            .frame_outputs
            .iter()
            .map(|(path, source)| BlobOutput {
                path,
                source: *source,
                semantic_point: &[],
            })
            .collect();
        let bytes = blob::encode(&BlobSource {
            model: &self.model,
            blocks: &self.blocks,
            schedule: &self.schedule,
            content_id: content_id
                .as_ref()
                .map(|id| content_id_field(Some(*id.digest()))),
            inputs: &inputs,
            outputs: &outputs,
        })?;
        blob::decode(&bytes)?;
        Ok(CompiledBlob { bytes, content_id })
    }

    /// The loaded executable's raw state words, borrowed: every stateful block's words in the
    /// executable's own layout, with no portability tag, manifest or checksum.
    ///
    /// The layout is the one `oce_graph::blob::Executable::state_words` returns for the blob
    /// [`Self::compile_blob`] encodes from this engine, so a host or a verifier compares the two
    /// word for word after the same frames (the blob round trip, GitHub #403). It is a verification
    /// view, not a continuation format: the words are not portable across engine versions or
    /// models, and nothing restores them through the facade. Use [`Self::state_snapshot`] or
    /// [`Self::checkpoint`] to continue state. Empty before a model is loaded and for a model with
    /// no stateful block. A pure read: no engine state changes.
    #[must_use]
    pub fn state_words(&self) -> &[u64] {
        &self.state.words
    }
}
