//! Accounting for the heap a constructed block keeps alive: the helpers behind
//! [`Block::constructed_words`](crate::Block::constructed_words) and the per-block construction
//! cap (issue #700).
//!
//! A registry constructor sizes its buffers from parameter names and values, so a loader of
//! untrusted parameters (the compiled-schedule blob decoder, the CXF load) sums what the
//! constructed blocks report and refuses a model past `oce_graph::MAX_CONSTRUCTED_WORDS`. The
//! helpers count capacity, not length, in 64-bit words, and count an index element as one word
//! on every pointer width, so the sum is the same on a 32-bit target as on the host that built
//! the model. Every constructor allocates its kept buffers at their exact length, so capacity is
//! not left to the allocator's growth policy either.

use alloc::vec::Vec;

/// Bytes in one accounted word.
const WORD_BYTES: usize = 8;

/// The most heap any registered constructor holds at one time while it constructs one block,
/// for any parameter table: 32 MiB.
///
/// This is the per-block cap that [`MAX_RESOLVED_PORT_WIDTH`](crate::MAX_RESOLVED_PORT_WIDTH)
/// implies. A constructor holds at most the buffers the block keeps and the scratch it reads
/// them from, each bounded by that width. The largest measured are `CDL.Routing.*ExtractSignal`
/// and `CDL.Reals.MatrixGain` at the full width (see `tests/constructed_words.rs`, which pins
/// the measured peak of every class's widest shape and holds every hostile table of the
/// parameter fuzz under this bound). A model-wide loader that charges
/// [`Block::constructed_words`](crate::Block::constructed_words) after each construction
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
