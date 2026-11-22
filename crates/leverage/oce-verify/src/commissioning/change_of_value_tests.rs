use super::*;

fn curve(points: &[(f64, f64)]) -> Curve {
    Curve::from_points(points.iter().copied())
}

#[test]
fn the_series_instants_are_the_window_ends_and_every_time_inside_each_once_in_order() {
    let engine = [-5.0, 0.0, 10.0, 20.0, 40.0];
    let trend = [3.0, 10.0, 10.0, 13.0, 35.0];
    let (instants, _) = series_instants(0.0, 30.0, &[&engine, &trend]);
    assert_eq!(instants, [0.0, 3.0, 10.0, 13.0, 20.0, 30.0]);
}

#[test]
fn a_window_of_one_instant_has_one_instant() {
    let (instants, _) = series_instants(5.0, 5.0, &[&[1.0, 5.0, 9.0]]);
    assert_eq!(instants, [5.0]);
}

#[test]
fn equal_times_in_one_series_are_one_instant() {
    // A switch written as two rows at one time (OBC §12.2) is one instant; the held value there
    // is the last row's, as on the grid path.
    let (instants, _) = series_instants(0.0, 10.0, &[&[0.0, 4.0, 4.0, 4.0, 10.0]]);
    assert_eq!(instants, [0.0, 4.0, 10.0]);
}

#[test]
fn merging_two_series_is_linear_in_their_lengths() {
    // The steps of the union at 1x, 2x and 4x the length: each list is merged once with the
    // instants so far, so the steps are at most the sum of the two merges' inputs.
    let mut steps_at = Vec::new();
    for scale in [1_u32, 2, 4] {
        let n = 25_000 * scale;
        let engine: Vec<f64> = (0..n).map(|k| f64::from(k) * 2.0).collect();
        let trend: Vec<f64> = (0..n).map(|k| f64::from(k) * 2.0 + 1.0).collect();
        let stop = f64::from(2 * n);
        let (instants, steps) = series_instants(0.0, stop, &[&engine, &trend]);
        assert_eq!(instants.len(), usize::try_from(2 * n + 1).expect("fits"));
        let bound = 2 + u64::from(n) + (u64::from(n) + 1 + u64::from(n));
        assert!(steps <= bound, "{steps} > {bound}");
        steps_at.push(steps);
    }
    // Exactly 3n + 3 steps: linear, so doubling the length doubles the work.
    assert_eq!(steps_at, [75_003, 150_003, 300_003]);
}

#[test]
fn a_step_series_carries_the_held_value_to_each_change() {
    let (x, y) = step_series(&[0.0, 10.0, 13.0, 20.0], &[0.0, 0.0, 1.0, 1.0]);
    assert_eq!(x, [0.0, 10.0, 13.0, 13.0, 20.0]);
    assert_eq!(y, [0.0, 0.0, 0.0, 1.0, 1.0]);
}

#[test]
fn a_step_series_that_never_changes_has_one_point_per_instant() {
    let (x, y) = step_series(&[0.0, 5.0, 9.0], &[2.5, 2.5, 2.5]);
    assert_eq!(x, [0.0, 5.0, 9.0]);
    assert_eq!(y, [2.5, 2.5, 2.5]);
}

#[test]
fn the_corners_inside_the_window_join_the_instants_and_inherit_from_the_one_before() {
    let lower = curve(&[(-1.0, 0.0), (11.0, 0.0), (11.0, 1.0), (21.0, 1.0)]);
    let upper = curve(&[(-1.0, 1.0), (9.0, 1.0), (9.0, 2.0), (21.0, 2.0)]);
    let (instants, origin) = with_corners(&[0.0, 10.0, 13.0, 20.0], &lower, &upper, 0.0, 20.0);
    assert_eq!(instants, [0.0, 9.0, 10.0, 11.0, 13.0, 20.0]);
    assert_eq!(origin, [0, 0, 1, 1, 2, 3]);
}

#[test]
fn a_bound_is_read_from_inside_each_stretch_at_a_vertical() {
    // The lower bound rises at 11 from 0 to 1: from the left it is 0 there, from the right 1.
    let lower = curve(&[(-1.0, 0.0), (11.0, 0.0), (11.0, 1.0), (21.0, 1.0)]);
    assert_eq!(bound_value(&lower, 11.0, false), 0.0);
    assert_eq!(bound_value(&lower, 11.0, true), 1.0);
    // Held before the first corner and after the last.
    assert_eq!(bound_value(&lower, -5.0, true), 0.0);
    assert_eq!(bound_value(&lower, 30.0, false), 1.0);
    // On a sloped segment, the line through the two corners.
    let sloped = curve(&[(0.0, 0.0), (4.0, 2.0)]);
    assert_eq!(bound_value(&sloped, 1.0, true), 0.5);
    assert_eq!(bound_value(&sloped, 1.0, false), 0.5);
}

#[test]
fn each_instant_stands_for_the_largest_error_over_the_stretch_to_the_next() {
    // The band of a step from 0 to 1 at 10 under a 1 s, 0.1 x tolerance.
    let lower = curve(&[(-1.0, -0.1), (11.0, -0.1), (11.0, 0.9), (21.0, 0.9)]);
    let upper = curve(&[(-1.0, 0.1), (9.0, 0.1), (9.0, 1.1), (21.0, 1.1)]);
    // A test that steps 3 s late, at 13.
    let instants = [0.0, 9.0, 10.0, 11.0, 13.0, 20.0];
    let test = [0.0, 0.0, 0.0, 0.0, 1.0, 1.0];
    let errors = piece_exceedances(&instants, &test, &lower, &upper);
    // From 11 to 13 the test is still 0 under a lower bound of 0.9.
    assert_eq!(errors, [0.0, 0.0, 0.0, 0.9, 0.0, 0.0]);
}

