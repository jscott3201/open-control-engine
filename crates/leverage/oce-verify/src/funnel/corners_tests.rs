//! Corner selection, against outlines derived by hand from funnel's rules. Every `x` here is a
//! binary fraction, so each expected corner is exact.

use super::*;

/// The outline of `side` around `points` with one half-width and one half-height.
fn outline_of(side: Side, points: &[(f64, f64)], half_x: f64, half_y: f64) -> Vec<(f64, f64)> {
    let x: Vec<f64> = points.iter().map(|point| point.0).collect();
    let y: Vec<f64> = points.iter().map(|point| point.1).collect();
    outline(
        side,
        &x,
        &y,
        &vec![half_x; points.len()],
        &vec![half_y; points.len()],
    )
    .into_iter()
    .map(|corner| (corner.x, corner.y))
    .collect()
}

#[test]
fn a_single_point_is_the_bottom_and_the_top_of_its_rectangle() {
    assert_eq!(
        outline_of(Side::Lower, &[(4.0, -8.0)], 0.5, 2.0),
        [(3.5, -10.0), (4.5, -10.0)]
    );
    assert_eq!(
        outline_of(Side::Upper, &[(4.0, -8.0)], 0.5, 2.0),
        [(3.5, -6.0), (4.5, -6.0)]
    );
}

#[test]
fn a_rise_takes_right_corners_below_and_left_corners_above() {
    let rise = [(0.0, 0.0), (4.0, 2.0), (8.0, 6.0)];
    // Below, the reference moves away at once: the first rectangle gives both bottom corners.
    assert_eq!(
        outline_of(Side::Lower, &rise, 1.0, 0.5),
        [(-1.0, -0.5), (1.0, -0.5), (5.0, 1.5), (9.0, 5.5)]
    );
    // Above, it moves towards the bound: the last rectangle gives both top corners.
    assert_eq!(
        outline_of(Side::Upper, &rise, 1.0, 0.5),
        [(-1.0, 0.5), (3.0, 2.5), (7.0, 6.5), (9.0, 6.5)]
    );
}

#[test]
fn a_fall_mirrors_a_rise() {
    let fall = [(0.0, 6.0), (4.0, 2.0), (8.0, 0.0)];
    assert_eq!(
        outline_of(Side::Lower, &fall, 1.0, 0.5),
        [(-1.0, 5.5), (3.0, 1.5), (7.0, -0.5), (9.0, -0.5)]
    );
    assert_eq!(
        outline_of(Side::Upper, &fall, 1.0, 0.5),
        [(-1.0, 6.5), (1.0, 6.5), (5.0, 2.5), (9.0, 0.5)]
    );
}

#[test]
fn a_straight_run_adds_no_corner_between_its_ends() {
    let line = [(0.0, 0.0), (1.0, 2.0), (2.0, 4.0), (3.0, 6.0), (4.0, 8.0)];
    assert_eq!(
        outline_of(Side::Upper, &line, 0.5, 1.0),
        [(-0.5, 1.0), (3.5, 9.0), (4.5, 9.0)]
    );
    let flat = [(0.0, 3.0), (1.0, 3.0), (2.0, 3.0), (5.0, 3.0)];
    for (side, level) in [(Side::Lower, 2.0), (Side::Upper, 4.0)] {
        assert_eq!(
            outline_of(side, &flat, 0.5, 1.0),
            [(-0.5, level), (5.5, level)]
        );
    }
}

#[test]
fn a_peak_adds_two_corners_in_the_order_the_outline_meets_them() {
    let peak = [(0.0, 0.0), (2.0, 0.0), (3.0, 4.0), (4.0, 0.0), (8.0, 0.0)];
    // Above, left then right: the outline walks over the peak.
    assert_eq!(
        outline_of(Side::Upper, &peak, 1.5, 0.5),
        [
            (-1.5, 0.5),
            (0.5, 0.5),
            (1.5, 4.5),
            (4.5, 4.5),
            (5.5, 0.5),
            (9.5, 0.5)
        ]
    );
    // Below, right then left: with rectangles wider than the peak the outline runs backwards
    // from x = 4.5 to x = 1.5, the loop the next step removes.
    assert_eq!(
        outline_of(Side::Lower, &peak, 1.5, 0.5),
        [
            (-1.5, -0.5),
            (3.5, -0.5),
            (4.5, 3.5),
            (1.5, 3.5),
            (2.5, -0.5),
            (9.5, -0.5)
        ]
    );
}

