//! Corner selection: the outline of one bound before loop removal, funnel's `getLower` and
//! `getUpper` (`algorithmRectangle.c`), part 1.
//!
//! Every reference point carries a rectangle. The lower bound runs along bottom corners and the
//! upper bound along top corners, and at each point the signs of the reference's slope before
//! and after it choose which corners join the outline. The two C functions are mirror images;
//! here one function takes the [`Side`], and each side's branch table is written out in the
//! order the C tests its conditions.

use super::equality::equ;

/// Which bound of the tube.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Side {
    /// The lower bound, along the bottom of the rectangles (`curInd == -1` in funnel).
    Lower,
    /// The upper bound, along the top of the rectangles (`curInd == 1` in funnel).
    Upper,
}

/// One point of a bound's outline. `x` is normalised while the outline is being built.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Corner {
    /// The corner's `x`.
    pub(super) x: f64,
    /// The corner's `y`.
    pub(super) y: f64,
}

/// A rectangle's left or right corner on the side being built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edge {
    Left,
    Right,
}

/// The slope sentinel for a vertical step: `x` does not change, so the slope is taken as
/// `1e15` going up and `-1e15` otherwise.
const VERTICAL_SLOPE: f64 = 1e15;

/// C's `sign` macro: 1, -1 or 0.
fn sign(value: f64) -> i8 {
    if value > 0.0 {
        1
    } else if value < 0.0 {
        -1
    } else {
        0
    }
}

/// The sign and the slope of the reference from point `from` to the next point.
fn slope(x: &[f64], y: &[f64], from: usize) -> (i8, f64) {
    let rise = y[from + 1] - y[from];
    let direction = sign(rise);
    let slope = if !equ(x[from + 1], x[from]) {
        rise / (x[from + 1] - x[from])
    } else if direction > 0 {
        VERTICAL_SLOPE
    } else {
        -VERTICAL_SLOPE
    };
    (direction, slope)
}

/// The corners an interior point adds, given the slope signs before (`before`) and after
/// (`after`) it. The conditions are tested in funnel's order; its fourth, the other extremum, is
/// the only case the first three leave, since a sign is -1, 0 or 1.
fn interior_corners(side: Side, before: i8, after: i8) -> &'static [Edge] {
    use Edge::{Left, Right};
    let rising_or_flat = before != -1 && after != -1;
    let falling_or_flat = before != 1 && after != 1;
    let valley = before == -1 && after == 1;
    let peak = before == 1 && after == -1;
    match side {
        Side::Lower => {
            if rising_or_flat {
                &[Right]
            } else if falling_or_flat {
                &[Left]
            } else if valley {
                &[Left, Right]
            } else {
                debug_assert!(peak);
                &[Right, Left]
            }
        }
        Side::Upper => {
            if rising_or_flat {
                &[Left]
            } else if falling_or_flat {
                &[Right]
            } else if peak {
                &[Left, Right]
            } else {
                debug_assert!(valley);
                &[Right, Left]
            }
        }
    }
}

/// The outline of one bound of the tube around a reference, before loop removal.
///
/// `x` is the reference's `x`, normalised; `half_x` the half-widths on the same scale; `y` and
/// `half_y` the values and the half-heights. All four have one length, at least one. The
/// outline has at least two corners and its `x` may run backwards where a rectangle is wider
/// than the gap to its neighbour: those stretches are the loops the next step removes.
///
/// The steps are funnel's: leading points equal to the next one are skipped; the outline starts
/// at the first rectangle's left corner (and its right corner when the reference then moves
/// away from this side: up for the lower bound, down for the upper); each later interior point
/// whose slope differs from the one before it adds the corners [`interior_corners`] names, after
/// which corners that would continue a flat run are dropped again; and it ends with the last
/// rectangle's right corner, preceded by its left corner when the last slope moves towards this
/// side.
pub(super) fn outline(
    side: Side,
    x: &[f64],
    y: &[f64],
    half_x: &[f64],
    half_y: &[f64],
) -> Vec<Corner> {
    let count = x.len();
    let level = |index: usize| match side {
        Side::Lower => y[index] - half_y[index],
        Side::Upper => y[index] + half_y[index],
    };
    let corner = |edge: Edge, index: usize| Corner {
        x: match edge {
            Edge::Left => x[index] - half_x[index],
            Edge::Right => x[index] + half_x[index],
        },
        y: level(index),
    };
    // The slope sign that moves the reference towards this side.
    let towards = match side {
        Side::Lower => -1,
        Side::Upper => 1,
    };

    let mut corners = Vec::with_capacity(count + 2);
    let mut first = 0;
    while first + 1 < count && equ(x[first], x[first + 1]) && equ(y[first], y[first + 1]) {
        first += 1;
    }
    corners.push(corner(Edge::Left, first));

    if first + 1 < count {
        let (mut before, mut slope_before) = slope(x, y, first);
        if before == -towards {
            corners.push(corner(Edge::Right, first));
        }

        for index in first + 1..count - 1 {
            if equ(x[index], x[index + 1]) && equ(y[index], y[index + 1]) {
                continue;
            }
            let (after, slope_after) = slope(x, y, index);
            if !equ(slope_before, slope_after) {
                for edge in interior_corners(side, before, after) {
                    corners.push(corner(*edge, index));
                }
                drop_flat_run(&mut corners, level(index + 1), before * after == -1);
            }
            before = after;
            slope_before = slope_after;
        }

        if before == towards {
            corners.push(corner(Edge::Left, count - 1));
        }
    }
    corners.push(corner(Edge::Right, count - 1));
    corners
}

/// Drops the corners just added when the outline would only continue a flat run: the next
/// reference point's level equals the last corner's, and so does the corner before the ones
/// added. Two corners go at an extremum (`extremum`), one otherwise.
fn drop_flat_run(corners: &mut Vec<Corner>, next_level: f64, extremum: bool) {
    let len = corners.len();
    let last = corners[len - 1].y;
    if !equ(next_level, last) {
        return;
    }
    if extremum && len >= 3 && equ(corners[len - 3].y, last) {
        corners.truncate(len - 2);
    } else if !extremum && len >= 2 && equ(corners[len - 2].y, last) {
        corners.truncate(len - 1);
    }
}

#[cfg(test)]
#[path = "corners_tests.rs"]
mod tests;
