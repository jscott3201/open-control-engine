//! Stateful `CDL.Logical` edge and latch blocks.
//!
//! These blocks all feed through on their current Boolean inputs: their outputs compare current
//! inputs with prior state or apply same-tick set/clear decisions. They are stateful but not loop
//! cuts (`01` §4.3), unlike `Logical.Pre`.

use oce_model::{ParamTable, Value};

use crate::settled::{Settled, settled_input};
use crate::{Block, BlockKind, BlockSignature, Ctx, EventSignal, PortKind, read_bool};

/// One iteration of `Latch` or `Toggle`'s `when` within a frame's event iteration: whether `u`
/// rose, whether `clr` rose, and the two inputs' values in that iteration.
#[derive(Clone, Copy)]
struct Iteration {
    u_rose: bool,
    clr_rose: bool,
    u: bool,
    clr: bool,
}

/// The two iterations of a frame (issue #800, design note `docs/design/pulse-event-inputs.md`).
///
/// At a later frame the inputs go `before -> v -> s`: `u`'s before is its stored `w(k-1)`;
/// `clr`'s is not stored and is taken as false, so `clr` true at the frame rises in the first
/// iteration. That is exact unless the output can be true while `clr` was already true before
/// the frame, which needs `clr`'s cone true between instants and false at one (a negated
/// sample): a registered limit. At the initial frame `v` is the cone at initialisation `i` and
/// the inputs go `i -> w -> s` (decision 3 as revised), with `clr`'s `i` known.
fn iterations(u: Settled, clr: Settled, prev_u: bool, initial: bool) -> [Iteration; 2] {
    let (u0, u1, clr0, clr1) = if initial {
        (u.v, u.w, clr.v, clr.w)
    } else {
        (prev_u, u.v, false, clr.v)
    };
    [
        Iteration {
            u_rose: u1 && !u0,
            clr_rose: clr1 && !clr0,
            u: u1,
            clr: clr1,
        },
        Iteration {
            u_rose: u.s && !u1,
            clr_rose: clr.s && !clr1,
            u: u.s,
            clr: clr.s,
        },
    ]
}

/// `u` and `clr` with their settled values, when the graph appended them.
fn latch_inputs(inputs: &[Value]) -> (Settled, Settled) {
    (
        settled_input(inputs, 0, 2, 0),
        settled_input(inputs, 1, 2, 1),
    )
}

fn bool_word(b: bool) -> u64 {
    u64::from(b)
}

fn word_bool(w: u64) -> bool {
    w != 0
}

/// `CDL.Logical.FallingEdge` — true exactly on a `true -> false` transition.
/// `[S]`, feedthrough `y <- {u}`, not a loop cut. Its output is a pulse to an event input's cone
/// (issue #800, [`crate::EventSignal::Pulse`]).
#[derive(Clone, Copy, Debug, Default)]
pub struct FallingEdge {
    pub(crate) pre_u_start: bool,
}

impl Block for FallingEdge {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Logical.FallingEdge",
            inputs: &[PortKind::Boolean],
            outputs: &[PortKind::Boolean],
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
        in_idx == 0 && out_idx == 0
    }

    fn state_len(&self) -> usize {
        1
    }

    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region[0] = bool_word(self.pre_u_start);
    }

    fn emit_from_state(
        &self,
        _ctx: &Ctx<'_>,
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        let u = read_bool(inputs, 0);
        emit(0, Value::Boolean(word_bool(region[0]) && !u));
    }

    fn update_state(&self, _ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        region[0] = bool_word(read_bool(inputs, 0));
    }
}

/// `CDL.Logical.Change` — true when the current Boolean input differs from the prior input.
/// `[S]`, feedthrough `y <- {u}`, not a loop cut. Its output is a pulse to an event input's cone
/// (issue #800, [`crate::EventSignal::Pulse`]): Modelica's `change(u)` is true only at the
/// instant, so an event input it feeds fires on every frame it is true.
#[derive(Clone, Copy, Debug, Default)]
pub struct LogicalChange {
    pub(crate) pre_u_start: bool,
}

impl Block for LogicalChange {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Logical.Change",
            inputs: &[PortKind::Boolean],
            outputs: &[PortKind::Boolean],
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
        in_idx == 0 && out_idx == 0
    }

    fn state_len(&self) -> usize {
        1
    }

    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region[0] = bool_word(self.pre_u_start);
    }

    fn emit_from_state(
        &self,
        _ctx: &Ctx<'_>,
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        emit(
            0,
            Value::Boolean(read_bool(inputs, 0) != word_bool(region[0])),
        );
    }

    fn update_state(&self, _ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        region[0] = bool_word(read_bool(inputs, 0));
    }
}

