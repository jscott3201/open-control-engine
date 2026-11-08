//! BUILD state-allocation budget: `allocate_state` sums `[S]` regions with checked arithmetic
//! against `MAX_STATE_WORDS` and refuses with a typed `StateTooLarge` before reserving anything,
//! so no input-derived `Block::state_len` can panic or abort the load (issue #365).

use super::*;
use crate::MAX_STATE_WORDS;

/// A stateful block whose region length is set directly, including lengths no real block could
/// allocate.
struct SizedState(usize);

impl Block for SizedState {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "test.SizedState",
            inputs: &[],
            outputs: &[],
            stateful: true,
        };
        &SIG
    }
    fn kind(&self) -> BlockKind {
        BlockKind::Stateful
    }
    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        false
    }
    fn state_len(&self) -> usize {
        self.0
    }
    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region.fill(0x5eed);
    }
}

fn model_of(lengths: &[usize]) -> ModelBuilder {
    let mut b = ModelBuilder::default();
    for &len in lengths {
        b.block_real(Box::new(SizedState(len)));
    }
    b
}

fn refusal(lengths: &[usize]) -> (String, u64, usize) {
    let b = model_of(lengths);
    match allocate_state(&b.model, &b.blocks) {
        Err(BuildError::StateTooLarge {
            block,
            requested,
            limit,
        }) => (block.0, requested, limit),
        Err(other) => panic!("expected StateTooLarge, got {other:?}"),
        Ok(state) => panic!("expected a refusal, allocated {} words", state.words.len()),
    }
}

#[test]
fn exactly_the_budget_allocates_and_seeds_every_word() {
    let b = model_of(&[MAX_STATE_WORDS - 10, 10]);
    let state = allocate_state(&b.model, &b.blocks).expect("exactly the budget allocates");
    assert_eq!(state.words.len(), MAX_STATE_WORDS);
    assert_eq!(state.slots.len(), 2);
    assert_eq!(state.slots[1].offset, MAX_STATE_WORDS - 10);
    assert!(state.words.iter().all(|&w| w == 0x5eed));
}

#[test]
fn one_word_past_the_budget_refuses_naming_the_crossing_block() {
    let (block, requested, limit) = refusal(&[MAX_STATE_WORDS - 10, 11]);
    assert_eq!(block, "test.SizedState#b1");
    assert_eq!(requested, (MAX_STATE_WORDS + 1) as u64);
    assert_eq!(limit, MAX_STATE_WORDS);
    // The first block past the budget is named even when later blocks are larger still.
    let (block, ..) = refusal(&[3, MAX_STATE_WORDS, usize::MAX]);
    assert_eq!(block, "test.SizedState#b1");
}

#[test]
fn overflowing_region_lengths_refuse_instead_of_panicking() {
    // `usize::MAX` alone fails the budget; two halves of the address space overflow the sum.
    let (block, requested, _) = refusal(&[usize::MAX]);
    assert_eq!(block, "test.SizedState#b0");
    assert_eq!(requested, u64::try_from(usize::MAX).unwrap_or(u64::MAX));
    let (block, requested, _) = refusal(&[1, usize::MAX]);
    assert_eq!(block, "test.SizedState#b1");
    assert_eq!(requested, u64::MAX, "an overflowing sum saturates");
}

#[test]
fn a_registry_moving_mean_window_past_the_budget_refuses_at_build() {
    // The issue #365 input that panicked with "capacity overflow" (n = 2^62).
    let mut b = ModelBuilder::default();
    b.block_real(make(
        "CDL.Discrete.TriggeredMovingMean",
        &[("n", Value::Integer(1 << 62))],
    ));
    match allocate_state(&b.model, &b.blocks) {
        Err(BuildError::StateTooLarge { requested, .. }) => {
            assert_eq!(requested, (1 << 62) + 4);
        }
        other => panic!("expected StateTooLarge, got {other:?}"),
    }
    let message = allocate_state(&b.model, &b.blocks).unwrap_err().to_string();
    assert_eq!(
        message,
        "run state too large: 4611686018427387908 state word(s) requested through block \
         CDL.Discrete.TriggeredMovingMean#b0 exceed the supported maximum (4194304 words) or \
         could not be allocated"
    );
}
