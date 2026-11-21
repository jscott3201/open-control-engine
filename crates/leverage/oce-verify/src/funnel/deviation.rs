//! The error of the test against the two bounds: funnel's `tube.c` (`interpolateValues`,
//! `compare`).

use super::corners::Corner;
use super::equality::equ;

/// A bound's `y` at each of `target_x`, by funnel's `interpolateValues`.
///
/// `bound` has at least two corners and `target_x` is non-decreasing. One cursor walks the
/// bound's segments as the targets advance; each value is linear on the segment the cursor is
/// on, in funnel's operation order, and the corner's own `y` where the segment is vertical or
/// the target sits on its start (the C's guard against a division by zero, decided by [`equ`]).
/// From the first target beyond the bound's last corner on, every value is the last corner's
/// `y`: the bound is held, never extrapolated. A target before the first corner is extrapolated
/// along the first segment, as in the C; the funnel never asks for one, because both series
/// start at the same `x` and the bound starts a half-width before it.
pub(super) fn bound_at(bound: &[Corner], target_x: &[f64]) -> Vec<f64> {
    let last = bound[bound.len() - 1];
    let mut values = Vec::with_capacity(target_x.len());
    let mut segment_end = 1;
    for x in target_x {
        if *x > last.x {
            values.resize(target_x.len(), last.y);
            break;
        }
        while bound[segment_end].x < *x && segment_end + 1 < bound.len() {
            segment_end += 1;
        }
        let Corner { x: x0, y: y0 } = bound[segment_end - 1];
        let Corner { x: x1, y: y1 } = bound[segment_end];
        values.push(if !equ((x1 - x0) * (x - x0), 0.0) {
            y0 + (((y1 - y0) / (x1 - x0)) * (x - x0))
        } else {
            y0
        });
    }
    values
}

/// The error at each test instant: how far `test_y` lies below `lower` or above `upper` there,
/// zero inside the band (the note's `max(0, y - y_up) - min(0, y - y_low)`, in the branches
/// funnel's `compare` takes). The three slices have one length. Never negative.
pub(super) fn errors(lower: &[f64], upper: &[f64], test_y: &[f64]) -> Vec<f64> {
    test_y
        .iter()
        .zip(lower.iter().zip(upper))
        .map(|(y, (lower, upper))| {
            if y < lower {
                lower - y
            } else if y > upper {
                y - upper
            } else {
                0.0
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "deviation_tests.rs"]
mod tests;
