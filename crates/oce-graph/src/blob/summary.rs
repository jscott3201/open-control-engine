//! Reading what a blob records about itself, without building it: [`inspect()`] (issue #448).
//!
//! A comparison of two blobs (`docs/design/diff.md`, decision 15) needs the content id, the class
//! table and each instance's initial-state CRC, and must be able to read them from a blob this
//! build would not run: a class whose signature hash differs from this build's is exactly what such
//! a comparison reports. So [`inspect()`] runs the decoder's own framing and structural pass
//! ([`decode()`](super::decode()) steps 1 to 6) and then reads the records with the decoder's
//! record parsers. It constructs no block, allocates no state and evaluates nothing.
//!
//! **A summary is not acceptance.** Only [`decode()`](super::decode()) (or
//! [`Executable::from_blob`](super::Executable::from_blob)) says whether this build runs a blob.

use alloc::vec::Vec;

use oce_blocks::{BlockKind, lookup};

use super::decode::index_u32;
use super::error::{BlobError, InstanceField};
use super::sections::{
    self, class_count, class_record, input_count, input_record, instance_counts, instance_record,
    output_count, output_record,
};
use super::{
    ENGINE_VERSION, EngineVersion, FORMAT_VERSION, any_class_target_bound, recorded_content_id,
};

/// What one blob records about itself: the header facts, the class table, one record per block
/// instance and the boundary I/O paths, as [`inspect()`] read them.
///
/// Every field is the blob's own statement, except [`target_bound`](Self::target_bound), which
/// is read from this build's registry. Nothing here has been compared with what this build would
/// construct: a signature hash may differ from this build's hash for the class, and an
/// initial-state CRC from the state this build would allocate. Paths borrow from the inspected
/// bytes.
///
/// # Invariants
/// For every value [`inspect()`] returns: `format_version` is [`FORMAT_VERSION`] and
/// `engine_version` is [`ENGINE_VERSION`] (any other is refused); `classes` is strictly
/// increasing by UTF-8 path bytes and every path is registered in this build; `instances[i].block`
/// is `i` and `instances[i].class` indexes `classes`; `inputs` is strictly increasing and `outputs`
/// non-decreasing by UTF-8 path bytes (the slot order of each list).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Summary<'a> {
    /// The format version in the header.
    pub format_version: u16,
    /// The encoder's engine version in the header.
    pub engine_version: EngineVersion,
    /// The content-id field verbatim, or `None` when it is 32 zero bytes (no content id). The
    /// bytes are never interpreted here; see [`content_id_field`](super::content_id_field).
    pub content_id: Option<[u8; 32]>,
    /// The class table, in blob order.
    pub classes: Vec<SummaryClass<'a>>,
    /// One record per block instance, in block id order.
    pub instances: Vec<SummaryInstance>,
    /// The boundary input paths, in slot order.
    pub inputs: Vec<&'a str>,
    /// The boundary output paths, in slot order. A path may repeat here:
    /// [`decode()`](super::decode()) refuses that, the structural pass does not.
    pub outputs: Vec<&'a str>,
    /// Whether any class in the class table is target-bound in this build's registry
    /// ([`oce_blocks::class_is_target_bound`]): the outputs may depend on the platform's
    /// transcendental math library. For a blob [`decode()`](super::decode()) accepts it equals
    /// [`Decoded::target_bound`](super::Decoded::target_bound).
    pub target_bound: bool,
}

/// One entry of a blob's class table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SummaryClass<'a> {
    /// The canonical class path, registered in this build.
    pub class_path: &'a str,
    /// The class signature hash the encoder recorded ([`oce_blocks::class_signature_hash`] in its
    /// build). Not compared with this build's hash for the class.
    pub signature_hash: u64,
}

/// One block instance of a blob: its class and what the blob records about its state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SummaryInstance {
    /// The block id: this record's position in the instance list.
    pub block: u32,
    /// Index into [`Summary::classes`], always in range.
    pub class: u32,
    /// Whether the blob records the block as stateful (its kind tag).
    pub stateful: bool,
    /// The number of 64-bit state words the blob records for the block.
    pub state_words: u32,
    /// The CRC-32C the blob records for the block's initial state words, each word as its eight
    /// little-endian bytes. Not compared with the state this build would allocate.
    pub initial_state_crc: u32,
}

