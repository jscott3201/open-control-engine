//! The `aligned-grid` envelope: the conformance harness's conservative band, copied
//! (commissioning note, "Envelope"; decisions 9 and 12).
//!
//! `oce-conformance` compares engine traces with this envelope (`build_bounds` and `compare` in
//! its `funnel.rs`). That crate is verification-only and never a dependency of this one, so the
//! arithmetic is copied here, not shared: a test holds the copy's bounds and errors equal to
//! goldens taken from the original, bit for bit. It is the second named mode beside
//! [`crate::funnel()`] and differs from it in three ways:
//!
//! - the tolerance terms of an axis **add**: the half-width at a reference point is
//!   `atolx + rtolx × range_x + ltolx × |x_i|`, where funnel takes the largest of the three;
//! - the band is built on a **grid** of every reference `x` and every rectangle edge, as the
//!   minimum and the maximum over every rectangle covering each grid point, where funnel walks
//!   corners. It is at least as wide as the rectangles everywhere, and never leaks at a sharp
//!   peak;
//! - the error is taken at the test's instants **and at every breakpoint of the two bounds**,
//!   the test interpolated there, where funnel looks at the test's instants only.
//!
//! Three things are not the original's. Input it reports as a failed comparison is refused here
//! with a typed [`CompareError`], as [`crate::funnel()`] refuses it. The smaller of `+0.0` and
//! `-0.0` is `-0.0` and the larger `+0.0`, on every target, where the original leaves the choice
//! to the platform's `min` and `max` (issue #686). And two scans are searches: a curve is
//! evaluated by binary search for the segment the original's scan stops at, and a breakpoint
//! that is already a test instant is found in a sorted set; the results are the same.
//!
//! The work is still quadratic, every grid point against every rectangle, which is why its point
//! cap ([`CompareLimits::max_aligned_grid_points`]) is far below the funnel's.

use std::collections::BTreeSet;

use crate::compare_error::CompareError;
use crate::compare_limits::CompareLimits;
use crate::envelope::{
    Applied, Curve, Envelope, FunnelResult, HalfSize, Series, Tolerances, check_inputs,
};

/// The revision of the grid envelope [`aligned_grid()`] builds: revision 1, the arithmetic of
/// `oce-conformance`'s `build_bounds` and `compare` as copied here. A receipt carries it as
/// `comparison.envelope_version` beside the envelope `aligned-grid`, as it carries
/// [`crate::FUNNEL_VERSION`] beside `funnel`. A change to the band or to the errors this
/// function gives moves it; funnel's string names an upstream commit, this one has none to
/// name.
pub const ALIGNED_GRID_VERSION: &str = "aligned-grid@1";

/// The points of a curve in order, each `(x, y)`.
type Points = Vec<(f64, f64)>;

/// The rectangle around one reference point.
#[derive(Clone, Copy)]
struct Rectangle {
    x: f64,
    y: f64,
    half_width: f64,
    half_height: f64,
}

