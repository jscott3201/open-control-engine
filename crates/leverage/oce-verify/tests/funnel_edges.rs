//! The funnel at its edges, through the public surface: every tolerance kind alone, the two
//! fallbacks for a half-size of zero, a vertical step, leading duplicates and a loop at a sharp
//! peak. Its refusals and its caps are in `tests/funnel_refusals.rs`.
//!
//! The expected bounds are worked out by hand from funnel's rules. Every `x` is a binary
//! fraction of a magnitude that is a power of two, so the normalisation is exact and so is
//! each expected corner.

mod support;

use oce_verify::{
    CompareError, CompareLimits, Curve, FUNNEL_VERSION, FunnelResult, Series, Tolerances, funnel,
};

/// The funnel of `test` against `reference` under the default limits.
fn compare(
    reference: &[(f64, f64)],
    test: &[(f64, f64)],
    tolerances: Tolerances,
) -> Result<FunnelResult, CompareError> {
    compare_under(reference, test, tolerances, &CompareLimits::default())
}

fn compare_under(
    reference: &[(f64, f64)],
    test: &[(f64, f64)],
    tolerances: Tolerances,
    limits: &CompareLimits,
) -> Result<FunnelResult, CompareError> {
    let columns =
        |points: &[(f64, f64)]| -> (Vec<f64>, Vec<f64>) { points.iter().copied().unzip() };
    let (reference, test) = (columns(reference), columns(test));
    funnel(
        Series::new(&reference.0, &reference.1),
        Series::new(&test.0, &test.1),
        &tolerances,
        limits,
    )
}

fn points(curve: &Curve) -> Vec<(f64, f64)> {
    curve.points().collect()
}

/// The one rectangle of a single-point reference, read off its bounds as `[left, right]` and
/// `[bottom, top]`: the lower bound is the rectangle's bottom edge and the upper bound its top.
fn rectangle_of(point: (f64, f64), tolerances: Tolerances) -> ([f64; 2], [f64; 2]) {
    let result = compare(&[point], &[point], tolerances).expect("a single point compares");
    let (lower, upper) = (points(&result.lower_bound), points(&result.upper_bound));
    assert_eq!((lower.len(), upper.len()), (2, 2));
    assert_eq!((lower[0].0, lower[1].0), (upper[0].0, upper[1].0));
    assert_eq!((lower[0].1, upper[0].1), (lower[1].1, upper[1].1));
    assert!(result.within_envelope());
    ([lower[0].0, lower[1].0], [lower[0].1, upper[0].1])
}

#[test]
fn the_version_string_names_the_pinned_funnel_commit() {
    assert_eq!(FUNNEL_VERSION, "funnel@a12fa96");
}

#[test]
fn each_tolerance_kind_alone_sizes_the_rectangle_on_its_own_axis() {
    // A single point at (4, -8): its range is zero on both axes and its magnitudes are 4 and 8.
    let point = (4.0, -8.0);
    let zero = Tolerances::ZERO;
    for tolerances in [
        Tolerances { atolx: 0.5, ..zero },
        // The range is zero, so a range-relative tolerance falls back to the magnitude.
        Tolerances {
            rtolx: 0.125,
            ..zero
        },
        Tolerances {
            ltolx: 0.125,
            ..zero
        },
    ] {
        assert_eq!(
            rectangle_of(point, tolerances).0,
            [3.5, 4.5],
            "{tolerances:?}"
        );
    }
    for tolerances in [
        Tolerances { atoly: 2.0, ..zero },
        Tolerances {
            rtoly: 0.25,
            ..zero
        },
        Tolerances {
            ltoly: 0.25,
            ..zero
        },
    ] {
        assert_eq!(
            rectangle_of(point, tolerances).1,
            [-10.0, -6.0],
            "{tolerances:?}"
        );
    }
}

#[test]
fn the_terms_of_an_axis_combine_by_maximum() {
    let all = Tolerances {
        atolx: 0.5,
        rtolx: 0.125, // × range 0 = 0
        ltolx: 0.25,  // × |4| = 1.0
        atoly: 1.0,
        rtoly: 0.25,  // × range 0 = 0
        ltoly: 0.375, // × |-8| = 3.0
    };
    // The largest term on each axis, 1.0 and 3.0; the sums would be 1.5 and 4.0.
    assert_eq!(rectangle_of((4.0, -8.0), all), ([3.0, 5.0], [-11.0, -5.0]));
}

