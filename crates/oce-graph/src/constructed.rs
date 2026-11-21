//! The model-wide budget on what block constructors allocate from parameters (issue #700).
//!
//! A registry constructor sizes its buffers from parameter names and values: a
//! `CDL.Reals.Sources.TimeTable` sizes its table from the largest `table_<row>_<col>` name it is
//! given, a `CDL.Reals.MatrixGain` holds `nin × nout` gains whether or not one is bound. Each
//! block is capped ([`oce_blocks::MAX_CONSTRUCTION_PEAK_BYTES`]) but a model has many, so a
//! loader of untrusted parameters charges every block it constructs against one
//! [`ConstructedBudget`] and stops at the first block that takes the total past
//! [`MAX_CONSTRUCTED_WORDS`]. The compiled-schedule blob decoder and the facade's model build
//! both do, with this one accumulator, so the two paths cannot count differently.

use oce_blocks::Block;
use oce_model::{BlockId, Model};

use crate::BuildError;
use crate::topo::block_path;

/// Maximum words the constructed blocks of one model may hold outside their `[S]` state regions
/// (`1 << 22` words = 32 MiB): the sum of [`Block::constructed_words`] over the model.
///
/// It is a budget of its own, not a share of [`MAX_STATE_WORDS`](crate::MAX_STATE_WORDS). The
/// state budget is tied to what a snapshot must carry and is spent by window lengths; this one
/// is spent by tables, gain matrices and port widths, which no snapshot carries, and the two
/// have different remedies. A model may spend both in full.
///
/// The value admits what the per-block width bound ([`oce_blocks::MAX_RESOLVED_PORT_WIDTH`])
/// admits for one block (the widest single block, a one-row `CDL.Reals.Sources.TimeTable` of
/// 1,048,575 output columns, reports 2,228,223 words) and three dense 1024 × 1024
/// `CDL.Reals.MatrixGain` blocks beside each other (1,048,832 words each). Real equipment models
/// use a few dozen: the largest of the 46 G36 catalog fixtures reports 42 words.
///
/// Crossing it is a typed refusal, [`BuildError::ConstructedTooLarge`] on a model build and
/// `BlobError::ConstructedTooLarge` on a blob decode, never a panic or an allocation abort. A
/// loader charges each block after constructing it, so its peak is this budget plus one block's
/// construction ([`oce_blocks::MAX_CONSTRUCTION_PEAK_BYTES`]).
pub const MAX_CONSTRUCTED_WORDS: usize = 1 << 22;

/// The running total of [`Block::constructed_words`] over the blocks a loader has constructed
/// so far, held to [`MAX_CONSTRUCTED_WORDS`].
///
/// Charge each block as soon as it is constructed and before the next one is, and drop the
/// model on a refusal: the blocks charged so far then hold at most the budget, and the refused
/// block at most one construction more.
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

    /// Charge the constructed block of `model`'s instance `id`, naming that instance in the
    /// refusal. `id` must be a block of `model`.
    ///
    /// # Errors
    /// [`BuildError::ConstructedTooLarge`] under the rule of [`charge`](Self::charge), carrying
    /// the instance's authored identity. Nothing is charged then.
    ///
    /// # Panics
    /// If `id` is not a block of `model`.
    pub fn charge_instance(
        &mut self,
        model: &Model,
        id: BlockId,
        block: &dyn Block,
    ) -> Result<(), BuildError> {
        self.charge(block)
            .map_err(|requested| BuildError::ConstructedTooLarge {
                block: block_path(model, id),
                requested,
                limit: MAX_CONSTRUCTED_WORDS,
            })
    }
}
