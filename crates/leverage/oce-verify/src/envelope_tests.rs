//! The shared types: the order of the input checks, a curve's text, the envelope's two spellings
//! and the figures read off an error curve.

use super::*;
use crate::compare_error::SeriesEnd;

const X: [f64; 3] = [0.0, 1.0, 2.0];
const Y: [f64; 3] = [5.0, 6.0, 7.0];

fn ok() -> Series<'static> {
    Series::new(&X, &Y)
}

fn refusal(reference: Series<'_>, test: Series<'_>, tolerances: &Tolerances) -> CompareError {
    check_inputs(reference, test, tolerances, 3).expect_err("a refusal")
}

#[test]
fn well_formed_inputs_pass_every_check() {
    assert_eq!(check_inputs(ok(), ok(), &Tolerances::ZERO, 3), Ok(()));
    // Equal x values are a step, not a decrease; a single point is a series.
    let step = Series::new(&[1.0, 1.0, 1.0], &[0.0, 5.0, -5.0]);
    assert_eq!(check_inputs(step, step, &Tolerances::ZERO, 3), Ok(()));
    let point = Series::new(&[1.0], &[0.0]);
    assert_eq!(check_inputs(point, point, &Tolerances::ZERO, 1), Ok(()));
    // -0.0 after +0.0 is not a decrease.
    let zeros = Series::new(&[0.0, -0.0], &[1.0, 2.0]);
    assert_eq!(check_inputs(zeros, zeros, &Tolerances::ZERO, 2), Ok(()));
}

#[test]
fn a_tolerance_that_is_negative_or_not_finite_is_refused_by_its_name() {
    type Setter = fn(&mut Tolerances, f64);
    let fields: [(&str, Setter); 6] = [
        ("atolx", |tolerances, value| tolerances.atolx = value),
        ("atoly", |tolerances, value| tolerances.atoly = value),
        ("rtolx", |tolerances, value| tolerances.rtolx = value),
        ("rtoly", |tolerances, value| tolerances.rtoly = value),
        ("ltolx", |tolerances, value| tolerances.ltolx = value),
        ("ltoly", |tolerances, value| tolerances.ltoly = value),
    ];
    for (name, set) in fields {
        for bad in [
            -1.0,
            -f64::MIN_POSITIVE,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ] {
            let mut tolerances = Tolerances::ZERO;
            set(&mut tolerances, bad);
            assert_eq!(
                refusal(ok(), ok(), &tolerances),
                CompareError::InvalidTolerance { name },
                "{name} = {bad}"
            );
        }
        // Zero of either sign and the largest finite value are tolerances.
        for good in [0.0, -0.0, f64::MAX, 5e-324] {
            let mut tolerances = Tolerances::ZERO;
            set(&mut tolerances, good);
            assert_eq!(check_inputs(ok(), ok(), &tolerances, 3), Ok(()));
        }
    }
    // Several bad fields: the first in the order atolx, atoly, rtolx, rtoly, ltolx, ltoly.
    let several = Tolerances {
        ltoly: -1.0,
        rtolx: f64::NAN,
        ..Tolerances::ZERO
    };
    assert_eq!(
        refusal(ok(), ok(), &several),
        CompareError::InvalidTolerance { name: "rtolx" }
    );
}

#[test]
fn the_shape_of_a_series_is_refused_from_its_lengths_alone() {
    let short_y = Series::new(&X, &Y[..2]);
    assert_eq!(
        refusal(short_y, ok(), &Tolerances::ZERO),
        CompareError::LengthMismatch {
            series: SeriesRole::Reference,
            x_len: 3,
            y_len: 2
        }
    );
    assert_eq!(
        refusal(ok(), Series::new(&X[..1], &Y), &Tolerances::ZERO),
        CompareError::LengthMismatch {
            series: SeriesRole::Test,
            x_len: 1,
            y_len: 3
        }
    );
    let empty = Series::new(&[], &[]);
    assert_eq!(
        refusal(empty, ok(), &Tolerances::ZERO),
        CompareError::EmptySeries {
            series: SeriesRole::Reference
        }
    );
    assert_eq!(
        refusal(ok(), empty, &Tolerances::ZERO),
        CompareError::EmptySeries {
            series: SeriesRole::Test
        }
    );
    // An empty x with a non-empty y is a length mismatch, not an empty series.
    assert_eq!(
        refusal(Series::new(&[], &Y), ok(), &Tolerances::ZERO),
        CompareError::LengthMismatch {
            series: SeriesRole::Reference,
            x_len: 0,
            y_len: 3
        }
    );
}

