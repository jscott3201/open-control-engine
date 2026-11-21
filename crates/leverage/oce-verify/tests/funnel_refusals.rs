//! What the funnel refuses, through the public surface: every refusal with its variant and its
//! code, each non-finite sample, the two ends, each cap accepted at its limit and refused one
//! past it, and the work bound on loop removal.

mod support;

use oce_verify::{
    Axis, COMPARE_ERROR_CODES, CompareError, CompareLimits, Envelope, FunnelResult, Series,
    SeriesEnd, SeriesRole, Tolerances, funnel,
};

use support::funnel_cases::case;

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

#[test]
fn the_code_list_is_pinned() {
    assert_eq!(
        COMPARE_ERROR_CODES,
        [
            "invalid-tolerance",
            "length-mismatch",
            "empty-series",
            "too-many-points",
            "non-finite-sample",
            "x-not-non-decreasing",
            "mismatched-endpoints",
            "work-limit-exceeded",
            "loop-removal-undefined",
            "non-finite-envelope",
        ]
    );
}

#[test]
fn every_refusal_is_reachable_with_its_variant_and_its_code() {
    let line = [(0.0, 0.0), (1.0, 1.0)];
    let tight = Tolerances {
        atolx: 0.25,
        atoly: 0.25,
        ..Tolerances::ZERO
    };
    let default = CompareLimits::default();
    let mut codes = Vec::new();
    let mut refused = |outcome: Result<FunnelResult, CompareError>, expected: CompareError| {
        let error = outcome.expect_err("a refusal");
        assert_eq!(error, expected);
        codes.push(error.code());
    };

    refused(
        compare(
            &line,
            &line,
            Tolerances {
                rtoly: -0.5,
                ..tight
            },
        ),
        CompareError::InvalidTolerance { name: "rtoly" },
    );
    refused(
        funnel(
            Series::new(&[0.0, 1.0], &[0.0]),
            Series::new(&[0.0, 1.0], &[0.0, 1.0]),
            &tight,
            &default,
        ),
        CompareError::LengthMismatch {
            series: SeriesRole::Reference,
            x_len: 2,
            y_len: 1,
        },
    );
    refused(
        compare(&line, &[], tight),
        CompareError::EmptySeries {
            series: SeriesRole::Test,
        },
    );
    refused(
        compare_under(
            &line,
            &line,
            tight,
            &CompareLimits {
                max_funnel_points: 1,
                ..default
            },
        ),
        CompareError::TooManyPoints {
            series: SeriesRole::Reference,
            points: 2,
            limit: 1,
        },
    );
    refused(
        compare(&line, &[(0.0, 0.0), (1.0, f64::NAN)], tight),
        CompareError::NonFiniteSample {
            series: SeriesRole::Test,
            axis: Axis::Y,
            index: 1,
        },
    );
    refused(
        compare(&[(0.0, 0.0), (1.0, 1.0), (0.5, 2.0)], &line, tight),
        CompareError::XNotNonDecreasing {
            series: SeriesRole::Reference,
            index: 2,
        },
    );
    refused(
        compare(&line, &[(0.0, 0.0), (1.5, 1.0)], tight),
        CompareError::MismatchedEndpoints {
            end: SeriesEnd::Last,
            reference: 1.0,
            test: 1.5,
        },
    );
    // A rising sawtooth under rectangles wider than a tooth, with no budget for its loops.
    let sawtooth: Vec<(f64, f64)> = (0..16_u8)
        .map(|k| (f64::from(k), f64::from(k % 2)))
        .collect();
    refused(
        compare_under(
            &sawtooth,
            &sawtooth,
            Tolerances {
                atolx: 2.5,
                ..tight
            },
            &CompareLimits {
                max_loop_removal_work: 3,
                ..default
            },
        ),
        CompareError::WorkLimitExceeded { limit: 3 },
    );
    // A local x tolerance above one puts a left corner left of the first: the backward search
    // of the loop removal leaves the corner list, where funnel's C reads outside its arrays.
    refused(
        compare(
            &[(1.0, 0.0), (2.0, 1.0), (3.0, 0.0)],
            &[(1.0, 0.0), (3.0, 0.0)],
            Tolerances {
                ltolx: 2.0,
                ..Tolerances::ZERO
            },
        ),
        CompareError::LoopRemovalUndefined,
    );
    // The range of y overflows, so the range-relative half-height is infinite.
    refused(
        compare(
            &[(0.0, f64::MAX), (1.0, -f64::MAX)],
            &line,
            Tolerances {
                rtoly: 0.5,
                ..tight
            },
        ),
        CompareError::NonFiniteEnvelope,
    );

    assert_eq!(codes, COMPARE_ERROR_CODES);
}

