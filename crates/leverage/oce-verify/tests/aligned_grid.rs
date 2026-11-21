//! The `aligned-grid` envelope: parity with the conformance harness it is copied from, the ways
//! it differs from the funnel, its refusals and its cap.
//!
//! **Parity.** `oce-verify` may not depend on `oce-conformance`, so the copy is held to goldens
//! taken from the original once (`tests/fixtures/aligned_grid/conformance/<case>.txt`: the
//! bounds, errors and figures of its `build_bounds` and `compare`, as hex bits, over the funnel
//! fixture's cases with a reference of at most 41 points). The comparison is byte for byte and
//! nothing here writes those files: the bless switch does not touch them.

mod support;

use std::collections::BTreeSet;

use oce_verify::{
    Axis, CompareError, CompareLimits, Curve, Envelope, FunnelResult, Series, SeriesRole,
    Tolerances, aligned_grid, funnel,
};

use support::fixture;
use support::funnel_cases::{Case, cases, curve_bits, optional_bits};

/// The largest reference a parity golden was taken for; the capture applies the same rule
/// (`tests/fixtures/aligned_grid/capture/conformance_bounds.rs`).
const MAX_REFERENCE_POINTS: usize = 41;

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
    aligned_grid(
        Series::new(&reference.0, &reference.1),
        Series::new(&test.0, &test.1),
        &tolerances,
        limits,
    )
}

fn of_case(case: &Case) -> Result<FunnelResult, CompareError> {
    aligned_grid(
        Series::new(&case.reference.0, &case.reference.1),
        Series::new(&case.test.0, &case.test.1),
        &case.tolerances,
        &CompareLimits::default(),
    )
}

fn points(curve: &Curve) -> Vec<(f64, f64)> {
    curve.points().collect()
}

/// A result in the text of a parity golden.
fn parity_text(case: &Case, result: &FunnelResult) -> String {
    format!(
        "# oce-conformance build_bounds and compare: {}/{}\n# Hex IEEE-754 bits, `x y` per \
         point. Captured by aligned_grid/capture/conformance_bounds.rs, never blessed.\n\
         {}{}{}[figures]\npassed {}\ncompared {}\nmax_error {:016x}\nfirst_error_x {}\n",
        case.set,
        case.name,
        curve_bits("lower_bound", &result.lower_bound),
        curve_bits("upper_bound", &result.upper_bound),
        curve_bits("errors", &result.errors),
        result.within_envelope(),
        result.compared,
        result.max_error.to_bits(),
        optional_bits(result.first_error_x),
    )
}

#[test]
fn the_copy_reproduces_the_conformance_harness_bit_for_bit_on_the_shared_cases() {
    let directory = fixture("aligned_grid/conformance");
    let mut compared = BTreeSet::new();
    for case in cases() {
        if case.reference.0.len() > MAX_REFERENCE_POINTS {
            continue;
        }
        let result = of_case(&case).expect("the original compares every shared case");
        let path = directory.join(format!("{}.txt", case.name));
        let golden = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let ours = parity_text(&case, &result);
        if ours != golden {
            let line = ours
                .lines()
                .zip(golden.lines())
                .position(|(a, b)| a != b)
                .unwrap_or_else(|| ours.lines().count().min(golden.lines().count()));
            panic!(
                "{}: the copy differs from oce-conformance at line {}: copy {:?}, original {:?}",
                case.name,
                line + 1,
                ours.lines().nth(line),
                golden.lines().nth(line)
            );
        }
        compared.insert(format!("{}.txt", case.name));
    }
    // 66 cases, less the seven with a larger reference (`fail1`, `success7`, `success9` and
    // the four noisy ones).
    assert_eq!(compared.len(), 59);
    let present: BTreeSet<String> = std::fs::read_dir(&directory)
        .expect("the parity goldens")
        .map(|entry| {
            entry
                .expect("a directory entry")
                .file_name()
                .into_string()
                .expect("a UTF-8 file name")
        })
        .collect();
    assert_eq!(present, compared);
}

#[test]
fn the_larger_cases_compare_too_and_twice_alike() {
    // No parity golden for these seven; the copy still compares them, deterministically.
    let mut seen = 0;
    for case in cases() {
        // `fail1` (5,041 points) is quadratic work a debug build should not repeat.
        if case.reference.0.len() <= MAX_REFERENCE_POINTS || case.name == "fail1" {
            continue;
        }
        let first = of_case(&case).expect("compares");
        let second = of_case(&case).expect("compares");
        assert_eq!(parity_text(&case, &first), parity_text(&case, &second));
        assert_eq!(first.envelope, Envelope::AlignedGrid);
        assert!(first.compared >= case.test.0.len());
        seen += 1;
    }
    assert_eq!(seen, 6);
}

