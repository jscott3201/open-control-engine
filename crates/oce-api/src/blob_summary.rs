//! Reading what an `OCEB` compiled-schedule blob records about itself, and whether this build
//! accepts it (issues #448 and #680).
//!
//! The reader is `oce_graph::blob::inspect`: the decoder's own framing and structural pass, then
//! the decoder's own building pass for the verdict. This module owns the plain-data result a host
//! or a leverage crate holds, so a blob comparison (`docs/design/diff.md`, decision 15) needs no
//! direct `oce-graph` dependency. See `docs/compiled-schedule-blob.md`, "Inspecting a blob".

use oce_graph::blob;

use crate::{BlobError, ContentId, EngineVersion, OcError};

/// What one compiled-schedule blob records about itself, as [`inspect_blob`] read it, and
/// whether this build accepts it: the header facts, the class table, one record per block
/// instance, the boundary I/O paths and the decoder's verdict.
///
/// It is the material for comparing two blobs as same or different: by content id, by class path
/// and class signature hash, and by each instance's initial-state CRC. Every field is the blob's
/// own statement except [`target_bound`](Self::target_bound), which is read from this build's
/// registry, and [`decode_refusal`](Self::decode_refusal), which is this build's verdict.
///
/// **Having a summary is not acceptance; [`decode_refusal`](Self::decode_refusal) says whether
/// the blob is accepted.** The lists are the blob as written, compared with nothing, so a blob
/// this build refuses (a class signature hash that is not this build's, an initial-state CRC that
/// is not the allocated state's, a parameter, connector, connection or schedule fault) still has
/// a summary: its `decode_refusal` is `Some`, carrying the decoder's error. A summary whose
/// `decode_refusal` is `None` is of a blob this build's decoder accepts.
///
/// # Invariants
/// For every value [`inspect_blob`] returns: `format_version` is 1 and `engine_version` is
/// [`ENGINE_VERSION`](crate::ENGINE_VERSION) (a blob of any other is refused); `classes` is
/// strictly increasing by UTF-8 path bytes and every path is a class this build registers;
/// `instances[i].block` is `i` and `instances[i].class` indexes `classes`; `inputs` is strictly
/// increasing and `outputs` non-decreasing by UTF-8 path bytes, which is the slot order of each;
/// `decode_refusal` is the error `oce_graph::blob::decode` returns for the same bytes, `None`
/// exactly when it accepts them.
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
    /// Whether this build accepts the blob.
    ///
    /// * `None`: this build's full decoder (`oce_graph::blob::decode`, which the execution entry
    ///   `oce_graph::blob::Executable::from_blob` runs) accepts these bytes. The blob is
    ///   **accepted by this build**.
    /// * `Some(error)`: this build **refuses** the blob, and `error` is the decoder's own first
    ///   refusal for these bytes: for example
    ///   [`BlobError::ClassSignatureMismatch`], [`BlobError::InitialStateMismatch`],
    ///   [`BlobError::InvalidSchedule`] or [`BlobError::UndrivenInput`]. The other fields then
    ///   describe a blob that will not run here.
    ///
    /// The value is the result of the decoder itself, run once over the same bytes with what it
    /// built dropped, not of a second set of checks; it never ticks. It is never one of the
    /// errors [`inspect_blob`] returns as `Err` (the framing and structural refusals,
    /// [`BlobError::UnknownClass`], [`InstanceField::ClassIndex`](crate::InstanceField)), because
    /// bytes refused that way have no summary. The verdict is this build's: the blob format
    /// admits only the engine version that wrote it, and within one version a build with another
    /// class registry may answer differently. It says nothing about where the bytes came from:
    /// a blob carries a CRC, not a signature.
    pub decode_refusal: Option<BlobError>,
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