#[test]
fn with_no_tolerance_every_rectangle_is_the_floor_of_1e_minus_10() {
    // At the origin nothing is normalised, so the corners are exact.
    let result = compare(&[(0.0, 0.0)], &[(0.0, 0.0)], Tolerances::ZERO).expect("compares");
    assert_eq!(
        points(&result.lower_bound),
        [(-1e-10, -1e-10), (1e-10, -1e-10)]
    );
    assert_eq!(
        points(&result.upper_bound),
        [(-1e-10, 1e-10), (1e-10, 1e-10)]
    );
    assert!(result.within_envelope());

    // The floor is a band: a test value 1e-10 away is inside it, 3e-10 away is outside.
    let inside = compare(&[(0.0, 0.0)], &[(0.0, 1e-10)], Tolerances::ZERO).expect("compares");
    assert!(inside.within_envelope());
    let outside = compare(&[(0.0, 0.0)], &[(0.0, 3e-10)], Tolerances::ZERO).expect("compares");
    assert_eq!(outside.max_error, 3e-10 - 1e-10);
    assert_eq!(outside.first_error_x, Some(0.0));
}

#[test]
fn a_constant_series_takes_rtol_times_its_magnitude_and_a_zero_series_the_floor() {
    let flat =
        |value: f64| -> Vec<(f64, f64)> { (0..5_u8).map(|k| (f64::from(k), value)).collect() };
    let relative = Tolerances {
        rtoly: 0.25,
        atolx: 0.5,
        ..Tolerances::ZERO
    };
    // Range zero, magnitude 16: the half-height is 0.25 × 16 = 4.
    let constant = compare(&flat(-16.0), &flat(-16.0), relative).expect("compares");
    assert_eq!(points(&constant.lower_bound), [(-0.5, -20.0), (4.5, -20.0)]);
    assert_eq!(points(&constant.upper_bound), [(-0.5, -12.0), (4.5, -12.0)]);
    // Range zero and magnitude zero: 0.25 × 0 is zero again, so the floor applies.
    let zero = compare(&flat(0.0), &flat(0.0), relative).expect("compares");
    assert_eq!(points(&zero.lower_bound), [(-0.5, -1e-10), (4.5, -1e-10)]);
    assert_eq!(points(&zero.upper_bound), [(-0.5, 1e-10), (4.5, 1e-10)]);
}

#[test]
fn a_vertical_step_may_come_up_to_the_x_tolerance_early_or_late() {
    let reference = [(0.0, 0.0), (4.0, 0.0), (4.0, 2.0), (8.0, 2.0)];
    let tolerances = Tolerances {
        atolx: 1.0,
        atoly: 0.5,
        ..Tolerances::ZERO
    };
    let on_time = compare(&reference, &reference, tolerances).expect("compares");
    // The upper bound steps one unit early and the lower bound one unit late.
    assert_eq!(
        points(&on_time.upper_bound),
        [(-1.0, 0.5), (3.0, 0.5), (3.0, 2.5), (9.0, 2.5)]
    );
    assert_eq!(
        points(&on_time.lower_bound),
        [(-1.0, -0.5), (5.0, -0.5), (5.0, 1.5), (9.0, 1.5)]
    );
    assert!(on_time.within_envelope());

    let stepping_at = |x: f64| {
        let test = [(0.0, 0.0), (x, 0.0), (x, 2.0), (8.0, 2.0)];
        compare(&reference, &test, tolerances).expect("compares")
    };
    assert!(stepping_at(3.5).within_envelope());
    assert!(stepping_at(4.75).within_envelope());
    // Half a unit beyond the tolerance, on either side, the step is outside by 1.5.
    let early = stepping_at(2.5);
    assert_eq!((early.max_error, early.max_error_x), (1.5, Some(2.5)));
    assert_eq!(early.first_error_x, Some(2.5));
    let late = stepping_at(5.5);
    assert_eq!((late.max_error, late.max_error_x), (1.5, Some(5.5)));
    // At the edge of the tolerance funnel is strict: at x = 3 the upper bound is still 0.5.
    assert_eq!(stepping_at(3.0).max_error, 1.5);
}

