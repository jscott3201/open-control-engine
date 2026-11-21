//! Reading what an `OCEB` compiled-schedule blob records about itself (issue #448).
//!
//! The reader is `oce_graph::blob::inspect`, the decoder's own framing and structural pass; this
//! module owns the plain-data result a host or a leverage crate holds, so a blob comparison
//! (`docs/design/diff.md`, decision 15) needs no direct `oce-graph` dependency. See
//! `docs/compiled-schedule-blob.md`, "Inspecting a blob".

use oce_graph::blob;

use crate::{ContentId, EngineVersion, OcError};

/// What one compiled-schedule blob records about itself, as [`inspect_blob`] read it: the header
/// facts, the class table, one record per block instance and the boundary I/O paths.
///
/// It is the material for comparing two blobs as same or different: by content id, by class path
/// and class signature hash, and by each instance's initial-state CRC. Every field is the blob's
/// own statement except [`target_bound`](Self::target_bound), which is read from this build's
/// registry. **A summary is not acceptance:** nothing here was compared with what this build would
/// construct, so a blob with a summary may still be refused by `oce_graph::blob::decode` (a class
/// signature hash that is not this build's, an initial-state CRC that is not the allocated
/// state's, a parameter, connector, connection or schedule fault).
///
/// # Invariants
/// For every value [`inspect_blob`] returns: `format_version` is 1 and `engine_version` is
/// [`ENGINE_VERSION`](crate::ENGINE_VERSION) (a blob of any other is refused); `classes` is
/// strictly increasing by UTF-8 path bytes and every path is a class this build registers;
/// `instances[i].block` is `i` and `instances[i].class` indexes `classes`; `inputs` is strictly
/// increasing and `outputs` non-decreasing by UTF-8 path bytes, which is the slot order of each.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct BlobSummary {
    /// The format version in the header: 1, the one format this build reads.
    pub format_version: u16,
    /// The encoder's engine version in the header: this build's, because a blob another engine
    /// version wrote is refused.
    pub engine_version: EngineVersion,
    /// The canonical content id the blob records: the 32 digest bytes of its header's content-id
    /// field, read back as the id [`Engine::content_id`](crate::Engine::content_id) minted when
    /// [`Engine::compile_blob`](crate::Engine::compile_blob) encoded the blob
    /// ([`CompiledBlob::content_id`](crate::CompiledBlob::content_id)). Present for every loaded
    /// CXF document, partial export or not; `None` when the 32 bytes are all zero, which is what
    /// a hand-built model with no id records. The id is read, never recomputed: nothing checks
    /// it against the blob's body, so it is evidence only as far as the bytes are authenticated.
    pub content_id: Option<ContentId>,
    /// The class table, in blob order.
    pub classes: Vec<BlobClass>,
    /// One record per block instance, in block id order.
    pub instances: Vec<BlobInstance>,
    /// The boundary input paths, in slot order.
    pub inputs: Vec<String>,
    /// The boundary output paths, in slot order. A path may repeat in a blob the decoder would
    /// refuse for it.
    pub outputs: Vec<String>,
    /// Whether any class in the class table is target-bound in this build's registry: the
    /// outputs may then depend on the platform's transcendental math library
    /// (`docs/state-compatibility.md`). For a blob the decoder accepts it is the flag the decoded
    /// executable reports.
    pub target_bound: bool,
}

/// One entry of a blob's class table.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct BlobClass {
    /// The canonical class path, a class this build registers.
    pub class_path: String,
    /// The class signature hash the encoder recorded for the class (FNV-1a-64 over its canonical
    /// record). It is not compared with this build's hash for the class, which
    /// `oce_blocks::catalog()` publishes, so two blobs of one engine version can differ here.
    pub signature_hash: u64,
}

