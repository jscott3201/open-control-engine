//! The set's clock and the one rule that decides what a tick is (vectors note, "Clock" and
//! "Windows"; decisions 3 and 11).
//!
//! Tick `k` is `(k as f64) * step_s`, by one IEEE-754 binary64 multiplication, for `k = 0..=N`.
//! A time `x` is **on the cadence** when some integer `k >= 0` gives `(k as f64) * step_s == x`
//! exactly. `k` is found by rounding the quotient `x / step_s` to the nearest integer `q` and
//! testing `q - 1`, `q` and `q + 1` by multiplication; a quotient is never floored, because
//! `floor(4.3 / 0.1)` is 42 while `43.0 * 0.1 == 4.3` exactly. Inside the tick bound the products
//! of distinct `k` are distinct, so at most one candidate holds.
//!
//! `horizon_s` must be on the cadence and its `k` is `N`. A diagnostic `at_s` is a tick by the
//! same function. A window is decided from tick values alone: tick `k` is in `[from_s, to_s]`
//! when `from_s <= (k as f64) * step_s <= to_s`, and the first and last such ticks are found by
//! bisection over `k`, with no division at all (the products are non-decreasing in `k`).
//!
//! Nothing here accumulates a time or allocates per tick: a clock of 16,777,216 ticks costs what
//! a clock of two does.

use crate::error::{VectorError, VectorErrorKind};
use crate::number::Number;

/// The time of tick `k` on a clock of step `step_s`: one multiplication.
#[expect(
    clippy::cast_precision_loss,
    reason = "a tick index is at most 2^24, where the cast is exact"
)]
fn tick_time(tick: u64, step_s: f64) -> f64 {
    (tick as f64) * step_s
}

/// The integer `k` in `0..=last` with `(k as f64) * step_s == time_s`, by the rule in the module
/// documentation; `None` when there is none, when `time_s` is negative or not finite, or when the
/// rounded quotient is past `last` (no candidate is then tested).
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "`last` is at most 2^24; the quotient is cast only after it is known to be an integer in `0..=last`"
)]
pub(crate) fn cadence_index(time_s: f64, step_s: f64, last: u64) -> Option<u64> {
    if !time_s.is_finite() || time_s < 0.0 {
        return None;
    }
    let quotient = (time_s / step_s).round();
    if quotient.is_nan() || quotient < 0.0 || quotient > last as f64 {
        return None;
    }
    let nearest = quotient as u64;
    [
        nearest.checked_sub(1),
        Some(nearest),
        nearest.checked_add(1),
    ]
    .into_iter()
    .flatten()
    .filter(|candidate| *candidate <= last)
    .find(|candidate| tick_time(*candidate, step_s) == time_s)
}

/// A vector set's clock: `tick_count()` ticks at `k * step_s`, the last of them `horizon_s`.
///
/// Built only by a load, so `step_s` is positive, `horizon_s` is not negative and is exactly
/// `last_tick() * step_s`, and the tick count is within [`crate::VerifyLimits::max_ticks`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clock {
    step: Number,
    horizon: Number,
    last_tick: u64,
}

