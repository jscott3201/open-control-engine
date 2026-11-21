//! The grid envelope's parts: summed rectangles, a hand-derived band, curve evaluation against
//! the original's scan, and the two signed-zero rules.

use super::*;

/// The original's `eval_curve` (`oce-conformance`, `funnel.rs`), kept verbatim as the oracle for
/// the binary search in [`value_at`].
fn value_at_by_scanning(curve: &[(f64, f64)], x: f64) -> f64 {
    if x <= curve[0].0 {
        return curve[0].1;
    }
    for w in curve.windows(2) {
        let (x0, y0) = w[0];
        let (x1, y1) = w[1];
        if x == x1 {
            return y1;
        }
        if x <= x1 {
            if x1 == x0 {
                return y1;
            }
            let a = (x - x0) / (x1 - x0);
            return y0 + a * (y1 - y0);
        }
    }
    curve[curve.len() - 1].1
}

/// A deterministic 64-bit generator (SplitMix64).
struct Sequence(u64);

impl Sequence {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: u32) -> f64 {
        f64::from((self.next() % u64::from(bound)) as u32)
    }
}

#[test]
fn a_rectangles_half_sizes_are_the_sum_of_the_three_terms() {
    let reference = [(8.0, -8.0), (12.0, 0.0), (16.0, -2.0)];
    let tolerances = Tolerances {
        atolx: 1.25,
        rtolx: 0.125, // × range 8 = 1.0
        ltolx: 0.125, // × |x| = 1.0, 1.5, 2.0
        atoly: 0.75,
        rtoly: 0.125, // × range 8 = 1.0
        ltoly: 0.25,  // × |y| = 2.0, 0.0, 0.5
    };
    let rectangles = rectangles(&reference, &tolerances).expect("finite");
    let half_widths: Vec<f64> = rectangles.iter().map(|r| r.half_width).collect();
    let half_heights: Vec<f64> = rectangles.iter().map(|r| r.half_height).collect();
    // The funnel takes the maximum of the same terms: 1.25, 1.5, 2.0 and 2.0, 1.0, 1.0.
    assert_eq!(half_widths, [3.25, 3.75, 4.25]);
    assert_eq!(half_heights, [3.75, 1.75, 2.25]);
}

#[test]
fn with_no_tolerance_a_rectangle_is_its_point_and_there_is_no_floor() {
    let reference = [(0.0, 1.0), (1.0, 3.0)];
    let rectangles = rectangles(&reference, &Tolerances::ZERO).expect("finite");
    assert!(
        rectangles
            .iter()
            .all(|r| r.half_width == 0.0 && r.half_height == 0.0)
    );
    let (lower, upper) = build_bounds(&reference, &Tolerances::ZERO).expect("finite");
    assert_eq!(lower, reference);
    assert_eq!(upper, reference);
}

#[test]
fn the_band_is_the_extreme_of_every_rectangle_covering_each_grid_point() {
    // A peak, half-width 1, half-height 0.5. The grid is every x and every x ± 1.
    let reference = [(0.0, 0.0), (2.0, 4.0), (4.0, 0.0)];
    let tolerances = Tolerances {
        atolx: 1.0,
        atoly: 0.5,
        ..Tolerances::ZERO
    };
    let (lower, upper) = build_bounds(&reference, &tolerances).expect("finite");
    let xs: Vec<f64> = lower.iter().map(|point| point.0).collect();
    assert_eq!(xs, [-1.0, 0.0, 1.0, 2.0, 3.0, 4.0, 5.0]);
    assert_eq!(upper.iter().map(|point| point.0).collect::<Vec<_>>(), xs);
    // At x = 1 the rectangles of the first two points both cover: the lower bound is the first
    // one's bottom and the upper bound the peak's top, though the reference there is 2. At
    // x = 2 only the peak's own rectangle covers.
    assert_eq!(
        lower.iter().map(|point| point.1).collect::<Vec<_>>(),
        [-0.5, -0.5, -0.5, 3.5, -0.5, -0.5, -0.5]
    );
    assert_eq!(
        upper.iter().map(|point| point.1).collect::<Vec<_>>(),
        [0.5, 0.5, 4.5, 4.5, 4.5, 0.5, 0.5]
    );
}

#[test]
fn a_rectangle_covers_its_own_edges() {
    let reference = [(0.0, 0.0), (2.0, 10.0)];
    let tolerances = Tolerances {
        atolx: 2.0,
        ..Tolerances::ZERO
    };
    let (lower, upper) = build_bounds(&reference, &tolerances).expect("finite");
    // The first rectangle spans [-2, 2] and the second [0, 4]: at x = 0 and x = 2 both cover.
    assert_eq!(lower, [(-2.0, 0.0), (0.0, 0.0), (2.0, 0.0), (4.0, 10.0)]);
    assert_eq!(upper, [(-2.0, 0.0), (0.0, 10.0), (2.0, 10.0), (4.0, 10.0)]);
}

#[test]
fn a_curve_is_evaluated_as_the_originals_scan_evaluates_it() {
    let mut sequence = Sequence(655);
    let mut compared = 0;
    for _ in 0..2_000 {
        // Non-decreasing x on a half-unit grid with repeats, values on a quarter grid.
        let count = 1 + sequence.next() % 12;
        let mut x = sequence.below(4) * 0.5 - 1.0;
        let curve: Vec<(f64, f64)> = (0..count)
            .map(|_| {
                x += sequence.below(3) * 0.5;
                (x, sequence.below(9) * 0.25 - 1.0)
            })
            .collect();
        let (first, last) = (curve[0].0, curve[curve.len() - 1].0);
        for step in 0..=40_u8 {
            // From before the first point to beyond the last, on and between the grid.
            let at = first - 1.0 + f64::from(step) * (last - first + 2.0) / 40.0;
            for target in [at, (at * 2.0).round() / 2.0] {
                assert_eq!(
                    value_at(&curve, target).to_bits(),
                    value_at_by_scanning(&curve, target).to_bits(),
                    "{curve:?} at {target}"
                );
                compared += 1;
            }
        }
    }
    assert_eq!(compared, 2_000 * 41 * 2);
}

