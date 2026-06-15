//! `CDL.Logical` starter blocks (`03` §4.3): the combinational `And`/`Not` `[A]` and the canonical
//! loop-breaker `Pre` `[S]`.

use oce_model::{ParamTable, Value};

use crate::{Block, BlockKind, BlockSignature, PortKind, Time, read_bool};

/// `CDL.Logical.And` — `y = u1 ∧ u2` (`03` §4.3).
#[derive(Clone, Copy, Debug, Default)]
pub struct And;

impl Block for And {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Logical.And",
            inputs: &[PortKind::Boolean, PortKind::Boolean],
            outputs: &[PortKind::Boolean],
            stateful: false,
        };
        &SIG
    }
    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }
    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        true
    }
    fn step_algebraic(&self, inputs: &[Value], _t: Time, emit: &mut dyn FnMut(usize, Value)) {
        emit(
            0,
            Value::Boolean(read_bool(inputs, 0) && read_bool(inputs, 1)),
        );
    }
}

/// `CDL.Logical.Not` — `y = ¬u` (`03` §4.3).
#[derive(Clone, Copy, Debug, Default)]
pub struct Not;

impl Block for Not {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Logical.Not",
            inputs: &[PortKind::Boolean],
            outputs: &[PortKind::Boolean],
            stateful: false,
        };
        &SIG
    }
    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }
    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        true
    }
    fn step_algebraic(&self, inputs: &[Value], _t: Time, emit: &mut dyn FnMut(usize, Value)) {
        emit(0, Value::Boolean(!read_bool(inputs, 0)));
    }
}

/// `CDL.Logical.Pre` — `y = pre(u)`, the canonical one-evaluation Boolean delay and **the** CDL
/// algebraic-loop breaker (`03` §4.3): stateful, with `feeds_through == false` so it cuts the
/// direct-feedthrough DAG. Per the arena model the one-tick Boolean memory lives in one `[S]` state
/// word (`0`/`1`), not in the struct; it is seeded from the `pre` parameter (CDL has no `start`).
#[derive(Clone, Copy, Debug, Default)]
pub struct Pre {
    pub(crate) y_start: bool,
}

impl Block for Pre {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Logical.Pre",
            inputs: &[PortKind::Boolean],
            outputs: &[PortKind::Boolean],
            stateful: true,
        };
        &SIG
    }
    fn kind(&self) -> BlockKind {
        BlockKind::Stateful
    }
    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        false // THE loop cut: output is the prior input, not the current one
    }
    fn state_len(&self) -> usize {
        1 // one word holds the one-tick-delayed boolean
    }
    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region[0] = u64::from(self.y_start);
    }
    fn emit_from_state(
        &self,
        _inputs: &[Value],
        _t: Time,
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        emit(0, Value::Boolean(region[0] != 0)); // the prior input, held since last tick
    }
    fn update_state(&self, inputs: &[Value], _t: Time, region: &mut [u64]) {
        region[0] = u64::from(read_bool(inputs, 0)); // latch the current input for next tick
    }
}