/// Read what one compiled-schedule blob records about itself and whether this build accepts it,
/// without running it.
///
/// The bytes go through `oce_graph::blob::inspect`: the decoder's own checks up to and including
/// its structural pass (size, magic, format version, engine version, length, CRC-32C, profile,
/// flags, then section framing, bounded counts, UTF-8, tags and canonical list order), by the
/// same code and in the same order, so those refusals are the decoder's. Two further checks make
/// the summary's own fields answerable: every class path is registered in this build
/// ([`BlobError::UnknownClass`]), and every instance's class index is inside the class table
/// ([`BlobError::InstanceMismatch`] with
/// [`InstanceField::ClassIndex`](crate::InstanceField::ClassIndex)). Those are the only `Err`s.
///
/// **Read the result in three cases (issue #680):**
///
/// * `Ok(summary)` with `summary.decode_refusal == None` **is acceptance by this build**: its
///   full decoder accepts these bytes, as the execution entry
///   (`oce_graph::blob::Executable::from_blob`) of the same build does.
/// * `Ok(summary)` with `summary.decode_refusal == Some(error)` **describes a blob this build
///   refuses**, with the decoder's own error: a class hash or initial-state CRC that is not this
///   build's, or a parameter, connector, connection, schedule or state fault. The summary is
///   what such a blob says about itself, which is what a comparison needs; it is not a blob to
///   run.
/// * `Err` is bytes with no summary at all, which this build refuses too.
///
/// So `inspect_blob(bytes).is_ok()` is **not** an acceptance check; the check is
/// `matches!(inspect_blob(bytes), Ok(summary) if summary.decode_refusal.is_none())`. The verdict
/// is the decoder's, not a second set of checks: after the checks above, the decoder's building
/// pass runs once over the same bytes (blocks are constructed and state is allocated, as on a
/// target) and what it built is dropped. Every block is constructed from the blob's parameters
/// and its state is seeded; the schedule never ticks. No [`Engine`](crate::Engine) is
/// involved and none is changed. Acceptance is of the bytes by this build; it does not say
/// where they came from.
///
/// # Cost
/// An `Err` allocates nothing. A summary costs the decoder's building pass first, exactly what
/// `oce_graph::blob::decode` allocates for the same bytes, all of it freed before the result is
/// built: its peak is the decoder's. That peak is bounded: at most 64 bytes per blob byte, plus
/// the constructed budget (what block constructors allocate from the blob's parameters:
/// 4,194,304 words, 32 MiB, across the model, with one block under construction, at most 32 MiB
/// more) and the state budget (the state the blocks declare: 4,194,304 words, 32 MiB). So a
/// summary of `n` blob bytes peaks within `64 n` bytes and 96 MiB. A blob whose blocks would
/// hold more has a summary whose `decode_refusal` is [`BlobError::ConstructedTooLarge`] or
/// [`BlobError::StateTooLarge`] (`docs/compiled-schedule-blob.md`, "What a summary costs"). The
/// result is then owned, one
/// `String` per non-empty path: at most 4 bytes are requested and held per blob byte while it is
/// built (a count of the bytes asked of the allocator, not of its per-allocation overhead, which
/// a blob of many short paths multiplies).
/// The two are never held together, so the peak is the larger of them, and a blob is at most
/// 64 MiB.
///
/// # Errors
/// [`OcError::Blob`] carrying the typed [`BlobError`] of the first failed check. Hostile bytes
/// are refused, never a panic. A refusal by the decoder's building pass is not an `Err`: it is
/// [`BlobSummary::decode_refusal`].
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
/// // This build's decoder accepts the blob.
/// assert_eq!(summary.decode_refusal, None);
///
/// // The same blob with one bit of its class signature hash changed, and the CRC-32C trailer
/// // (CRC-32C is `crc32c` in the core; restated here) recomputed: it still has a summary, which
/// // shows the changed hash, and the summary says this build refuses it and why.
/// let mut changed = blob.to_vec();
/// let hash_at = 52 + 4 + 4 + 4 + 4 + "CDL.Reals.AddParameter".len();
/// changed[hash_at] ^= 1;
/// let body = changed.len() - 4;
/// let mut crc = !0u32;
/// for &byte in &changed[..body] {
///     crc ^= u32::from(byte);
///     for _ in 0..8 {
///         crc = (crc >> 1) ^ (0x82F6_3B78 & 0u32.wrapping_sub(crc & 1));
///     }
/// }
/// changed[body..].copy_from_slice(&(!crc).to_le_bytes());
/// let refused = oce_api::inspect_blob(&changed)?;
/// assert_ne!(refused.classes[0].signature_hash, summary.classes[0].signature_hash);
/// assert!(matches!(
///     refused.decode_refusal,
///     Some(oce_api::BlobError::ClassSignatureMismatch { class: 0, .. })
/// ));
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
        decode_refusal,
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
        decode_refusal,
    })
}
