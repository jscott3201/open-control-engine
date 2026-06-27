//! `CDL.Logical.VariablePulse` duty-cycle pulse generator.
//!
//! The Buildings block is a protected composite around a sampled input-width detector, an inner
//! pulse-cycle block, and `TrueFalseHold`. This native implementation keeps the same observable
//! scalar state in arena words: last sampled width, current cycle anchor time, held Boolean output,
//! current held-state dwell timer, and previous tick time.

use std::cmp::Ordering;

use oce_model::{ParamTable, Value, determinism::canonicalize_real};

use crate::dynamics::{PREV_T_UNSET, is_first_tick, tick_dt};
use crate::{Block, BlockKind, BlockSignature, Ctx, PortKind, Time, read_real};

const SAMPLED_U_WORD: usize = 0;
const T0_WORD: usize = 1;
const HELD_WORD: usize = 2;
const TIMER_WORD: usize = 3;
const PREV_T_WORD: usize = 4;

/// Minimum valid `minTruFalHol` parameter from `CDL.Constants.small`.
pub(crate) const MIN_VARIABLE_PULSE_TIME: f64 = 1e-37;
/// Source minimum for `deltaU`.
pub(crate) const MIN_DELTA_U: f64 = 0.001;
/// Source maximum for `deltaU`.
pub(crate) const MAX_DELTA_U: f64 = 0.5;

/// `CDL.Logical.VariablePulse` - Boolean pulse with runtime duty ratio.
///
/// `period` is the nominal pulse period in seconds and is required by the source but has no hard
/// lower-bound assertion. `deltaU` is the minimum duty-ratio change that resets the cycle anchor.
/// `minTruFalHol` is the minimum dwell time for either output state. If
/// `period < 2*minTruFalHol`, the Buildings source emits a warning and uses
/// `max(period, 2.02*minTruFalHol)` internally; the registry publishes that warning and this block
/// uses the adjusted period. The block has one `Real` input `u` in the source-declared `[0, 1]`
/// duty-ratio range and one `Boolean` output `y`. It is stateful and feeds through from `u` because
/// a same-tick width change can reset the cycle and affect the held output.
///
/// The block never panics for finite or non-finite direct-construction values. Validated models
/// reject invalid bounded parameters before runtime; direct non-finite or invalid bounded
/// parameters degrade to deterministic defaults.
#[derive(Clone, Copy, Debug)]
pub struct LogicalVariablePulse {
    /// Nominal period in seconds.
    pub(crate) period: f64,
    /// Duty-ratio change threshold.
    pub(crate) delta_u: f64,
    /// Minimum true/false dwell time in seconds.
    pub(crate) min_true_false_hold: f64,
}

impl Default for LogicalVariablePulse {
    fn default() -> Self {
        Self {
            period: 1.0,
            delta_u: 0.01,
            min_true_false_hold: 0.01,
        }
    }
}

impl LogicalVariablePulse {
    fn step(self, ctx: &Ctx<'_>, inputs: &[Value], region: &[u64]) -> VariablePulseStep {
        let first_tick = is_first_tick(region[PREV_T_WORD]);
        let t = ctx.t();
        let u = read_real(inputs, 0);
        let sampled_u = f64::from_bits(region[SAMPLED_U_WORD]);
        let old_t0 = if first_tick {
            t
        } else {
            f64::from_bits(region[T0_WORD])
        };

        let width_changed = width_change_exceeds_delta(sampled_u, u, effective_delta_u(self));
        let next_t0 = if first_tick || width_changed {
            t
        } else {
            old_t0
        };
        let next_sampled_u = if width_changed { u } else { sampled_u };
        let cycle_y = cycle_output(t, u, next_t0, adjusted_period(self));

        let (next_held, next_timer) = if first_tick {
            (cycle_y, 0.0)
        } else {
            held_output_and_timer(
                cycle_y,
                region,
                t,
                effective_min_hold(self.min_true_false_hold),
            )
        };

        VariablePulseStep {
            y: next_held,
            sampled_u: next_sampled_u,
            t0: next_t0,
            held: next_held,
            timer: next_timer,
        }
    }
}

