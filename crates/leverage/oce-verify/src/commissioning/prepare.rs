//! The comparison instants of a reference, and each series held at them (commissioning note,
//! "Comparison" under "Sources, sampling and time"; decisions 14 and 16; the owner's rulings of
//! 2026-11-21 in "As landed: the run, the comparison and the receipt").
//!
//! With a `sampling`, both series of every output are compared at one set of instants (without
//! one, [`super::compare_change`] compares by change of value): `start + k * s` for every
//! `k >= 0` with `start + k * s <= stop`, by multiply as replay's grid, and then `stop` itself
//! when the grid does not reach it ("each series gets a sample exactly at `start` and `stop`"). Every value is taken by
//! **zero-order hold**: the last sample at or before an instant, the last of an equal-time pair
//! included. Never linear interpolation.
//!
//! A series with no sample at or before `start` cannot be held there: its output is `no_data`.

/// The comparison window and sampling of a reference.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Instants {
    /// The window's start, model seconds.
    pub(crate) start: f64,
    /// The window's stop, included.
    pub(crate) stop: f64,
    /// The sampling, seconds: finite and above zero.
    pub(crate) step: f64,
}

impl Instants {
    /// The grid instant `k`: `start + k * s`, one rounding of the product and one of the sum.
    #[expect(
        clippy::cast_precision_loss,
        reason = "k is at most the instant count, which bind holds far below 2^53"
    )]
    fn grid(&self, k: u64) -> f64 {
        self.start + (k as f64) * self.step
    }

    /// The number of comparison instants, exactly: the grid instants `start + k * s <= stop`,
    /// found by a binary search over `k` on the arithmetic the instants are made with (they
    /// never decrease as `k` grows), plus `stop` when the grid's last instant is before it.
    /// When there are more than `cap` it gives some value above `cap`, so the cost is at most
    /// about 66 evaluations whatever the window.
    pub(crate) fn count(&self, cap: u64) -> u64 {
        let within = |k: u64| self.grid(k) <= self.stop;
        let limit = cap.saturating_add(1);
        // `within(0)` holds, since `start <= stop`.
        if within(limit) {
            return limit.saturating_add(1);
        }
        let (mut low, mut high) = (0_u64, limit);
        while high - low > 1 {
            let middle = low + (high - low) / 2;
            if within(middle) {
                low = middle;
            } else {
                high = middle;
            }
        }
        let grid = low + 1;
        if self.grid(low) < self.stop {
            grid + 1
        } else {
            grid
        }
    }

    /// The `count` comparison instants ([`Self::count`]), in order: the grid, then `stop` when
    /// it is off the grid.
    pub(crate) fn instants(&self, count: u64) -> Vec<f64> {
        let mut instants: Vec<f64> = Vec::with_capacity(usize::try_from(count).unwrap_or(0));
        for k in 0..count {
            let t = self.grid(k);
            if t > self.stop {
                break;
            }
            instants.push(t);
        }
        if instants.last().is_some_and(|last| *last < self.stop) {
            instants.push(self.stop);
        }
        instants
    }
}

/// The index of the first of `instants` equal to the one before it, when one is: with a large
/// `start` and a small `sampling`, `start + k * s` can round to the same value for two `k`.
pub(crate) fn first_repeat(instants: &[f64]) -> Option<usize> {
    instants
        .windows(2)
        .position(|pair| pair[1] <= pair[0])
        .map(|at| at + 1)
}

/// The index of the sample of `times` (non-decreasing) held at each of `instants`
/// (non-decreasing), or `None` when the series has no sample at or before the first instant.
/// One pass over both.
pub(crate) fn held_indexes(times: &[f64], instants: &[f64]) -> Option<Vec<usize>> {
    let first = *instants.first()?;
    let mut sample = times.partition_point(|x| *x <= first).checked_sub(1)?;
    let mut held = Vec::with_capacity(instants.len());
    for t in instants {
        while sample + 1 < times.len() && times[sample + 1] <= *t {
            sample += 1;
        }
        held.push(sample);
    }
    Some(held)
}

/// The values at `indexes`.
pub(crate) fn held<T: Copy>(indexes: &[usize], values: &[T]) -> Vec<T> {
    indexes.iter().map(|index| values[*index]).collect()
}

#[cfg(test)]
#[path = "prepare_tests.rs"]
mod tests;
