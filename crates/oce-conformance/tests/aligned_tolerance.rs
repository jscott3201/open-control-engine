//! Aligned tolerant-comparison coverage for scalar oracle traces.

use oce_conformance::{Series, Tolerances, ValueKind, compare_aligned_tolerance};

fn series<'a>(x: &'a [f64], y: &'a [f64]) -> Series<'a> {
    Series { x, y }
}

fn zero_tol() -> Tolerances {
    Tolerances {
        atolx: 0.0,
        atoly: 0.0,
        rtolx: 0.0,
        rtoly: 0.0,
        ltolx: 0.0,
        ltoly: 0.0,
    }
}

#[test]
fn finite_reals_pass_within_absolute_tolerance() {
    let x = [0.0, 1.0, 2.0];
    let reference = [0.0, 1.0, -2.0];
    let actual = [0.0, 1.000_000_000_5, -2.000_000_000_5];
    let tol = Tolerances {
        atoly: 1.0e-9,
        ..zero_tol()
    };

    let result = compare_aligned_tolerance(
        series(&x, &reference),
        series(&x, &actual),
        ValueKind::Real,
        &tol,
    );

    assert!(result.passed);
    assert_eq!(result.compared_points, 3);
    assert_eq!(result.first_mismatch, None);
}

#[test]
fn finite_reals_fail_at_first_out_of_band_sample() {
    let x = [0.0, 1.0, 2.0];
    let reference = [0.0, 1.0, 2.0];
    let actual = [0.0, 1.25, 2.0];
    let tol = Tolerances {
        atoly: 0.125,
        ..zero_tol()
    };

    let result = compare_aligned_tolerance(
        series(&x, &reference),
        series(&x, &actual),
        ValueKind::Real,
        &tol,
    );

    assert!(!result.passed);
    assert_eq!(result.compared_points, 3);
    let mismatch = result.first_mismatch.expect("first mismatch");
    assert_eq!(mismatch.index, 1);
    assert_eq!(mismatch.x.to_bits(), 1.0f64.to_bits());
    assert_eq!(mismatch.expected.to_bits(), 1.0f64.to_bits());
    assert_eq!(mismatch.actual.to_bits(), 1.25f64.to_bits());
    assert_eq!(mismatch.error.to_bits(), 0.25f64.to_bits());
    assert_eq!(mismatch.bound.to_bits(), 0.125f64.to_bits());
}

#[test]
fn relative_and_local_tolerances_use_reference_range_and_value() {
    let x = [0.0, 1.0];
    let reference = [10.0, 20.0];
    let actual = [10.25, 20.5];
    let tol = Tolerances {
        rtoly: 0.01,
        ltoly: 0.025,
        ..zero_tol()
    };

    let result = compare_aligned_tolerance(
        series(&x, &reference),
        series(&x, &actual),
        ValueKind::Real,
        &tol,
    );

    assert!(result.passed);

    let fail = compare_aligned_tolerance(
        series(&x, &reference),
        series(&x, &[10.351, 20.6]),
        ValueKind::Real,
        &tol,
    );
    assert!(!fail.passed);
    let mismatch = fail.first_mismatch.expect("first mismatch");
    assert_eq!(mismatch.index, 0);
    assert_eq!(mismatch.bound.to_bits(), 0.35f64.to_bits());
}

#[test]
fn aligned_mode_requires_exact_time_bits_and_lengths() {
    let reference_x = [0.0, 1.0, 2.0];
    let reference_y = [1.0, 2.0, 3.0];
    let shifted_x = [0.0, f64::from_bits(1.0f64.to_bits() + 1), 2.0];
    let tol = Tolerances {
        atoly: 1.0,
        ..zero_tol()
    };

    let shifted = compare_aligned_tolerance(
        series(&reference_x, &reference_y),
        series(&shifted_x, &reference_y),
        ValueKind::Real,
        &tol,
    );
    assert!(!shifted.passed);
    let mismatch = shifted.first_mismatch.expect("time mismatch");
    assert_eq!(mismatch.index, 1);
    assert_eq!(mismatch.expected.to_bits(), 1.0f64.to_bits());
    assert_eq!(mismatch.actual.to_bits(), shifted_x[1].to_bits());

    let shorter = compare_aligned_tolerance(
        series(&reference_x, &reference_y),
        series(&reference_x[..2], &reference_y[..2]),
        ValueKind::Real,
        &tol,
    );
    assert!(!shorter.passed);
    assert_eq!(shorter.compared_points, 2);
    assert_eq!(shorter.first_mismatch.expect("length mismatch").index, 2);
}