impl Block for LogicalVariablePulse {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Logical.VariablePulse",
            inputs: &[PortKind::Real],
            outputs: &[PortKind::Boolean],
            stateful: true,
        };
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Stateful
    }

    fn feeds_through(&self, in_idx: usize, out_idx: usize) -> bool {
        in_idx == 0 && out_idx == 0
    }

    fn state_len(&self) -> usize {
        5
    }

    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region[SAMPLED_U_WORD] = 0.0f64.to_bits();
        region[T0_WORD] = 0.0f64.to_bits();
        region[HELD_WORD] = bool_word(false);
        region[TIMER_WORD] = 0.0f64.to_bits();
        region[PREV_T_WORD] = PREV_T_UNSET;
    }

    fn emit_from_state(
        &self,
        ctx: &Ctx<'_>,
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        emit(0, Value::Boolean(self.step(ctx, inputs, region).y));
    }

    fn update_state(&self, ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        let step = self.step(ctx, inputs, region);
        region[SAMPLED_U_WORD] = canonicalize_real(step.sampled_u).to_bits();
        region[T0_WORD] = canonicalize_real(step.t0).to_bits();
        region[HELD_WORD] = bool_word(step.held);
        region[TIMER_WORD] = canonicalize_real(step.timer).to_bits();
        region[PREV_T_WORD] = ctx.t().to_bits();
    }
}

#[derive(Clone, Copy, Debug)]
struct VariablePulseStep {
    y: bool,
    sampled_u: f64,
    t0: f64,
    held: bool,
    timer: f64,
}

fn held_output_and_timer(cycle_y: bool, region: &[u64], t: Time, min_hold: f64) -> (bool, f64) {
    let held = word_bool(region[HELD_WORD]);
    let timer = f64::from_bits(region[TIMER_WORD]) + tick_dt(t, region[PREV_T_WORD]);
    if cycle_y == held {
        (held, timer)
    } else if timer >= min_hold {
        (cycle_y, 0.0)
    } else {
        (held, timer)
    }
}

fn width_change_exceeds_delta(sampled_u: f64, u: f64, delta_u: f64) -> bool {
    matches!(
        (sampled_u - u).abs().partial_cmp(&delta_u),
        Some(Ordering::Greater)
    )
}

fn cycle_output(t: Time, u: f64, t0: Time, period: f64) -> bool {
    if !t.is_finite() || !t0.is_finite() {
        return false;
    }
    if !u.is_finite() || u <= 0.0 {
        return false;
    }
    if u >= 1.0 {
        return true;
    }

    let t_start = t0 + buildings_round_six(((t - t0) / period).floor() * period);
    let t_end = t_start + u * period;
    t >= t_start && t < t_end
}

fn adjusted_period(block: LogicalVariablePulse) -> f64 {
    let period = effective_period(block.period);
    let min_hold = effective_min_hold(block.min_true_false_hold);
    period.max(min_hold * 2.02)
}

fn effective_period(period: f64) -> f64 {
    if period.is_finite() { period } else { 1.0 }
}

fn effective_delta_u(block: LogicalVariablePulse) -> f64 {
    if block.delta_u.is_finite() {
        block.delta_u.clamp(MIN_DELTA_U, MAX_DELTA_U)
    } else {
        0.01
    }
}

fn effective_min_hold(min_hold: f64) -> f64 {
    if min_hold.is_finite() && min_hold >= MIN_VARIABLE_PULSE_TIME {
        min_hold
    } else {
        MIN_VARIABLE_PULSE_TIME
    }
}

fn buildings_round_six(x: f64) -> f64 {
    const FACTOR: f64 = 1_000_000.0;
    if x > 0.0 {
        (x * FACTOR + 0.5).floor() / FACTOR
    } else {
        (x * FACTOR - 0.5).ceil() / FACTOR
    }
}

fn bool_word(value: bool) -> u64 {
    u64::from(value)
}

fn word_bool(word: u64) -> bool {
    word != 0
}