#[test]
fn the_tolerance_terms_of_an_axis_add_where_the_funnel_takes_their_maximum() {
    let point = [(4.0, -8.0)];
    let tolerances = Tolerances {
        atolx: 0.5,
        ltolx: 0.25, // × |4| = 1.0
        atoly: 1.0,
        ltoly: 0.375, // × |-8| = 3.0
        ..Tolerances::ZERO
    };
    // Half-width 1.5 and half-height 4: the grid is the point and its rectangle's two edges.
    let summed = compare(&point, &point, tolerances).expect("compares");
    assert_eq!(
        points(&summed.lower_bound),
        [(2.5, -12.0), (4.0, -12.0), (5.5, -12.0)]
    );
    assert_eq!(
        points(&summed.upper_bound),
        [(2.5, -4.0), (4.0, -4.0), (5.5, -4.0)]
    );
    // The funnel's rectangle for the same input: half-width 1 and half-height 3.
    let largest = funnel(
        Series::new(&[4.0], &[-8.0]),
        Series::new(&[4.0], &[-8.0]),
        &tolerances,
        &CompareLimits::default(),
    )
    .expect("compares");
    assert_eq!(points(&largest.lower_bound), [(3.0, -11.0), (5.0, -11.0)]);
}

#[test]
fn with_no_tolerance_the_band_is_the_reference_itself_with_no_floor() {
    let reference = [(0.0, 1.0), (1.0, 3.0), (2.0, 2.0)];
    let exact = compare(&reference, &reference, Tolerances::ZERO).expect("compares");
    assert_eq!(points(&exact.lower_bound), reference);
    assert_eq!(points(&exact.upper_bound), reference);
    assert!(exact.within_envelope());
    // One ulp off the reference is outside: there is no 1e-10 floor here, as the funnel has.
    let above = f64::from_bits(3.0_f64.to_bits() + 1);
    let moved = compare(
        &reference,
        &[(0.0, 1.0), (1.0, above), (2.0, 2.0)],
        Tolerances::ZERO,
    );
    let moved = moved.expect("compares");
    assert_eq!(moved.max_error, above - 3.0);
    assert_eq!(moved.max_error_x, Some(1.0));
}

#[test]
fn the_band_holds_every_rectangle_covering_a_grid_point() {
    let reference = [(0.0, 0.0), (2.0, 4.0), (4.0, 0.0)];
    let tolerances = Tolerances {
        atolx: 1.0,
        atoly: 0.5,
        ..Tolerances::ZERO
    };
    let result = compare(&reference, &reference, tolerances).expect("compares");
    assert_eq!(result.envelope, Envelope::AlignedGrid);
    assert_eq!(
        points(&result.lower_bound),
        [
            (-1.0, -0.5),
            (0.0, -0.5),
            (1.0, -0.5),
            (2.0, 3.5),
            (3.0, -0.5),
            (4.0, -0.5),
            (5.0, -0.5)
        ]
    );
    assert_eq!(
        points(&result.upper_bound),
        [
            (-1.0, 0.5),
            (0.0, 0.5),
            (1.0, 4.5),
            (2.0, 4.5),
            (3.0, 4.5),
            (4.0, 0.5),
            (5.0, 0.5)
        ]
    );
    // The three test instants and the four grid points that are not one, in ascending x.
    assert_eq!(result.compared, 7);
    assert_eq!(result.errors.x(), [-1.0, 0.0, 1.0, 2.0, 3.0, 4.0, 5.0]);
    assert!(result.within_envelope());
}

#[test]
fn a_test_that_leaves_the_band_between_its_samples_is_caught_at_a_breakpoint() {
    // The test misses the peak. It is inside the band at both of its own instants, which is
    // all the funnel looks at; the grid envelope also looks at x = 2, where the test,
    // interpolated, is 3.5 below the lower bound.
    let reference = [(0.0, 0.0), (2.0, 4.0), (4.0, 0.0)];
    let test = [(0.0, 0.0), (4.0, 0.0)];
    let tolerances = Tolerances {
        atolx: 1.0,
        atoly: 0.5,
        ..Tolerances::ZERO
    };
    let grid = compare(&reference, &test, tolerances).expect("compares");
    assert!(!grid.within_envelope());
    assert_eq!((grid.max_error, grid.max_error_x), (3.5, Some(2.0)));
    assert_eq!(grid.first_error_x, Some(2.0));
    assert_eq!(grid.compared, 7);
    assert_eq!(grid.errors.y(), [0.0, 0.0, 0.0, 3.5, 0.0, 0.0, 0.0]);

    let tube = funnel(
        Series::new(&[0.0, 2.0, 4.0], &[0.0, 4.0, 0.0]),
        Series::new(&[0.0, 4.0], &[0.0, 0.0]),
        &tolerances,
        &CompareLimits::default(),
    )
    .expect("compares");
    assert_eq!(tube.compared, 2);
    assert!(tube.within_envelope());
}