#[test]
fn a_curve_is_held_outside_its_ends_and_takes_the_first_of_two_points_at_one_x() {
    let curve = [(0.0, 1.0), (2.0, 5.0), (2.0, 9.0), (4.0, 9.0)];
    assert_eq!(value_at(&curve, -3.0), 1.0);
    assert_eq!(value_at(&curve, 0.0), 1.0);
    assert_eq!(value_at(&curve, 1.0), 3.0);
    // The scan stops at the first point whose x is the target's.
    assert_eq!(value_at(&curve, 2.0), 5.0);
    assert_eq!(value_at(&curve, 3.0), 9.0);
    assert_eq!(value_at(&curve, 4.0), 9.0);
    assert_eq!(value_at(&curve, 1e300), 9.0);
    assert_eq!(value_at(&[(7.0, -2.0)], 0.0), -2.0);
    assert_eq!(value_at(&[(7.0, -2.0)], 8.0), -2.0);
}

#[test]
fn the_smaller_and_the_larger_of_two_zeros_do_not_depend_on_their_order() {
    let negative = (-0.0_f64).to_bits();
    let positive = 0.0_f64.to_bits();
    assert_eq!(smaller(0.0, -0.0).to_bits(), negative);
    assert_eq!(smaller(-0.0, 0.0).to_bits(), negative);
    assert_eq!(larger(0.0, -0.0).to_bits(), positive);
    assert_eq!(larger(-0.0, 0.0).to_bits(), positive);
    assert_eq!(smaller(0.0, 0.0).to_bits(), positive);
    assert_eq!(larger(-0.0, -0.0).to_bits(), negative);
    assert_eq!((smaller(1.0, -2.0), smaller(-2.0, 1.0)), (-2.0, -2.0));
    assert_eq!((larger(1.0, -2.0), larger(-2.0, 1.0)), (1.0, 1.0));
}

#[test]
fn a_bound_over_zeros_of_both_signs_is_the_same_on_every_target() {
    // A reference of -0.0 with no height: every rectangle's bottom is -0.0 - 0.0 = -0.0 and its
    // top -0.0 + 0.0 = +0.0, and between the samples the reference interpolates to +0.0. So the
    // lower bound at x = 1 is the smaller of +0.0 and -0.0, and the upper bound at a sample the
    // larger of -0.0 and +0.0: the two choices the original leaves to the platform's `min` and
    // `max`. Here they are -0.0 and +0.0.
    let reference = [(0.0, -0.0), (2.0, -0.0)];
    let tolerances = Tolerances {
        atolx: 1.0,
        ..Tolerances::ZERO
    };
    let (lower, upper) = build_bounds(&reference, &tolerances).expect("finite");
    let bits = |curve: &[(f64, f64)]| -> Vec<u64> { curve.iter().map(|p| p.1.to_bits()).collect() };
    assert_eq!(
        lower.iter().map(|p| p.0).collect::<Vec<_>>(),
        [-1.0, 0.0, 1.0, 2.0, 3.0]
    );
    assert_eq!(value_at(&reference, 1.0).to_bits(), 0.0_f64.to_bits());
    assert_eq!(bits(&lower), [(-0.0_f64).to_bits(); 5]);
    assert_eq!(bits(&upper), [0.0_f64.to_bits(); 5]);
}

#[test]
fn the_error_is_the_distance_outside_the_band_and_a_positive_zero_inside() {
    let lower = [(0.0, 0.0), (4.0, 0.0)];
    let upper = [(0.0, 1.0), (4.0, 1.0)];
    assert_eq!(error_at(2.0, 1.75, &lower, &upper), Ok(0.75));
    assert_eq!(error_at(2.0, -0.5, &lower, &upper), Ok(0.5));
    for inside in [0.0, -0.0, 0.5, 1.0] {
        let error = error_at(2.0, inside, &lower, &upper).expect("finite");
        assert_eq!(error.to_bits(), 0.0_f64.to_bits(), "{inside}");
    }
    // Where the bounds cross, both distances count, as in the original's formula.
    let crossed_lower = [(0.0, 2.0), (4.0, 2.0)];
    assert_eq!(error_at(2.0, 1.5, &crossed_lower, &upper), Ok(1.0));
}

#[test]
fn a_rectangle_or_an_error_that_overflows_is_refused() {
    // The range in y is infinite, so every half-height is infinite or NaN (0 × infinity), with
    // or without a tolerance.
    let spanning = [(0.0, f64::MAX), (1.0, -f64::MAX)];
    for rtoly in [0.5, 0.0] {
        let tolerances = Tolerances {
            rtoly,
            ..Tolerances::ZERO
        };
        assert_eq!(
            rectangles(&spanning, &tolerances).err(),
            Some(CompareError::NonFiniteEnvelope)
        );
    }
    // A finite band at the largest double: a test value at the other extreme is an infinite
    // distance away.
    let top = [(0.0, f64::MAX), (1.0, f64::MAX)];
    let (lower, upper) = build_bounds(&top, &Tolerances::ZERO).expect("finite rectangles");
    assert_eq!(
        error_at(0.0, -f64::MAX, &lower, &upper),
        Err(CompareError::NonFiniteEnvelope)
    );
    assert_eq!(error_at(0.0, f64::MAX, &lower, &upper), Ok(0.0));
}