#[test]
fn leading_and_repeated_points_do_not_move_the_bounds() {
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
    let tolerances = Tolerances {
        atolx: 1.0,
        atoly: 0.5,
        ..Tolerances::ZERO
    };
    let expected = compare(&plain, &plain, tolerances).expect("compares");
    let result = compare(&repeated, &plain, tolerances).expect("compares");
    assert_eq!(result.lower_bound, expected.lower_bound);
    assert_eq!(result.upper_bound, expected.upper_bound);
    assert_eq!(result.errors, expected.errors);
    assert_eq!(
        points(&result.upper_bound),
        [(-1.0, 1.5), (3.0, 3.5), (9.0, 3.5)]
    );
    assert_eq!(
        points(&result.lower_bound),
        [(-1.0, 0.5), (1.0, 0.5), (5.0, 2.5), (9.0, 2.5)]
    );
    // The reference curve of the result is the series as given, repeats included.
    assert_eq!(result.reference.len(), 7);
}

#[test]
fn a_peak_narrower_than_the_rectangles_is_cut_out_of_the_lower_bound() {
    let reference = [(0.0, 0.0), (2.0, 0.0), (3.0, 4.0), (4.0, 0.0), (8.0, 0.0)];
    let tolerances = Tolerances {
        atolx: 1.5,
        atoly: 0.5,
        ..Tolerances::ZERO
    };
    let result = compare(&reference, &reference, tolerances).expect("compares");
    // Above, the outline walks over the peak. Below, it climbs to the peak's right corner at
    // x = 4.5 and runs back to its left corner at x = 1.5: that loop is removed, and the lower
    // bound is the flat line the rectangles on either side already cover.
    assert_eq!(
        points(&result.upper_bound),
        [
            (-1.5, 0.5),
            (0.5, 0.5),
            (1.5, 4.5),
            (4.5, 4.5),
            (5.5, 0.5),
            (9.5, 0.5)
        ]
    );
    assert_eq!(
        points(&result.lower_bound),
        [(-1.5, -0.5), (2.5, -0.5), (9.5, -0.5)]
    );
    assert!(result.within_envelope());

    // So a test that misses the peak altogether is inside the band,
    let missed = compare(
        &reference,
        &[(0.0, 0.0), (3.0, 0.0), (8.0, 0.0)],
        tolerances,
    );
    assert!(missed.expect("compares").within_envelope());
    // and one that overshoots it, or dips under the flat line, is outside by what it exceeds.
    let over = compare(
        &reference,
        &[(0.0, 0.0), (3.0, 4.75), (8.0, 0.0)],
        tolerances,
    );
    assert_eq!(over.expect("compares").max_error, 0.25);
    let under = compare(
        &reference,
        &[(0.0, 0.0), (3.0, -1.0), (8.0, 0.0)],
        tolerances,
    );
    let under = under.expect("compares");
    assert_eq!((under.max_error, under.max_error_x), (0.5, Some(3.0)));
    assert_eq!(under.errors.y(), [0.0, 0.5, 0.0]);
}

#[test]
fn the_error_is_taken_at_the_tests_own_instants_only() {
    // The test leaves the band between its samples and is back inside at each of them: funnel
    // looks at the samples, so this is inside. (`aligned-grid` also looks at the bounds'
    // breakpoints; `tests/aligned_grid.rs` holds the difference.)
    let reference = [(0.0, 0.0), (2.0, 0.0), (4.0, 4.0), (6.0, 0.0), (8.0, 0.0)];
    let tolerances = Tolerances {
        atolx: 0.5,
        atoly: 0.5,
        ..Tolerances::ZERO
    };
    let result = compare(&reference, &[(0.0, 0.0), (8.0, 0.0)], tolerances).expect("compares");
    assert_eq!(result.compared, 2);
    assert!(result.within_envelope());
    assert_eq!(result.errors.x(), [0.0, 8.0]);
}

#[test]
fn a_test_beyond_the_bounds_last_corner_is_compared_with_that_corner() {
    // The last test instant equals the reference's by funnel's equality (5e-5 in a million)
    // and lies beyond the bounds' last corner, a millionth past the reference's end.
    let reference = [(0.0, 0.0), (1_048_576.0, 0.0)];
    let test = [(0.0, 0.0), (1_048_576.000_05, 0.75)];
    let tolerances = Tolerances {
        atolx: 1.0 / 1_048_576.0,
        atoly: 0.5,
        ..Tolerances::ZERO
    };
    let result = compare(&reference, &test, tolerances).expect("the ends are equal");
    assert!(result.upper_bound.x()[1] < test[1].0);
    assert_eq!(result.errors.y(), [0.0, 0.25]);
}
