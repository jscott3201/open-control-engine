//! Inventory-selected Real and unchanged exact Boolean SunRiseSet conformance.

mod block_harness;

use block_harness::{aligned_cases::SUN, assert_cases_match_aligned_tolerance_oracle};

#[test]
fn utility_blocks_match_aligned_tolerance_oracle() {
    assert_cases_match_aligned_tolerance_oracle(SUN, "CDL/Utilities", "single-block-utilities");
}
