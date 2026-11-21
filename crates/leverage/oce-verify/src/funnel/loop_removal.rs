//! Loop removal: funnel's `removeLoop` (`algorithmRectangle.c`), under a work counter.
//!
//! Where a rectangle is wider than the gap to its neighbour, the outline from
//! [`super::corners::outline`] runs backwards in `x` and crosses itself. For each backward
//! segment `(j, j + 1)` the C finds the segment `(i - 1, i)` before it and the segment
//! `(k - 1, k)` after it that intersect, deletes the corners `i` to `k - 1`, puts the
//! intersection in their place and resumes from there.
//!
//! The arithmetic and every comparison are funnel's, in its order. Three things differ, none of
//! them in a result funnel defines:
//!
//! - **The corner list is a gap buffer** ([`CornerList`]). The C copies both arrays whole for
//!   every loop it removes, which is quadratic in the reference's points. Edits here happen at
//!   a gap that follows the scan, so the result is the same list and the cost is what the
//!   searches cost.
//! - **Every search step and every corner moved is counted** ([`Work`]) and the comparison is
//!   refused when the count passes the caller's bound: a hostile outline can make the searches
//!   quadratic or keep the scan from advancing. No clock is read.
//! - **Where the C reads outside its arrays the comparison is refused** with
//!   [`CompareError::LoopRemovalUndefined`]. There is one such place: the backward search for
//!   `i` running past the first corner (the C reads `X[-1]`), which a local `x` tolerance above
//!   one reaches.
//!
//! Two more checks here are defensive, and no input reaches them. The forward search for `i`
//! cannot leave the list: it starts at or before corner `k - 1`; on a segment `(k - 1, k)` that
//! is not vertical the line's value at corner `k - 1` is exactly that corner's `y` (the slope
//! times zero), so the search stops there at the latest, and on a vertical one it stops at `k`.
//! So `i` never passes `k`, the check `i >= len` never fires, and the range deleted is never
//! negative: the branch of [`CornerList::keep_before_and_from`] that repeats corners, which is
//! what the C's `removeRange` does with a negative count, is never taken. Both are kept so that
//! a mistake in that reasoning is a typed refusal or funnel's own edit, never a panic.

use super::corners::{Corner, Side};
use super::equality::equ;
use crate::compare_error::CompareError;

/// The work a comparison may still spend on loop removal, in units: one per corner a search step
/// examines, moves or drops.
#[derive(Debug)]
pub(super) struct Work {
    spent: u64,
    limit: u64,
}

impl Work {
    /// A counter that refuses the unit after `limit`.
    pub(super) fn new(limit: u64) -> Self {
        Self { spent: 0, limit }
    }

    /// Spends `units`, or refuses when that would pass the limit.
    fn spend(&mut self, units: u64) -> Result<(), CompareError> {
        self.spent = self.spent.saturating_add(units);
        if self.spent > self.limit {
            Err(CompareError::WorkLimitExceeded { limit: self.limit })
        } else {
            Ok(())
        }
    }

    /// The units spent so far.
    #[cfg(test)]
    pub(super) fn spent(&self) -> u64 {
        self.spent
    }
}

/// A list of corners with a gap at the place of the last edit: the corners before the gap in
/// order, the corners after it reversed, so an edit at the gap moves nothing and moving the gap
/// costs its distance.
struct CornerList {
    before: Vec<Corner>,
    after_reversed: Vec<Corner>,
}

impl CornerList {
    /// The list `corners`, its gap at the start.
    fn new(mut corners: Vec<Corner>) -> Self {
        corners.reverse();
        Self {
            before: Vec::new(),
            after_reversed: corners,
        }
    }

    fn len(&self) -> usize {
        self.before.len() + self.after_reversed.len()
    }

    /// The corner at `index`, which is below [`CornerList::len`].
    fn at(&self, index: usize) -> Corner {
        if index < self.before.len() {
            self.before[index]
        } else {
            self.after_reversed[self.after_reversed.len() - 1 - (index - self.before.len())]
        }
    }

    fn x(&self, index: usize) -> f64 {
        self.at(index).x
    }

    fn y(&self, index: usize) -> f64 {
        self.at(index).y
    }

