//! Deterministic L1 tolerance-band comparison (`07` §3).
//!
//! The x tolerance is static horizontal rectangle widening only. No timestamp snapping,
//! resampling, sliding window, or dynamic-time-warping is performed.

/// L1 tolerance-band parameters. Defaults mirror CDL Listing 12.1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tolerances {
    /// Absolute tolerance on the x/time axis.
    pub atolx: f64,
    /// Absolute tolerance on the y/value axis.
    pub atoly: f64,
    /// Relative-to-reference-range tolerance on the x/time axis.
    pub rtolx: f64,
    /// Relative-to-reference-range tolerance on the y/value axis.
    pub rtoly: f64,
    /// Local tolerance on the x/time axis, scaled by `abs(x_i)`.
    pub ltolx: f64,
    /// Local tolerance on the y/value axis, scaled by `abs(y_i)`.
    pub ltoly: f64,
}

impl Default for Tolerances {
    fn default() -> Self {
        Self {
            atolx: 10.0,
            atoly: 0.0,
            rtolx: 0.002,
            rtoly: 0.002,
            ltolx: 0.0,
            ltoly: 0.0,
        }
    }
}

/// A non-decreasing-in-x time-series view.
#[derive(Clone, Copy, Debug)]
pub struct Series<'a> {
    /// The x/time samples.
    pub x: &'a [f64],
    /// The y/value samples.
    pub y: &'a [f64],
}

impl Series<'_> {
    /// Number of sample pairs in the series.
    #[must_use]
    pub fn len(&self) -> usize {
        self.x.len().min(self.y.len())
    }

    /// Whether this view contains no complete sample pairs.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn pairs(&self) -> impl Iterator<Item = (f64, f64)> + '_ {
        self.x.iter().copied().zip(self.y.iter().copied())
    }
}

/// One sampled point on a funnel curve.
pub type Point = (f64, f64);

/// A piecewise-linear sampled curve.
pub type Curve = Vec<Point>;

/// The five funnel outputs plus the pass/fail verdict.
#[derive(Clone, Debug, PartialEq)]
pub struct FunnelResult {
    /// True iff every checked point has zero positive error.
    pub passed: bool,
    /// Supremum of the pointwise positive error.
    pub max_error: f64,
    /// Earliest x where `error > 0`.
    pub first_failure_x: Option<f64>,
    /// Reference samples copied from the input view.
    pub reference: Curve,
    /// Test samples copied from the input view.
    pub test: Curve,
    /// Lower monotone tolerance-bound curve.
    pub lower_bound: Curve,
    /// Upper monotone tolerance-bound curve.
    pub upper_bound: Curve,
    /// Checked `(x, error)` samples.
    pub errors: Curve,
}

#[derive(Clone, Copy)]
struct Rect {
    x: f64,
    y: f64,
    hw: f64,
    hh: f64,
}

/// Build conservative monotone lower and upper bound curves for `reference`.
///
/// The curve grid contains every reference sample and every rectangle edge. At each grid point the
/// lower bound is the minimum of the clamped reference value and every covering rectangle's lower
/// edge; the upper bound is the corresponding maximum. Between adjacent grid points, rectangle
/// coverage is constant and the reference is linear, so the straight chord between these sampled
/// extrema still encloses the reference curve and all reference rectangles. This is deliberately
/// conservative: it prefers a slightly wider envelope over the leaky local-corner construction that
/// can reject a bit-identical trace when `atolx` is large relative to sample spacing.
#[must_use]
pub fn build_bounds(reference: Series<'_>, tol: &Tolerances) -> (Curve, Curve) {
    let Some(rects) = rectangles(reference, tol) else {
        return (Vec::new(), Vec::new());
    };
    if rects.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let reference_vec: Curve = reference.pairs().collect();
    let mut grid = Vec::with_capacity(rects.len() * 3);
    for r in &rects {
        grid.push(r.x - r.hw);
        grid.push(r.x);
        grid.push(r.x + r.hw);
    }
    grid.sort_by(f64::total_cmp);
    grid.dedup_by(|a, b| a.to_bits() == b.to_bits());

    let mut lower = Vec::new();
    let mut upper = Vec::new();
    for x in grid {
        let y = eval_curve(&reference_vec, x);
        let mut lo = y;
        let mut hi = y;
        for r in &rects {
            if ((r.x - r.hw)..=(r.x + r.hw)).contains(&x) {
                lo = lo.min(r.y - r.hh);
                hi = hi.max(r.y + r.hh);
            }
        }
        lower.push((x, lo));
        upper.push((x, hi));
    }
    (lower, upper)
}

