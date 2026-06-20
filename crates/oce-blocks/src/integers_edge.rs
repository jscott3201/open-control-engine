//! Stateful `CDL.Integers` edge/count blocks (`03` §4.2).
//!
//! `Change` is transparent/feedthrough because its outputs compare the current input against the
//! prior value. `OnCounter` is the A4 integer loop cut: its output is the prior accumulated count;
//! trigger/reset edges are applied only in the state update pass.

use oce_model::{ParamTable, Value};

use crate::{Block, BlockKind, BlockSignature, Ctx, PortKind, read_bool, read_int};

const COUNT_WORD: usize = 0;
const PREV_TRIGGER_WORD: usize = 1;
const PREV_RESET_WORD: usize = 2;

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

/// `CDL.Integers.OnCounter` — count rising edges on `trigger`, reset on a rising `reset`.
///
/// Stateful `[S]` loop cut: `y` is purely the prior accumulated count, so neither `trigger` nor
/// `reset` feed through to the same-tick output. If trigger and reset rise on the same tick, reset
/// has priority. Count overflow wraps two's-complement like the A3 integer arithmetic family.
#[derive(Clone, Copy, Debug, Default)]
pub struct OnCounter {
    pub(crate) y_start: i64,
}

impl Block for OnCounter {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Integers.OnCounter",
            inputs: &[PortKind::Boolean, PortKind::Boolean],
            outputs: &[PortKind::Integer],
            stateful: true,
        };
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Stateful
    }

    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        false
    }

    fn state_len(&self) -> usize {
        3
    }

    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region[COUNT_WORD] = i64_word(self.y_start);
        region[PREV_TRIGGER_WORD] = 0;
        region[PREV_RESET_WORD] = 0;
    }

    fn emit_from_state(
        &self,
        _ctx: &Ctx<'_>,
        _inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        emit(0, Value::Integer(word_i64(region[COUNT_WORD])));
    }

    fn update_state(&self, _ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        let trigger = read_bool(inputs, 0);
        let reset = read_bool(inputs, 1);
        let prev_trigger = word_bool(region[PREV_TRIGGER_WORD]);
        let prev_reset = word_bool(region[PREV_RESET_WORD]);
        let trigger_rising = trigger && !prev_trigger;
        let reset_rising = reset && !prev_reset;

        let count = word_i64(region[COUNT_WORD]);
        let next_count = if reset_rising {
            self.y_start
        } else if trigger_rising {
            count.wrapping_add(1)
        } else {
            count
        };

        region[COUNT_WORD] = i64_word(next_count);
        region[PREV_TRIGGER_WORD] = bool_word(trigger);
        region[PREV_RESET_WORD] = bool_word(reset);
    }
}

/// `CDL.Integers.Change` — compare the current integer input against its prior value.
///
/// Stateful `[S]`, but transparent: all three outputs (`y`, `up`, `down`) feed through from the
/// current `u` because each is a current-vs-prior comparison, not a loop cut.
#[derive(Clone, Copy, Debug, Default)]
pub struct IntegerChange {
    pub(crate) pre_u_start: i64,
}

impl Block for IntegerChange {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Integers.Change",
            inputs: &[PortKind::Integer],
            outputs: &[PortKind::Boolean, PortKind::Boolean, PortKind::Boolean],
            stateful: true,
        };
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Stateful
    }

    fn feeds_through(&self, in_idx: usize, out_idx: usize) -> bool {
        in_idx == 0 && out_idx < 3
    }

    fn state_len(&self) -> usize {
        1
    }

    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region[0] = i64_word(self.pre_u_start);
    }

    fn emit_from_state(
        &self,
        _ctx: &Ctx<'_>,
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        let u = read_int(inputs, 0);
        let prev = word_i64(region[0]);
        emit(0, Value::Boolean(u != prev));
        emit(1, Value::Boolean(u > prev));
        emit(2, Value::Boolean(u < prev));
    }

    fn update_state(&self, _ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        region[0] = i64_word(read_int(inputs, 0));
    }
}
