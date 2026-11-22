//! The model-wide constructed budget: `ConstructedBudget` sums `Block::constructed_words` with
//! checked arithmetic against `MAX_CONSTRUCTED_WORDS`, accepts exactly the budget, and refuses
//! the first block past it with the words requested, leaving the total unchanged (issue #700).

use std::sync::Arc;

use super::*;
use crate::{ConstructedBudget, MAX_CONSTRUCTED_WORDS, MAX_STATE_WORDS, charge_instance};

/// A block that reports a given number of constructed words, including counts no real block
/// could hold.
struct ReportsWords(usize);

impl Block for ReportsWords {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "test.ReportsWords",
            inputs: &[],
            outputs: &[],
            stateful: false,
        };
        &SIG
    }
    fn constructed_words(&self) -> usize {
        self.0
    }
    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }
    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        false
    }
}

/// Charge blocks of the given sizes in order: the total after the last accepted charge, and the
/// index and requested words of the first refusal.
fn charge_all(words: &[usize]) -> (usize, Option<(usize, u64)>) {
    let mut budget = ConstructedBudget::new();
    for (index, &size) in words.iter().enumerate() {
        if let Err(requested) = budget.charge(&ReportsWords(size)) {
            return (budget.words(), Some((index, requested)));
        }
    }
    (budget.words(), None)
}

#[test]
fn the_budget_is_the_documented_thirty_two_mebibytes_of_words_and_its_own() {
    assert_eq!(MAX_CONSTRUCTED_WORDS, 1 << 22);
    assert_eq!(MAX_CONSTRUCTED_WORDS * 8, 32 * 1024 * 1024);
    // A budget of its own: a model may spend this and the state budget in full.
    assert_eq!(MAX_CONSTRUCTED_WORDS, MAX_STATE_WORDS);
    assert_eq!(ConstructedBudget::new(), ConstructedBudget::default());
    assert_eq!(ConstructedBudget::new().words(), 0);
}

#[test]
fn exactly_the_budget_is_accepted_in_one_block_or_many() {
    assert_eq!(
        charge_all(&[MAX_CONSTRUCTED_WORDS]),
        (MAX_CONSTRUCTED_WORDS, None)
    );
    assert_eq!(
        charge_all(&[MAX_CONSTRUCTED_WORDS - 10, 0, 10, 0]),
        (MAX_CONSTRUCTED_WORDS, None)
    );
    assert_eq!(charge_all(&[]), (0, None));
}

#[test]
fn one_word_past_the_budget_refuses_at_the_crossing_block_and_charges_nothing() {
    let past = (MAX_CONSTRUCTED_WORDS + 1) as u64;
    assert_eq!(
        charge_all(&[MAX_CONSTRUCTED_WORDS + 1]),
        (0, Some((0, past)))
    );
    assert_eq!(
        charge_all(&[MAX_CONSTRUCTED_WORDS - 10, 11]),
        (MAX_CONSTRUCTED_WORDS - 10, Some((1, past)))
    );
    // The first block past the budget is the one refused, however large the later ones are.
    assert_eq!(
        charge_all(&[3, MAX_CONSTRUCTED_WORDS, usize::MAX]),
        (3, Some((1, (MAX_CONSTRUCTED_WORDS + 3) as u64)))
    );
}

#[test]
fn a_refused_charge_leaves_the_budget_usable_for_a_block_that_fits() {
    let mut budget = ConstructedBudget::new();
    budget
        .charge(&ReportsWords(MAX_CONSTRUCTED_WORDS - 1))
        .unwrap();
    assert_eq!(
        budget.charge(&ReportsWords(2)),
        Err((MAX_CONSTRUCTED_WORDS + 1) as u64)
    );
    assert_eq!(budget.words(), MAX_CONSTRUCTED_WORDS - 1);
    budget.charge(&ReportsWords(0)).unwrap();
    budget.charge(&ReportsWords(1)).unwrap();
    assert_eq!(budget.words(), MAX_CONSTRUCTED_WORDS);
    // At the budget a block that holds nothing is still accepted; one word is not.
    budget.charge(&ReportsWords(0)).unwrap();
    assert_eq!(
        budget.charge(&ReportsWords(1)),
        Err((MAX_CONSTRUCTED_WORDS + 1) as u64)
    );
}

#[test]
fn overflowing_reports_refuse_saturated_instead_of_wrapping_or_panicking() {
    assert_eq!(charge_all(&[usize::MAX]), (0, Some((0, usize::MAX as u64))));
    // Two reports whose sum overflows `usize`: saturated, never wrapped into the budget.
    assert_eq!(charge_all(&[1, usize::MAX]), (1, Some((1, u64::MAX))));
    assert_eq!(
        charge_all(&[MAX_CONSTRUCTED_WORDS, usize::MAX - 1]),
        (MAX_CONSTRUCTED_WORDS, Some((1, u64::MAX)))
    );
}

#[test]
fn a_model_refusal_names_the_crossing_instance_by_its_authored_identity() {
    let mut b = ModelBuilder::default();
    for size in [MAX_CONSTRUCTED_WORDS - 4, 4, 1] {
        b.block_real(Box::new(ReportsWords(size)));
    }
    b.model.blocks[2].instance_iri = Some(Arc::from("urn:test:M.table"));

    let mut budget = ConstructedBudget::new();
    for index in 0..2 {
        charge_instance(
            &mut budget,
            &b.model,
            BlockId(index),
            b.blocks[index as usize].as_ref(),
        )
        .expect("within the budget");
    }
    match charge_instance(&mut budget, &b.model, BlockId(2), b.blocks[2].as_ref()) {
        Err(BuildError::ConstructedTooLarge {
            block,
            requested,
            limit,
        }) => {
            assert_eq!(block.0, "urn:test:M.table");
            assert_eq!(requested, (MAX_CONSTRUCTED_WORDS + 1) as u64);
            assert_eq!(limit, MAX_CONSTRUCTED_WORDS);
        }
        other => panic!("expected ConstructedTooLarge, got {other:?}"),
    }
    assert_eq!(budget.words(), MAX_CONSTRUCTED_WORDS);

    // A hand-built instance without an IRI is named by class and block id, as in every other
    // build refusal, and the message carries the request and the limit.
    let error = charge_instance(
        &mut ConstructedBudget::new(),
        &b.model,
        BlockId(1),
        &ReportsWords(MAX_CONSTRUCTED_WORDS + 7),
    )
    .expect_err("past the budget");
    assert_eq!(
        error.to_string(),
        "constructed blocks too large: 4194311 word(s) held by the blocks through block \
         test.ReportsWords#b1 exceed the supported maximum (4194304 words)"
    );
}

#[test]
fn registered_blocks_charge_what_their_constructors_hold() {
    let mut budget = ConstructedBudget::new();
    // 1024 inputs and outputs (128 words of port kinds each) and 2^20 gains: `K`'s corner cell
    // sets its shape (issue #773; the constructor reads the shape the cells span, and the load
    // gate and the blob decoder refuse the unbound cells before a model gets here).
    let gain = make("CDL.Reals.MatrixGain", &[("K_1024_1024", Value::Real(1.0))]);
    for held in 1..=3 {
        budget.charge(gain.as_ref()).unwrap();
        assert_eq!(budget.words(), held * 1_048_832);
    }
    assert_eq!(budget.charge(gain.as_ref()), Err(4 * 1_048_832));
    // A block that owns no buffer never spends the budget.
    let add = make("CDL.Reals.Add", &[]);
    for _ in 0..1000 {
        budget.charge(add.as_ref()).unwrap();
    }
    assert_eq!(budget.words(), 3 * 1_048_832);
}
