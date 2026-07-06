//! Stateful `CDL.Logical` timing and hold blocks.
//!
//! The time-based blocks all use the shared `dynamics` tick-delta convention: `dt=0` on the first
//! tick and otherwise `ctx.t() - prev_t`. No block assumes a fixed internal step.

use oce_model::{ParamTable, Value, determinism::det_max};

use crate::dynamics::{PREV_T_UNSET, is_first_tick, tick_dt};
use crate::{Block, BlockKind, BlockSignature, Ctx, PortKind, emit_real, read_bool};

fn bool_word(b: bool) -> u64 {
    u64::from(b)
}

fn word_bool(w: u64) -> bool {
    w != 0
}

fn positive_duration(x: f64) -> f64 {
    det_max(x, 0.0)
}

const ACC_WORD: usize = 0;
const PREV_T_WORD: usize = 1;
const PREV_U_WORD: usize = 2;
const TIMER_PASSED_WORD: usize = 3;

/// `CDL.Logical.Timer` — elapsed time since the last rising edge of `u`.
///
/// Buildings `Controls/OBC/CDL/Logical/Timer.mo` computes elapsed time as
/// `time - entryTime`, a single subtraction from the stored rising-edge timestamp, instead of a
/// per-tick accumulation. The output resets to zero while `u` is false.
/// Outputs `(y, passed)`. `passed` is the upstream Modelica discrete latch: it initializes to
/// `t <= 0` (`pre(passed) = t <= 0`, so an input held false from the start reports `passed =
/// true` for the default non-positive threshold), a rising `u` re-arms it to `t <= 0`, crossing
/// the threshold `y >= t` while `u` is true sets it, a falling `u` clears it (`elsewhen not u`),
/// and it otherwise holds its previous value — no when-clause fires without an edge. `[S]`,
/// feedthrough `y,passed <- {u}`, not a loop cut.
#[derive(Clone, Copy, Debug, Default)]
pub struct Timer {
    pub(crate) t: f64,
}

impl Timer {
    fn y_now(self, ctx: &Ctx<'_>, inputs: &[Value], region: &[u64]) -> f64 {
        let u = read_bool(inputs, 0);
        if !u {
            return 0.0;
        }
        if is_first_tick(region[PREV_T_WORD]) || !word_bool(region[PREV_U_WORD]) {
            0.0
        } else {
            ctx.t() - f64::from_bits(region[ACC_WORD])
        }
    }

    fn passed_now(self, inputs: &[Value], region: &[u64], live_y: f64) -> bool {
        let u = read_bool(inputs, 0);
        let prev_u = word_bool(region[PREV_U_WORD]);
        let pre_passed = word_bool(region[TIMER_PASSED_WORD]);
        if u && !prev_u {
            // Upstream `when u`: the first (highest-priority) clause re-arms the latch.
            self.t <= 0.0
        } else if u && live_y >= self.t {
            // Upstream `elsewhen (u and time >= t + pre(entryTime))`.
            true
        } else if !u && prev_u {
            // Upstream `elsewhen not u`: a falling edge clears the latch.
            false
        } else {
            // No edge fired: the discrete latch holds its previous value.
            pre_passed
        }
    }
}

impl Block for Timer {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Logical.Timer",
            inputs: &[PortKind::Boolean],
            outputs: &[PortKind::Real, PortKind::Boolean],
            stateful: true,
        };
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Stateful
    }

    fn feeds_through(&self, in_idx: usize, out_idx: usize) -> bool {
        in_idx == 0 && out_idx < 2
    }

    fn state_len(&self) -> usize {
        4
    }

    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region[ACC_WORD] = 0.0f64.to_bits();
        region[PREV_T_WORD] = PREV_T_UNSET;
        region[PREV_U_WORD] = 0;
        region[TIMER_PASSED_WORD] = bool_word(self.t <= 0.0);
    }

    fn emit_from_state(
        &self,
        ctx: &Ctx<'_>,
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        let y = self.y_now(ctx, inputs, region);
        let passed = self.passed_now(inputs, region, y);
        emit_real(0, y, emit);
        emit(1, Value::Boolean(passed));
    }

    fn update_state(&self, ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        let u = read_bool(inputs, 0);
        let y = self.y_now(ctx, inputs, region);
        let passed = self.passed_now(inputs, region, y);
        if u && (is_first_tick(region[PREV_T_WORD]) || !word_bool(region[PREV_U_WORD])) {
            region[ACC_WORD] = ctx.t().to_bits();
        }
        region[PREV_T_WORD] = ctx.t().to_bits();
        region[PREV_U_WORD] = bool_word(u);
        region[TIMER_PASSED_WORD] = bool_word(passed);
    }
}

