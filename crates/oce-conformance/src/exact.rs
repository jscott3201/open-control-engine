//! Tier-1 exact trace comparison for closed-form block goldens.
//!
//! This comparator is deliberately separate from the funnel band. Tier-1 block traces are exact
//! scalar math or discrete logic, so they compare encoded sample values directly, including IEEE
//! non-finite classes for Real outputs.

use crate::{Series, ValueKind};

/// Exact-comparison verdict for one output signal.
#[derive(Clone, Debug, PartialEq)]
pub struct ExactResult {
    /// True iff every aligned sample matched exactly under the signal kind's equality rule.
    pub passed: bool,
    /// Number of aligned sample pairs in the comparison domain.
    pub compared_points: usize,
    /// First mismatch, if any.
    pub first_mismatch: Option<ExactMismatch>,
}

/// First observed exact-comparison mismatch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExactMismatch {
    /// Zero-based sample index.
    pub index: usize,
    /// Reference x/time at the mismatch when available.
    pub x: f64,
    /// Expected encoded value.
    pub expected: f64,
    /// Actual encoded value.
    pub actual: f64,
}

/// Compare two aligned series exactly under `kind`.
///
/// Real values compare by bit equality when finite; NaN compares equal to any NaN payload; positive
/// and negative infinity compare only to their matching signed infinity. Integer and Boolean
/// signals compare their encoded numeric cells exactly.
#[must_use]
pub fn compare_exact(reference: Series<'_>, test: Series<'_>, kind: ValueKind) -> ExactResult {
    let compared_points = reference.len().min(test.len());
    for index in 0..compared_points {
        let x_ref = reference.x[index];
        let x_test = test.x[index];
        if x_ref.to_bits() != x_test.to_bits() {
            return mismatch(index, x_ref, x_ref, x_test, compared_points);
        }

        let expected = reference.y[index];
        let actual = test.y[index];
        if !values_equal(kind, expected, actual) {
            return mismatch(index, x_ref, expected, actual, compared_points);
        }
    }

    if reference.len() != test.len() {
        let index = compared_points;
        let x = reference
            .x
            .get(index)
            .copied()
            .or_else(|| test.x.get(index).copied())
            .unwrap_or(f64::NAN);
        let expected = reference.y.get(index).copied().unwrap_or(f64::NAN);
        let actual = test.y.get(index).copied().unwrap_or(f64::NAN);
        return mismatch(index, x, expected, actual, compared_points);
    }

    ExactResult {
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
    compared_points: usize,
) -> ExactResult {
    ExactResult {
        passed: false,
        compared_points,
        first_mismatch: Some(ExactMismatch {
            index,
            x,
            expected,
            actual,
        }),
    }
}

fn values_equal(kind: ValueKind, expected: f64, actual: f64) -> bool {
    match kind {
        ValueKind::Real => real_equal(expected, actual),
        ValueKind::Integer | ValueKind::Boolean => expected.to_bits() == actual.to_bits(),
    }
}

fn real_equal(expected: f64, actual: f64) -> bool {
    if expected.is_nan() || actual.is_nan() {
        return expected.is_nan() && actual.is_nan();
    }
    expected.to_bits() == actual.to_bits()
}