#[test]
fn a_valley_adds_two_corners_and_the_first_fall_adds_the_first_right_corner() {
    let valley = [(0.0, 4.0), (4.0, 0.0), (8.0, 4.0)];
    assert_eq!(
        outline_of(Side::Upper, &valley, 3.0, 0.0),
        [
            (-3.0, 4.0),
            (3.0, 4.0),
            (7.0, 0.0),
            (1.0, 0.0),
            (5.0, 4.0),
            (11.0, 4.0)
        ]
    );
    assert_eq!(
        outline_of(Side::Lower, &valley, 3.0, 0.0),
        [(-3.0, 4.0), (1.0, 0.0), (7.0, 0.0), (11.0, 4.0)]
    );
}

#[test]
fn a_vertical_step_moves_the_upper_bound_left_and_the_lower_bound_right() {
    // Two points share x = 4: the slope there is the sentinel, which differs from the flat
    // slopes around it, so both points add a corner.
    let step = [(0.0, 0.0), (4.0, 0.0), (4.0, 2.0), (8.0, 2.0)];
    assert_eq!(
        outline_of(Side::Upper, &step, 1.0, 0.5),
        [(-1.0, 0.5), (3.0, 0.5), (3.0, 2.5), (9.0, 2.5)]
    );
    assert_eq!(
        outline_of(Side::Lower, &step, 1.0, 0.5),
        [(-1.0, -0.5), (5.0, -0.5), (5.0, 1.5), (9.0, 1.5)]
    );
    // Stepping down, the roles swap.
    let down = [(0.0, 2.0), (4.0, 2.0), (4.0, 0.0), (8.0, 0.0)];
    assert_eq!(
        outline_of(Side::Upper, &down, 1.0, 0.5),
        [(-1.0, 2.5), (5.0, 2.5), (5.0, 0.5), (9.0, 0.5)]
    );
    assert_eq!(
        outline_of(Side::Lower, &down, 1.0, 0.5),
        [(-1.0, 1.5), (3.0, 1.5), (3.0, -0.5), (9.0, -0.5)]
    );
}

#[test]
fn the_slope_of_a_vertical_step_is_the_sentinel_with_the_sign_of_the_step() {
    let x = [1.0, 1.0, 1.0, 1.0 + 5e-11];
    let y = [0.0, 2.0, -1.0, -1.0];
    assert_eq!(slope(&x, &y, 0), (1, 1e15));
    assert_eq!(slope(&x, &y, 1), (-1, -1e15));
    // Equal by funnel's equality, not by `==`: still vertical, and a flat step counts as down.
    assert_eq!(slope(&x, &y, 2), (0, -1e15));
    assert_eq!(slope(&[0.0, 0.5], &[1.0, 2.0], 0), (1, 2.0));
}

#[test]
fn points_equal_to_the_next_one_are_skipped_at_the_start_and_inside() {
    let plain = [(0.0, 1.0), (4.0, 3.0), (8.0, 3.0)];
    let repeated = [
        (0.0, 1.0),
        (0.0, 1.0),
        (0.0, 1.0),
        (4.0, 3.0),
        (4.0, 3.0),
        (8.0, 3.0),
        (8.0, 3.0),
    ];
    for side in [Side::Lower, Side::Upper] {
        assert_eq!(
            outline_of(side, &repeated, 1.0, 0.5),
            outline_of(side, &plain, 1.0, 0.5),
            "{side:?}"
        );
    }
    // A series that is one point repeated is that point.
    assert_eq!(
        outline_of(Side::Upper, &[(2.0, 1.0), (2.0, 1.0), (2.0, 1.0)], 1.0, 0.5),
        [(1.0, 1.5), (3.0, 1.5)]
    );
}

