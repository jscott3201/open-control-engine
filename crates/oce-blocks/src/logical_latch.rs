//! Stateful `CDL.Logical` edge and latch blocks.
//!
//! These blocks all feed through on their current Boolean inputs: their outputs compare current
//! inputs with prior state or apply same-tick set/clear decisions. They are stateful but not loop
//! cuts (`01` §4.3), unlike `Logical.Pre`.

use oce_model::{ParamTable, Value};

use crate::{Block, BlockKind, BlockSignature, Ctx, PortKind, read_bool};

fn bool_word(b: bool) -> u64 {
    u64::from(b)
}

fn word_bool(w: u64) -> bool {
    w != 0
}

/// `CDL.Logical.FallingEdge` — true exactly on a `true -> false` transition.
/// `[S]`, feedthrough `y <- {u}`, not a loop cut.
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
/// `[S]`, feedthrough `y <- {u}`, not a loop cut.
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
/// `[S]`, feedthrough `y <- {u, clr}`, not a loop cut.
#[derive(Clone, Copy, Debug, Default)]
pub struct Latch;

impl Latch {
    fn output(inputs: &[Value], region: &[u64]) -> bool {
        let u = read_bool(inputs, 0);
        let clr = read_bool(inputs, 1);
        let held = word_bool(region[0]);
        let prev_u = word_bool(region[1]);
        !clr && ((u && !prev_u) || held)
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

    fn state_len(&self) -> usize {
        2
    }

    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region[0] = 0;
        region[1] = 0;
    }

    fn emit_from_state(
        &self,
        _ctx: &Ctx<'_>,
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        emit(0, Value::Boolean(Self::output(inputs, region)));
    }

    fn update_state(&self, _ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        region[0] = bool_word(Self::output(inputs, region));
        region[1] = bool_word(read_bool(inputs, 0));
    }
}

/// `CDL.Logical.Toggle` — clear-dominant toggle on each rising edge of `u`.
/// `[S]`, feedthrough `y <- {u, clr}`, not a loop cut.
#[derive(Clone, Copy, Debug, Default)]
pub struct Toggle;

impl Toggle {
    fn output(inputs: &[Value], region: &[u64]) -> bool {
        let u = read_bool(inputs, 0);
        let clr = read_bool(inputs, 1);
        let held = word_bool(region[0]);
        let prev_u = word_bool(region[1]);
        !clr && if u && !prev_u { !held } else { held }
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

    fn state_len(&self) -> usize {
        2
    }

    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region[0] = 0;
        region[1] = 0;
    }

    fn emit_from_state(
        &self,
        _ctx: &Ctx<'_>,
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        emit(0, Value::Boolean(Self::output(inputs, region)));
    }

    fn update_state(&self, _ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        region[0] = bool_word(Self::output(inputs, region));
        region[1] = bool_word(read_bool(inputs, 0));
    }
}
