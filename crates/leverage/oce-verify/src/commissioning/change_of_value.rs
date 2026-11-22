//! The change-of-value comparison: the instants and the step series of a reference that gives no
//! `sampling` (commissioning note, "Comparison" and "As landed: the change-of-value comparison";
//! issue #746).
//!
//! A trend logged on change, and the engine's output between two frames, each hold their last
//! value until the next sample. Both series are therefore **step functions** of time, and the
//! comparison treats them as such:
//!
//! - **The series instants** ([`series_instants`]) are the window's `start` and `stop` and every
//!   sample time, within the window, of the engine series, the trend series and each of the
//!   output's indicators, and every end of an uncovered stretch: each once, in order. Between
//!   two of them neither series, no indicator and no stretch changes.
//! - **A step series** ([`step_series`]) is a series held at those instants and written so that
//!   the funnel's linear interpolation draws a step, not a ramp: where the value changes, the
//!   held value is carried to the instant first, then the new value starts there.
//! - **The pieces.** For a Real output the instants also take every corner of the envelope's two
//!   bounds inside the window ([`with_corners`]), so between two instants the test is constant
//!   and each bound is a straight line. An instant stands for the stretch from it to the next
//!   one, and its exceedance is the largest error over that stretch ([`piece_exceedances`]):
//!   taken at its two ends, the bounds read from inside the stretch at each, because the error
//!   of a constant against two straight lines is largest at an end. The last instant, `stop`,
//!   stands for itself.
//!
//! Every list here is walked once, or merged once with another, so the work is linear in the
//! samples; each function that walks lists returns the steps it took, which the cost tests pin.

use crate::envelope::Curve;

/// The instants of one output: `start`, `stop`, and every time of `lists` within
/// `[start, stop]`, each value once, ascending; with the merge steps taken. Each list is
/// ascending (non-decreasing); `start <= stop`.
///
/// One two-way merge per list, so the steps are at most the sum, over the lists, of the
/// instants so far and the list's length.
pub(crate) fn series_instants(start: f64, stop: f64, lists: &[&[f64]]) -> (Vec<f64>, u64) {
    let mut instants = if start < stop {
        vec![start, stop]
    } else {
        vec![start]
    };
    let mut steps: u64 = 0;
    for list in lists {
        let from = list.partition_point(|t| *t < start);
        let to = list.partition_point(|t| *t <= stop);
        let within = &list[from..to];
        if within.is_empty() {
            continue;
        }
        let mut merged = Vec::with_capacity(instants.len() + within.len());
        let (mut a, mut b) = (0, 0);
        while a < instants.len() || b < within.len() {
            steps += 1;
            let next = if b == within.len() || (a < instants.len() && instants[a] <= within[b]) {
                a += 1;
                instants[a - 1]
            } else {
                b += 1;
                within[b - 1]
            };
            if merged.last() != Some(&next) {
                merged.push(next);
            }
        }
        instants = merged;
    }
    (instants, steps)
}

/// The step series of `values` held at `instants` (one value per instant): each instant once,
/// and where the value differs from the one before, first a point carrying the value before to
/// that instant. So a change from `a` at `t0` to `b` at `t1` is the three points `(t0, a)`,
/// `(t1, a)`, `(t1, b)`, which the funnel reads as a level and a vertical step.
pub(crate) fn step_series(instants: &[f64], values: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let mut x = Vec::with_capacity(instants.len() * 2);
    let mut y = Vec::with_capacity(instants.len() * 2);
    for (k, (t, value)) in instants.iter().zip(values).enumerate() {
        if k > 0 && *value != values[k - 1] {
            x.push(*t);
            y.push(values[k - 1]);
        }
        x.push(*t);
        y.push(*value);
    }
    (x, y)
}

