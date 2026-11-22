//! The types the two tolerance envelopes share (commissioning note, "Envelope"; vectors note
//! decision 9): the six tolerances, a borrowed series, an owned curve, the envelope's name and
//! the result of one comparison.
//!
//! An envelope answers one question about two sampled curves: is the **test** inside a band
//! built around the **reference**? The band is asymmetric, so which curve is the reference is
//! part of the question. Two bands are built here, and a result names the one that made it:
//!
//! - [`Envelope::Funnel`] ([`crate::funnel()`]): the port of LBNL funnel, the tool OBC §12 cites.
//! - [`Envelope::AlignedGrid`] ([`crate::aligned_grid()`]): the wider grid envelope of this
//!   repository's conformance harness.
//!
//! Nothing here reads a file, a clock or a vector set. How a vector file or a commissioning spec
//! asks for a tolerance is decided elsewhere; these are the arithmetic's own types.

use std::fmt;

use crate::compare_error::{Axis, CompareError, SeriesRole};
use crate::compare_limits::CompareLimits;
use crate::g16::write_g16;

/// The six tolerances of one comparison, three per axis, with funnel's names (vectors note
/// decision 9 fixes these as the one tolerance type).
///
/// `x` is the independent variable (time, in seconds, for a trend) and `y` the compared value, in
/// whatever unit the two series share. Every field is finite and at least zero; a comparison
/// refuses any other value as [`CompareError::InvalidTolerance`]. There is no `Default`: a
/// tolerance is always written down by whoever asks for the comparison.
///
/// How the three terms of an axis combine is the envelope's business, not this type's:
/// [`crate::funnel()`] takes their **maximum** and [`crate::aligned_grid()`] their **sum**.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tolerances {
    /// Absolute tolerance along `x`, in the unit of `x`.
    pub atolx: f64,
    /// Absolute tolerance along `y`, in the unit of `y`.
    pub atoly: f64,
    /// Tolerance along `x` relative to the reference's range in `x` (largest `x` minus smallest):
    /// a dimensionless fraction.
    pub rtolx: f64,
    /// Tolerance along `y` relative to the reference's range in `y`: a dimensionless fraction.
    pub rtoly: f64,
    /// Tolerance along `x` relative to the local value `|x_i|` of each reference point: a
    /// dimensionless fraction.
    pub ltolx: f64,
    /// Tolerance along `y` relative to the local value `|y_i|` of each reference point: a
    /// dimensionless fraction.
    pub ltoly: f64,
}

impl Tolerances {
    /// Every tolerance zero. [`crate::funnel()`] still gives each reference point a rectangle of
    /// half-size `1e-10` on both axes, as funnel does; [`crate::aligned_grid()`] compares against
    /// the reference curve itself.
    pub const ZERO: Self = Self {
        atolx: 0.0,
        atoly: 0.0,
        rtolx: 0.0,
        rtoly: 0.0,
        ltolx: 0.0,
        ltoly: 0.0,
    };

    /// The six fields with their names, in the order a refusal reports the first bad one.
    fn named(&self) -> [(&'static str, f64); 6] {
        [
            ("atolx", self.atolx),
            ("atoly", self.atoly),
            ("rtolx", self.rtolx),
            ("rtoly", self.rtoly),
            ("ltolx", self.ltolx),
            ("ltoly", self.ltoly),
        ]
    }

    /// Refuses a tolerance that is negative, infinite or NaN.
    fn check(&self) -> Result<(), CompareError> {
        for (name, value) in self.named() {
            if !(value.is_finite() && value >= 0.0) {
                return Err(CompareError::InvalidTolerance { name });
            }
        }
        Ok(())
    }
}

/// One sampled curve, borrowed: `x[i]` pairs with `y[i]`.
///
/// A comparison requires the two slices to have one length, at least one point, every sample
/// finite and `x` non-decreasing (two equal `x` values mark a step); it refuses anything else
/// with a typed [`CompareError`], never a panic. Constructing a `Series` checks nothing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Series<'a> {
    /// The independent variable, non-decreasing; seconds for a trend.
    pub x: &'a [f64],
    /// The value at each `x`.
    pub y: &'a [f64],
}

impl<'a> Series<'a> {
    /// A series over two slices.
    #[must_use]
    pub const fn new(x: &'a [f64], y: &'a [f64]) -> Self {
        Self { x, y }
    }

