//! Inventory-selected conformance for scalar `CDL.Psychrometrics` formula blocks.

mod block_harness;

use block_harness::{aligned_cases::PSYCHROMETRICS, assert_cases_match_aligned_tolerance_oracle};

#[test]
fn psychrometric_formula_blocks_match_aligned_tolerance_oracle() {
    assert_cases_match_aligned_tolerance_oracle(
        PSYCHROMETRICS,
        "CDL/Psychrometrics",
        "single-block-psychrometrics",
    );
}
