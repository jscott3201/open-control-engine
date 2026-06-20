//! Shared discrete-time helpers for dynamic `CDL.Reals` blocks.
//!
//! These helpers are pure arithmetic only. State layout stays owned by each block module; this
//! module centralizes the tick-delta sentinel convention and the stable one-pole filter step so
//! integrators, PID, derivatives, and slew limiters do not drift.

use crate::Time;

/// First-tick sentinel for a stored previous-time word.
///
/// This deliberately aliases a NaN bit pattern. Model time is contractually non-NaN, and callers
/// compare raw bits before any `from_bits`, so the sentinel is never interpreted as a time.
pub(crate) const PREV_T_UNSET: u64 = u64::MAX;

#[must_use]
pub(crate) fn is_first_tick(prev_t_word: u64) -> bool {
    prev_t_word == PREV_T_UNSET
}

#[must_use]
pub(crate) fn tick_dt(t_now: Time, prev_t_word: u64) -> f64 {
    if is_first_tick(prev_t_word) {
        0.0
    } else {
        t_now - f64::from_bits(prev_t_word)
    }
}

#[must_use]
pub(crate) fn forward_euler_accumulate(x: f64, slope: f64, dt: f64) -> f64 {
    x + slope * dt
}

#[must_use]
pub(crate) fn first_order_filter_implicit(x: f64, input: f64, time_const: f64, dt: f64) -> f64 {
    let alpha = dt / time_const;
    (x + alpha * input) / (1.0 + alpha)
}
