//! Rectangle half-sizes: each tolerance kind alone, the maximum of the three, and funnel's two
//! fallbacks for a half-size of zero.

use super::*;

/// A reference whose range is 8 on both axes and whose magnitude is 16 in `x` and 8 in `y`.
const X: [f64; 3] = [8.0, 12.0, 16.0];
const Y: [f64; 3] = [-8.0, 0.0, -2.0];

fn sizes(tolerances: Tolerances) -> HalfSizes {
    let reference = Series::new(&X, &Y);
    half_sizes(reference, &Extent::of(reference), &tolerances)
}

#[test]
fn the_extent_is_the_range_and_the_magnitude_on_each_axis() {
    let extent = Extent::of(Series::new(&X, &Y));
    assert_eq!(
        extent,
        Extent {
            range_x: 8.0,
            range_y: 8.0,
            magnitude_x: 16.0,
            magnitude_y: 8.0,
        }
    );
    // The magnitude is the larger of the largest value and the absolute smallest.
    let extent = Extent::of(Series::new(&[-3.0, -1.0], &[5.0, -2.0]));
    assert_eq!((extent.magnitude_x, extent.magnitude_y), (3.0, 5.0));
    assert_eq!((extent.range_x, extent.range_y), (2.0, 7.0));
    let point = Extent::of(Series::new(&[-4.0], &[0.0]));
    assert_eq!((point.range_x, point.magnitude_x), (0.0, 4.0));
    assert_eq!((point.range_y, point.magnitude_y), (0.0, 0.0));
}

#[test]
fn an_absolute_tolerance_alone_is_the_half_size_at_every_point() {
    let half = sizes(Tolerances {
        atolx: 0.5,
        atoly: 0.25,
        ..Tolerances::ZERO
    });
    assert_eq!(half.x, [0.5, 0.5, 0.5]);
    assert_eq!(half.y, [0.25, 0.25, 0.25]);
}

#[test]
fn a_range_relative_tolerance_alone_is_a_fraction_of_the_references_range() {
    let half = sizes(Tolerances {
        rtolx: 0.125,
        rtoly: 0.25,
        ..Tolerances::ZERO
    });
    assert_eq!(half.x, [1.0, 1.0, 1.0]);
    assert_eq!(half.y, [2.0, 2.0, 2.0]);
}

#[test]
fn a_local_tolerance_alone_is_a_fraction_of_each_points_own_magnitude() {
    let half = sizes(Tolerances {
        ltolx: 0.125,
        ltoly: 0.25,
        ..Tolerances::ZERO
    });
    assert_eq!(half.x, [1.0, 1.5, 2.0]);
    // At y = 0 the local term is zero, and with no range-relative tolerance the floor applies.
    assert_eq!(half.y, [2.0, 1e-10, 0.5]);
}

#[test]
fn the_three_terms_combine_by_maximum_never_by_sum() {
    let half = sizes(Tolerances {
        atolx: 1.25,
        rtolx: 0.125, // 1.0
        ltolx: 0.125, // 1.0, 1.5, 2.0
        atoly: 0.75,
        rtoly: 0.125, // 1.0
        ltoly: 0.25,  // 2.0, 0.0, 0.5
    });
    assert_eq!(half.x, [1.25, 1.5, 2.0]);
    assert_eq!(half.y, [2.0, 1.0, 1.0]);
}

#[test]
fn a_zero_half_size_takes_rtol_times_the_magnitude_when_rtol_is_not_zero() {
    // A constant series: both ranges are zero, so the range-relative term is zero, and the
    // fallback multiplies by the magnitude instead.
    let x = [4.0, 4.0];
    let y = [-8.0, -8.0];
    let reference = Series::new(&x, &y);
    let half = half_sizes(
        reference,
        &Extent::of(reference),
        &Tolerances {
            rtolx: 0.125,
            rtoly: 0.25,
            ..Tolerances::ZERO
        },
    );
    assert_eq!(half.x, [0.5, 0.5]);
    assert_eq!(half.y, [2.0, 2.0]);
}