    /// Refuses a series a comparison cannot read, in this order: unequal lengths, no point, more
    /// than `max_points` points. All three are decided from the lengths alone.
    fn check_shape(&self, role: SeriesRole, max_points: usize) -> Result<(), CompareError> {
        if self.x.len() != self.y.len() {
            return Err(CompareError::LengthMismatch {
                series: role,
                x_len: self.x.len(),
                y_len: self.y.len(),
            });
        }
        if self.x.is_empty() {
            return Err(CompareError::EmptySeries { series: role });
        }
        if self.x.len() > max_points {
            return Err(CompareError::TooManyPoints {
                series: role,
                points: self.x.len(),
                limit: max_points,
            });
        }
        Ok(())
    }

    /// Refuses the first non-finite sample (`x` before `y` at one index), then the first `x`
    /// smaller than the one before it.
    fn check_samples(&self, role: SeriesRole) -> Result<(), CompareError> {
        for (index, (x, y)) in self.x.iter().zip(self.y).enumerate() {
            for (axis, value) in [(Axis::X, x), (Axis::Y, y)] {
                if !value.is_finite() {
                    return Err(CompareError::NonFiniteSample {
                        series: role,
                        axis,
                        index,
                    });
                }
            }
        }
        match self.x.windows(2).position(|pair| pair[1] < pair[0]) {
            Some(before) => Err(CompareError::XNotNonDecreasing {
                series: role,
                index: before + 1,
            }),
            None => Ok(()),
        }
    }
}

/// The checks both envelopes make before any arithmetic, in a fixed order so that one input has
/// one refusal: the tolerances; then the shape of the reference and of the test (lengths, empty,
/// the point cap), decided before a sample is read; then the samples of the reference and of the
/// test (finite, `x` non-decreasing).
pub(crate) fn check_inputs(
    reference: Series<'_>,
    test: Series<'_>,
    tolerances: &Tolerances,
    max_points: usize,
) -> Result<(), CompareError> {
    tolerances.check()?;
    reference.check_shape(SeriesRole::Reference, max_points)?;
    test.check_shape(SeriesRole::Test, max_points)?;
    reference.check_samples(SeriesRole::Reference)?;
    test.check_samples(SeriesRole::Test)
}

/// One sampled curve, owned: the points of a result, in order.
///
/// `x` and `y` always have one length. Between two points the curve is the straight line through
/// them.
#[derive(Clone, Debug, PartialEq)]
pub struct Curve {
    x: Vec<f64>,
    y: Vec<f64>,
}

impl Curve {
    /// A curve from its two columns. The caller has made them one length.
    pub(crate) fn from_columns(x: Vec<f64>, y: Vec<f64>) -> Self {
        debug_assert_eq!(x.len(), y.len(), "a curve's columns have one length");
        Self { x, y }
    }

    /// A curve from points in order.
    pub(crate) fn from_points(points: impl IntoIterator<Item = (f64, f64)>) -> Self {
        let (x, y) = points.into_iter().unzip();
        Self { x, y }
    }

    /// The `x` column.
    #[must_use]
    pub fn x(&self) -> &[f64] {
        &self.x
    }

    /// The `y` column, as long as [`Curve::x`].
    #[must_use]
    pub fn y(&self) -> &[f64] {
        &self.y
    }

    /// The number of points.
    #[must_use]
    pub fn len(&self) -> usize {
        self.x.len()
    }

    /// Whether the curve has no point.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.x.is_empty()
    }

    /// The points in order, each `(x, y)`.
    pub fn points(&self) -> impl ExactSizeIterator<Item = (f64, f64)> + '_ {
        self.x.iter().copied().zip(self.y.iter().copied())
    }

    /// The curve as a borrowed [`Series`].
    #[must_use]
    pub fn as_series(&self) -> Series<'_> {
        Series::new(&self.x, &self.y)
    }

    /// The curve as pyfunnel writes one: the line `x,y`, then one line `<x>,<y>` per point with
    /// both numbers in C's `%.16g` ([`crate::format_g16`]), every line ended by a line feed.
    ///
    /// Sixteen significant digits do not always tell two doubles apart, so this text is for
    /// people and for comparison with funnel's own files; the bits are in [`Curve::x`] and
    /// [`Curve::y`].
    #[must_use]
    pub fn to_csv(&self) -> String {
        let mut text = String::with_capacity(4 + self.len() * 40);
        text.push_str("x,y\n");
        for (x, y) in self.points() {
            write_g16(&mut text, x);
            text.push(',');
            write_g16(&mut text, y);
            text.push('\n');
        }
        text
    }
}

