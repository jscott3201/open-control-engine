//! The funnel envelope: a port of LBNL funnel's tube construction (commissioning note,
//! "Envelope", decisions 1, 12, 13 and 22).
//!
//! OBC §12 verifies a controller's trended output against the control sequence's with
//! [funnel](https://github.com/lbl-srg/funnel), so a verdict here has to be funnel's verdict.
//! This module is its C library's arithmetic, operation for operation, at commit
//! `a12fa96917ca73b78fcb6a17a1140b3a27a61e31` (`pyfunnel` 2.1.1, [`FUNNEL_VERSION`]), in five
//! steps, one per file:
//!
//! 1. [`tube_size`]: a rectangle around each reference point (`tubeSize.c`), its half-size on
//!    each axis the maximum of the absolute, range-relative and local tolerance terms, with
//!    funnel's fallbacks for a half-size of zero; and `x` normalised by its magnitude.
//! 2. [`corners`]: the outline of each bound, picked from the rectangles' corners by the
//!    reference's slope signs (`getLower`, `getUpper`).
//! 3. [`loop_removal`]: the stretches where an outline runs backwards cut at the intersection of
//!    the segments around them (`removeLoop`), under a work counter.
//! 4. [`deviation`]: both bounds interpolated at the test's instants and the error there
//!    (`tube.c`).
//! 5. [`equality`]: the one equality every step uses, funnel's `equ`.
//!
//! The port is proven by a differential test against the pinned `pyfunnel` on funnel's own test
//! set and a hostile set (`tests/funnel_differential.rs`). It differs from the C only where the
//! C defines no result or would hide one: input it reads without checking is refused with a
//! typed [`CompareError`] (a non-finite sample, a decreasing `x`), and so are the inputs on
//! which the C reads outside its arrays or its arithmetic overflows.
//!
//! This is not the envelope of `oce-conformance`, which this crate never depends on: that one
//! sums the tolerance terms and builds a grid; it is copied as [`crate::aligned_grid()`].
//!
//! # Origin and licence
//!
//! The files of this module (`mod.rs`, `tube_size.rs`, `corners.rs`, `loop_removal.rs`,
//! `deviation.rs`, `equality.rs`, and the transliteration of `removeLoop` in
//! `loop_removal_tests.rs`) are a port to Rust of funnel's C sources `tubeSize.c`,
//! `algorithmRectangle.c`, `tube.c`, `compare.c` and `data_structure.h`, from
//! <https://github.com/lbl-srg/funnel> at commit `a12fa96917ca73b78fcb6a17a1140b3a27a61e31`.
//! They are a derived work of that software:
//!
//! > "pyfunnel" Copyright (c) 2020, The Regents of the University of California, through
//! > Lawrence Berkeley National Laboratory (subject to receipt of any required approvals from
//! > the U.S. Dept. of Energy). All rights reserved.
//!
//! funnel is distributed under a 3-clause BSD licence with Lawrence Berkeley National
//! Laboratory's added clause on enhancements. Its licence and copyright notice are reproduced
//! verbatim in `LICENSE-funnel` at this crate's root, which travels with this module; the rest
//! of the crate is under the repository's own licence.

mod corners;
mod deviation;
mod equality;
mod loop_removal;
mod tube_size;

use self::corners::{Corner, Side};
use self::equality::equ;
use self::loop_removal::Work;
use self::tube_size::{Extent, HalfSizes};
use crate::compare_error::{CompareError, SeriesEnd};
use crate::compare_limits::CompareLimits;
use crate::envelope::{
    Applied, Curve, Envelope, FunnelResult, HalfSize, Series, Tolerances, check_inputs,
};

/// The funnel this crate's [`funnel()`] reproduces: `lbl-srg/funnel` at commit `a12fa96`, the
/// source of `pyfunnel` 2.1.1. A commissioning receipt carries it as
/// `comparison.envelope_version`.
pub const FUNNEL_VERSION: &str = "funnel@a12fa96";