const TA_PREV_RESET_WORD: usize = 3;
const TA_PASSED_WORD: usize = 4;

/// `CDL.Logical.TimerAccumulating` — accumulated true-time, cleared by a reset rising edge.
/// Outputs `(y, passed)`. `y` accumulates true-time and holds while `u` is false; reset clears the
/// accumulation to zero. `passed` is a Modelica discrete latch: reset sets it to `t <= 0`, a rising
/// `u` samples the accumulated value before the new interval, crossing the threshold while `u` is
/// true sets it, and `not u` holds the previous value. `[S]`, feedthrough
/// `y,passed <- {u,reset}`, not a loop cut.
#[derive(Clone, Copy, Debug, Default)]
pub struct TimerAccumulating {
    pub(crate) t: f64,
}

impl TimerAccumulating {
    fn y_now(self, ctx: &Ctx<'_>, inputs: &[Value], region: &[u64]) -> f64 {
        let u = read_bool(inputs, 0);
        let reset = read_bool(inputs, 1);
        let reset_rising = reset && !word_bool(region[TA_PREV_RESET_WORD]);
        if reset_rising {
            return 0.0;
        }
        let acc = f64::from_bits(region[ACC_WORD]);
        if !u || is_first_tick(region[PREV_T_WORD]) || !word_bool(region[PREV_U_WORD]) {
            acc
        } else {
            acc + tick_dt(ctx.t(), region[PREV_T_WORD])
        }
    }

    fn passed_now(self, inputs: &[Value], region: &[u64], live_y: f64) -> bool {
        let u = read_bool(inputs, 0);
        let reset = read_bool(inputs, 1);
        let reset_rising = reset && !word_bool(region[TA_PREV_RESET_WORD]);
        let u_rising = u && !word_bool(region[PREV_U_WORD]);
        let pre_passed = word_bool(region[TA_PASSED_WORD]);
        if reset_rising {
            self.t <= 0.0
        } else if u_rising {
            self.t <= f64::from_bits(region[ACC_WORD])
        } else if u && live_y >= self.t {
            true
        } else {
            pre_passed
        }
    }
}

impl Block for TimerAccumulating {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Logical.TimerAccumulating",
            inputs: &[PortKind::Boolean, PortKind::Boolean],
            outputs: &[PortKind::Real, PortKind::Boolean],
            stateful: true,
        };
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Stateful
    }

    fn feeds_through(&self, in_idx: usize, out_idx: usize) -> bool {
        in_idx < 2 && out_idx < 2
    }

    fn state_len(&self) -> usize {
        5
    }

    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region[ACC_WORD] = 0.0f64.to_bits();
        region[PREV_T_WORD] = PREV_T_UNSET;
        region[PREV_U_WORD] = 0;
        region[TA_PREV_RESET_WORD] = 0;
        region[TA_PASSED_WORD] = bool_word(self.t <= 0.0);
    }

    fn emit_from_state(
        &self,
        ctx: &Ctx<'_>,
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        let y = self.y_now(ctx, inputs, region);
        let passed = self.passed_now(inputs, region, y);
        emit_real(0, y, emit);
        emit(1, Value::Boolean(passed));
    }

    fn update_state(&self, ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        let y = self.y_now(ctx, inputs, region);
        let passed = self.passed_now(inputs, region, y);
        region[ACC_WORD] = y.to_bits();
        region[PREV_T_WORD] = ctx.t().to_bits();
        region[PREV_U_WORD] = bool_word(read_bool(inputs, 0));
        region[TA_PREV_RESET_WORD] = bool_word(read_bool(inputs, 1));
        region[TA_PASSED_WORD] = bool_word(passed);
    }
}