#[test]
fn a_half_size_still_zero_is_the_floor_of_1e_minus_10() {
    // No tolerance at all.
    let half = sizes(Tolerances::ZERO);
    assert_eq!(half.x, [1e-10; 3]);
    assert_eq!(half.y, [1e-10; 3]);

    // A range-relative tolerance on a constant zero series: the magnitude is zero as well.
    let zero = [0.0, 0.0];
    let reference = Series::new(&zero, &zero);
    let half = half_sizes(
        reference,
        &Extent::of(reference),
        &Tolerances {
            rtolx: 0.5,
            rtoly: 0.5,
            ..Tolerances::ZERO
        },
    );
    assert_eq!(half.x, [1e-10; 2]);
    assert_eq!(half.y, [1e-10; 2]);
}

#[test]
fn zero_is_decided_by_funnels_equality_so_a_tolerance_up_to_1e_minus_10_is_replaced() {
    let at = |atol: f64| {
        sizes(Tolerances {
            atolx: atol,
            atoly: atol,
            ..Tolerances::ZERO
        })
    };
    // At most 1e-10 reads as zero and becomes the floor; just above it is kept.
    assert_eq!(at(5e-11).x, [1e-10; 3]);
    assert_eq!(at(1e-10).y, [1e-10; 3]);
    assert_eq!(at(1.5e-10).x, [1.5e-10; 3]);

    // The same equality decides whether `rtol` is zero: 1e-10 is, so the magnitude fallback is
    // skipped and the floor applies; 2e-10 is not, and 2e-10 × 16 is above the floor.
    let constant = [16.0, 16.0];
    let reference = Series::new(&constant, &constant);
    let with = |rtol: f64| {
        half_sizes(
            reference,
            &Extent::of(reference),
            &Tolerances {
                rtolx: rtol,
                ..Tolerances::ZERO
            },
        )
        .x
    };
    assert_eq!(with(1e-10), [1e-10; 2]);
    assert_eq!(with(2e-10), [3.2e-9; 2]);

    // A fallback that is itself at most 1e-10 falls through to the floor: 2e-10 × 0.25.
    let quarter = [0.25, 0.25];
    let reference = Series::new(&quarter, &quarter);
    let half = half_sizes(
        reference,
        &Extent::of(reference),
        &Tolerances {
            rtolx: 2e-10,
            ..Tolerances::ZERO
        },
    );
    assert_eq!(half.x, [1e-10; 2]);
}

#[test]
fn x_is_normalised_by_its_magnitude_only_above_1e_minus_5() {
    let extent_of = |magnitude: f64| {
        let x = [0.0, magnitude];
        Extent::of(Series::new(&x, &[0.0, 1.0]))
    };
    let at = extent_of(1e-5);
    assert!(!at.normalises_x());
    assert_eq!(at.normalised(&[5e-6, 1e-5]), [5e-6, 1e-5]);
    assert_eq!(at.denormalised(0.5), 0.5);

    let above = extent_of(1.000_000_000_000_000_2e-5);
    assert!(above.normalises_x());
    assert_eq!(above.normalised(&[1.000_000_000_000_000_2e-5]), [1.0]);

    let large = extent_of(1024.0);
    assert_eq!(
        large.normalised(&[256.0, 1024.0, -512.0]),
        [0.25, 1.0, -0.5]
    );
    assert_eq!(large.denormalised(0.25), 256.0);

    // A magnitude of zero (a series at x = 0) is left alone, never divided by.
    let zero = extent_of(0.0);
    assert!(!zero.normalises_x());
    assert_eq!(zero.normalised(&[0.0]), [0.0]);
}

#[test]
fn the_larger_of_two_values_is_cs_macro_not_a_maximum_function() {
    assert_eq!(larger(1.0, 2.0), 2.0);
    assert_eq!(larger(2.0, 1.0), 2.0);
    // Equal or unordered: the second operand.
    assert_eq!(larger(0.0, -0.0).to_bits(), (-0.0_f64).to_bits());
    assert_eq!(larger(-0.0, 0.0).to_bits(), 0.0_f64.to_bits());
    assert!(larger(1.0, f64::NAN).is_nan());
    assert_eq!(larger(f64::NAN, 1.0), 1.0);
}