    /// Moves the gap to sit before `index`, which is at most [`CornerList::len`].
    fn move_gap(&mut self, index: usize, work: &mut Work) -> Result<(), CompareError> {
        work.spend(self.before.len().abs_diff(index) as u64)?;
        while self.before.len() > index {
            let Some(corner) = self.before.pop() else {
                break;
            };
            self.after_reversed.push(corner);
        }
        while self.before.len() < index {
            let Some(corner) = self.after_reversed.pop() else {
                break;
            };
            self.before.push(corner);
        }
        Ok(())
    }

    /// Replaces the list by its corners before `from` followed by its corners from `to` on:
    /// funnel's `removeRange(array, size, from, to - from)`. For `to >= from` that deletes the
    /// corners `from..to`. For `to < from`, a negative count in the C, it repeats the corners
    /// `to..from`, which is what `removeRange` then does; the loop removal never asks for that
    /// (see the module's notes), and the branch is kept as funnel's edit, not as a path any
    /// input takes.
    fn keep_before_and_from(
        &mut self,
        from: usize,
        to: usize,
        work: &mut Work,
    ) -> Result<(), CompareError> {
        if to >= from {
            self.move_gap(from, work)?;
            work.spend((to - from) as u64)?;
            self.after_reversed
                .truncate(self.after_reversed.len() - (to - from));
        } else {
            work.spend((from - to) as u64)?;
            let repeated: Vec<Corner> = (to..from).map(|index| self.at(index)).collect();
            self.move_gap(from, work)?;
            self.after_reversed.extend(repeated.into_iter().rev());
        }
        Ok(())
    }

    /// Inserts `corner` so that it is at `index`.
    fn insert(
        &mut self,
        index: usize,
        corner: Corner,
        work: &mut Work,
    ) -> Result<(), CompareError> {
        self.move_gap(index, work)?;
        work.spend(1)?;
        self.after_reversed.push(corner);
        Ok(())
    }

    /// Removes the corner at `index`, which is below [`CornerList::len`].
    fn remove(&mut self, index: usize, work: &mut Work) -> Result<(), CompareError> {
        self.move_gap(index, work)?;
        work.spend(1)?;
        self.after_reversed.pop();
        Ok(())
    }

    /// The corners in order.
    fn into_corners(mut self) -> Vec<Corner> {
        self.before.extend(self.after_reversed.into_iter().rev());
        self.before
    }
}

/// `y` at `x` on the line through corners `from` and `to` of `list`, in funnel's operation order.
fn on_line(list: &CornerList, from: usize, to: usize, x: f64) -> f64 {
    (list.y(to) - list.y(from)) / (list.x(to) - list.x(from)) * (x - list.x(from)) + list.y(from)
}

/// The outline `corners` of the `side` bound with every loop removed, in funnel's result.
///
/// # Errors
///
/// [`CompareError::WorkLimitExceeded`] when the searches and edits need more units than `work`
/// has left; [`CompareError::LoopRemovalUndefined`] where funnel's backward search leaves the
/// list.
pub(super) fn remove_loops(
    corners: Vec<Corner>,
    side: Side,
    work: &mut Work,
) -> Result<Vec<Corner>, CompareError> {
    let lower = side == Side::Lower;
    let mut list = CornerList::new(corners);
    let mut j = 1_usize;
    while j + 2 < list.len() {
        work.spend(1)?;
        if list.x(j + 1) < list.x(j) {
            j = remove_loop_at(&mut list, j, lower, work)?;
        }
        j += 1;
    }
    Ok(list.into_corners())
}

