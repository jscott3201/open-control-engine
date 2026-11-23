//! Stateful `CDL.Integers` edge/count blocks (`03` §4.2).
//!
//! `Change` is transparent/feedthrough because its outputs compare the current input against the
//! prior value. `OnCounter` applies a trigger or reset event at the frame of the event, so both of
//! its inputs feed through to the count it emits; it is not a loop cut.

use oce_model::{ParamTable, Value};

use crate::settled::{fire, settled_input};
use crate::{
    Block, BlockKind, BlockSignature, Ctx, EventInputRole, EventSignal, PortKind, read_int,
};

const COUNT_WORD: usize = 0;
const PREV_TRIGGER_WORD: usize = 1;
const PREV_RESET_WORD: usize = 2;
const HAS_HISTORY_WORD: usize = 3;

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

/// `CDL.Integers.OnCounter` — count `trigger` events unless `reset` is high.
///
/// Stateful `[S]`, with both inputs feeding through to `y`. Per Buildings `OnCounter.mo`, an event
/// fires when either `trigger` or `reset` becomes true; if `reset` is high the count becomes
/// `y_start`, otherwise it increments. Reset therefore has priority on simultaneous events and
/// while held high across a trigger edge. The event applies at its own frame: the frame where an
/// input rises emits the new count, and its update stores it. A condition already true at frame 0
/// does not fire a Modelica `when` (initialisation leaves it inactive), so frame 0 emits `y_start`
/// and only captures the input levels as history. A counter whose output reaches its own `trigger`
/// or `reset` (`y -> GreaterEqualThreshold -> reset`, say) is an algebraic loop; a
/// `CDL.Logical.Pre` on that path breaks it and applies the event one frame later. Count overflow
/// wraps two's-complement like the integer arithmetic family. OpenModelica 1.25.1 agrees exactly
/// at every frame, with the trigger true from frame 0 and with it rising at frame 1, a `reset` held
/// across a trigger edge included (issue #670; `oce-conformance`'s
/// `open_modelica_event_triggers.rs`).
///
/// Both inputs take the settled-value rule (issue #800, `docs/design/pulse-event-inputs.md`):
/// when the event plan gives them `w` and `s` (a pulse or sample upstream), an input fires on
/// `(v and not w(k-1)) or (not v and s)` and the block stores `w` as its previous value, so a
/// `CDL.Logical.Change` true on consecutive frames counts each frame, as OpenModelica does
/// (`open_modelica_event_pulses.rs`); each firing iteration takes `reset`'s value in that
/// iteration. Without them `w = s = v` and the rule is the rising edge above. At the initial frame
/// the inputs go `i -> w -> s` from initialisation (decision 3 as revised): a sample instant at
/// the start fires, and so does a negated initialisation pulse; a level never does.
#[derive(Clone, Copy, Debug, Default)]
pub struct OnCounter {
    pub(crate) y_start: i64,
}

impl OnCounter {
    /// The count after this frame's events, or the stored count when the frame has none.
    ///
    /// Both inputs follow the settled-value rule (issue #800): the `when {trigger, reset}` fires
    /// once per iteration of the instant in which either input rises, and each firing assigns
    /// `if reset then y_start else pre(y) + 1` with `reset`'s value in that iteration. At the
    /// initial frame (no history yet, or the frame driver's initial frame) the two steps are
    /// `i -> w -> s` (decision 3 as revised), the first reading `reset`'s `w`.
    fn count_after_event(&self, ctx: &Ctx<'_>, inputs: &[Value], region: &[u64]) -> i64 {
        let mut count = word_i64(region[COUNT_WORD]);
        let initial = ctx.is_initial() || !word_bool(region[HAS_HISTORY_WORD]);
        let trigger = settled_input(inputs, 0, 2, 0);
        let reset = settled_input(inputs, 1, 2, 1);
        let on_trigger = fire(initial, word_bool(region[PREV_TRIGGER_WORD]), trigger);
        let on_reset = fire(initial, word_bool(region[PREV_RESET_WORD]), reset);
        if on_trigger.first || on_reset.first {
            // The first step's values: the frame values at a later frame, `w` at the initial one.
            let reset_first = if initial { reset.w } else { reset.v };
            count = if reset_first {
                self.y_start
            } else {
                count.wrapping_add(1)
            };
        }
        if on_trigger.second || on_reset.second {
            count = if reset.s {
                self.y_start
            } else {
                count.wrapping_add(1)
            };
        }
        count
    }
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

    fn feeds_through(&self, in_idx: usize, _out_idx: usize) -> bool {
        in_idx < 2
    }

    fn event_input_role(&self, in_idx: usize) -> Option<EventInputRole> {
        (in_idx < 2).then_some(EventInputRole::Condition)
    }

    fn settled_event_inputs(&self) -> &[usize] {
        &[0, 1]
    }

    fn state_len(&self) -> usize {
        4
    }

    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region[COUNT_WORD] = i64_word(self.y_start);
        region[PREV_TRIGGER_WORD] = 0;
        region[PREV_RESET_WORD] = 0;
        region[HAS_HISTORY_WORD] = 0;
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
            Value::Integer(self.count_after_event(ctx, inputs, region)),
        );
    }

    fn update_state(&self, ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        let trigger = settled_input(inputs, 0, 2, 0);
        let reset = settled_input(inputs, 1, 2, 1);
        region[COUNT_WORD] = i64_word(self.count_after_event(ctx, inputs, region));
        region[PREV_TRIGGER_WORD] = bool_word(trigger.w);
        region[PREV_RESET_WORD] = bool_word(reset.w);
        region[HAS_HISTORY_WORD] = 1;
    }
}

/// `CDL.Integers.Change` — compare the current integer input against its prior value.
///
/// Stateful `[S]`, but transparent: all three outputs (`y`, `up`, `down`) feed through from the
/// current `u` because each is a current-vs-prior comparison, not a loop cut. All three are pulses
/// to an event input's cone (issue #800, [`crate::EventSignal::Pulse`]).
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

    fn event_signal(&self) -> EventSignal {
        EventSignal::Pulse
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