/// One block instance of a blob: its class and what the blob records about its state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct BlobInstance {
    /// The block id: this record's position in [`BlobSummary::instances`].
    pub block: u32,
    /// Index into [`BlobSummary::classes`], always in range.
    pub class: u32,
    /// Whether the blob records the block as stateful.
    pub stateful: bool,
    /// The number of 64-bit state words the blob records for the block. In a blob the decoder
    /// accepts it is 0 for a block without state.
    pub state_words: u32,
    /// The CRC-32C (Castagnoli) the blob records for the block's initial state words, each word
    /// as its eight little-endian bytes. In a blob the decoder accepts it is 0 for a block
    /// without state. Two instances compare by it only when their `state_words` are equal. It is
    /// not compared with the state this build would allocate.
    pub initial_state_crc: u32,
}

/// Read what one compiled-schedule blob records about itself, without building or running it.
///
/// The bytes go through `oce_graph::blob::inspect`: the decoder's own checks up to and including
/// its structural pass (size, magic, format version, engine version, length, CRC-32C, profile,
/// flags, then section framing, bounded counts, UTF-8, tags and canonical list order), by the
/// same code and in the same order, so those refusals are the decoder's. Two further checks make
/// the summary's own fields answerable: every class path is registered in this build
/// ([`BlobError::UnknownClass`](crate::BlobError::UnknownClass)), and every instance's class
/// index is inside the class table ([`BlobError::InstanceMismatch`](crate::BlobError) with
/// [`InstanceField::ClassIndex`](crate::InstanceField::ClassIndex)).
///
/// No signature hash is compared, no block is constructed, no state is allocated and nothing is
/// executed; see [`BlobSummary`] for what follows from that. No [`Engine`](crate::Engine) is
/// involved and none is changed.
///
/// The result is owned: at most 4 bytes are held per blob byte while it is built, and a blob is
/// at most 64 MiB.
///
/// # Errors
/// [`OcError::Blob`] carrying the typed [`BlobError`](crate::BlobError) of the first failed
/// check. Hostile bytes are refused, never a panic.
///
/// # Examples
/// ```
/// // The checked-in blob of a one-block G36 fixture: an economizer high-limit cutoff.
/// let blob: &[u8] = include_bytes!(
///     "../tests/fixtures/blob/generic_air_economizer_high_limits_ashrae_differential.oceb"
/// );
/// let summary = oce_api::inspect_blob(blob)?;
/// assert_eq!(summary.engine_version, oce_api::ENGINE_VERSION);
/// assert!(summary.content_id.is_some());
/// assert_eq!(summary.classes.len(), 1);
/// assert_eq!(summary.classes[0].class_path, "CDL.Reals.AddParameter");
/// assert_eq!(summary.instances.len(), 1);
/// assert_eq!((summary.inputs.len(), summary.outputs.len()), (1, 1));
/// assert!(!summary.target_bound);
///
/// // Bytes that are not a blob are refused with the decoder's typed error.
/// assert!(matches!(
///     oce_api::inspect_blob(b"not a blob"),
///     Err(oce_api::OcError::Blob(oce_api::BlobError::Malformed { .. }))
/// ));
/// # Ok::<(), oce_api::OcError>(())
/// ```
pub fn inspect_blob(bytes: &[u8]) -> Result<BlobSummary, OcError> {
    let blob::Summary {
        format_version,
        engine_version,
        content_id,
        classes,
        instances,
        inputs,
        outputs,
        target_bound,
    } = blob::inspect(bytes)?;
    Ok(BlobSummary {
        format_version,
        engine_version,
        content_id: content_id.map(ContentId::from_digest),
        classes: classes
            .into_iter()
            .map(|class| {
                let blob::SummaryClass {
                    class_path,
                    signature_hash,
                } = class;
                BlobClass {
                    class_path: class_path.to_owned(),
                    signature_hash,
                }
            })
            .collect(),
        instances: instances
            .into_iter()
            .map(|instance| {
                let blob::SummaryInstance {
                    block,
                    class,
                    stateful,
                    state_words,
                    initial_state_crc,
                } = instance;
                BlobInstance {
                    block,
                    class,
                    stateful,
                    state_words,
                    initial_state_crc,
                }
            })
            .collect(),
        inputs: inputs.into_iter().map(str::to_owned).collect(),
        outputs: outputs.into_iter().map(str::to_owned).collect(),
        target_bound,
    })
}
