//! Inventory-selected Sin and unchanged exact CalendarTime conformance.

mod block_harness;

use block_harness::{
    aligned_cases::{CALENDAR, SOURCE_SIN},
    assert_cases_match_aligned_tolerance_oracle, assert_cases_match_exact_oracle,
};

#[test]
fn reals_source_transcendental_blocks_match_aligned_tolerance_oracle() {
    assert_cases_match_aligned_tolerance_oracle(
        SOURCE_SIN,
        "CDL/Reals",
        "single-block-reals-source-transcendental",
    );
}

#[test]
fn reals_calendar_time_blocks_match_exact_oracle() {
    assert_cases_match_exact_oracle(
        CALENDAR,
        "CDL/Reals",
        "single-block-reals-source-calendar-time",
    );
}
