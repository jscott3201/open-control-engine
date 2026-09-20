//! Inventory-selected conformance for scalar `CDL.Reals` transcendental blocks.

mod block_harness;

use block_harness::{aligned_cases::REALS, assert_cases_match_aligned_tolerance_oracle};

#[test]
fn reals_transcendental_blocks_match_aligned_tolerance_oracle() {
    assert_cases_match_aligned_tolerance_oracle(REALS, "CDL/Reals", "single-block-reals");
}