const TD_TIMER_WORD: usize = 0;
const TD_PREV_T_WORD: usize = 1;
const TD_PREV_U_WORD: usize = 2;
const TD_HELD_WORD: usize = 3;

/// `CDL.Logical.TrueDelay` — delays rising edges by `delayTime`; falling edges pass immediately.
/// `[S]`, feedthrough `y <- {u}`, not a loop cut.
#[derive(Clone, Copy, Debug, Default)]
pub struct TrueDelay {
    pub(crate) delay_time: f64,
    pub(crate) delay_on_init: bool,
}

impl TrueDelay {
    fn output_and_timer(self, ctx: &Ctx<'_>, inputs: &[Value], region: &[u64]) -> (bool, f64) {
        let u = read_bool(inputs, 0);
        if !u {
            return (false, 0.0);
        }
        let first = is_first_tick(region[TD_PREV_T_WORD]);
        let held = word_bool(region[TD_HELD_WORD]);
        let prev_u = word_bool(region[TD_PREV_U_WORD]);
        let delay = positive_duration(self.delay_time);
        if first {
            if self.delay_on_init && delay > 0.0 {
                (false, 0.0)
            } else {
                (true, delay)
            }
        } else if held {
            (true, delay)
        } else if !prev_u {
            (delay <= 0.0, 0.0)
        } else {
            let timer =
                f64::from_bits(region[TD_TIMER_WORD]) + tick_dt(ctx.t(), region[TD_PREV_T_WORD]);
            (timer >= delay, timer)
        }
    }
}

impl Block for TrueDelay {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Logical.TrueDelay",
            inputs: &[PortKind::Boolean],
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
        4
    }

    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region[TD_TIMER_WORD] = 0.0f64.to_bits();
        region[TD_PREV_T_WORD] = PREV_T_UNSET;
        region[TD_PREV_U_WORD] = 0;
        region[TD_HELD_WORD] = 0;
    }

    fn emit_from_state(
        &self,
        ctx: &Ctx<'_>,
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        emit(
            0,
            Value::Boolean(self.output_and_timer(ctx, inputs, region).0),
        );
    }

    fn update_state(&self, ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        let (y, timer) = self.output_and_timer(ctx, inputs, region);
        region[TD_TIMER_WORD] = timer.to_bits();
        region[TD_PREV_T_WORD] = ctx.t().to_bits();
        region[TD_PREV_U_WORD] = bool_word(read_bool(inputs, 0));
        region[TD_HELD_WORD] = bool_word(y);
    }
}

const TFH_HELD_WORD: usize = 0;
const TFH_TIMER_WORD: usize = 1;
const TFH_PREV_T_WORD: usize = 2;

/// `CDL.Logical.TrueFalseHold` — minimum dwell time for both true and false output states.
/// `[S]`, feedthrough `y <- {u}`, not a loop cut.
///
/// The registry factory is the source of truth for parameter defaults: `falseHoldDuration` defaults
/// to the resolved `trueHoldDuration`, while the bare Rust [`Default`] uses zero durations.
#[derive(Clone, Copy, Debug)]
pub struct TrueFalseHold {
    pub(crate) true_hold_duration: f64,
    pub(crate) false_hold_duration: f64,
}

impl Default for TrueFalseHold {
    fn default() -> Self {
        Self {
            true_hold_duration: 0.0,
            false_hold_duration: 0.0,
        }
    }
}

impl TrueFalseHold {
    fn output_and_timer(self, ctx: &Ctx<'_>, inputs: &[Value], region: &[u64]) -> (bool, f64) {
        let u = read_bool(inputs, 0);
        if is_first_tick(region[TFH_PREV_T_WORD]) {
            return (u, 0.0);
        }
        let held = word_bool(region[TFH_HELD_WORD]);
        let timer =
            f64::from_bits(region[TFH_TIMER_WORD]) + tick_dt(ctx.t(), region[TFH_PREV_T_WORD]);
        if u == held {
            (held, timer)
        } else {
            let required = if held {
                positive_duration(self.true_hold_duration)
            } else {
                positive_duration(self.false_hold_duration)
            };
            if timer >= required {
                (u, 0.0)
            } else {
                (held, timer)
            }
        }
    }
}

