//! Tier-1 exact-comparison coverage for closed-form block goldens.

use oce_conformance::{Series, ValueKind, compare_exact};

fn series<'a>(x: &'a [f64], y: &'a [f64]) -> Series<'a> {
    Series { x, y }
}

#[test]
fn exact_real_bit_match_passes_and_counts_points() {
    let x = [0.0, 1.0, 2.0];
    let y = [0.1_f64, -0.0, 3.5];
    let result = compare_exact(series(&x, &y), series(&x, &y), ValueKind::Real);
    assert!(result.passed);
    assert_eq!(result.compared_points, 3);
    assert_eq!(result.first_mismatch, None);
}

#[test]
fn exact_real_single_ulp_mismatch_fails_with_first_mismatch() {
    let x = [0.0, 1.0, 2.0];
    let expected = [0.1_f64, 1.0, 2.0];
    let actual = [0.1_f64, f64::from_bits(1.0f64.to_bits() + 1), 2.0];
    let result = compare_exact(series(&x, &expected), series(&x, &actual), ValueKind::Real);
    assert!(!result.passed);
    assert_eq!(result.compared_points, 3);
    let mismatch = result.first_mismatch.expect("first mismatch");
    assert_eq!(mismatch.index, 1);
    assert_eq!(mismatch.x.to_bits(), 1.0f64.to_bits());
    assert_eq!(mismatch.expected.to_bits(), 1.0f64.to_bits());
    assert_eq!(mismatch.actual.to_bits(), actual[1].to_bits());
}

#[test]
fn exact_real_nan_class_matches_without_payload_equality() {
    let x = [0.0];
    let expected = [f64::from_bits(0x7ff8_0000_0000_0001)];
    let actual = [f64::from_bits(0x7ff8_0000_0000_0002)];
    let result = compare_exact(series(&x, &expected), series(&x, &actual), ValueKind::Real);
    assert!(result.passed, "NaN class equality should ignore payload");
    assert_eq!(result.compared_points, 1);
}

#[test]
fn exact_real_signed_infinities_must_match() {
    let x = [0.0];
    let result = compare_exact(
        series(&x, &[f64::INFINITY]),
        series(&x, &[f64::NEG_INFINITY]),
        ValueKind::Real,
    );
    assert!(!result.passed);
    let mismatch = result.first_mismatch.expect("first mismatch");
    assert_eq!(mismatch.expected.to_bits(), f64::INFINITY.to_bits());
    assert_eq!(mismatch.actual.to_bits(), f64::NEG_INFINITY.to_bits());
}

#[test]
fn exact_boolean_and_integer_cells_are_exact() {
    let x = [0.0, 1.0];
    let bool_pass = compare_exact(
        series(&x, &[0.0, 1.0]),
        series(&x, &[0.0, 1.0]),
        ValueKind::Boolean,
    );
    assert!(bool_pass.passed);

    let bool_fail = compare_exact(
        series(&x, &[0.0, 1.0]),
        series(&x, &[0.0, 0.0]),
        ValueKind::Boolean,
    );
    assert!(!bool_fail.passed);
    assert_eq!(bool_fail.first_mismatch.expect("bool mismatch").index, 1);

    let int_pass = compare_exact(
        series(&x, &[42.0, -7.0]),
        series(&x, &[42.0, -7.0]),
        ValueKind::Integer,
    );
    assert!(int_pass.passed);

    let int_fail = compare_exact(
        series(&x, &[42.0, -7.0]),
        series(&x, &[42.0, -8.0]),
        ValueKind::Integer,
    );
    assert!(!int_fail.passed);
    assert_eq!(int_fail.first_mismatch.expect("int mismatch").index, 1);
}

#[test]
fn exact_length_mismatch_fails_at_prefix_boundary() {
    let reference_x = [0.0, 1.0, 2.0];
    let reference_y = [10.0, 20.0, 30.0];

    let shorter_x = [0.0, 1.0];
    let shorter_y = [10.0, 20.0];
    let shorter = compare_exact(
        series(&reference_x, &reference_y),
        series(&shorter_x, &shorter_y),
        ValueKind::Real,
    );
    assert!(!shorter.passed);
    assert_eq!(shorter.compared_points, shorter_x.len());
    let mismatch = shorter.first_mismatch.expect("short trace mismatch");
    assert_eq!(mismatch.index, shorter.compared_points);
    assert_eq!(mismatch.x.to_bits(), reference_x[2].to_bits());
    assert_eq!(mismatch.expected.to_bits(), reference_y[2].to_bits());
    assert!(mismatch.actual.is_nan());

    let longer_x = [0.0, 1.0, 2.0, 3.0];
    let longer_y = [10.0, 20.0, 30.0, 40.0];
    let longer = compare_exact(
        series(&reference_x, &reference_y),
        series(&longer_x, &longer_y),
        ValueKind::Real,
    );
    assert!(!longer.passed);
    assert_eq!(longer.compared_points, reference_x.len());
    let mismatch = longer.first_mismatch.expect("long trace mismatch");
    assert_eq!(mismatch.index, longer.compared_points);
    assert_eq!(mismatch.x.to_bits(), longer_x[3].to_bits());
    assert!(mismatch.expected.is_nan());
    assert_eq!(mismatch.actual.to_bits(), longer_y[3].to_bits());
}