/// Compare `test` against the grid envelope built around `reference`: the conformance
/// harness's band, with summed tolerances.
///
/// Each reference point gets a rectangle of half-width `atolx + rtolx × range_x + ltolx × |x_i|`
/// and the same in `y`. The band's grid is every reference `x` and every rectangle's left and
/// right edge; at each grid point the lower bound is the smallest of the reference's value there
/// and the bottom of every rectangle covering it, and the upper bound the largest of the value
/// and the tops. The error is taken at each of the test's instants and at each grid point that
/// is not one (the test interpolated linearly there, held outside its ends), and is how far the
/// test lies outside the band, in the unit of `y`.
///
/// Unlike [`crate::funnel()`] it does not require the two series to start and end at the same
/// `x`: outside the band's grid the bounds are held, and outside the test's range the test is.
/// The result is a pure function of the four arguments, the same bits on every target. Its work
/// is quadratic in the reference's points. It never panics.
///
/// # Errors
///
/// In this order: [`CompareError::InvalidTolerance`]; for the reference and then the test,
/// [`CompareError::LengthMismatch`], [`CompareError::EmptySeries`] and
/// [`CompareError::TooManyPoints`] (over [`CompareLimits::max_aligned_grid_points`]); for the
/// reference and then the test, [`CompareError::NonFiniteSample`] and
/// [`CompareError::XNotNonDecreasing`]; then [`CompareError::NonFiniteEnvelope`] when a
/// rectangle's size or an error overflows.
///
/// ```
/// use oce_verify::{CompareLimits, Series, Tolerances, aligned_grid};
///
/// let time = [0.0, 1.0, 2.0];
/// let reference = Series::new(&time, &[0.0, 1.0, 0.0]);
/// let test = Series::new(&time, &[0.0, 1.25, 0.0]);
/// let tolerances = Tolerances { atoly: 0.125, ..Tolerances::ZERO };
/// let result = aligned_grid(reference, test, &tolerances, &CompareLimits::default())?;
///
/// assert!(!result.within_envelope());
/// assert_eq!(result.max_error, 0.125);
/// assert_eq!(result.max_error_x, Some(1.0));
/// # Ok::<(), oce_verify::CompareError>(())
/// ```
pub fn aligned_grid(
    reference: Series<'_>,
    test: Series<'_>,
    tolerances: &Tolerances,
    limits: &CompareLimits,
) -> Result<FunnelResult, CompareError> {
    let limits = limits.effective();
    check_inputs(reference, test, tolerances, limits.max_aligned_grid_points)?;

    let reference_points: Vec<(f64, f64)> = pairs(reference).collect();
    let test_points: Vec<(f64, f64)> = pairs(test).collect();
    let rectangles = rectangles(&reference_points, tolerances)?;
    let (lower, upper) = bounds_over(&reference_points, &rectangles);

    let mut errors = Vec::with_capacity(test_points.len());
    for &(x, y) in &test_points {
        errors.push((x, error_at(x, y, &lower, &upper)?));
    }
    let applied = Applied {
        errors_at_test: errors.iter().map(|(_, error)| *error).collect(),
        half_width: HalfSize::of(rectangles.iter().map(|r| r.half_width)),
        half_height: HalfSize::of(rectangles.iter().map(|r| r.half_height)),
    };
    let test_instants: BTreeSet<u64> = test_points.iter().map(|(x, _)| x.to_bits()).collect();
    let mut breakpoints: Vec<f64> = lower
        .iter()
        .chain(&upper)
        .map(|(x, _)| *x)
        .filter(|x| !test_instants.contains(&x.to_bits()))
        .collect();
    breakpoints.sort_by(f64::total_cmp);
    breakpoints.dedup_by(|a, b| a.to_bits() == b.to_bits());
    for x in breakpoints {
        let y = value_at(&test_points, x);
        errors.push((x, error_at(x, y, &lower, &upper)?));
    }
    errors.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));

    Ok(FunnelResult::from_curves(
        Envelope::AlignedGrid,
        reference,
        test,
        (Curve::from_points(lower), Curve::from_points(upper)),
        Curve::from_points(errors),
        applied,
    ))
}

fn pairs<'a>(series: Series<'a>) -> impl Iterator<Item = (f64, f64)> + 'a {
    series.x.iter().copied().zip(series.y.iter().copied())
}

/// The lower and upper bound around `reference` under `tolerances`: [`bounds_over`] its
/// rectangles. The original's `build_bounds`, which the parity tests call.
#[cfg(test)]
fn build_bounds(
    reference: &[(f64, f64)],
    tolerances: &Tolerances,
) -> Result<(Points, Points), CompareError> {
    Ok(bounds_over(reference, &rectangles(reference, tolerances)?))
}

/// The lower and upper bound around `reference`, on the grid of every reference `x` and every
/// edge of its `rectangles`.
///
/// Between two adjacent grid points the set of covering rectangles is constant and the reference
/// is linear, so the chord between the extrema sampled at the grid points still encloses the
/// reference and every rectangle: the band is conservative by construction.
fn bounds_over(reference: &[(f64, f64)], rectangles: &[Rectangle]) -> (Points, Points) {
    let mut grid = Vec::with_capacity(rectangles.len() * 3);
    for rectangle in rectangles {
        grid.push(rectangle.x - rectangle.half_width);
        grid.push(rectangle.x);
        grid.push(rectangle.x + rectangle.half_width);
    }
    grid.sort_by(f64::total_cmp);
    grid.dedup_by(|a, b| a.to_bits() == b.to_bits());

    let mut lower = Vec::with_capacity(grid.len());
    let mut upper = Vec::with_capacity(grid.len());
    for x in grid {
        let y = value_at(reference, x);
        let mut low = y;
        let mut high = y;
        for rectangle in rectangles {
            if ((rectangle.x - rectangle.half_width)..=(rectangle.x + rectangle.half_width))
                .contains(&x)
            {
                low = smaller(low, rectangle.y - rectangle.half_height);
                high = larger(high, rectangle.y + rectangle.half_height);
            }
        }
        lower.push((x, low));
        upper.push((x, high));
    }
    (lower, upper)
}