/// Compare `test` against `reference` within `tol`.
#[must_use]
pub fn compare(reference: Series<'_>, test: Series<'_>, tol: &Tolerances) -> FunnelResult {
    let reference_vec: Vec<_> = reference.pairs().collect();
    let test_vec: Vec<_> = test.pairs().collect();
    if reference_vec.is_empty() && test_vec.is_empty() {
        return FunnelResult {
            passed: true,
            max_error: 0.0,
            first_failure_x: None,
            reference: reference_vec,
            test: test_vec,
            lower_bound: Vec::new(),
            upper_bound: Vec::new(),
            errors: vec![],
        };
    }
    if !reference_vec.is_empty() && test_vec.is_empty() {
        return FunnelResult {
            passed: false,
            max_error: f64::INFINITY,
            first_failure_x: reference_vec.first().map(|(x, _)| *x),
            reference: reference_vec,
            test: test_vec,
            lower_bound: Vec::new(),
            upper_bound: Vec::new(),
            errors: vec![],
        };
    }
    let (lower_bound, upper_bound) = build_bounds(reference, tol);
    if lower_bound.is_empty() || upper_bound.is_empty() || !valid_pairs(&test_vec) {
        let x = first_bad_x(&reference_vec).or_else(|| first_bad_x(&test_vec));
        return FunnelResult {
            passed: false,
            max_error: f64::INFINITY,
            first_failure_x: x,
            reference: reference_vec,
            test: test_vec,
            lower_bound,
            upper_bound,
            errors: vec![],
        };
    }

    let mut errors = Vec::new();
    let mut max_error = 0.0;
    let mut first_failure_x = None;

    for &(x, y) in &test_vec {
        record_error(
            x,
            y,
            &lower_bound,
            &upper_bound,
            &mut errors,
            &mut max_error,
            &mut first_failure_x,
        );
    }
    let mut breakpoints: Vec<f64> = lower_bound
        .iter()
        .chain(&upper_bound)
        .map(|(x, _)| *x)
        .filter(|x| !test_vec.iter().any(|(tx, _)| tx.to_bits() == x.to_bits()))
        .collect();
    breakpoints.sort_by(f64::total_cmp);
    breakpoints.dedup_by(|a, b| a.to_bits() == b.to_bits());
    for x in breakpoints {
        let y = eval_curve(&test_vec, x);
        record_error(
            x,
            y,
            &lower_bound,
            &upper_bound,
            &mut errors,
            &mut max_error,
            &mut first_failure_x,
        );
    }
    errors.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    FunnelResult {
        passed: max_error == 0.0,
        max_error,
        first_failure_x,
        reference: reference_vec,
        test: test_vec,
        lower_bound,
        upper_bound,
        errors,
    }
}

fn record_error(
    x: f64,
    y: f64,
    lower: &[(f64, f64)],
    upper: &[(f64, f64)],
    errors: &mut Vec<(f64, f64)>,
    max_error: &mut f64,
    first_failure_x: &mut Option<f64>,
) {
    let lo = eval_curve(lower, x);
    let hi = eval_curve(upper, x);
    if !y.is_finite() || !lo.is_finite() || !hi.is_finite() {
        *max_error = f64::INFINITY;
        if first_failure_x.is_none_or(|prev| x < prev) {
            *first_failure_x = Some(x);
        }
        errors.push((x, f64::INFINITY));
        return;
    }
    let err = (y - hi).max(0.0) - (y - lo).min(0.0);
    if err > *max_error {
        *max_error = err;
    }
    if err > 0.0 && first_failure_x.is_none_or(|prev| x < prev) {
        *first_failure_x = Some(x);
    }
    errors.push((x, err));
}

fn rectangles(reference: Series<'_>, tol: &Tolerances) -> Option<Vec<Rect>> {
    if ![
        tol.atolx, tol.atoly, tol.rtolx, tol.rtoly, tol.ltolx, tol.ltoly,
    ]
    .iter()
    .all(|v| v.is_finite() && *v >= 0.0)
    {
        return None;
    }
    let pairs: Vec<_> = reference.pairs().collect();
    if !valid_pairs(&pairs) {
        return None;
    }
    if pairs.is_empty() {
        return Some(Vec::new());
    }
    let (mut min_x, mut max_x) = (pairs[0].0, pairs[0].0);
    let (mut min_y, mut max_y) = (pairs[0].1, pairs[0].1);
    for &(x, y) in &pairs {
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
    }
    let (range_x, range_y) = (max_x - min_x, max_y - min_y);
    Some(
        pairs
            .into_iter()
            .map(|(x, y)| Rect {
                x,
                y,
                hw: tol.atolx + tol.rtolx * range_x + tol.ltolx * x.abs(),
                hh: tol.atoly + tol.rtoly * range_y + tol.ltoly * y.abs(),
            })
            .collect(),
    )
}

fn valid_pairs(pairs: &[(f64, f64)]) -> bool {
    pairs.windows(2).all(|w| w[0].0 <= w[1].0)
        && pairs.iter().all(|(x, y)| x.is_finite() && y.is_finite())
}

fn first_bad_x(pairs: &[(f64, f64)]) -> Option<f64> {
    pairs
        .iter()
        .find_map(|(x, y)| (!x.is_finite() || !y.is_finite()).then_some(*x))
}

fn eval_curve(curve: &[(f64, f64)], x: f64) -> f64 {
    if curve.is_empty() {
        return f64::NAN;
    }
    if x <= curve[0].0 {
        return curve[0].1;
    }
    for w in curve.windows(2) {
        let (x0, y0) = w[0];
        let (x1, y1) = w[1];
        if x == x1 {
            return y1;
        }
        if x <= x1 {
            if x1 == x0 {
                return y1;
            }
            let a = (x - x0) / (x1 - x0);
            return y0 + a * (y1 - y0);
        }
    }
    curve[curve.len() - 1].1
}
