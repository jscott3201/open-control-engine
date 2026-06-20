//! Indicator and don't-care masking for conformance comparison.
//!
//! CDL verification permits a boolean indicator signal to suspend checking for a variable. Indicator
//! samples use step-hold semantics: the last value at or before `t` is active until a later sample
//! changes it. Masked funnel comparison is segmented at every don't-care boundary; it never builds a
//! tolerance band across an inactive gap.

use crate::{
    FunnelResult, Series, Tolerances, compare,
    funnel::{Curve, eval_curve},
};

/// A boolean signal used to gate checking of one or more output variables.
#[derive(Clone, Debug, PartialEq)]
pub struct Indicator {
    /// Captured indicator signal name, such as `fanSta.y`.
    pub signal: String,
    /// Step-held `(time, value)` samples. The value holds until the next sample.
    pub samples: Vec<(f64, bool)>,
}

impl Indicator {
    /// Return the step-held value at `t`.
    ///
    /// Before the first sample, or for an empty sample list, the indicator is inactive.
    #[must_use]
    pub fn active_at(&self, t: f64) -> bool {
        let mut active = false;
        for &(sample_t, value) in &self.samples {
            if sample_t > t {
                break;
            }
            active = value;
        }
        active
    }
}

/// A per-variable mask made of one or more ANDed indicators.
#[derive(Clone, Debug, PartialEq)]
pub struct Mask {
    /// Indicators that must all be active for the variable to be compared.
    pub indicators: Vec<Indicator>,
}

impl Mask {
    /// Return true when all indicators are active at `t`.
    ///
    /// A mask with no indicators is a no-op and is active at every timestamp.
    #[must_use]
    pub fn active_at(&self, t: f64) -> bool {
        self.indicators
            .iter()
            .all(|indicator| indicator.active_at(t))
    }

    /// Copy the active subset of `series` into caller-owned scratch storage.
    ///
    /// The same mask should be applied to both the reference and test series before funnel
    /// comparison so don't-care intervals cannot contribute to the error.
    pub fn apply<'a>(
        &self,
        series: Series<'a>,
        scratch_x: &'a mut Vec<f64>,
        scratch_y: &'a mut Vec<f64>,
    ) -> Series<'a> {
        scratch_x.clear();
        scratch_y.clear();
        for (x, y) in series.x.iter().copied().zip(series.y.iter().copied()) {
            if self.active_at(x) {
                scratch_x.push(x);
                scratch_y.push(y);
            }
        }
        Series {
            x: scratch_x,
            y: scratch_y,
        }
    }
}

