//! Stateful `CDL.Integers.Stage` Real-to-Integer staging block.
//!
//! The Buildings block is an evented stage selector: the current Real input can trigger a
//! same-tick output update once the minimum hold time has elapsed, while the selected stage and
//! hysteresis thresholds persist between events.

use oce_model::{ParamTable, Value};

use crate::dynamics::{PREV_T_UNSET, is_first_tick};
use crate::{Block, BlockKind, BlockSignature, Ctx, PortKind, read_real};

const Y_WORD: usize = 0;
const T_NEXT_WORD: usize = 1;
const UPPER_THRESHOLD_WORD: usize = 2;
const LOWER_THRESHOLD_WORD: usize = 3;
const PREV_CHECK_UPPER_WORD: usize = 4;
const PREV_CHECK_LOWER_WORD: usize = 5;
const PREV_WHEN_CONDITION_WORD: usize = 6;
const PREV_T_WORD: usize = 7;
const STATE_LEN: usize = 8;

fn bool_word(value: bool) -> u64 {
    u64::from(value)
}

fn word_bool(word: u64) -> bool {
    word != 0
}

fn i64_word(value: i64) -> u64 {
    value.cast_unsigned()
}

fn word_i64(word: u64) -> i64 {
    word.cast_signed()
}

fn threshold(stage_idx: i64, n: i64) -> f64 {
    (stage_idx - 1) as f64 / n as f64
}

/// `CDL.Integers.Stage` — select the number of enabled stages from a normalized Real input.
///
/// Parameters are `n` (maximum stage count), `holdDuration` (minimum dwell time), `h`
/// (hysteresis), and `pre_y_start` (initial `pre(y)`). The source output annotation says
/// `min=1`, but the equations can emit the authored `pre_y_start` before the first event and can
/// compute `0` for out-of-range low inputs; this implementation does not clamp that behavior.
///
/// The block is stateful but direct-feedthrough: once `time >= pre(tNext)`, the current `u` can
/// update and emit `y` on the same tick.
#[derive(Clone, Copy, Debug)]
pub struct IntegerStage {
    /// Maximum number of stages.
    pub(crate) n: i64,
    /// Minimum time that the output must be held constant, seconds.
    pub(crate) hold_duration: f64,
    /// Hysteresis around stage thresholds.
    pub(crate) h: f64,
    /// Initial `pre(y)` value.
    pub(crate) pre_y_start: i64,
}

impl Default for IntegerStage {
    fn default() -> Self {
        Self {
            n: 1,
            hold_duration: 0.0,
            h: 0.02,
            pre_y_start: 0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct StageState {
    y: i64,
    t_next: f64,
    upper_threshold: f64,
    lower_threshold: f64,
    prev_check_upper: bool,
    prev_check_lower: bool,
    prev_when_condition: bool,
}

impl IntegerStage {
    fn safe_n(self) -> i64 {
        self.n.max(1)
    }

    fn initial_state(self, t: f64) -> StageState {
        StageState {
            y: self.pre_y_start,
            t_next: t + self.hold_duration,
            upper_threshold: 0.0,
            lower_threshold: 0.0,
            prev_check_upper: false,
            prev_check_lower: true,
            prev_when_condition: false,
        }
    }

    fn stored_or_initial(self, ctx: &Ctx<'_>, region: &[u64]) -> StageState {
        if is_first_tick(region[PREV_T_WORD]) {
            return self.initial_state(ctx.t());
        }
        StageState {
            y: word_i64(region[Y_WORD]),
            t_next: f64::from_bits(region[T_NEXT_WORD]),
            upper_threshold: f64::from_bits(region[UPPER_THRESHOLD_WORD]),
            lower_threshold: f64::from_bits(region[LOWER_THRESHOLD_WORD]),
            prev_check_upper: word_bool(region[PREV_CHECK_UPPER_WORD]),
            prev_check_lower: word_bool(region[PREV_CHECK_LOWER_WORD]),
            prev_when_condition: word_bool(region[PREV_WHEN_CONDITION_WORD]),
        }
    }

    fn stage_for_u(self, u: f64) -> i64 {
        let n = self.safe_n();
        if u >= threshold(n, n) {
            return n;
        }
        for i in 2..=n {
            if u < threshold(i, n) && u >= threshold(i - 1, n) {
                return i - 1;
            }
        }
        0
    }

    fn next_state(self, ctx: &Ctx<'_>, inputs: &[Value], region: &[u64]) -> StageState {
        let u = read_real(inputs, 0);
        let pre = self.stored_or_initial(ctx, region);
        let check_upper = (!pre.prev_check_upper && u > pre.upper_threshold + self.h)
            || (pre.prev_check_upper && u >= pre.upper_threshold - self.h);
        let check_lower = (!pre.prev_check_lower && u > pre.lower_threshold + self.h)
            || (pre.prev_check_lower && u >= pre.lower_threshold - self.h);

        let condition = ctx.t() >= pre.t_next && (check_upper || !check_lower);
        let mut next = StageState {
            prev_check_upper: check_upper,
            prev_check_lower: check_lower,
            prev_when_condition: condition,
            ..pre
        };

        if !is_first_tick(region[PREV_T_WORD]) && condition && !pre.prev_when_condition {
            let y = self.stage_for_u(u);
            let n = self.safe_n();
            next.y = y;
            next.t_next = ctx.t() + self.hold_duration;
            next.upper_threshold = if y == n {
                threshold(n, n)
            } else {
                threshold(y + 1, n)
            };
            next.lower_threshold = if y == 0 {
                pre.lower_threshold
            } else {
                threshold(y, n)
            };
        }

        next
    }

    fn write_state(state: StageState, t: f64, region: &mut [u64]) {
        region[Y_WORD] = i64_word(state.y);
        region[T_NEXT_WORD] = state.t_next.to_bits();
        region[UPPER_THRESHOLD_WORD] = state.upper_threshold.to_bits();
        region[LOWER_THRESHOLD_WORD] = state.lower_threshold.to_bits();
        region[PREV_CHECK_UPPER_WORD] = bool_word(state.prev_check_upper);
        region[PREV_CHECK_LOWER_WORD] = bool_word(state.prev_check_lower);
        region[PREV_WHEN_CONDITION_WORD] = bool_word(state.prev_when_condition);
        region[PREV_T_WORD] = t.to_bits();
    }
}

impl Block for IntegerStage {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Integers.Stage",
            inputs: &[PortKind::Real],
            outputs: &[PortKind::Integer],
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
        STATE_LEN
    }

    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region[Y_WORD] = i64_word(self.pre_y_start);
        region[T_NEXT_WORD] = self.hold_duration.to_bits();
        region[UPPER_THRESHOLD_WORD] = 0.0f64.to_bits();
        region[LOWER_THRESHOLD_WORD] = 0.0f64.to_bits();
        region[PREV_CHECK_UPPER_WORD] = 0;
        region[PREV_CHECK_LOWER_WORD] = 1;
        region[PREV_WHEN_CONDITION_WORD] = 0;
        region[PREV_T_WORD] = PREV_T_UNSET;
    }

    fn emit_from_state(
        &self,
        ctx: &Ctx<'_>,
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        emit(0, Value::Integer(self.next_state(ctx, inputs, region).y));
    }

    fn update_state(&self, ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        let next = self.next_state(ctx, inputs, region);
        Self::write_state(next, ctx.t(), region);
    }
}