/// `CDL.Logical.Latch` — clear-dominant set/reset latch.
/// `[S]`, feedthrough `y <- {u, clr}`, not a loop cut. Both inputs take the settled-value rule of
/// issue #800 (`docs/design/pulse-event-inputs.md`), and `when {clr, u} then y = not clr and u`
/// is evaluated per iteration of the frame's event iteration: in the first (before -> during) if
/// `u` rose from its stored `w(k-1)` or `clr` is true, with the frame values; in the second
/// (during -> settled) if either rose, with the settled values. The second state word holds `u`'s
/// `w`, which only the event test reads; `clr`'s previous value is not stored, which is exact
/// unless `clr`'s cone holds a negated sample (true between instants and false at one), where a
/// `clr` already true before the frame is read as rising. At the initial frame (decision 3 as
/// revised) the initial equation reads the cones at initialisation `i` (pulses at their frame
/// value, samples false), then the `when` runs on the two steps `i -> w -> s`.
/// Where no pulse or sample is upstream (`i = v = s`) this is the clear-dominant rising-edge
/// latch, bit for bit.
#[derive(Clone, Copy, Debug, Default)]
pub struct Latch;

impl Latch {
    fn output(ctx: &Ctx<'_>, inputs: &[Value], region: &[u64]) -> bool {
        let (u, clr) = latch_inputs(inputs);
        let held = word_bool(region[0]);
        let initial = ctx.is_initial();
        // At the initial frame `v` is the cone at initialisation `i`: the initial equation first.
        let start = if initial {
            !clr.v && (u.v || held)
        } else {
            held
        };
        iterations(u, clr, word_bool(region[1]), initial)
            .iter()
            .fold(start, |y, it| {
                if it.u_rose || it.clr_rose {
                    !it.clr && it.u
                } else {
                    y
                }
            })
    }
}

impl Block for Latch {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Logical.Latch",
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

    fn settled_event_inputs(&self) -> &[usize] {
        &[0, 1]
    }

    fn state_len(&self) -> usize {
        2
    }

    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region[0] = 0;
        region[1] = 0;
    }

    fn emit_from_state(
        &self,
        ctx: &Ctx<'_>,
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        emit(0, Value::Boolean(Self::output(ctx, inputs, region)));
    }

    fn update_state(&self, ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        region[0] = bool_word(Self::output(ctx, inputs, region));
        region[1] = bool_word(settled_input(inputs, 0, 2, 0).w);
    }
}

/// `CDL.Logical.Toggle` — clear-dominant toggle on each event of `u`.
/// `[S]`, feedthrough `y <- {u, clr}`, not a loop cut. Both inputs take the settled-value rule of
/// issue #800 as `Latch`'s do, and `Toggle.mo`'s `when`/`elsewhen` chain is evaluated per
/// iteration: `u` rising with `clr` false toggles, otherwise `clr` rising clears, otherwise the
/// output holds (a falling `u` keeps `pre(y)`). The second word holds `u`'s `w`; `clr`'s previous
/// value is read as `Latch` reads it. The initial frame applies `when initial() then y = if clr
/// then false else u` to the cones at initialisation `i`, then the chain on the two steps
/// `i -> w -> s` (decision 3 as revised).
#[derive(Clone, Copy, Debug, Default)]
pub struct Toggle;

impl Toggle {
    fn output(ctx: &Ctx<'_>, inputs: &[Value], region: &[u64]) -> bool {
        let (u, clr) = latch_inputs(inputs);
        let held = word_bool(region[0]);
        let initial = ctx.is_initial();
        // At the initial frame `v` is the cone at initialisation `i`: `when initial()` first.
        let start = if initial {
            !clr.v && if u.v { !held } else { held }
        } else {
            held
        };
        iterations(u, clr, word_bool(region[1]), initial)
            .iter()
            .fold(start, |y, it| {
                if !it.clr && it.u_rose {
                    !y
                } else if it.clr_rose {
                    false
                } else {
                    y
                }
            })
    }
}

impl Block for Toggle {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Logical.Toggle",
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

    fn settled_event_inputs(&self) -> &[usize] {
        &[0, 1]
    }

    fn state_len(&self) -> usize {
        2
    }

    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region[0] = 0;
        region[1] = 0;
    }

    fn emit_from_state(
        &self,
        ctx: &Ctx<'_>,
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        emit(0, Value::Boolean(Self::output(ctx, inputs, region)));
    }

    fn update_state(&self, ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        region[0] = bool_word(Self::output(ctx, inputs, region));
        region[1] = bool_word(settled_input(inputs, 0, 2, 0).w);
    }
}