#[test]
fn the_two_series_need_not_share_their_ends() {
    // The funnel refuses this pair; here the bounds are held outside the grid and the test
    // outside its own range.
    let reference = [(0.0, 0.0), (4.0, 0.0)];
    let test = [(1.0, 0.25), (9.0, 0.25), (10.0, 2.0)];
    let tolerances = Tolerances {
        atolx: 1.0,
        atoly: 0.5,
        ..Tolerances::ZERO
    };
    let result = compare(&reference, &test, tolerances).expect("no precondition on the ends");
    assert_eq!(points(&result.upper_bound).last(), Some(&(5.0, 0.5)));
    // Breakpoints -1, 0, 3, 4 and 5 are added to the test's 1, 9 and 10. Before its first
    // sample the test is held at 0.25; beyond the grid the band is held at ±0.5.
    assert_eq!(
        result.errors.x(),
        [-1.0, 0.0, 1.0, 3.0, 4.0, 5.0, 9.0, 10.0]
    );
    assert_eq!(result.errors.y(), [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.5]);
    assert_eq!(result.first_error_x, Some(10.0));
}

#[test]
fn two_test_samples_at_one_instant_are_both_compared_in_order_of_their_error() {
    let reference = [(0.0, 0.0), (2.0, 0.0)];
    let test = [(0.0, 0.0), (1.0, 3.0), (1.0, 0.0), (2.0, 0.0)];
    let tolerances = Tolerances {
        atoly: 1.0,
        ..Tolerances::ZERO
    };
    let result = compare(&reference, &test, tolerances).expect("compares");
    // No tolerance in x: the grid is the reference's own instants, both of them test instants.
    assert_eq!(result.compared, 4);
    assert_eq!(result.errors.x(), [0.0, 1.0, 1.0, 2.0]);
    assert_eq!(result.errors.y(), [0.0, 0.0, 2.0, 0.0]);
    assert_eq!((result.max_error, result.max_error_x), (2.0, Some(1.0)));
}

#[test]
fn malformed_input_is_refused_as_the_funnel_refuses_it() {
    let line = [(0.0, 0.0), (1.0, 1.0)];
    let zero = Tolerances::ZERO;
    assert_eq!(
        compare(
            &line,
            &line,
            Tolerances {
                ltolx: f64::NAN,
                ..zero
            }
        ),
        Err(CompareError::InvalidTolerance { name: "ltolx" })
    );
    assert_eq!(
        compare(&[], &line, zero),
        Err(CompareError::EmptySeries {
            series: SeriesRole::Reference
        })
    );
    assert_eq!(
        aligned_grid(
            Series::new(&[0.0], &[0.0, 1.0]),
            Series::new(&[0.0], &[0.0]),
            &zero,
            &CompareLimits::default()
        ),
        Err(CompareError::LengthMismatch {
            series: SeriesRole::Reference,
            x_len: 1,
            y_len: 2
        })
    );
    assert_eq!(
        compare(&[(0.0, 0.0), (f64::INFINITY, 1.0)], &line, zero),
        Err(CompareError::NonFiniteSample {
            series: SeriesRole::Reference,
            axis: Axis::X,
            index: 1
        })
    );
    assert_eq!(
        compare(&line, &[(1.0, 0.0), (0.0, 1.0)], zero),
        Err(CompareError::XNotNonDecreasing {
            series: SeriesRole::Test,
            index: 1
        })
    );
    // A half-height that overflows.
    assert_eq!(
        compare(
            &[(0.0, f64::MAX), (1.0, f64::MAX)],
            &line,
            Tolerances { ltoly: 2.0, ..zero }
        ),
        Err(CompareError::NonFiniteEnvelope)
    );
    // A band at the top of the range against a test at the bottom: an infinite distance.
    assert_eq!(
        compare(
            &[(0.0, f64::MAX), (1.0, f64::MAX)],
            &[(0.0, -f64::MAX), (1.0, -f64::MAX)],
            zero
        ),
        Err(CompareError::NonFiniteEnvelope)
    );
}