#[test]
fn corners_that_would_continue_a_flat_run_are_dropped_again() {
    // Wiggles of 5e-11 on a fine grid: the slope changes at every point, so corners are added,
    // and the level is unchanged by funnel's equality, so they are dropped. What is left is the
    // ends, as for a flat run.
    let x: Vec<f64> = (0..6_u8).map(|index| f64::from(index) / 1024.0).collect();
    let y = [1.0, 1.0, 1.0 + 5e-11, 1.0, 1.0 + 5e-11, 1.0];
    let half_x = vec![1.0 / 4096.0; 6];
    let half_y = vec![0.25; 6];
    let points = |side| -> Vec<(f64, f64)> {
        outline(side, &x, &y, &half_x, &half_y)
            .into_iter()
            .map(|corner| (corner.x, corner.y))
            .collect()
    };
    let (start, end) = (-1.0 / 4096.0, 5.0 / 1024.0 + 1.0 / 4096.0);
    assert_eq!(points(Side::Upper), [(start, 1.25), (end, 1.25)]);
    // The series ends falling, which is towards the lower bound: the last rectangle gives its
    // left corner as well.
    assert_eq!(
        points(Side::Lower),
        [
            (start, 0.75),
            (5.0 / 1024.0 - 1.0 / 4096.0, 0.75),
            (end, 0.75)
        ]
    );
}

#[test]
fn the_flat_run_rule_drops_two_corners_at_an_extremum_and_one_otherwise() {
    let at = |y: f64| Corner { x: 0.0, y };
    // An extremum added two corners; the corner before them is at the same level.
    let mut corners = vec![at(1.0), at(1.0), at(1.0)];
    drop_flat_run(&mut corners, 1.0, true);
    assert_eq!(corners.len(), 1);
    // Not an extremum: one corner goes.
    let mut corners = vec![at(1.0), at(1.0), at(1.0)];
    drop_flat_run(&mut corners, 1.0, false);
    assert_eq!(corners.len(), 2);
    // The next level differs: nothing goes.
    let mut corners = vec![at(1.0), at(1.0), at(1.0)];
    drop_flat_run(&mut corners, 2.0, true);
    assert_eq!(corners.len(), 3);
    // The corner before the added ones is at another level: nothing goes.
    let mut corners = vec![at(0.0), at(1.0), at(1.0)];
    drop_flat_run(&mut corners, 1.0, true);
    assert_eq!(corners.len(), 3);
    let mut corners = vec![at(0.0), at(1.0)];
    drop_flat_run(&mut corners, 1.0, false);
    assert_eq!(corners.len(), 2);
    // Too few corners to hold the ones the rule would drop.
    let mut corners = vec![at(1.0), at(1.0)];
    drop_flat_run(&mut corners, 1.0, true);
    assert_eq!(corners.len(), 2);
    let mut corners = vec![at(1.0)];
    drop_flat_run(&mut corners, 1.0, false);
    assert_eq!(corners.len(), 1);
}

#[test]
fn every_pair_of_slope_signs_has_funnels_corners_on_each_side() {
    use Edge::{Left, Right};
    type Row = ((i8, i8), &'static [Edge], &'static [Edge]);
    let table: [Row; 9] = [
        // (before, after), lower, upper
        ((1, 1), &[Right], &[Left]),
        ((1, 0), &[Right], &[Left]),
        ((0, 1), &[Right], &[Left]),
        ((0, 0), &[Right], &[Left]),
        ((-1, -1), &[Left], &[Right]),
        ((-1, 0), &[Left], &[Right]),
        ((0, -1), &[Left], &[Right]),
        ((-1, 1), &[Left, Right], &[Right, Left]),
        ((1, -1), &[Right, Left], &[Left, Right]),
    ];
    for ((before, after), lower, upper) in table {
        assert_eq!(interior_corners(Side::Lower, before, after), lower);
        assert_eq!(interior_corners(Side::Upper, before, after), upper);
    }
}

#[test]
fn the_sign_of_a_zero_of_either_sign_is_zero() {
    assert_eq!(sign(0.0), 0);
    assert_eq!(sign(-0.0), 0);
    assert_eq!(sign(f64::MIN_POSITIVE), 1);
    assert_eq!(sign(-5e-324), -1);
}
