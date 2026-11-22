//! The model-wide budget on what block constructors allocate from parameters (issues #700 and
//! #722), applied to a model build.
//!
//! A registry constructor sizes its buffers from parameter names and values: a
//! `CDL.Reals.Sources.TimeTable` sizes its table from the `table_<row>_<col>` cells it is given,
//! a `CDL.Reals.MatrixGain` holds the gains its `K` spans. Each block is capped
//! ([`oce_blocks::MAX_CONSTRUCTION_PEAK_BYTES`]) but a model has many, so a loader of untrusted
//! parameters charges every block it constructs against one [`ConstructedBudget`] and stops at
//! the first block that takes the total past [`MAX_CONSTRUCTED_WORDS`]. Both live in
//! `oce-blocks`, beside [`Block::constructed_words`], so the compiled-schedule blob decoder, the
//! facade's model build, the CXF resolver and the validator charge with one accumulator and
//! cannot count differently; this crate re-exports them and adds the charge that names a model's
//! instance in a [`BuildError`].

use oce_blocks::{Block, ConstructedBudget, MAX_CONSTRUCTED_WORDS};
use oce_model::{BlockId, Model};

use crate::BuildError;
use crate::topo::block_path;

/// Charge the constructed block of `model`'s instance `id` to `budget`, naming that instance in
/// the refusal. `id` must be a block of `model`.
///
/// # Errors
/// [`BuildError::ConstructedTooLarge`] under the rule of [`ConstructedBudget::charge`], carrying
/// the instance's authored identity. Nothing is charged then.
///
/// # Panics
/// If `id` is not a block of `model`.
pub fn charge_instance(
    budget: &mut ConstructedBudget,
    model: &Model,
    id: BlockId,
    block: &dyn Block,
) -> Result<(), BuildError> {
    budget
        .charge(block)
        .map_err(|requested| BuildError::ConstructedTooLarge {
            block: block_path(model, id),
            requested,
            limit: MAX_CONSTRUCTED_WORDS,
        })
}
