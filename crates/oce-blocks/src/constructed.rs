//! Accounting for the heap a constructed block keeps alive: the helpers behind
//! [`Block::constructed_words`], the per-block construction
//! cap and the model-wide budget every loader charges (issues #700 and #722).
//!
//! A registry constructor sizes its buffers from parameter names and values, so a loader of
//! untrusted parameters (the compiled-schedule blob decoder, the CXF load) sums what the
//! constructed blocks report in one [`ConstructedBudget`] and refuses a model past
//! [`MAX_CONSTRUCTED_WORDS`]. The CXF load constructs each block in three passes (the resolver
//! and the validator construct one to read its resolved signature, then drop it; the facade
//! keeps the one it builds), and each pass charges its own constructions to its own budget, so
//! the work a refused model costs is bounded by the budget, never by its block count. The
//! helpers count capacity, not length, in 64-bit words, and count an index element as one word
//! on every pointer width, so the sum is the same on a 32-bit target as on the host that built
//! the model. Every constructor allocates its kept buffers at their exact length, so capacity is
//! not left to the allocator's growth policy either.

use alloc::vec::Vec;

use crate::Block;

/// Bytes in one accounted word.
const WORD_BYTES: usize = 8;

/// Maximum words the constructed blocks of one model may hold outside their `[S]` state regions
/// (`1 << 22` words = 32 MiB): the sum of [`Block::constructed_words`] over the model.
///
/// It is a budget of its own, not a share of the run-state budget (`oce_graph::MAX_STATE_WORDS`).
/// The state budget is tied to what a snapshot must carry and is spent by window lengths; this
/// one is spent by tables, gain matrices and port widths, which no snapshot carries, and the two
/// have different remedies. A model may spend both in full.
///
/// The value admits what the per-block width bound ([`crate::MAX_RESOLVED_PORT_WIDTH`]) admits
/// for one block (the widest single block, a one-row `CDL.Reals.Sources.TimeTable` of 1,048,575
/// output columns, reports 2,228,223 words) and three dense 1024 × 1024 `CDL.Reals.MatrixGain`
/// blocks beside each other (1,048,832 words each). Real equipment models use a few dozen: the
/// largest of the 46 G36 catalog fixtures reports 42 words. No valid CXF document reaches it (a
/// dense matrix writes its cells, issue #773, and a document holds at most 2^20 array elements);
/// it bounds blobs and graphs a host builds directly, and the work a hostile document's probe
/// constructions can cost before it is refused (issue #722).
///
/// Every loader charges it through [`ConstructedBudget`], after constructing each block and
/// before constructing the next, so a loader's peak is this budget plus one block's construction
/// ([`MAX_CONSTRUCTION_PEAK_BYTES`]). Crossing it is a typed refusal on every path, never a panic
/// or an allocation abort: `oce_graph::BuildError::ConstructedTooLarge` on a model build,
/// `oce_graph::blob::BlobError::ConstructedTooLarge` on a blob decode, and a
/// `resource-limit-exceeded` diagnostic from the CXF resolver and from the validator.
pub const MAX_CONSTRUCTED_WORDS: usize = 1 << 22;

/// The running total of [`Block::constructed_words`] over the blocks one pass has constructed so
/// far, held to [`MAX_CONSTRUCTED_WORDS`].
///
/// One accumulator for every loader (the blob decoder, the facade's model build, the CXF
/// resolver's and the validator's signature probes), so no two paths can count differently.
/// Charge each block as soon as it is constructed and before the next one is, and stop at the
/// first refusal: the blocks charged so far then hold at most the budget, and the refused block
/// at most one construction more. A pass that drops each block after reading it charges it all
/// the same, so the construction work a pass does is bounded by the budget too.
///
/// # Invariants
/// [`words`](Self::words) never exceeds [`MAX_CONSTRUCTED_WORDS`]: a charge that would take it
/// past the budget is refused and leaves the total unchanged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConstructedBudget {
    words: usize,
}

impl ConstructedBudget {
    /// A budget with nothing charged.
    #[must_use]
    pub const fn new() -> Self {
        Self { words: 0 }
    }

    /// The words charged so far, at most [`MAX_CONSTRUCTED_WORDS`].
    #[must_use]
    pub const fn words(&self) -> usize {
        self.words
    }

    /// Charge one constructed block. A total equal to the budget is accepted.
    ///
    /// # Errors
    /// The words requested through this block (the total had it been charged, saturated at
    /// `u64::MAX`) when that total exceeds [`MAX_CONSTRUCTED_WORDS`]. Nothing is charged then.
    /// Never panics.
    pub fn charge(&mut self, block: &dyn Block) -> Result<(), u64> {
        match self.words.checked_add(block.constructed_words()) {
            Some(next) if next <= MAX_CONSTRUCTED_WORDS => {
                self.words = next;
                Ok(())
            }
            over => Err(over.map_or(u64::MAX, |n| u64::try_from(n).unwrap_or(u64::MAX))),
        }
    }
}

/// The most heap any registered constructor holds at one time while it constructs one block,
/// for any parameter table: 32 MiB.
///
/// This is the per-block cap that [`MAX_RESOLVED_PORT_WIDTH`](crate::MAX_RESOLVED_PORT_WIDTH)
/// implies. A constructor holds at most the buffers the block keeps and the scratch it reads
/// them from, each bounded by that width. The largest measured are `CDL.Routing.*ExtractSignal`
/// and `CDL.Reals.MatrixGain` at the full width (see `tests/constructed_words.rs`, which pins
/// the measured peak of every class's widest shape and holds every hostile table of the
/// parameter fuzz under this bound). A model-wide loader that charges
/// [`Block::constructed_words`] after each construction
/// therefore peaks at its budget plus this value.
pub const MAX_CONSTRUCTION_PEAK_BYTES: usize = 32 * 1024 * 1024;

/// The words `buffer` holds: its capacity in bytes, rounded up to whole words. Not for a buffer
/// of indices ([`index_buffer_words`]), whose element size follows the pointer width.
pub(crate) fn buffer_words<T>(buffer: &Vec<T>) -> usize {
    buffer
        .capacity()
        .saturating_mul(size_of::<T>())
        .div_ceil(WORD_BYTES)
}

/// The words a buffer of indices holds: one per element of capacity on every pointer width, so
/// a 32-bit target reports what a 64-bit host does.
pub(crate) fn index_buffer_words(buffer: &Vec<usize>) -> usize {
    buffer.capacity()
}
