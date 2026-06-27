//! Aligned tolerance comparison for scalar oracle traces.
//!
//! This comparator is for algebraic traces whose sample times must already be identical but whose
//! finite `Real` values may need a small platform tolerance. It deliberately keeps non-finite Real
//! behavior class-based, so domain and overflow samples can stay in the same oracle trace.

use crate::{Series, Tolerances, ValueKind};

/// Aligned tolerant-comparison verdict for one output signal.
#[derive(Clone, Debug, PartialEq)]
pub struct AlignedToleranceResult {
    /// True iff every aligned sample matched under the signal kind's equality/tolerance rule.
    pub passed: bool,
    /// Number of aligned sample pairs in the comparison domain.
    pub compared_points: usize,
    /// First mismatch, if any.
    pub first_mismatch: Option<AlignedToleranceMismatch>,
}

/// First observed aligned tolerant-comparison mismatch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AlignedToleranceMismatch {
    /// Zero-based sample index.
    pub index: usize,
    /// Reference x/time at the mismatch when available.
    pub x: f64,
    /// Expected encoded value.
    pub expected: f64,
    /// Actual encoded value.
    pub actual: f64,
    /// Absolute value error for finite Real samples, or infinity for class/exact mismatches.
    pub error: f64,
    /// Allowed value error at this sample.
    pub bound: f64,
}

/// Compare two aligned series under `kind`.
///
/// Time samples must match by raw `f64` bits. Finite `Real` values compare with
/// `atoly + rtoly * reference_range + ltoly * abs(reference_y)`, where the range is computed over
/// finite reference values. If that bound is exactly zero, finite Reals compare by raw bits so
/// signed-zero-sensitive traces can still be pinned. NaN compares equal to any NaN payload and
/// infinities compare only to the same signed infinity. Integer and Boolean signals compare their
/// encoded numeric cells exactly.
#[must_use]
pub fn compare_aligned_tolerance(
    reference: Series<'_>,
    test: Series<'_>,
    kind: ValueKind,
    tol: &Tolerances,
) -> AlignedToleranceResult {
    if !valid_tolerances(tol) {
        return mismatch(
            0,
            first_x(reference, test),
            f64::NAN,
            f64::NAN,
            f64::INFINITY,
            f64::NAN,
            0,
        );
    }

    let Some(reference_len) = exact_series_len(reference) else {
        return malformed_series_mismatch(reference, test);
    };
    let Some(test_len) = exact_series_len(test) else {
        return malformed_series_mismatch(reference, test);
    };
    let Some(range_y) = reference_range_y(reference) else {
        return mismatch(
            0,
            first_x(reference, test),
            f64::NAN,
            f64::NAN,
            f64::INFINITY,
            f64::NAN,
            0,
        );
    };

    let compared_points = reference_len.min(test_len);
    for index in 0..compared_points {
        let x_ref = reference.x[index];
        let x_test = test.x[index];
        if x_ref.to_bits() != x_test.to_bits() {
            return mismatch(
                index,
                x_ref,
                x_ref,
                x_test,
                finite_abs_error(x_ref, x_test),
                0.0,
                compared_points,
            );
        }

        let expected = reference.y[index];
        let actual = test.y[index];
        if !values_equal(kind, expected, actual, range_y, tol) {
            let bound = value_bound(kind, expected, range_y, tol);
            return mismatch(
                index,
                x_ref,
                expected,
                actual,
                value_error(kind, expected, actual),
                bound,
                compared_points,
            );
        }
    }

    if reference_len != test_len {
        let index = compared_points;
        let x = reference
            .x
            .get(index)
            .copied()
            .or_else(|| test.x.get(index).copied())
            .unwrap_or(f64::NAN);
        let expected = reference.y.get(index).copied().unwrap_or(f64::NAN);
        let actual = test.y.get(index).copied().unwrap_or(f64::NAN);
        return mismatch(
            index,
            x,
            expected,
            actual,
            f64::INFINITY,
            value_bound(kind, expected, range_y, tol),
            compared_points,
        );
    }

    AlignedToleranceResult {
        passed: true,
        compared_points,
        first_mismatch: None,
    }
}