#[test]
fn malformed_series_lengths_fail_closed_even_when_min_lengths_match() {
    let reference_x_extra = compare_aligned_tolerance(
        series(&[0.0, 1.0, 2.0], &[10.0, 20.0]),
        series(&[0.0, 1.0], &[10.0, 20.0]),
        ValueKind::Real,
        &zero_tol(),
    );
    assert!(!reference_x_extra.passed);
    assert_eq!(reference_x_extra.compared_points, 0);
    assert_eq!(
        reference_x_extra
            .first_mismatch
            .expect("malformed reference x")
            .index,
        2
    );

    let reference_y_extra = compare_aligned_tolerance(
        series(&[0.0, 1.0], &[10.0, 20.0, 30.0]),
        series(&[0.0, 1.0], &[10.0, 20.0]),
        ValueKind::Real,
        &zero_tol(),
    );
    assert!(!reference_y_extra.passed);
    assert_eq!(reference_y_extra.compared_points, 0);
    assert_eq!(
        reference_y_extra
            .first_mismatch
            .expect("malformed reference y")
            .index,
        2
    );

    let test_x_extra = compare_aligned_tolerance(
        series(&[0.0, 1.0], &[10.0, 20.0]),
        series(&[0.0, 1.0, 2.0], &[10.0, 20.0]),
        ValueKind::Real,
        &zero_tol(),
    );
    assert!(!test_x_extra.passed);
    assert_eq!(test_x_extra.compared_points, 0);
    assert_eq!(
        test_x_extra.first_mismatch.expect("malformed test x").index,
        2
    );

    let test_y_extra = compare_aligned_tolerance(
        series(&[0.0, 1.0], &[10.0, 20.0]),
        series(&[0.0, 1.0], &[10.0, 20.0, 30.0]),
        ValueKind::Real,
        &zero_tol(),
    );
    assert!(!test_y_extra.passed);
    assert_eq!(test_y_extra.compared_points, 0);
    assert_eq!(
        test_y_extra.first_mismatch.expect("malformed test y").index,
        2
    );
}

#[test]
fn non_finite_reals_compare_by_class_and_infinity_sign() {
    let x = [0.0, 1.0, 2.0];
    let reference = [
        f64::from_bits(0x7ff8_0000_0000_0001),
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];
    let actual = [
        f64::from_bits(0x7ff8_0000_0000_0002),
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];

    let pass = compare_aligned_tolerance(
        series(&x, &reference),
        series(&x, &actual),
        ValueKind::Real,
        &zero_tol(),
    );
    assert!(pass.passed);

    let fail = compare_aligned_tolerance(
        series(&x, &reference),
        series(&x, &[reference[0], f64::NEG_INFINITY, reference[2]]),
        ValueKind::Real,
        &Tolerances {
            atoly: 1.0,
            ..zero_tol()
        },
    );
    assert!(!fail.passed);
    let mismatch = fail.first_mismatch.expect("infinity sign mismatch");
    assert_eq!(mismatch.index, 1);
    assert_eq!(mismatch.expected.to_bits(), f64::INFINITY.to_bits());
    assert_eq!(mismatch.actual.to_bits(), f64::NEG_INFINITY.to_bits());
    assert!(mismatch.error.is_infinite());
    assert_eq!(mismatch.bound.to_bits(), 0.0f64.to_bits());
}

