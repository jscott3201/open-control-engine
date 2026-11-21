//! The bounds at the test's instants and the error there.

use super::*;

fn bound(points: &[(f64, f64)]) -> Vec<Corner> {
    points.iter().map(|&(x, y)| Corner { x, y }).collect()
}

#[test]
fn a_target_between_two_corners_is_on_the_line_through_them() {
    let ramp = bound(&[(0.0, 0.0), (4.0, 8.0), (8.0, 0.0)]);
    assert_eq!(
        bound_at(&ramp, &[0.0, 1.0, 2.0, 4.0, 5.0, 8.0]),
        [0.0, 2.0, 4.0, 8.0, 6.0, 0.0]
    );
}

#[test]
fn beyond_the_last_corner_the_bound_is_held_never_extrapolated() {
    let ramp = bound(&[(0.0, 0.0), (4.0, 8.0)]);
    assert_eq!(
        bound_at(&ramp, &[2.0, 4.0, 4.5, 100.0, 1e300]),
        [4.0, 8.0, 8.0, 8.0, 8.0]
    );
    // Every target beyond the end: all held.
    assert_eq!(bound_at(&ramp, &[5.0, 6.0]), [8.0, 8.0]);
}

#[test]
fn before_the_first_corner_the_first_segment_is_extended_as_in_funnel() {
    // The funnel never asks for this (its bounds start a half-width before the first instant);
    // the port still answers as the C does, by the first segment's line.
    let ramp = bound(&[(0.0, 0.0), (4.0, 8.0), (8.0, 8.0)]);
    assert_eq!(bound_at(&ramp, &[-1.0]), [-2.0]);
}

#[test]
fn at_a_vertical_segment_the_value_is_the_corner_the_cursor_reached_first() {
    // A step up at x = 2. The cursor stops at the first corner at or beyond the target, so at
    // the step itself the lower corner's segment answers.
    let step = bound(&[(0.0, 1.0), (2.0, 1.0), (2.0, 5.0), (4.0, 5.0)]);
    assert_eq!(bound_at(&step, &[1.0, 2.0, 2.0, 3.0]), [1.0, 1.0, 1.0, 5.0]);
    // A bound that is one vertical segment: the division is skipped, the first corner answers.
    let wall = bound(&[(2.0, 1.0), (2.0, 5.0)]);
    assert_eq!(bound_at(&wall, &[2.0]), [1.0]);
}

#[test]
fn a_target_within_1e_minus_10_of_a_segments_start_takes_the_starts_value() {
    // The guard against a division by zero is funnel's equality on (x1 - x0) × (x - x0): a
    // product of at most 1e-10 reads as zero and the segment's first corner answers.
    let ramp = bound(&[(0.0, 0.0), (1.0, 4.0)]);
    assert_eq!(bound_at(&ramp, &[1e-10]), [0.0]);
    assert_eq!(bound_at(&ramp, &[2e-10]), [8e-10]);
}

#[test]
fn the_cursor_only_advances_so_equal_and_increasing_targets_cost_one_pass() {
    let saw = bound(&[(0.0, 0.0), (1.0, 2.0), (2.0, 0.0), (3.0, 2.0), (4.0, 0.0)]);
    let targets = [0.5, 0.5, 1.5, 1.5, 1.5, 3.5, 4.0];
    assert_eq!(
        bound_at(&saw, &targets),
        [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0]
    );
    assert_eq!(bound_at(&saw, &[]), [] as [f64; 0]);
}

#[test]
fn the_error_is_the_distance_outside_the_band_and_zero_inside() {
    let lower = [0.0, 0.0, 0.0, 0.0, 0.0];
    let upper = [1.0, 1.0, 1.0, 1.0, 1.0];
    let test = [-0.5, 0.0, 0.5, 1.0, 1.75];
    assert_eq!(errors(&lower, &upper, &test), [0.5, 0.0, 0.0, 0.0, 0.75]);
}

#[test]
fn the_error_is_never_negative_and_its_zero_is_positive_zero() {
    let lower = [-0.0, 0.0, -1.0];
    let upper = [0.0, -0.0, 1.0];
    let test = [0.0, -0.0, -0.0];
    for error in errors(&lower, &upper, &test) {
        assert_eq!(error.to_bits(), 0.0_f64.to_bits());
    }
}

#[test]
fn where_the_bounds_cross_the_lower_one_is_tested_first_as_in_funnel() {
    // Upper below lower cannot come from one rectangle; funnel's branch order still decides.
    assert_eq!(errors(&[2.0], &[1.0], &[1.5]), [0.5]);
    assert_eq!(errors(&[2.0], &[1.0], &[3.0]), [2.0]);
    assert_eq!(errors(&[2.0], &[1.0], &[0.0]), [2.0]);
}

#[test]
fn a_nan_bound_reads_as_inside_which_is_why_the_caller_refuses_it() {
    // Both comparisons are false against a NaN. `funnel` checks its bounds are finite before a
    // result is built, so this zero never reaches one.
    assert_eq!(errors(&[f64::NAN], &[f64::NAN], &[5.0]), [0.0]);
}