fn mismatch(
    index: usize,
    x: f64,
    expected: f64,
    actual: f64,
    error: f64,
    bound: f64,
    compared_points: usize,
) -> AlignedToleranceResult {
    AlignedToleranceResult {
        passed: false,
        compared_points,
        first_mismatch: Some(AlignedToleranceMismatch {
            index,
            x,
            expected,
            actual,
            error,
            bound,
        }),
    }
}

fn valid_tolerances(tol: &Tolerances) -> bool {
    [
        tol.atolx, tol.atoly, tol.rtolx, tol.rtoly, tol.ltolx, tol.ltoly,
    ]
    .iter()
    .all(|v| v.is_finite() && *v >= 0.0)
}

fn first_x(reference: Series<'_>, test: Series<'_>) -> f64 {
    reference
        .x
        .first()
        .copied()
        .or_else(|| test.x.first().copied())
        .unwrap_or(f64::NAN)
}

fn exact_series_len(series: Series<'_>) -> Option<usize> {
    (series.x.len() == series.y.len()).then_some(series.x.len())
}

fn malformed_series_mismatch(reference: Series<'_>, test: Series<'_>) -> AlignedToleranceResult {
    let index = first_malformed_index(reference)
        .or_else(|| first_malformed_index(test))
        .unwrap_or(0);
    let x = reference
        .x
        .get(index)
        .copied()
        .or_else(|| test.x.get(index).copied())
        .unwrap_or(f64::NAN);
    let expected = reference.y.get(index).copied().unwrap_or(f64::NAN);
    let actual = test.y.get(index).copied().unwrap_or(f64::NAN);
    mismatch(index, x, expected, actual, f64::INFINITY, 0.0, 0)
}

fn first_malformed_index(series: Series<'_>) -> Option<usize> {
    (series.x.len() != series.y.len()).then_some(series.x.len().min(series.y.len()))
}

fn values_equal(
    kind: ValueKind,
    expected: f64,
    actual: f64,
    range_y: f64,
    tol: &Tolerances,
) -> bool {
    match kind {
        ValueKind::Real => real_equal(expected, actual, range_y, tol),
        ValueKind::Integer | ValueKind::Boolean => expected.to_bits() == actual.to_bits(),
    }
}

fn real_equal(expected: f64, actual: f64, range_y: f64, tol: &Tolerances) -> bool {
    if expected.is_nan() || actual.is_nan() {
        return expected.is_nan() && actual.is_nan();
    }
    if expected.is_infinite() || actual.is_infinite() {
        return expected.to_bits() == actual.to_bits();
    }
    let Some(bound) = real_bound(expected, range_y, tol) else {
        return false;
    };
    if bound == 0.0 {
        return expected.to_bits() == actual.to_bits();
    }
    let error = finite_abs_error(expected, actual);
    error.is_finite() && error <= bound
}

fn value_bound(kind: ValueKind, expected: f64, range_y: f64, tol: &Tolerances) -> f64 {
    match kind {
        ValueKind::Real if expected.is_finite() => {
            real_bound(expected, range_y, tol).unwrap_or(f64::NAN)
        }
        _ => 0.0,
    }
}

fn real_bound(expected: f64, range_y: f64, tol: &Tolerances) -> Option<f64> {
    let bound = tol.atoly + tol.rtoly * range_y + tol.ltoly * expected.abs();
    bound.is_finite().then_some(bound)
}

fn value_error(kind: ValueKind, expected: f64, actual: f64) -> f64 {
    match kind {
        ValueKind::Real => finite_abs_error(expected, actual),
        ValueKind::Integer | ValueKind::Boolean => {
            if expected.to_bits() == actual.to_bits() {
                0.0
            } else {
                f64::INFINITY
            }
        }
    }
}

fn finite_abs_error(expected: f64, actual: f64) -> f64 {
    if expected.is_finite() && actual.is_finite() {
        (actual - expected).abs()
    } else {
        f64::INFINITY
    }
}

fn reference_range_y(reference: Series<'_>) -> Option<f64> {
    let mut finite = reference
        .y
        .iter()
        .copied()
        .filter(|value| value.is_finite());
    let Some(first) = finite.next() else {
        return Some(0.0);
    };
    let (mut min_y, mut max_y) = (first, first);
    for y in finite {
        min_y = min_y.min(y);
        max_y = max_y.max(y);
    }
    let range = max_y - min_y;
    range.is_finite().then_some(range)
}