#[test]
fn a_step_one_tolerance_late_is_inside_at_the_vertical() {
    let lower = curve(&[(-1.0, -0.1), (11.0, -0.1), (11.0, 0.9), (21.0, 0.9)]);
    let upper = curve(&[(-1.0, 0.1), (9.0, 0.1), (9.0, 1.1), (21.0, 1.1)]);
    let instants = [0.0, 9.0, 10.0, 11.0, 20.0];
    let test = [0.0, 0.0, 0.0, 1.0, 1.0];
    let errors = piece_exceedances(&instants, &test, &lower, &upper);
    assert_eq!(errors, [0.0; 5]);
}

#[test]
fn the_last_instant_reads_the_bounds_from_its_left() {
    let lower = curve(&[(0.0, 0.0), (5.0, 0.0), (5.0, 3.0), (6.0, 3.0)]);
    let upper = curve(&[(0.0, 1.0), (6.0, 4.0)]);
    let errors = piece_exceedances(&[0.0, 5.0], &[0.5, 0.5], &lower, &upper);
    // The last instant, 5, reads the lower bound's 0 from the left, not its 3.
    assert_eq!(errors[1], 0.0);
}

#[test]
fn within_counts_the_times_in_the_closed_window() {
    let times = [0.0, 1.0, 1.0, 2.0, 3.0];
    assert_eq!(within(&times, 1.0, 2.0), 3);
    assert_eq!(within(&times, 5.0, 6.0), 0);
    assert_eq!(within(&[], 0.0, 1.0), 0);
}

#[test]
fn a_bound_that_jumps_and_then_falls_away_is_read_at_the_stretchs_start() {
    // The lower bound rises at 11 to 1 and falls back to 0 by 13: over the stretch from 11 to
    // 13 a test at 0.5 is outside by 0.5 just after 11, and inside at 13. Only the reading at
    // the stretch's start, from its right, sees it.
    let lower = curve(&[
        (-1.0, 0.0),
        (11.0, 0.0),
        (11.0, 1.0),
        (13.0, 0.0),
        (21.0, 0.0),
    ]);
    let upper = curve(&[(-1.0, 2.0), (21.0, 2.0)]);
    let errors = piece_exceedances(&[0.0, 11.0, 13.0, 20.0], &[0.5; 4], &lower, &upper);
    assert_eq!(errors, [0.0, 0.5, 0.0, 0.0]);
}

#[test]
fn the_snap_radius_is_four_epsilon_of_the_magnitude_and_half_width() {
    assert_eq!(snap_radius(30.0, 1.0), 4.0 * f64::EPSILON * 31.0);
    // About 8.9e-16 s per second of magnitude and half-width.
    assert!(snap_radius(3_600.0, 60.0) < 3.3e-12);
    assert!(snap_radius(30.0, 4.0) < 3.1e-14);
}

#[test]
fn a_corner_within_the_radius_lies_on_the_nearest_instant_and_one_beyond_does_not() {
    let radius = snap_radius(30.0, 1.0);
    let instant = 6.0_f64;
    // The farthest doubles on each side still within the radius, and the next ones out.
    let mut within = instant - radius;
    while instant - within > radius {
        within = within.next_up();
    }
    let mut above = instant + radius;
    while above - instant > radius {
        above = above.next_down();
    }
    let beyond = within.next_down();
    let after = above.next_up();
    assert!(instant - beyond > radius && after - instant > radius);
    let bound = curve(&[(beyond, 0.0), (within, 1.0), (above, 2.0), (after, 3.0)]);
    let moved = snapped(&bound, &[0.0, instant, 30.0], radius);
    assert_eq!(moved.x(), [beyond, instant, instant, after]);
    assert_eq!(moved.y(), bound.y(), "only x moves");
}

#[test]
fn snapping_moves_corners_only_and_never_merges_two_series_instants() {
    // Two series instants one unit in the last place apart: a corner between them goes to the
    // nearer, the earlier on a tie, and both instants stay instants.
    let a = 6.0_f64;
    let b = a.next_up();
    let radius = snap_radius(30.0, 1.0);
    let bound = curve(&[(-1.0, 0.0), (a, 0.0), (b, 1.0), (31.0, 1.0)]);
    let moved = snapped(&bound, &[0.0, a, b, 30.0], radius);
    assert_eq!(moved.x(), [-1.0, a, b, 31.0]);
    let (instants, _) = with_corners(&[0.0, a, b, 30.0], &moved, &moved, 0.0, 30.0);
    assert_eq!(instants, [0.0, a, b, 30.0]);
    // A corner exactly between two instants 2 ulps apart takes the earlier.
    let c = b.next_up();
    let between = curve(&[(b, 0.0), (b, 1.0)]);
    let moved = snapped(&between, &[a, c], radius);
    assert_eq!(moved.x(), [a, a]);
}

#[test]
fn a_bound_that_closes_in_across_a_stretch_is_read_at_the_stretchs_end() {
    // The lower bound rises from 0 to 1 over the stretch from 0 to 10, with no corner inside:
    // a test at 0.5 is inside at the start and outside by 0.5 at the end. Only the reading at
    // the stretch's end, from its left, sees it.
    let lower = curve(&[(-1.0, 0.0), (0.0, 0.0), (10.0, 1.0), (11.0, 1.0)]);
    let upper = curve(&[(-1.0, 2.0), (11.0, 2.0)]);
    let errors = piece_exceedances(&[0.0, 10.0], &[0.5, 1.0], &lower, &upper);
    assert_eq!(errors, [0.5, 0.0]);
}