/// Compare `test` against the funnel built around `reference`: LBNL funnel's comparison
/// ([`FUNNEL_VERSION`]).
///
/// Each reference point gets a rectangle whose half-width is
/// `max(atolx, rtolx × range_x, ltolx × |x_i|)` and whose half-height is the same in `y`; a
/// half-size of zero becomes `rtol × magnitude` when `rtol` is not zero, then at least `1e-10`.
/// The band's two bounds run along the rectangles' corners. The error at each of the test's
/// instants is how far its value lies outside the band there, both bounds interpolated linearly
/// and held beyond their last corner. `x` is the independent variable (seconds for a trend) and
/// the errors are in the unit of `y`.
///
/// The result is a pure function of the four arguments: the same bits on every target and in
/// every build, and nothing but memory is touched. Its cost is linear in the two series but for
/// loop removal, which is counted against [`CompareLimits::max_loop_removal_work`]. It never
/// panics.
///
/// # Errors
///
/// In this order: [`CompareError::InvalidTolerance`]; for the reference and then the test,
/// [`CompareError::LengthMismatch`], [`CompareError::EmptySeries`] and
/// [`CompareError::TooManyPoints`] (over [`CompareLimits::max_funnel_points`]); for the
/// reference and then the test, [`CompareError::NonFiniteSample`] and
/// [`CompareError::XNotNonDecreasing`]; [`CompareError::MismatchedEndpoints`] when the two do
/// not start, or do not end, at the same `x` (funnel's precondition, by its own equality);
/// then, from the construction, [`CompareError::WorkLimitExceeded`],
/// [`CompareError::LoopRemovalUndefined`] and [`CompareError::NonFiniteEnvelope`].
///
/// ```
/// use oce_verify::{CompareLimits, Series, Tolerances, funnel};
///
/// let time = [0.0, 1.0, 2.0, 3.0];
/// let reference = Series::new(&time, &[0.0, 1.0, 1.0, 0.0]);
/// let test = Series::new(&time, &[0.0, 1.05, 0.95, 0.5]);
/// let tolerances = Tolerances { atolx: 0.1, atoly: 0.1, ..Tolerances::ZERO };
/// let result = funnel(reference, test, &tolerances, &CompareLimits::default())?;
///
/// assert_eq!(result.compared, 4);
/// assert!(!result.within_envelope());
/// assert_eq!(result.first_error_x, Some(3.0));
/// assert_eq!(result.max_error_x, Some(3.0));
/// # Ok::<(), oce_verify::CompareError>(())
/// ```
pub fn funnel(
    reference: Series<'_>,
    test: Series<'_>,
    tolerances: &Tolerances,
    limits: &CompareLimits,
) -> Result<FunnelResult, CompareError> {
    let limits = limits.effective();
    check_inputs(reference, test, tolerances, limits.max_funnel_points)?;
    for (end, reference_x, test_x) in [
        (SeriesEnd::First, reference.x[0], test.x[0]),
        (
            SeriesEnd::Last,
            reference.x[reference.x.len() - 1],
            test.x[test.x.len() - 1],
        ),
    ] {
        if !equ(reference_x, test_x) {
            return Err(CompareError::MismatchedEndpoints {
                end,
                reference: reference_x,
                test: test_x,
            });
        }
    }

    let extent = Extent::of(reference);
    let half = tube_size::half_sizes(reference, &extent, tolerances);
    let mut work = Work::new(limits.max_loop_removal_work);
    let lower = bound(Side::Lower, reference, &extent, &half, &mut work)?;
    let upper = bound(Side::Upper, reference, &extent, &half, &mut work)?;

    let lower_at_test = deviation::bound_at(&lower, test.x);
    let upper_at_test = deviation::bound_at(&upper, test.x);
    let errors = deviation::errors(&lower_at_test, &upper_at_test, test.y);
    // A NaN bound compares as "inside the band", so nothing non-finite may reach a result.
    if !(all_finite(&lower_at_test) && all_finite(&upper_at_test) && all_finite(&errors)) {
        return Err(CompareError::NonFiniteEnvelope);
    }

    let applied = Applied {
        errors_at_test: errors.clone(),
        half_width: HalfSize::of(half.x.iter().copied()),
        half_height: HalfSize::of(half.y.iter().copied()),
    };
    Ok(FunnelResult::from_curves(
        Envelope::Funnel,
        reference,
        test,
        (curve(&lower), curve(&upper)),
        Curve::from_columns(test.x.to_vec(), errors),
        applied,
    ))
}

/// One bound of the tube, in the reference's own `x`: the outline, its loops removed, `x`
/// denormalised.
fn bound(
    side: Side,
    reference: Series<'_>,
    extent: &Extent,
    half: &HalfSizes,
    work: &mut Work,
) -> Result<Vec<Corner>, CompareError> {
    let x = extent.normalised(reference.x);
    let half_x = extent.normalised(&half.x);
    let outline = corners::outline(side, &x, reference.y, &half_x, &half.y);
    let mut corners = loop_removal::remove_loops(outline, side, work)?;
    for corner in &mut corners {
        corner.x = extent.denormalised(corner.x);
    }
    if corners.len() < 2 {
        // Defensive: the C's interpolation reads the second corner unconditionally, and a
        // loop removal leaves at least the corner before the loop and the one after it. No
        // input is known to bring a bound below two corners.
        return Err(CompareError::LoopRemovalUndefined);
    }
    if !corners
        .iter()
        .all(|corner| corner.x.is_finite() && corner.y.is_finite())
    {
        return Err(CompareError::NonFiniteEnvelope);
    }
    Ok(corners)
}

fn all_finite(values: &[f64]) -> bool {
    values.iter().all(|value| value.is_finite())
}

fn curve(corners: &[Corner]) -> Curve {
    Curve::from_points(corners.iter().map(|corner| (corner.x, corner.y)))
}