impl Clock {
    /// Check `step` and `horizon` as written and find the last tick, allowing at most `max_ticks`
    /// ticks.
    ///
    /// # Errors
    /// `invalid-clock` for a step that is not positive or a negative horizon; `too-many-ticks`
    /// when the rounded quotient puts the tick count past `max_ticks`, before any candidate is
    /// tested; `horizon-off-cadence` when no tick lands on the horizon.
    #[expect(
        clippy::cast_precision_loss,
        reason = "`max_ticks` is at most 2^24 after `VerifyLimits::effective`, where the cast is exact"
    )]
    pub(crate) fn new(step: Number, horizon: Number, max_ticks: u64) -> Result<Self, VectorError> {
        let (step_s, horizon_s) = (step.seconds(), horizon.seconds());
        if !(step_s.is_finite() && step_s > 0.0) {
            return Err(VectorError::on(
                VectorErrorKind::InvalidClock,
                "clock.step_s",
                "step_s must be positive",
            ));
        }
        if !(horizon_s.is_finite() && horizon_s >= 0.0) {
            return Err(VectorError::on(
                VectorErrorKind::InvalidClock,
                "clock.horizon_s",
                "horizon_s must not be negative",
            ));
        }
        let too_many = || {
            VectorError::on(
                VectorErrorKind::TooManyTicks,
                "clock",
                format!("the clock has more than {max_ticks} ticks"),
            )
        };
        let Some(bound) = max_ticks.checked_sub(1) else {
            return Err(too_many());
        };
        let quotient = (horizon_s / step_s).round();
        if quotient.is_nan() || quotient > bound as f64 {
            return Err(too_many());
        }
        let Some(last_tick) = cadence_index(horizon_s, step_s, bound) else {
            return Err(VectorError::on(
                VectorErrorKind::HorizonOffCadence,
                "clock.horizon_s",
                "no whole number of steps reaches horizon_s exactly",
            ));
        };
        Ok(Self {
            step,
            horizon,
            last_tick,
        })
    }

    /// `step_s` as the set writes it.
    pub(crate) fn step_written(&self) -> Number {
        self.step
    }

    /// `horizon_s` as the set writes it.
    pub(crate) fn horizon_written(&self) -> Number {
        self.horizon
    }

    /// Seconds between ticks: finite and positive.
    #[must_use]
    pub fn step_s(&self) -> f64 {
        self.step.seconds()
    }

    /// The time of the last tick, in seconds: finite, not negative, and exactly
    /// `last_tick() as f64 * step_s()`.
    #[must_use]
    pub fn horizon_s(&self) -> f64 {
        self.horizon.seconds()
    }

    /// The index `N` of the last tick; ticks are `0..=N`.
    #[must_use]
    pub fn last_tick(&self) -> u64 {
        self.last_tick
    }

    /// The number of ticks, `N + 1`: the frames one scenario runs.
    #[must_use]
    pub fn tick_count(&self) -> u64 {
        self.last_tick + 1
    }

    /// The time of tick `tick` in seconds, `tick as f64 * step_s()`; `None` past the last tick.
    #[must_use]
    pub fn tick_time(&self, tick: u64) -> Option<f64> {
        (tick <= self.last_tick).then(|| tick_time(tick, self.step_s()))
    }

    /// The tick whose time is exactly `time_s`, by the cadence rule; `None` when `time_s` is
    /// between ticks, after the horizon, negative or not finite.
    #[must_use]
    pub fn tick_at(&self, time_s: f64) -> Option<u64> {
        cadence_index(time_s, self.step_s(), self.last_tick)
    }

    /// The first and last tick whose time is in `[from_s, to_s]`, both ends included, decided
    /// from tick values; `None` when the window holds no tick (or either bound is NaN).
    #[must_use]
    pub fn ticks_within(&self, from_s: f64, to_s: f64) -> Option<(u64, u64)> {
        if from_s.is_nan() || to_s.is_nan() {
            return None;
        }
        let first = self.ticks_where(|time| time < from_s);
        let after = self.ticks_where(|time| time <= to_s);
        (first < after).then(|| (first, after - 1))
    }

    /// How many ticks, counted from tick 0, satisfy `holds`, which must be true for a prefix of
    /// the ticks and false for the rest. Bisection: at most 25 tick times are computed.
    fn ticks_where(&self, holds: impl Fn(f64) -> bool) -> u64 {
        let step_s = self.step_s();
        let (mut low, mut high) = (0, self.last_tick + 1);
        while low < high {
            let middle = low + (high - low) / 2;
            if holds(tick_time(middle, step_s)) {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        low
    }
}

#[cfg(test)]
#[path = "clock_tests.rs"]
mod tests;