#[test]
fn the_point_cap_accepts_its_limit_and_refuses_one_more_before_reading_a_sample() {
    assert_eq!(check_inputs(ok(), ok(), &Tolerances::ZERO, 3), Ok(()));
    // The samples are NaN: the cap is decided first, from the length.
    let nan = [f64::NAN; 3];
    let unread = Series::new(&nan, &nan);
    assert_eq!(
        check_inputs(unread, ok(), &Tolerances::ZERO, 2),
        Err(CompareError::TooManyPoints {
            series: SeriesRole::Reference,
            points: 3,
            limit: 2
        })
    );
    assert_eq!(
        check_inputs(Series::new(&X[..2], &Y[..2]), unread, &Tolerances::ZERO, 2),
        Err(CompareError::TooManyPoints {
            series: SeriesRole::Test,
            points: 3,
            limit: 2
        })
    );
    // A cap of zero refuses every series that has a point.
    assert_eq!(
        check_inputs(ok(), ok(), &Tolerances::ZERO, 0),
        Err(CompareError::TooManyPoints {
            series: SeriesRole::Reference,
            points: 3,
            limit: 0
        })
    );
}

#[test]
fn every_non_finite_sample_is_refused_with_its_series_column_and_index() {
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for index in 0..3 {
            for axis in [Axis::X, Axis::Y] {
                let (mut x, mut y) = (X, Y);
                match axis {
                    Axis::X => x[index] = bad,
                    Axis::Y => y[index] = bad,
                }
                let broken = Series::new(&x, &y);
                assert_eq!(
                    refusal(broken, ok(), &Tolerances::ZERO),
                    CompareError::NonFiniteSample {
                        series: SeriesRole::Reference,
                        axis,
                        index
                    }
                );
                assert_eq!(
                    refusal(ok(), broken, &Tolerances::ZERO),
                    CompareError::NonFiniteSample {
                        series: SeriesRole::Test,
                        axis,
                        index
                    }
                );
            }
        }
    }
    // Both columns bad at one index: x is named. An earlier index wins over a later one.
    let nan = [f64::NAN; 3];
    assert_eq!(
        refusal(Series::new(&nan, &nan), ok(), &Tolerances::ZERO),
        CompareError::NonFiniteSample {
            series: SeriesRole::Reference,
            axis: Axis::X,
            index: 0
        }
    );
    assert_eq!(
        refusal(
            Series::new(&[0.0, 1.0, f64::NAN], &[0.0, f64::INFINITY, 0.0]),
            ok(),
            &Tolerances::ZERO
        ),
        CompareError::NonFiniteSample {
            series: SeriesRole::Reference,
            axis: Axis::Y,
            index: 1
        }
    );
}

#[test]
fn a_decreasing_x_is_refused_at_the_first_sample_below_its_predecessor() {
    let back = Series::new(&[0.0, 2.0, 1.0, 0.5], &[0.0; 4]);
    assert_eq!(
        check_inputs(back, ok(), &Tolerances::ZERO, 4),
        Err(CompareError::XNotNonDecreasing {
            series: SeriesRole::Reference,
            index: 2
        })
    );
    // One ulp is a decrease.
    let ulp = [1.0, f64::from_bits(1.0_f64.to_bits() - 1)];
    assert_eq!(
        check_inputs(ok(), Series::new(&ulp, &[0.0; 2]), &Tolerances::ZERO, 4),
        Err(CompareError::XNotNonDecreasing {
            series: SeriesRole::Test,
            index: 1
        })
    );
}

#[test]
fn one_input_has_one_refusal_the_first_in_the_documented_order() {
    let nan = [f64::NAN; 3];
    let bad_samples = Series::new(&nan, &Y);
    let bad_shape = Series::new(&X, &Y[..1]);
    let bad_tolerance = Tolerances {
        atoly: -1.0,
        ..Tolerances::ZERO
    };
    // The tolerances before anything about the series.
    assert_eq!(
        refusal(bad_shape, bad_samples, &bad_tolerance).code(),
        "invalid-tolerance"
    );
    // The shape of the test before the samples of the reference.
    assert_eq!(
        refusal(bad_samples, bad_shape, &Tolerances::ZERO).code(),
        "length-mismatch"
    );
    // The reference's samples before the test's; non-finite before order within a series.
    let back = Series::new(&[2.0, 1.0, f64::NAN], &Y);
    assert_eq!(
        refusal(back, bad_samples, &Tolerances::ZERO),
        CompareError::NonFiniteSample {
            series: SeriesRole::Reference,
            axis: Axis::X,
            index: 2
        }
    );
    assert_eq!(
        refusal(
            Series::new(&[2.0, 1.0, 3.0], &Y),
            bad_samples,
            &Tolerances::ZERO
        )
        .code(),
        "x-not-non-decreasing"
    );
}

#[test]
fn a_curve_prints_as_pyfunnel_writes_its_files() {
    let curve = Curve::from_columns(
        vec![0.0, 0.1 + 0.2, 1e16, -0.0],
        vec![-1.5, 1e-5, 0.0001, 2.0],
    );
    assert_eq!(
        curve.to_csv(),
        "x,y\n0,-1.5\n0.3,1e-05\n1e+16,0.0001\n-0,2\n"
    );
    assert_eq!(
        Curve::from_columns(Vec::new(), Vec::new()).to_csv(),
        "x,y\n"
    );
}