/// `instants` with every corner `x` of `lower` and `upper` strictly inside `(start, stop)` added,
/// each value once and ascending, and for each the index of the last of `instants` at or before
/// it, which it inherits its fate and its test value from.
pub(crate) fn with_corners(
    instants: &[f64],
    lower: &Curve,
    upper: &Curve,
    start: f64,
    stop: f64,
) -> (Vec<f64>, Vec<usize>) {
    let mut corners: Vec<f64> = lower
        .x()
        .iter()
        .chain(upper.x())
        .copied()
        .filter(|x| start < *x && *x < stop)
        .collect();
    corners.sort_by(f64::total_cmp);
    let mut merged = Vec::with_capacity(instants.len() + corners.len());
    let mut origin = Vec::with_capacity(instants.len() + corners.len());
    let mut next_corner = 0;
    for (index, t) in instants.iter().enumerate() {
        // Corners before `t` follow the instant before it, which `start` always is.
        while next_corner < corners.len() && corners[next_corner] < *t {
            let corner = corners[next_corner];
            if merged.last() != Some(&corner) {
                merged.push(corner);
                origin.push(index - 1);
            }
            next_corner += 1;
        }
        while next_corner < corners.len() && corners[next_corner] == *t {
            next_corner += 1;
        }
        merged.push(*t);
        origin.push(index);
    }
    (merged, origin)
}

/// A bound's value at `t` read from the right of `t` (`from_right`) or from its left: the last
/// or the first corner at `t` when one is there, else the bound's straight line through `t` in
/// funnel's operation order; held before its first corner and after its last.
fn bound_value(bound: &Curve, t: f64, from_right: bool) -> f64 {
    let (x, y) = (bound.x(), bound.y());
    let last = x.len() - 1;
    if from_right {
        let after = x.partition_point(|corner| *corner <= t);
        if after == 0 {
            return y[0];
        }
        if x[after - 1] == t || after > last {
            return y[after - 1];
        }
        interpolate(bound, after - 1, t)
    } else {
        let at = x.partition_point(|corner| *corner < t);
        if at > last {
            return y[last];
        }
        if x[at] == t || at == 0 {
            return y[at];
        }
        interpolate(bound, at - 1, t)
    }
}

/// The bound's straight line from corner `from` to the next, at `t` between them.
fn interpolate(bound: &Curve, from: usize, t: f64) -> f64 {
    let (x0, y0) = (bound.x()[from], bound.y()[from]);
    let (x1, y1) = (bound.x()[from + 1], bound.y()[from + 1]);
    y0 + (((y1 - y0) / (x1 - x0)) * (t - x0))
}

/// How far `y` lies outside `[lower, upper]`: funnel's error, in its branches.
fn error(y: f64, lower: f64, upper: f64) -> f64 {
    if y < lower {
        lower - y
    } else if y > upper {
        y - upper
    } else {
        0.0
    }
}

/// The exceedance each of `instants` (ascending, distinct, every bound corner inside the window
/// among them) stands for, `test` the value held from each: the larger of the error at the
/// instant, the bounds read from its right, and the error at the next instant, the bounds read
/// from its left. The last instant's is the error there, the bounds read from its left. Both
/// bounds have at least one corner.
pub(crate) fn piece_exceedances(
    instants: &[f64],
    test: &[f64],
    lower: &Curve,
    upper: &Curve,
) -> Vec<f64> {
    let mut exceedances = Vec::with_capacity(instants.len());
    for (k, (t, y)) in instants.iter().zip(test).enumerate() {
        let exceedance = match instants.get(k + 1) {
            Some(next) => {
                let at_start = error(
                    *y,
                    bound_value(lower, *t, true),
                    bound_value(upper, *t, true),
                );
                let at_end = error(
                    *y,
                    bound_value(lower, *next, false),
                    bound_value(upper, *next, false),
                );
                if at_end > at_start { at_end } else { at_start }
            }
            None => error(
                *y,
                bound_value(lower, *t, false),
                bound_value(upper, *t, false),
            ),
        };
        exceedances.push(exceedance);
    }
    exceedances
}

/// The number of `times` (ascending) within `[start, stop]`.
pub(crate) fn within(times: &[f64], start: f64, stop: f64) -> u64 {
    let count = times.partition_point(|t| *t <= stop) - times.partition_point(|t| *t < start);
    u64::try_from(count).unwrap_or(u64::MAX)
}

#[cfg(test)]
#[path = "change_of_value_tests.rs"]
mod tests;
