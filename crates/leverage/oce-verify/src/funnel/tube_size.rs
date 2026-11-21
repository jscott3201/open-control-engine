//! The rectangle around each reference point: funnel's `tubeSize.c` (`get_data_char`,
//! `set_tube_size`), and the normalisation of `x` from `algorithmRectangle.c`.

use super::equality::equ;
use crate::envelope::{Series, Tolerances};

/// C's `max` macro, `(a > b) ? a : b`: the second operand when the two compare equal or
/// unordered. Not `f64::max`, whose result for `+0.0` against `-0.0` is not specified.
fn larger(a: f64, b: f64) -> f64 {
    if a > b { a } else { b }
}

/// The range and the magnitude of a reference series on each axis (funnel's `data_char`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Extent {
    /// Largest `x` minus smallest `x`.
    pub(super) range_x: f64,
    /// Largest `y` minus smallest `y`.
    pub(super) range_y: f64,
    /// The larger of the largest `x` and the absolute smallest `x`.
    pub(super) magnitude_x: f64,
    /// The larger of the largest `y` and the absolute smallest `y`.
    pub(super) magnitude_y: f64,
}

impl Extent {
    /// The extent of a series with at least one point.
    pub(super) fn of(series: Series<'_>) -> Self {
        let (min_x, max_x) = min_max(series.x);
        let (min_y, max_y) = min_max(series.y);
        Self {
            range_x: max_x - min_x,
            range_y: max_y - min_y,
            magnitude_x: larger(max_x, min_x.abs()),
            magnitude_y: larger(max_y, min_y.abs()),
        }
    }

    /// Whether the tube is built on `x` divided by its magnitude: funnel normalises when the
    /// magnitude exceeds `1e-5`, to keep the slopes `dy/dx` away from zero for a large `x`.
    pub(super) fn normalises_x(&self) -> bool {
        self.magnitude_x > 1e-5
    }

    /// `values` divided by the magnitude of `x` when [`Extent::normalises_x`], else unchanged.
    pub(super) fn normalised(&self, values: &[f64]) -> Vec<f64> {
        if self.normalises_x() {
            values
                .iter()
                .map(|value| value / self.magnitude_x)
                .collect()
        } else {
            values.to_vec()
        }
    }

    /// The inverse of [`Extent::normalised`] for one value, by multiplication.
    pub(super) fn denormalised(&self, value: f64) -> f64 {
        if self.normalises_x() {
            value * self.magnitude_x
        } else {
            value
        }
    }
}

/// The smallest and the largest of a non-empty slice, by `<` and `>` from the first element.
fn min_max(values: &[f64]) -> (f64, f64) {
    let mut min = values[0];
    let mut max = values[0];
    for value in values {
        if *value < min {
            min = *value;
        }
        if *value > max {
            max = *value;
        }
    }
    (min, max)
}

/// The half-width and half-height of the rectangle around each reference point, in the units of
/// `x` and `y`.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct HalfSizes {
    /// Half-width per reference point, in the unit of `x`: at least `1e-10`, unless the range
    /// of `x` overflowed (then infinite or NaN, which the comparison refuses later).
    pub(super) x: Vec<f64>,
    /// Half-height per reference point, in the unit of `y`, with the same floor.
    pub(super) y: Vec<f64>,
}

/// funnel's rectangle half-sizes for `reference`.
///
/// On each axis the half-size at point `i` is the **maximum** of the three terms,
/// `max(atol, rtol × range, ltol × |value_i|)`. A half-size that is zero by [`equ`] (at most
/// `1e-10`) is replaced: by `rtol × magnitude` when `rtol` is not zero (for a constant series,
/// whose range is zero, as csv-compare does), and then, if still zero, by `1e-10`.
pub(super) fn half_sizes(
    reference: Series<'_>,
    extent: &Extent,
    tolerances: &Tolerances,
) -> HalfSizes {
    let axis = |values: &[f64], atol: f64, rtol: f64, ltol: f64, range: f64, magnitude: f64| {
        values
            .iter()
            .map(|value| {
                let mut size = larger(larger(atol, rtol * range), ltol * value.abs());
                if equ(size, 0.0) {
                    if !equ(rtol, 0.0) {
                        size = larger(size, rtol * magnitude);
                    }
                    if equ(size, 0.0) {
                        size = larger(size, 1e-10);
                    }
                }
                size
            })
            .collect()
    };
    HalfSizes {
        x: axis(
            reference.x,
            tolerances.atolx,
            tolerances.rtolx,
            tolerances.ltolx,
            extent.range_x,
            extent.magnitude_x,
        ),
        y: axis(
            reference.y,
            tolerances.atoly,
            tolerances.rtoly,
            tolerances.ltoly,
            extent.range_y,
            extent.magnitude_y,
        ),
    }
}

#[cfg(test)]
#[path = "tube_size_tests.rs"]
mod tests;