/// Which envelope a comparison builds around the reference (commissioning note, decisions 1 and
/// 12).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Envelope {
    /// LBNL funnel's tube, ported ([`crate::funnel()`]): the tolerance terms of an axis combine
    /// by maximum, and the band's outline is made of rectangle corners. The envelope OBC §12
    /// cites. Spelled `funnel`.
    Funnel,
    /// The conformance harness's grid envelope ([`crate::aligned_grid()`]): the tolerance terms
    /// of an axis add, and the band is the minimum and maximum over every rectangle covering each
    /// grid point. Wider than funnel's. Spelled `aligned-grid`.
    AlignedGrid,
}

impl Envelope {
    /// Both envelopes, in declaration order.
    pub const ALL: [Self; 2] = [Self::Funnel, Self::AlignedGrid];

    /// The envelope's spelling in a spec and a receipt: `funnel` or `aligned-grid`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Funnel => "funnel",
            Self::AlignedGrid => "aligned-grid",
        }
    }

    /// The version of the arithmetic behind this envelope, the string a receipt carries as
    /// `comparison.envelope_version`: [`crate::FUNNEL_VERSION`] (`funnel@a12fa96`) or
    /// [`crate::ALIGNED_GRID_VERSION`] (`aligned-grid@1`).
    #[must_use]
    pub const fn version(self) -> &'static str {
        match self {
            Self::Funnel => crate::funnel::FUNNEL_VERSION,
            Self::AlignedGrid => crate::aligned_grid::ALIGNED_GRID_VERSION,
        }
    }

    /// The envelope spelled `name`, exactly; `None` for any other string.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|envelope| envelope.as_str() == name)
    }

    /// Compare `test` against `reference` with this envelope: [`crate::funnel()`] or
    /// [`crate::aligned_grid()`].
    ///
    /// # Errors
    ///
    /// Whatever the chosen comparison refuses; see [`CompareError`].
    pub fn compare(
        self,
        reference: Series<'_>,
        test: Series<'_>,
        tolerances: &Tolerances,
        limits: &CompareLimits,
    ) -> Result<FunnelResult, CompareError> {
        match self {
            Self::Funnel => crate::funnel::funnel(reference, test, tolerances, limits),
            Self::AlignedGrid => {
                crate::aligned_grid::aligned_grid(reference, test, tolerances, limits)
            }
        }
    }
}

impl fmt::Display for Envelope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The smallest and the largest of one axis's half-sizes over the reference points: what a
/// receipt reports as `half_width` or `half_height` (commissioning note, "Report and receipt").
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HalfSize {
    /// The smallest half-size.
    pub min: f64,
    /// The largest half-size.
    pub max: f64,
}

impl HalfSize {
    /// The extremes of `sizes`, which is not empty and holds no NaN, by `<` and `>` from the
    /// first. `0.0` and `0.0` for an empty slice, which no comparison gives.
    pub(crate) fn of(sizes: impl IntoIterator<Item = f64>) -> Self {
        let mut sizes = sizes.into_iter();
        let Some(first) = sizes.next() else {
            return Self { min: 0.0, max: 0.0 };
        };
        let (mut min, mut max) = (first, first);
        for size in sizes {
            if size < min {
                min = size;
            }
            if size > max {
                max = size;
            }
        }
        Self { min, max }
    }
}

