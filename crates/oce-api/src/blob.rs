//! Encoding the loaded executable as an `OCEB` compiled-schedule blob (GitHub #400).
//!
//! The byte format, its tag tables and the decoder live in `oce_graph::blob`; this module adds the
//! facade-only facts the format carries — the content id, the boundary input definitions with their
//! fan-out targets, and the executable boundary output identities — and checks its own output by
//! decoding it before returning. See `docs/compiled-schedule-blob.md`.

use oce_graph::blob::{self, BlobInput, BlobOutput, BlobSource, content_id_field};

use crate::{Engine, OcError};

/// A compiled-schedule blob for the loaded executable, and the content id it records.
///
/// `bytes` is the canonical `OCEB` format-1 encoding: identical for one loaded executable on every
/// architecture and codegen profile, decoded by `oce_graph::blob::decode` on a `no_std + alloc`
/// target built from the same engine version. It carries no run state, no host or architecture
/// field and no signature; authenticating and transporting it is the host's job.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CompiledBlob {
    /// The canonical blob bytes, header to CRC-32C trailer.
    pub bytes: Vec<u8>,
    /// The facade content id the blob records, `cxf:fnv1a128:<32 hex>` exactly as
    /// [`crate::ExportReport::content_id_complete`] mints it, or `None` when the export is
    /// partial (any block deferred from the emitted document) or refused: the blob then records 32
    /// zero bytes, meaning "no content id".
    pub content_id: Option<String>,
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
    /// [`OcError::Blob`] when the executable cannot be encoded (for example two boundary outputs
    /// with one path, or a non-finite parameter) or its encoding does not decode.
    pub fn compile_blob(&self) -> Result<CompiledBlob, OcError> {
        self.capture_preconditions()?;
        let content_hash = self
            .export_cxf()
            .ok()
            .and_then(|report| report.complete_content_hash());

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
            content_id: content_hash.map(|hash| content_id_field(Some(hash))),
            inputs: &inputs,
            outputs: &outputs,
        })?;
        blob::decode(&bytes)?;
        Ok(CompiledBlob {
            bytes,
            content_id: content_hash.map(|hash| format!("cxf:fnv1a128:{hash:032x}")),
        })
    }
}