#[test]
fn the_point_cap_accepts_its_limit_and_refuses_one_more_for_each_series() {
    let ramp: Vec<(f64, f64)> = (0..9_u8).map(|k| (f64::from(k), f64::from(k))).collect();
    let ends = [ramp[0], ramp[8]];
    let limits = |points: usize| CompareLimits {
        max_aligned_grid_points: points,
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
    // The funnel's cap and its work bound are other fields and do not bind this envelope.
    let other = CompareLimits {
        max_funnel_points: 0,
        max_loop_removal_work: 0,
        ..CompareLimits::default()
    };
    assert!(compare_under(&ramp, &ramp, Tolerances::ZERO, &other).is_ok());
}

#[test]
fn the_documented_maximum_of_32_768_points_is_accepted_and_one_more_refused() {
    // Every sample at one of two instants, so the grid has six points and the quadratic step
    // is 6 × 32,768 rectangle tests: the cap itself is what is exercised, not an hour of work.
    let count = 32_768_usize;
    let x: Vec<f64> = (0..count)
        .map(|k| if k < count / 2 { 0.0 } else { 8.0 })
        .collect();
    let y: Vec<f64> = (0..count).map(|k| (k % 5) as f64).collect();
    let tolerances = Tolerances {
        atolx: 1.0,
        atoly: 0.25,
        ..Tolerances::ZERO
    };
    // A caller cannot raise the cap: asking for more is read as the maximum.
    let raised = CompareLimits {
        max_aligned_grid_points: usize::MAX,
        ..CompareLimits::default()
    };
    let at_the_cap = aligned_grid(
        Series::new(&x, &y),
        Series::new(&x, &y),
        &tolerances,
        &raised,
    )
    .expect("the documented maximum compares");
    assert_eq!(at_the_cap.lower_bound.len(), 6);
    assert_eq!(at_the_cap.compared, count + 4);
    assert!(at_the_cap.within_envelope());

    // One more point, decided from the lengths: the samples are never read (they are NaN).
    let unread = vec![f64::NAN; count + 1];
    for limits in [CompareLimits::default(), raised] {
        assert_eq!(
            aligned_grid(
                Series::new(&x, &y),
                Series::new(&unread, &unread),
                &tolerances,
                &limits
            ),
            Err(CompareError::TooManyPoints {
                series: SeriesRole::Test,
                points: count + 1,
                limit: count
            })
        );
    }
}

#[test]
fn the_result_does_not_depend_on_where_the_caller_keeps_its_samples() {
    for case in cases() {
        if case.reference.0.len() > MAX_REFERENCE_POINTS {
            continue;
        }
        let expected = parity_text(&case, &of_case(&case).expect("compares"));
        for offset in [1_usize, 5] {
            let window = |values: &[f64]| -> Vec<f64> {
                let mut buffer = vec![f64::NAN; offset];
                buffer.extend_from_slice(values);
                buffer.push(f64::INFINITY);
                buffer
            };
            let buffers = [
                window(&case.reference.0),
                window(&case.reference.1),
                window(&case.test.0),
                window(&case.test.1),
            ];
            let slice = |index: usize, len: usize| &buffers[index][offset..offset + len];
            let (reference_len, test_len) = (case.reference.0.len(), case.test.0.len());
            let result = aligned_grid(
                Series::new(slice(0, reference_len), slice(1, reference_len)),
                Series::new(slice(2, test_len), slice(3, test_len)),
                &case.tolerances,
                &CompareLimits::default(),
            )
            .expect("compares");
            assert_eq!(parity_text(&case, &result), expected, "{}", case.name);
        }
    }
}

#[test]
fn the_errors_are_in_ascending_x_and_hold_every_test_instant() {
    for case in cases() {
        if case.reference.0.len() > MAX_REFERENCE_POINTS {
            continue;
        }
        let result = of_case(&case).expect("compares");
        let x = result.errors.x();
        assert!(x.windows(2).all(|pair| pair[0] <= pair[1]), "{}", case.name);
        assert_eq!(result.compared, x.len(), "{}", case.name);
        assert!(result.errors.y().iter().all(|error| *error >= 0.0));
        let instants: BTreeSet<u64> = x.iter().map(|x| x.to_bits()).collect();
        for test_x in &case.test.0 {
            assert!(instants.contains(&test_x.to_bits()), "{}", case.name);
        }
        // The two bounds share one grid and never cross.
        assert_eq!(result.lower_bound.x(), result.upper_bound.x());
        let (lower, upper) = (result.lower_bound.y(), result.upper_bound.y());
        assert!(lower.iter().zip(upper).all(|(low, high)| low <= high));
    }
}