#[test]
fn a_non_finite_sample_anywhere_is_refused_and_never_compared() {
    let line = [(0.0, 0.0), (1.0, 1.0), (2.0, 0.0)];
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for index in 0..3 {
            for axis in [Axis::X, Axis::Y] {
                let mut broken = line;
                match axis {
                    Axis::X => broken[index].0 = bad,
                    Axis::Y => broken[index].1 = bad,
                }
                for (series, outcome) in [
                    (
                        SeriesRole::Reference,
                        compare(&broken, &line, Tolerances::ZERO),
                    ),
                    (SeriesRole::Test, compare(&line, &broken, Tolerances::ZERO)),
                ] {
                    assert_eq!(
                        outcome,
                        Err(CompareError::NonFiniteSample {
                            series,
                            axis,
                            index
                        }),
                        "{bad} at {axis:?}[{index}] of the {series:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn a_bound_that_overflows_is_refused_whatever_the_test_holds() {
    // A series spanning the doubles: the slope between its two points is infinite, and so is
    // a bound interpolated along it. A non-finite bound would compare as "inside".
    let spanning = [(0.0, f64::MAX), (1.0, -f64::MAX)];
    for tolerances in [
        Tolerances::ZERO,
        Tolerances {
            atoly: 1.0,
            ..Tolerances::ZERO
        },
    ] {
        assert_eq!(
            compare(&spanning, &spanning, tolerances),
            Err(CompareError::NonFiniteEnvelope)
        );
    }
    // A finite band at the top of the range: the bound is finite, the distance to a test value
    // at the bottom is not.
    let top = [(0.0, f64::MAX), (1.0, f64::MAX)];
    let bottom = [(0.0, -f64::MAX), (1.0, -f64::MAX)];
    assert_eq!(
        compare(
            &top,
            &bottom,
            Tolerances {
                atoly: 1.0,
                ..Tolerances::ZERO
            }
        ),
        Err(CompareError::NonFiniteEnvelope)
    );
    // The same band against itself is an ordinary comparison.
    let same = compare(
        &top,
        &top,
        Tolerances {
            atoly: 1.0,
            ..Tolerances::ZERO
        },
    );
    assert!(same.expect("finite").within_envelope());
}

#[test]
fn the_ends_must_agree_by_funnels_equality_and_the_first_is_reported() {
    let reference = [(0.0, 0.0), (1_000_000.0, 1.0)];
    let tolerances = Tolerances {
        atolx: 1.0,
        atoly: 1.0,
        ..Tolerances::ZERO
    };
    let ends =
        |first: f64, last: f64| compare(&reference, &[(first, 0.0), (last, 1.0)], tolerances);
    // Up to 1e-10 absolutely at the start, and 1e-10 of a million at the end.
    assert!(ends(1e-10, 1_000_000.000_05).is_ok());
    assert!(ends(-1e-10, 999_999.999_95).is_ok());
    assert_eq!(
        ends(2e-10, 1_000_000.0),
        Err(CompareError::MismatchedEndpoints {
            end: SeriesEnd::First,
            reference: 0.0,
            test: 2e-10
        })
    );
    assert_eq!(
        ends(0.0, 1_000_000.000_2),
        Err(CompareError::MismatchedEndpoints {
            end: SeriesEnd::Last,
            reference: 1_000_000.0,
            test: 1_000_000.000_2
        })
    );
    // Both differ: the first.
    assert!(matches!(
        ends(0.5, 2_000_000.0),
        Err(CompareError::MismatchedEndpoints {
            end: SeriesEnd::First,
            ..
        })
    ));
    // The aligned-grid envelope has no such precondition.
    let longer = [(0.5, 0.0), (2_000_000.0, 1.0)];
    assert!(
        Envelope::AlignedGrid
            .compare(
                Series::new(&[0.0, 1_000_000.0], &[0.0, 1.0]),
                Series::new(&[longer[0].0, longer[1].0], &[0.0, 1.0]),
                &tolerances,
                &CompareLimits::default()
            )
            .is_ok()
    );
}

#[test]
fn the_point_cap_accepts_its_limit_and_refuses_one_more_for_each_series() {
    let ramp: Vec<(f64, f64)> = (0..9_u8).map(|k| (f64::from(k), f64::from(k))).collect();
    let ends = [ramp[0], ramp[8]];
    let limits = |points: usize| CompareLimits {
        max_funnel_points: points,
        ..CompareLimits::default()
    };
    assert!(compare_under(&ramp, &ramp, Tolerances::ZERO, &limits(9)).is_ok());
    assert_eq!(
        compare_under(&ramp, &ends, Tolerances::ZERO, &limits(8)),
        Err(CompareError::TooManyPoints {
            series: SeriesRole::Reference,
            points: 9,
            limit: 8
        })
    );
    assert_eq!(
        compare_under(&ends, &ramp, Tolerances::ZERO, &limits(8)),
        Err(CompareError::TooManyPoints {
            series: SeriesRole::Test,
            points: 9,
            limit: 8
        })
    );
    // The aligned-grid cap is a different field and does not bind the funnel.
    let other = CompareLimits {
        max_aligned_grid_points: 0,
        ..CompareLimits::default()
    };
    assert!(compare_under(&ramp, &ramp, Tolerances::ZERO, &other).is_ok());
}

#[test]
fn the_documented_maximum_of_1_048_576_points_is_accepted_and_one_more_refused() {
    // A step series: the slope changes twice, so the cost is the linear passes alone.
    let count = 1_048_576_usize;
    let x: Vec<f64> = (0..count).map(|k| k as f64).collect();
    let y: Vec<f64> = (0..count)
        .map(|k| if k < count / 2 { 0.0 } else { 1.0 })
        .collect();
    let tolerances = Tolerances {
        atolx: 2.0,
        atoly: 0.25,
        ..Tolerances::ZERO
    };
    // A caller cannot raise the cap: asking for more is read as the maximum.
    let raised = CompareLimits {
        max_funnel_points: usize::MAX,
        ..CompareLimits::default()
    };
    let at_the_cap = funnel(
        Series::new(&x, &y),
        Series::new(&x, &y),
        &tolerances,
        &raised,
    )
    .expect("the documented maximum compares");
    assert_eq!(at_the_cap.compared, count);
    assert!(at_the_cap.within_envelope());
    assert_eq!(at_the_cap.upper_bound.len(), 4);

    // One more point, decided from the lengths: the samples are never read (they are NaN).
    let unread = vec![f64::NAN; count + 1];
    for limits in [CompareLimits::default(), raised] {
        assert_eq!(
            funnel(
                Series::new(&unread, &unread),
                Series::new(&x, &y),
                &tolerances,
                &limits
            ),
            Err(CompareError::TooManyPoints {
                series: SeriesRole::Reference,
                points: count + 1,
                limit: count
            })
        );
    }
}

#[test]
fn the_work_bound_accepts_its_limit_and_refuses_one_unit_less() {
    // The hostile sawtooth under wide rectangles: every tooth is a loop on both bounds.
    let case = case("sawtooth_wide_rectangles");
    let under = |work: u64| {
        funnel(
            Series::new(&case.reference.0, &case.reference.1),
            Series::new(&case.test.0, &case.test.1),
            &case.tolerances,
            &CompareLimits {
                max_loop_removal_work: work,
                ..CompareLimits::default()
            },
        )
    };
    // The units its two bounds need together.
    let needed = 430;
    let exact = under(needed).expect("the work it needs is the work it is allowed");
    assert_eq!(Ok(exact), under(u64::MAX));
    assert_eq!(
        under(needed - 1),
        Err(CompareError::WorkLimitExceeded { limit: needed - 1 })
    );
    assert_eq!(under(0), Err(CompareError::WorkLimitExceeded { limit: 0 }));
}

#[test]
fn an_outline_too_short_to_hold_a_loop_needs_no_work() {
    let no_work = CompareLimits {
        max_loop_removal_work: 0,
        ..CompareLimits::default()
    };
    // One point: two corners per bound. Two points on a line: three corners.
    for reference in [vec![(1.0, 2.0)], vec![(0.0, 0.0), (4.0, 4.0)]] {
        let tolerances = Tolerances {
            atolx: 0.5,
            ..Tolerances::ZERO
        };
        assert!(compare_under(&reference, &reference, tolerances, &no_work).is_ok());
    }
}

#[test]
fn a_reference_whose_loop_removal_is_quadratic_is_refused_not_ground_through() {
    // A rising sawtooth under a local x tolerance: each tooth's rectangle reaches back over
    // most of the teeth before it, and every loop's search walks them again. The work is about
    // 0.7 units per point squared (measured: 721,052 for 1,000 points, 2,863,187 for 2,000).
    let sawtooth = |count: u32| -> Vec<(f64, f64)> {
        (0..count)
            .map(|k| (1.0 + f64::from(k), f64::from(k) * 0.01 + f64::from(k % 2)))
            .collect()
    };
    let local = Tolerances {
        ltolx: 0.9,
        ..Tolerances::ZERO
    };
    let under = |count: u32, work: u64| {
        let reference = sawtooth(count);
        compare_under(
            &reference,
            &reference,
            local,
            &CompareLimits {
                max_loop_removal_work: work,
                ..CompareLimits::default()
            },
        )
    };
    assert!(under(1_000, 721_052).is_ok());
    assert_eq!(
        under(1_000, 721_051),
        Err(CompareError::WorkLimitExceeded { limit: 721_051 })
    );
    assert!(under(2_000, 2_863_187).is_ok());
    assert_eq!(
        under(2_000, 2_863_186),
        Err(CompareError::WorkLimitExceeded { limit: 2_863_186 })
    );
    // 8,000 points would need about 46 million units. Under a bound of four million the
    // comparison stops there: a count, not a clock, decides. (At the default bound, 2^28, the
    // same shape is compared up to 19,300 points and refused from 19,500: measured by hand,
    // because spending 2^28 units takes a debug build several seconds.)
    assert_eq!(
        under(8_000, 4_000_000),
        Err(CompareError::WorkLimitExceeded { limit: 4_000_000 })
    );
}