/// What one comparison found: the five curves funnel reports and the figures a verdict is read
/// from.
///
/// The names follow pyfunnel's five files (`reference.csv`, `test.csv`, `lowerBound.csv`,
/// `upperBound.csv`, `errors.csv`). Every number in it is finite. A result is plain data: it
/// holds no verdict beyond [`FunnelResult::within_envelope`], and what counts as a pass (a
/// masked instant, an output with no data) is the caller's rule.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub struct FunnelResult {
    /// The envelope that produced this result.
    pub envelope: Envelope,
    /// The reference series, as given.
    pub reference: Curve,
    /// The test series, as given.
    pub test: Curve,
    /// The lower edge of the band, in the reference's units.
    pub lower_bound: Curve,
    /// The upper edge of the band, in the reference's units.
    pub upper_bound: Curve,
    /// The error at each compared instant: how far the test lies outside the band there, in the
    /// unit of `y`; zero inside it, never negative. For [`Envelope::Funnel`] the instants are
    /// the test's own, in order; for [`Envelope::AlignedGrid`] they are the test's and every
    /// breakpoint of the two bounds, in ascending `x`.
    pub errors: Curve,
    /// The number of compared instants: the length of [`FunnelResult::errors`]. At least one.
    pub compared: usize,
    /// The largest error, in the unit of `y`; `0.0` when the test never leaves the band.
    pub max_error: f64,
    /// The `x` of the first instant, in the order of [`FunnelResult::errors`], whose error is
    /// [`FunnelResult::max_error`]; `None` when that is zero.
    pub max_error_x: Option<f64>,
    /// The `x` of the first instant, in the order of [`FunnelResult::errors`], whose error is
    /// above zero; `None` when there is none.
    pub first_error_x: Option<f64>,
    /// The error at each of the test's own instants, in the test's order.
    errors_at_test: Vec<f64>,
    /// The half-widths applied, over the reference points.
    half_width: HalfSize,
    /// The half-heights applied, over the reference points.
    half_height: HalfSize,
}

/// What [`FunnelResult::from_curves`] takes beside the curves: the error at each test instant
/// in the test's order, and the half-sizes applied.
pub(crate) struct Applied {
    /// One error per test instant, in the test's order.
    pub(crate) errors_at_test: Vec<f64>,
    /// The half-widths over the reference points.
    pub(crate) half_width: HalfSize,
    /// The half-heights over the reference points.
    pub(crate) half_height: HalfSize,
}

impl FunnelResult {
    /// Builds a result from its curves, reading the three figures off the error curve.
    pub(crate) fn from_curves(
        envelope: Envelope,
        reference: Series<'_>,
        test: Series<'_>,
        (lower_bound, upper_bound): (Curve, Curve),
        errors: Curve,
        applied: Applied,
    ) -> Self {
        let mut max_error = 0.0;
        let mut max_error_x = None;
        let mut first_error_x = None;
        for (x, error) in errors.points() {
            if error > 0.0 && first_error_x.is_none() {
                first_error_x = Some(x);
            }
            if error > max_error {
                max_error = error;
                max_error_x = Some(x);
            }
        }
        Self {
            envelope,
            reference: Curve::from_columns(reference.x.to_vec(), reference.y.to_vec()),
            test: Curve::from_columns(test.x.to_vec(), test.y.to_vec()),
            lower_bound,
            upper_bound,
            compared: errors.len(),
            errors,
            max_error,
            max_error_x,
            first_error_x,
            errors_at_test: applied.errors_at_test,
            half_width: applied.half_width,
            half_height: applied.half_height,
        }
    }

    /// The error at each of the test's own instants, one per test sample and in the test's
    /// order, equal times included. For [`Envelope::Funnel`] it is [`FunnelResult::errors`]'s
    /// `y`; for [`Envelope::AlignedGrid`] it leaves out the breakpoints between test instants,
    /// which [`FunnelResult::errors`] also lists, and keeps the order of a repeated instant's
    /// samples, which that list sorts by error. A verdict reads this (commissioning note,
    /// decision 32).
    #[must_use]
    pub fn errors_at_test(&self) -> &[f64] {
        &self.errors_at_test
    }

    /// The smallest and largest half-width applied over the reference points, in the unit of
    /// `x` and before `x` is normalised. Under [`Envelope::Funnel`], "Envelope" step 1's
    /// half-sizes with their fallbacks, so never zero; under [`Envelope::AlignedGrid`], the sum
    /// of the three terms, with no fallback, so zero when every tolerance of the axis is zero.
    #[must_use]
    pub fn half_width(&self) -> HalfSize {
        self.half_width
    }

    /// The smallest and largest half-height applied over the reference points, in the unit of
    /// `y`, read as [`FunnelResult::half_width`] is.
    #[must_use]
    pub fn half_height(&self) -> HalfSize {
        self.half_height
    }

    /// Whether the test lies inside the band at every compared instant: no error is above zero.
    #[must_use]
    pub fn within_envelope(&self) -> bool {
        self.first_error_x.is_none()
    }
}

#[cfg(test)]
#[path = "envelope_tests.rs"]
mod tests;