/// Removes the loop whose backward segment is `(j, j + 1)` and returns the index the scan
/// resumes after.
fn remove_loop_at(
    list: &mut CornerList,
    j: usize,
    lower: bool,
    work: &mut Work,
) -> Result<usize, CompareError> {
    let len = list.len();

    // ===== 1. Find i and k: segment (i - 1, i) intersects segment (k - 1, k). =====
    // Walk i back from j while corner i - 1 still lies right of corner j + 1.
    let mut i = j;
    let mut i_previous = i;
    loop {
        if i == 0 {
            // The C would read X[-1] here.
            return Err(CompareError::LoopRemovalUndefined);
        }
        if list.x(j + 1) < list.x(i - 1) {
            i -= 1;
            work.spend(1)?;
        } else {
            break;
        }
    }
    // k searches no further than the first corner back at or right of corner j.
    let mut k_max = j + 1;
    while list.x(k_max) < list.x(j) && k_max < len - 1 {
        k_max += 1;
        work.spend(1)?;
    }

    let mut k = j + 1;
    let mut y = list.y(i - 1);
    while ((lower && y < list.y(k)) || (!lower && list.y(k) < y)) && k < k_max {
        i_previous = i;
        k += 1;
        work.spend(1)?;
        while (list.x(i) < list.x(k)
            || (lower
                && equ(list.x(i), list.x(k))
                && list.y(i) < list.y(k)
                && !(k + 1 < len && equ(list.x(k), list.x(k + 1)) && list.y(k + 1) < list.y(k)))
            || (!lower
                && equ(list.x(i), list.x(k))
                && list.y(i) > list.y(k)
                && !(k + 1 < len && equ(list.x(k), list.x(k + 1)) && list.y(k + 1) > list.y(k))))
            && i < j
        {
            i += 1;
            work.spend(1)?;
        }
        // y at corner k's x on segment (i - 1, i).
        y = if !equ(list.x(i), list.x(i - 1)) {
            on_line(list, i - 1, i, list.x(k))
        } else {
            list.y(i)
        };
    }

    // k is located; the intersection is on the polygonal line from i_previous - 1 to i.
    i = if i_previous > 1 {
        i_previous - 1
    } else {
        i_previous
    };
    let k_vertical = equ(list.x(k), list.x(k - 1));
    if !k_vertical {
        y = on_line(list, k - 1, k, list.x(i));
    }
    while (!k_vertical && ((lower && list.y(i) < y) || (!lower && y < list.y(i))))
        || (k_vertical && list.x(i) < list.x(k))
    {
        i += 1;
        work.spend(1)?;
        if i >= len {
            // Defensive: the search stops at corner k - 1, or at k on a vertical segment (see
            // the module's notes), so no input reaches this.
            return Err(CompareError::LoopRemovalUndefined);
        }
        if !k_vertical {
            y = on_line(list, k - 1, k, list.x(i));
        }
    }

    // ===== 2. The intersection of segments (i - 1, i) and (k - 1, k). =====
    let i_vertical = equ(list.x(i), list.x(i - 1));
    let intersection = if i_vertical && k_vertical {
        None
    } else if i_vertical {
        let x = list.x(i);
        Some(Corner {
            x,
            y: list.y(k - 1)
                + ((x - list.x(k - 1)) * (list.y(k) - list.y(k - 1))) / (list.x(k) - list.x(k - 1)),
        })
    } else if k_vertical {
        let x = list.x(k);
        Some(Corner {
            x,
            y: list.y(i - 1)
                + ((x - list.x(i - 1)) * (list.y(i) - list.y(i - 1))) / (list.x(i) - list.x(i - 1)),
        })
    } else {
        let slope_i = (list.y(i) - list.y(i - 1)) / (list.x(i) - list.x(i - 1));
        let slope_k = (list.y(k) - list.y(k - 1)) / (list.x(k) - list.x(k - 1));
        if equ(slope_i, slope_k) {
            None
        } else {
            let x = (slope_i * list.x(i - 1) - slope_k * list.x(k - 1) - list.y(i - 1)
                + list.y(k - 1))
                / (slope_i - slope_k);
            // y is taken on the flatter of the two segments.
            let y = if slope_i.abs() > slope_k.abs() {
                slope_k * (x - list.x(k - 1)) + list.y(k - 1)
            } else {
                slope_i * (x - list.x(i - 1)) + list.y(i - 1)
            };
            Some(Corner { x, y })
        }
    };

    // ===== 3. Delete corners i to k - 1. 4. Add the intersection unless it is there. =====
    list.keep_before_and_from(i, k, work)?;
    if let Some(corner) = intersection
        && (!equ(list.x(i), corner.x) || !equ(list.y(i), corner.y))
    {
        list.insert(i, corner, work)?;
    }

    // ===== 5. Resume at i. 6. Drop corner i when it repeats corner i - 1. =====
    if equ(list.x(i - 1), list.x(i)) && equ(list.y(i - 1), list.y(i)) {
        list.remove(i, work)?;
        Ok(i - 1)
    } else {
        Ok(i)
    }
}

#[cfg(test)]
#[path = "loop_removal_tests.rs"]
mod tests;
