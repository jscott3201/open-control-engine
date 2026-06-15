//! `CDL.Reals` starter blocks (`03` §4.1) — all stateless `[A]`, full feedthrough on the math path.

use oce_model::Value;

use crate::{Block, BlockKind, BlockSignature, PortKind, Time, read_bool, read_real};

/// `CDL.Reals.Sources.Constant` — the only truly stateless source: `y = k` (`03` §4.1).
#[derive(Clone, Copy, Debug)]
pub struct Constant {
    pub(crate) k: f64,
}

impl Block for Constant {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.Sources.Constant",
            inputs: &[],
            outputs: &[PortKind::Real],
            stateful: false,
        };
        &SIG
    }
    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }
    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        false // no inputs — pure source root
    }
    fn step_algebraic(&self, _inputs: &[Value], _t: Time, emit: &mut dyn FnMut(usize, Value)) {
        emit(0, Value::Real(self.k));
    }
}

/// `CDL.Reals.Add` — `y = u1 + u2` (`03` §4.1).
#[derive(Clone, Copy, Debug, Default)]
pub struct Add;

impl Block for Add {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.Add",
            inputs: &[PortKind::Real, PortKind::Real],
            outputs: &[PortKind::Real],
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
        emit(0, Value::Real(read_real(inputs, 0) + read_real(inputs, 1)));
    }
}

/// `CDL.Reals.Subtract` — `y = u1 − u2` (`03` §4.1).
#[derive(Clone, Copy, Debug, Default)]
pub struct Subtract;

impl Block for Subtract {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.Subtract",
            inputs: &[PortKind::Real, PortKind::Real],
            outputs: &[PortKind::Real],
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
        emit(0, Value::Real(read_real(inputs, 0) - read_real(inputs, 1)));
    }
}

/// `CDL.Reals.MultiplyByParameter` — `y = k·u`, gain `k` (`03` §4.1).
#[derive(Clone, Copy, Debug)]
pub struct MultiplyByParameter {
    pub(crate) k: f64,
}

impl Block for MultiplyByParameter {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.MultiplyByParameter",
            inputs: &[PortKind::Real],
            outputs: &[PortKind::Real],
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
        emit(0, Value::Real(self.k * read_real(inputs, 0)));
    }
}

/// `CDL.Reals.Limiter` — clip `u` to `[u_min, u_max]` (an explicit block, not an attribute clamp;
/// `03` §4.1). The clamp is written manually (not `f64::clamp`) so an inverted `u_min > u_max`
/// parameter degrades to `u_max` deterministically rather than panicking.
#[derive(Clone, Copy, Debug)]
pub struct Limiter {
    pub(crate) u_min: f64,
    pub(crate) u_max: f64,
}

impl Block for Limiter {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.Limiter",
            inputs: &[PortKind::Real],
            outputs: &[PortKind::Real],
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
        let y = read_real(inputs, 0).max(self.u_min).min(self.u_max);
        emit(0, Value::Real(y));
    }
}

/// `CDL.Reals.Greater` — `y = u1 > u2` (`03` §4.1). M0 implements the **`h = 0` fast path**: a pure
/// combinational comparison (`[A]`, full feedthrough). Hysteresis (`h > 0`, the `[S]` variant) is
/// an M1 block; `feeds_through` stays `true` either way (a comparator is never a loop cut, R-REALS-1).
#[derive(Clone, Copy, Debug, Default)]
pub struct Greater;

impl Block for Greater {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.Greater",
            inputs: &[PortKind::Real, PortKind::Real],
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
            Value::Boolean(read_real(inputs, 0) > read_real(inputs, 1)),
        );
    }
}

/// `CDL.Reals.Switch` — `y = u1 if u2 else u3`, with the Boolean selector `u2` in the **middle**
/// (`03` §4.1). All three inputs feed through to `y`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Switch;

impl Block for Switch {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.Switch",
            inputs: &[PortKind::Real, PortKind::Boolean, PortKind::Real],
            outputs: &[PortKind::Real],
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
        let y = if read_bool(inputs, 1) {
            read_real(inputs, 0)
        } else {
            read_real(inputs, 2)
        };
        emit(0, Value::Real(y));
    }
}