/// The rectangle around each reference point, its half-sizes the **sum** of the three tolerance
/// terms of each axis.
fn rectangles(
    reference: &[(f64, f64)],
    tolerances: &Tolerances,
) -> Result<Vec<Rectangle>, CompareError> {
    let (mut min_x, mut max_x) = (reference[0].0, reference[0].0);
    let (mut min_y, mut max_y) = (reference[0].1, reference[0].1);
    for &(x, y) in reference {
        min_x = smaller(min_x, x);
        max_x = larger(max_x, x);
        min_y = smaller(min_y, y);
        max_y = larger(max_y, y);
    }
    let (range_x, range_y) = (max_x - min_x, max_y - min_y);
    reference
        .iter()
        .map(|&(x, y)| {
            let rectangle = Rectangle {
                x,
                y,
                half_width: tolerances.atolx
                    + tolerances.rtolx * range_x
                    + tolerances.ltolx * x.abs(),
                half_height: tolerances.atoly
                    + tolerances.rtoly * range_y
                    + tolerances.ltoly * y.abs(),
            };
            let edges = [
                rectangle.x - rectangle.half_width,
                rectangle.x + rectangle.half_width,
                rectangle.y - rectangle.half_height,
                rectangle.y + rectangle.half_height,
            ];
            if edges.iter().all(|edge| edge.is_finite()) {
                Ok(rectangle)
            } else {
                Err(CompareError::NonFiniteEnvelope)
            }
        })
        .collect()
}

/// How far `y` lies outside the band at `x`: above the upper bound or below the lower, zero
/// inside. Refused when a bound or the error there is not finite.
fn error_at(
    x: f64,
    y: f64,
    lower: &[(f64, f64)],
    upper: &[(f64, f64)],
) -> Result<f64, CompareError> {
    let low = value_at(lower, x);
    let high = value_at(upper, x);
    let above = y - high;
    let below = y - low;
    let error = (if above > 0.0 { above } else { 0.0 }) - (if below < 0.0 { below } else { 0.0 });
    if y.is_finite() && low.is_finite() && high.is_finite() && error.is_finite() {
        Ok(error)
    } else {
        Err(CompareError::NonFiniteEnvelope)
    }
}

/// The value of the piecewise-linear `curve` at `x`: its first value at or before its first
/// point, its last value beyond its last, the later point's value where two points share an
/// `x`. `curve` is not empty and its `x` is non-decreasing.
///
/// The original scans the segments from the first; the segment it stops at is the first whose
/// end is at or beyond `x`, which a binary search finds as well.
fn value_at(curve: &[(f64, f64)], x: f64) -> f64 {
    if x <= curve[0].0 {
        return curve[0].1;
    }
    let end = 1 + curve[1..].partition_point(|point| point.0 < x);
    if end == curve.len() {
        return curve[curve.len() - 1].1;
    }
    let (x0, y0) = curve[end - 1];
    let (x1, y1) = curve[end];
    if x == x1 || x1 == x0 {
        return y1;
    }
    let fraction = (x - x0) / (x1 - x0);
    y0 + fraction * (y1 - y0)
}

/// The smaller of two finite values; `-0.0` against `+0.0` is `-0.0` (IEEE 754-2019 `minimum`).
fn smaller(a: f64, b: f64) -> f64 {
    if b < a || (b == a && b.is_sign_negative()) {
        b
    } else {
        a
    }
}

/// The larger of two finite values; `+0.0` against `-0.0` is `+0.0` (IEEE 754-2019 `maximum`).
fn larger(a: f64, b: f64) -> f64 {
    if b > a || (b == a && b.is_sign_positive()) {
        b
    } else {
        a
    }
}

#[cfg(test)]
#[path = "aligned_grid_tests.rs"]
mod tests;