/// Compare two series after applying the same don't-care mask to both sides.
///
/// Unlike [`Mask::apply`], this does not simply drop inactive samples and compare the survivors.
/// It builds one funnel per contiguous active segment, clipping the reference curve to each segment
/// boundary so no tolerance-band chord is fabricated across a don't-care interval.
#[must_use]
pub fn compare_masked(
    reference: Series<'_>,
    test: Series<'_>,
    tol: &Tolerances,
    mask: &Mask,
) -> FunnelResult {
    if mask.indicators.is_empty() {
        return compare(reference, test, tol);
    }

    let reference_vec: Curve = reference
        .x
        .iter()
        .copied()
        .zip(reference.y.iter().copied())
        .collect();
    let test_vec: Curve = test.x.iter().copied().zip(test.y.iter().copied()).collect();
    let Some((domain_start, domain_end)) = reference_domain(&reference_vec) else {
        return compare(reference, test, tol);
    };

    let segments = active_segments(mask, domain_start, domain_end);
    let mut combined = FunnelResult {
        passed: true,
        max_error: 0.0,
        compared_points: 0,
        first_failure_x: None,
        reference: Vec::new(),
        test: Vec::new(),
        lower_bound: Vec::new(),
        upper_bound: Vec::new(),
        errors: Vec::new(),
    };

    for segment in segments {
        let segment_ref = reference_segment(&reference_vec, segment);
        if segment_ref.is_empty() {
            continue;
        }
        let segment_test = test_segment(&test_vec, segment);
        if segment_test.is_empty() {
            continue;
        }
        let ref_x: Vec<_> = segment_ref.iter().map(|(x, _)| *x).collect();
        let ref_y: Vec<_> = segment_ref.iter().map(|(_, y)| *y).collect();
        let test_x: Vec<_> = segment_test.iter().map(|(x, _)| *x).collect();
        let test_y: Vec<_> = segment_test.iter().map(|(_, y)| *y).collect();
        let result = compare(
            Series {
                x: &ref_x,
                y: &ref_y,
            },
            Series {
                x: &test_x,
                y: &test_y,
            },
            tol,
        );
        merge_segment_result(&mut combined, result);
    }

    sort_combined_curves(&mut combined);
    combined.passed = combined.max_error == 0.0;
    combined
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Segment {
    start: f64,
    end: f64,
}

fn reference_domain(reference: &[(f64, f64)]) -> Option<(f64, f64)> {
    let start = reference.first()?.0;
    let end = reference.last()?.0;
    (start.is_finite() && end.is_finite() && start <= end).then_some((start, end))
}

fn active_segments(mask: &Mask, start: f64, end: f64) -> Vec<Segment> {
    if start == end {
        return mask
            .active_at(start)
            .then_some(Segment { start, end })
            .into_iter()
            .collect();
    }

    let mut cuts = vec![start, end];
    for indicator in &mask.indicators {
        for &(t, _) in &indicator.samples {
            if t.is_finite() && start < t && t < end {
                cuts.push(t);
            }
        }
    }
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|a, b| a.to_bits() == b.to_bits());

    let mut segments = Vec::new();
    for window in cuts.windows(2) {
        let (a, b) = (window[0], window[1]);
        if a < b && mask.active_at(a) {
            segments.push(Segment { start: a, end: b });
        }
    }
    segments
}

fn reference_segment(reference: &[(f64, f64)], segment: Segment) -> Curve {
    let mut out = Vec::new();
    push_point(
        &mut out,
        (segment.start, eval_curve(reference, segment.start)),
    );
    for &(x, y) in reference {
        if segment.start < x && x < segment.end {
            out.push((x, y));
        }
    }
    push_point(&mut out, (segment.end, eval_curve(reference, segment.end)));
    out.retain(|(x, y)| x.is_finite() && y.is_finite());
    out.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    out.dedup_by(|a, b| a.0.to_bits() == b.0.to_bits() && a.1.to_bits() == b.1.to_bits());
    out
}

fn test_segment(test: &[(f64, f64)], segment: Segment) -> Curve {
    test.iter()
        .copied()
        .filter(|(x, _)| segment.start <= *x && *x <= segment.end)
        .collect()
}

fn push_point(out: &mut Curve, point: (f64, f64)) {
    if !out.iter().any(|existing| {
        existing.0.to_bits() == point.0.to_bits() && existing.1.to_bits() == point.1.to_bits()
    }) {
        out.push(point);
    }
}

fn merge_segment_result(combined: &mut FunnelResult, result: FunnelResult) {
    combined.max_error = combined.max_error.max(result.max_error);
    combined.compared_points += result.compared_points;
    if let Some(x) = result.first_failure_x
        && combined.first_failure_x.is_none_or(|prev| x < prev)
    {
        combined.first_failure_x = Some(x);
    }
    combined.reference.extend(result.reference);
    combined.test.extend(result.test);
    combined.lower_bound.extend(result.lower_bound);
    combined.upper_bound.extend(result.upper_bound);
    combined.errors.extend(result.errors);
}

fn sort_combined_curves(result: &mut FunnelResult) {
    for curve in [
        &mut result.reference,
        &mut result.test,
        &mut result.lower_bound,
        &mut result.upper_bound,
        &mut result.errors,
    ] {
        curve.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    }
}