#[test]
fn finite_range_or_bound_overflow_fails_closed() {
    let x = [0.0, 1.0];
    let reference = [-f64::MAX, f64::MAX];
    let actual = [-f64::MAX, 0.0];
    let tol = Tolerances {
        rtoly: 0.002,
        ..zero_tol()
    };

    let result = compare_aligned_tolerance(
        series(&x, &reference),
        series(&x, &actual),
        ValueKind::Real,
        &tol,
    );

    assert!(!result.passed);
    assert_eq!(result.compared_points, 0);
    let mismatch = result.first_mismatch.expect("overflowed range mismatch");
    assert!(mismatch.error.is_infinite());
    assert!(mismatch.bound.is_nan());

    let single_x = [0.0];
    let bound_overflow = compare_aligned_tolerance(
        series(&single_x, &[f64::MAX]),
        series(&single_x, &[0.0]),
        ValueKind::Real,
        &Tolerances {
            ltoly: 2.0,
            ..zero_tol()
        },
    );
    assert!(!bound_overflow.passed);
    let mismatch = bound_overflow
        .first_mismatch
        .expect("overflowed bound mismatch");
    assert!(mismatch.error.is_finite());
    assert!(mismatch.bound.is_nan());
}

#[test]
fn zero_tolerance_preserves_signed_zero_bits() {
    let x = [0.0];
    let reference = [-0.0_f64];
    let actual = [0.0_f64];

    let exact = compare_aligned_tolerance(
        series(&x, &reference),
        series(&x, &actual),
        ValueKind::Real,
        &zero_tol(),
    );
    assert!(!exact.passed);

    let tolerant = compare_aligned_tolerance(
        series(&x, &reference),
        series(&x, &actual),
        ValueKind::Real,
        &Tolerances {
            atoly: 1.0e-15,
            ..zero_tol()
        },
    );
    assert!(tolerant.passed);
}

#[test]
fn negative_zero_tolerance_preserves_signed_zero_bits() {
    let x = [0.0];
    let reference = [-0.0_f64];
    let actual = [0.0_f64];

    let result = compare_aligned_tolerance(
        series(&x, &reference),
        series(&x, &actual),
        ValueKind::Real,
        &Tolerances {
            atoly: -0.0,
            rtoly: -0.0,
            ltoly: -0.0,
            ..zero_tol()
        },
    );

    assert!(!result.passed);
    let mismatch = result.first_mismatch.expect("signed-zero mismatch");
    assert_eq!(mismatch.error.to_bits(), 0.0f64.to_bits());
    assert_eq!(mismatch.bound.to_bits(), (-0.0f64).to_bits());
}

#[test]
fn integer_and_boolean_cells_remain_exact() {
    let x = [0.0, 1.0];
    let tol = Tolerances {
        atoly: 10.0,
        ..zero_tol()
    };

    let int_pass = compare_aligned_tolerance(
        series(&x, &[42.0, -7.0]),
        series(&x, &[42.0, -7.0]),
        ValueKind::Integer,
        &tol,
    );
    assert!(int_pass.passed);

    let int_fail = compare_aligned_tolerance(
        series(&x, &[42.0, -7.0]),
        series(&x, &[42.0, -8.0]),
        ValueKind::Integer,
        &tol,
    );
    assert!(!int_fail.passed);
    assert!(
        int_fail
            .first_mismatch
            .expect("int mismatch")
            .error
            .is_infinite()
    );

    let bool_fail = compare_aligned_tolerance(
        series(&x, &[0.0, 1.0]),
        series(&x, &[0.0, 0.0]),
        ValueKind::Boolean,
        &tol,
    );
    assert!(!bool_fail.passed);
    assert_eq!(bool_fail.first_mismatch.expect("bool mismatch").index, 1);
}

#[test]
fn invalid_tolerances_fail_without_panicking() {
    let x = [0.0];
    let y = [1.0];
    let result = compare_aligned_tolerance(
        series(&x, &y),
        series(&x, &y),
        ValueKind::Real,
        &Tolerances {
            atoly: f64::NAN,
            ..zero_tol()
        },
    );

    assert!(!result.passed);
    assert_eq!(result.compared_points, 0);
    let mismatch = result.first_mismatch.expect("invalid tolerance mismatch");
    assert_eq!(mismatch.index, 0);
    assert!(mismatch.error.is_infinite());
    assert!(mismatch.bound.is_nan());
}