#[test]
fn a_curve_keeps_its_points_in_order_in_both_views() {
    let curve = Curve::from_points([(0.0, 1.0), (2.0, 3.0), (2.0, -3.0)]);
    assert_eq!(curve.len(), 3);
    assert!(!curve.is_empty());
    assert_eq!(curve.x(), [0.0, 2.0, 2.0]);
    assert_eq!(curve.y(), [1.0, 3.0, -3.0]);
    assert_eq!(
        curve.points().collect::<Vec<_>>(),
        [(0.0, 1.0), (2.0, 3.0), (2.0, -3.0)]
    );
    assert_eq!(
        curve.as_series(),
        Series::new(&[0.0, 2.0, 2.0], &[1.0, 3.0, -3.0])
    );
    assert!(Curve::from_points([]).is_empty());
}

#[test]
fn the_two_envelopes_are_spelled_funnel_and_aligned_grid() {
    assert_eq!(Envelope::Funnel.as_str(), "funnel");
    assert_eq!(Envelope::AlignedGrid.as_str(), "aligned-grid");
    assert_eq!(Envelope::Funnel.to_string(), "funnel");
    assert_eq!(Envelope::AlignedGrid.to_string(), "aligned-grid");
    for envelope in Envelope::ALL {
        assert_eq!(Envelope::from_name(envelope.as_str()), Some(envelope));
    }
    // Each envelope names the version of its own arithmetic, and the two strings are pinned.
    assert_eq!(crate::FUNNEL_VERSION, "funnel@a12fa96");
    assert_eq!(crate::ALIGNED_GRID_VERSION, "aligned-grid@1");
    assert_eq!(Envelope::Funnel.version(), crate::FUNNEL_VERSION);
    assert_eq!(Envelope::AlignedGrid.version(), crate::ALIGNED_GRID_VERSION);
    for envelope in Envelope::ALL {
        // `<spelling>@<revision>`: a receipt's `envelope_version` starts with its `envelope`.
        let (name, revision) = envelope.version().split_once('@').expect("an `@`");
        assert_eq!(name, envelope.as_str());
        assert!(!revision.is_empty());
    }
    // Exact spellings only.
    for other in [
        "",
        "Funnel",
        "aligned_grid",
        "alignedGrid",
        "funnel ",
        "aligned-grid\n",
    ] {
        assert_eq!(Envelope::from_name(other), None, "{other:?}");
    }
}

#[test]
fn an_envelope_dispatches_to_its_own_comparison() {
    let reference = Series::new(&X, &Y);
    let tolerances = Tolerances {
        atolx: 0.25,
        atoly: 0.5,
        ..Tolerances::ZERO
    };
    let limits = CompareLimits::default();
    for envelope in Envelope::ALL {
        let result = envelope
            .compare(reference, reference, &tolerances, &limits)
            .expect("a comparison");
        assert_eq!(result.envelope, envelope);
        assert!(result.within_envelope());
    }
    // Only the funnel requires the two series to end at one x.
    let longer = Series::new(&[0.0, 1.0, 3.0], &Y);
    assert_eq!(
        Envelope::Funnel.compare(reference, longer, &tolerances, &limits),
        Err(CompareError::MismatchedEndpoints {
            end: SeriesEnd::Last,
            reference: 2.0,
            test: 3.0
        })
    );
    assert!(
        Envelope::AlignedGrid
            .compare(reference, longer, &tolerances, &limits)
            .is_ok()
    );
}

#[test]
fn the_figures_are_read_off_the_error_curve_in_its_order() {
    let result = |errors: &[(f64, f64)]| {
        FunnelResult::from_curves(
            Envelope::Funnel,
            ok(),
            ok(),
            (Curve::from_points([]), Curve::from_points([])),
            Curve::from_points(errors.iter().copied()),
            crate::envelope::Applied {
                errors_at_test: errors.iter().map(|(_, error)| *error).collect(),
                half_width: HalfSize::of([1.0]),
                half_height: HalfSize::of([1.0]),
            },
        )
    };
    let inside = result(&[(0.0, 0.0), (1.0, 0.0)]);
    assert_eq!(inside.compared, 2);
    assert!(inside.within_envelope());
    assert_eq!(inside.max_error.to_bits(), 0.0_f64.to_bits());
    assert_eq!((inside.max_error_x, inside.first_error_x), (None, None));

    // The first instant above zero, and the first instant holding the largest error.
    let outside = result(&[
        (0.0, 0.0),
        (1.0, 0.25),
        (2.0, 0.75),
        (3.0, 0.75),
        (4.0, 0.5),
    ]);
    assert_eq!(outside.compared, 5);
    assert!(!outside.within_envelope());
    assert_eq!(outside.first_error_x, Some(1.0));
    assert_eq!(outside.max_error, 0.75);
    assert_eq!(outside.max_error_x, Some(2.0));
    assert_eq!(
        outside.reference,
        Curve::from_columns(X.to_vec(), Y.to_vec())
    );

    // The smallest positive error is an error.
    let barely = result(&[(7.0, 5e-324)]);
    assert!(!barely.within_envelope());
    assert_eq!(barely.max_error_x, Some(7.0));
}