/// Read what one `OCEB` blob records about itself, without building or running it.
///
/// The checks are [`decode()`](super::decode())'s steps 1 to 6, by the same code and in the same
/// order (size, header and magic, format version, engine version, length, CRC-32C, profile, flags
/// and the whole structural pass), so every refusal those steps give is given here unchanged.
/// Two checks follow, because the summary's own fields depend on them:
///
/// * every class path is registered in this build ([`BlobError::UnknownClass`], the first such
///   class in table order): `target_bound` is read from this build's registry and cannot be
///   answered for a class it does not hold;
/// * every instance's class index is inside the class table ([`BlobError::InstanceMismatch`] with
///   [`InstanceField::ClassIndex`], the first such block), so the index always resolves.
///
/// Nothing else of steps 7 to 10 runs: signature hashes are not compared, no parameter is read
/// as a value, no block is constructed, no connector, connection or schedule entry is checked
/// and no state is allocated. A blob that [`decode()`](super::decode()) refuses at one of those
/// steps is summarised as written, and after step 6 the first refusal here need not be the one
/// [`decode()`](super::decode()) reports.
///
/// Every check runs before anything is allocated, so a refusal allocates nothing, whatever the
/// counts in the refused bytes say. A summary costs the four lists it returns, each allocated
/// once at a count the structural pass proved against the input, and nothing else: at most 2
/// bytes held per blob byte.
///
/// # Errors
/// A typed [`BlobError`] naming the first failed check; never a panic.
pub fn inspect(bytes: &[u8]) -> Result<Summary<'_>, BlobError> {
    let layout = sections::frame(bytes)?;
    sections::check_structure(&layout)?;

    // The two checks of this reader, in table order then block order, before any allocation.
    let mut r = layout.classes.reader();
    let class_total = class_count(&mut r)?;
    for index in 0..class_total {
        if lookup(class_record(&mut r)?.path).is_none() {
            return Err(BlobError::UnknownClass {
                class: index_u32(index),
            });
        }
    }
    let mut r = layout.instances.reader();
    let (block_total, _) = instance_counts(&mut r)?;
    for index in 0..block_total {
        let class = instance_record(&mut r)?.class;
        if !usize::try_from(class).is_ok_and(|class| class < class_total) {
            return Err(BlobError::InstanceMismatch {
                block: index_u32(index),
                field: InstanceField::ClassIndex,
            });
        }
    }

    // Nothing below refuses: every record was parsed by the structural pass and checked above.
    let mut r = layout.classes.reader();
    class_count(&mut r)?;
    let mut classes = Vec::with_capacity(class_total);
    for _ in 0..class_total {
        let record = class_record(&mut r)?;
        classes.push(SummaryClass {
            class_path: record.path,
            signature_hash: record.hash,
        });
    }
    let target_bound = any_class_target_bound(classes.iter().map(|class| class.class_path));

    let mut r = layout.instances.reader();
    instance_counts(&mut r)?;
    let mut instances = Vec::with_capacity(block_total);
    for index in 0..block_total {
        let record = instance_record(&mut r)?;
        instances.push(SummaryInstance {
            block: index_u32(index),
            class: record.class,
            stateful: record.kind == BlockKind::Stateful,
            state_words: record.state_len,
            initial_state_crc: record.state_crc,
        });
    }

    let mut r = layout.io.reader();
    let input_total = input_count(&mut r)?;
    let mut inputs = Vec::with_capacity(input_total);
    for _ in 0..input_total {
        inputs.push(input_record(&mut r)?.path);
    }
    let output_total = output_count(&mut r)?;
    let mut outputs = Vec::with_capacity(output_total);
    for _ in 0..output_total {
        outputs.push(output_record(&mut r)?.path);
    }

    Ok(Summary {
        // `frame` refused every other format and engine version, so the header holds these two.
        format_version: FORMAT_VERSION,
        engine_version: ENGINE_VERSION,
        content_id: recorded_content_id(layout.content_id),
        classes,
        instances,
        inputs,
        outputs,
        target_bound,
    })
}