impl Block for TrueFalseHold {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Logical.TrueFalseHold",
            inputs: &[PortKind::Boolean],
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
        3
    }

    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region[TFH_HELD_WORD] = 0;
        region[TFH_TIMER_WORD] = 0.0f64.to_bits();
        region[TFH_PREV_T_WORD] = PREV_T_UNSET;
    }

    fn emit_from_state(
        &self,
        ctx: &Ctx<'_>,
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        emit(
            0,
            Value::Boolean(self.output_and_timer(ctx, inputs, region).0),
        );
    }

    fn update_state(&self, ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        let (y, timer) = self.output_and_timer(ctx, inputs, region);
        region[TFH_HELD_WORD] = bool_word(y);
        region[TFH_TIMER_WORD] = timer.to_bits();
        region[TFH_PREV_T_WORD] = ctx.t().to_bits();
    }
}

const THR_HELD_WORD: usize = 0;
const THR_TIMER_WORD: usize = 1;
const THR_PREV_T_WORD: usize = 2;

/// `CDL.Logical.TrueHoldWithReset` — true output is held for at least `duration`; `clr` resets.
/// `[S]`, feedthrough `y <- {u, clr}`, not a loop cut.
///
/// This class is spec-authoritative here: the oracle is the CDL specification's Logical block
/// catalog for `TrueHoldWithReset` (`obc.lbl.gov/specification`, §7.6 Elementary Blocks), not current
/// Buildings master, where this class is absent. Its canonical min-hold behavior is equivalent to
/// [`TrueFalseHold`] with `falseHoldDuration = 0`, plus the explicit clear input.
#[derive(Clone, Copy, Debug, Default)]
pub struct TrueHoldWithReset {
    pub(crate) duration: f64,
}

impl TrueHoldWithReset {
    fn output_and_timer(self, ctx: &Ctx<'_>, inputs: &[Value], region: &[u64]) -> (bool, f64) {
        let u = read_bool(inputs, 0);
        let clr = read_bool(inputs, 1);
        if clr {
            return (false, 0.0);
        }
        if is_first_tick(region[THR_PREV_T_WORD]) {
            return (u, 0.0);
        }
        let held = word_bool(region[THR_HELD_WORD]);
        let timer = if held {
            f64::from_bits(region[THR_TIMER_WORD]) + tick_dt(ctx.t(), region[THR_PREV_T_WORD])
        } else {
            0.0
        };
        let duration = positive_duration(self.duration);
        let y = u || (held && timer < duration);
        let next_timer = if y {
            if held { timer } else { 0.0 }
        } else {
            0.0
        };
        (y, next_timer)
    }
}

impl Block for TrueHoldWithReset {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Logical.TrueHoldWithReset",
            inputs: &[PortKind::Boolean, PortKind::Boolean],
            outputs: &[PortKind::Boolean],
            stateful: true,
        };
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Stateful
    }

    fn feeds_through(&self, in_idx: usize, out_idx: usize) -> bool {
        in_idx < 2 && out_idx == 0
    }

    fn state_len(&self) -> usize {
        3
    }

    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region[THR_HELD_WORD] = 0;
        region[THR_TIMER_WORD] = 0.0f64.to_bits();
        region[THR_PREV_T_WORD] = PREV_T_UNSET;
    }

    fn emit_from_state(
        &self,
        ctx: &Ctx<'_>,
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        emit(
            0,
            Value::Boolean(self.output_and_timer(ctx, inputs, region).0),
        );
    }

    fn update_state(&self, ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        let (y, timer) = self.output_and_timer(ctx, inputs, region);
        region[THR_HELD_WORD] = bool_word(y);
        region[THR_TIMER_WORD] = timer.to_bits();
        region[THR_PREV_T_WORD] = ctx.t().to_bits();
    }
}
