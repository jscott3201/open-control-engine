//! `CDL.Integers` exact scalar and vector-reduction `[A]` blocks (`03` §4.2). Integer arithmetic is
//! deliberately two's-complement wrapping (`wrapping_*`) so debug and release builds are
//! bit-identical and panic-free on input-derived overflow. Checked/saturating arithmetic plus
//! diagnostics is deferred to a future validation/diagnostic seam if the spec tightens this policy.

use std::borrow::Cow;

use oce_model::Value;

use crate::{
    Block, BlockKind, BlockSignature, Ctx, PortKind, PortShape, ResolvedBlockSignature, read_bool,
    read_int,
};

fn emit_int(value: i64, emit: &mut dyn FnMut(usize, Value)) {
    emit(0, Value::Integer(value));
}

fn emit_bool(value: bool, emit: &mut dyn FnMut(usize, Value)) {
    emit(0, Value::Boolean(value));
}

/// `CDL.Integers.Sources.Constant` — `y = k`. Stateless `[A]` source, no feedthrough edges.
#[derive(Clone, Copy, Debug, Default)]
pub struct IntegerConstant {
    pub(crate) k: i64,
}

impl Block for IntegerConstant {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Integers.Sources.Constant",
            inputs: &[],
            outputs: &[PortKind::Integer],
            stateful: false,
        };
        &SIG
    }
    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }
    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        false
    }
    fn step_algebraic(
        &self,
        _ctx: &Ctx<'_>,
        _inputs: &[Value],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        emit_int(self.k, emit);
    }
}

/// `CDL.Integers.Abs` — `y = abs(u)`, wrapping at `i64::MIN`. Stateless `[A]`, full feedthrough.
#[derive(Clone, Copy, Debug, Default)]
pub struct IntegerAbs;

impl Block for IntegerAbs {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Integers.Abs",
            inputs: &[PortKind::Integer],
            outputs: &[PortKind::Integer],
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
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit_int(read_int(inputs, 0).wrapping_abs(), emit);
    }
}

/// `CDL.Integers.Add` — `y = u1 + u2` with wrapping overflow. Stateless `[A]`, full feedthrough.
#[derive(Clone, Copy, Debug, Default)]
pub struct IntegerAdd;

impl Block for IntegerAdd {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Integers.Add",
            inputs: &[PortKind::Integer, PortKind::Integer],
            outputs: &[PortKind::Integer],
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
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit_int(read_int(inputs, 0).wrapping_add(read_int(inputs, 1)), emit);
    }
}

/// `CDL.Integers.Subtract` — `y = u1 - u2` with wrapping overflow. Stateless `[A]`, full feedthrough.
#[derive(Clone, Copy, Debug, Default)]
pub struct IntegerSubtract;

impl Block for IntegerSubtract {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Integers.Subtract",
            inputs: &[PortKind::Integer, PortKind::Integer],
            outputs: &[PortKind::Integer],
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
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit_int(read_int(inputs, 0).wrapping_sub(read_int(inputs, 1)), emit);
    }
}

/// `CDL.Integers.Multiply` — `y = u1 * u2` with wrapping overflow. Stateless `[A]`, full feedthrough.
#[derive(Clone, Copy, Debug, Default)]
pub struct IntegerMultiply;

impl Block for IntegerMultiply {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Integers.Multiply",
            inputs: &[PortKind::Integer, PortKind::Integer],
            outputs: &[PortKind::Integer],
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
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit_int(read_int(inputs, 0).wrapping_mul(read_int(inputs, 1)), emit);
    }
}

/// `CDL.Integers.AddParameter` — `y = u + p` with wrapping overflow. Stateless `[A]`, full feedthrough.
#[derive(Clone, Copy, Debug, Default)]
pub struct IntegerAddParameter {
    pub(crate) p: i64,
}

impl Block for IntegerAddParameter {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Integers.AddParameter",
            inputs: &[PortKind::Integer],
            outputs: &[PortKind::Integer],
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
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit_int(read_int(inputs, 0).wrapping_add(self.p), emit);
    }
}

/// `CDL.Integers.Max` — `y = max(u1, u2)`. Stateless `[A]`, full feedthrough.
#[derive(Clone, Copy, Debug, Default)]
pub struct IntegerMax;

impl Block for IntegerMax {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Integers.Max",
            inputs: &[PortKind::Integer, PortKind::Integer],
            outputs: &[PortKind::Integer],
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
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit_int(read_int(inputs, 0).max(read_int(inputs, 1)), emit);
    }
}

/// `CDL.Integers.Min` — `y = min(u1, u2)`. Stateless `[A]`, full feedthrough.
#[derive(Clone, Copy, Debug, Default)]
pub struct IntegerMin;

impl Block for IntegerMin {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Integers.Min",
            inputs: &[PortKind::Integer, PortKind::Integer],
            outputs: &[PortKind::Integer],
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
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit_int(read_int(inputs, 0).min(read_int(inputs, 1)), emit);
    }
}

/// `CDL.Integers.MultiSum` — `y = sum(k[i] * u[i])`; empty input yields `0`.
#[derive(Clone, Debug, Default)]
pub struct IntegerMultiSum {
    pub(crate) inputs: Vec<PortKind>,
    pub(crate) gains: Vec<i64>,
}

impl IntegerMultiSum {
    pub(crate) fn new(gains: Vec<i64>) -> Self {
        Self {
            inputs: PortShape::new(PortKind::Integer, gains.len()).to_kinds(),
            gains,
        }
    }
}

impl Block for IntegerMultiSum {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Integers.MultiSum",
            inputs: &[],
            outputs: &[PortKind::Integer],
            stateful: false,
        };
        &SIG
    }
    fn resolved_signature(&self) -> ResolvedBlockSignature<'_> {
        static OUTPUTS: [PortKind; 1] = [PortKind::Integer];
        ResolvedBlockSignature {
            class_path: self.signature().class_path,
            inputs: Cow::Borrowed(self.inputs.as_slice()),
            outputs: Cow::Borrowed(&OUTPUTS),
        }
    }
    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }
    fn feeds_through(&self, in_idx: usize, out_idx: usize) -> bool {
        out_idx == 0 && in_idx < self.inputs.len()
    }
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        let y = self
            .gains
            .iter()
            .enumerate()
            .fold(0_i64, |acc, (idx, gain)| {
                acc.wrapping_add(gain.wrapping_mul(read_int(inputs, idx)))
            });
        emit_int(y, emit);
    }
}

/// `CDL.Integers.Switch` — `y = if u2 then u1 else u3` (`u2` is Boolean selector).
/// Stateless `[A]`, full feedthrough.
#[derive(Clone, Copy, Debug, Default)]
pub struct IntegerSwitch;

impl Block for IntegerSwitch {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Integers.Switch",
            inputs: &[PortKind::Integer, PortKind::Boolean, PortKind::Integer],
            outputs: &[PortKind::Integer],
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
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        let y = if read_bool(inputs, 1) {
            read_int(inputs, 0)
        } else {
            read_int(inputs, 2)
        };
        emit_int(y, emit);
    }
}

/// `CDL.Integers.Greater` — `y = u1 > u2`. Stateless `[A]`, full feedthrough.
#[derive(Clone, Copy, Debug, Default)]
pub struct IntegerGreater;

impl Block for IntegerGreater {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Integers.Greater",
            inputs: &[PortKind::Integer, PortKind::Integer],
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
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit_bool(read_int(inputs, 0) > read_int(inputs, 1), emit);
    }
}

/// `CDL.Integers.Equal` — `y = u1 == u2`. Stateless `[A]`, full feedthrough.
#[derive(Clone, Copy, Debug, Default)]
pub struct IntegerEqual;

impl Block for IntegerEqual {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Integers.Equal",
            inputs: &[PortKind::Integer, PortKind::Integer],
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
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit_bool(read_int(inputs, 0) == read_int(inputs, 1), emit);
    }
}

/// `CDL.Integers.GreaterThreshold` — `y = u > t`. Stateless `[A]`, full feedthrough.
#[derive(Clone, Copy, Debug, Default)]
pub struct IntegerGreaterThreshold {
    pub(crate) t: i64,
}

impl Block for IntegerGreaterThreshold {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Integers.GreaterThreshold",
            inputs: &[PortKind::Integer],
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
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit_bool(read_int(inputs, 0) > self.t, emit);
    }
}

/// `CDL.Integers.GreaterEqual` — `y = u1 >= u2`. Stateless `[A]`, full feedthrough.
#[derive(Clone, Copy, Debug, Default)]
pub struct IntegerGreaterEqual;

impl Block for IntegerGreaterEqual {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Integers.GreaterEqual",
            inputs: &[PortKind::Integer, PortKind::Integer],
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
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit_bool(read_int(inputs, 0) >= read_int(inputs, 1), emit);
    }
}

/// `CDL.Integers.GreaterEqualThreshold` — `y = u >= t`. Stateless `[A]`, full feedthrough.
#[derive(Clone, Copy, Debug, Default)]
pub struct IntegerGreaterEqualThreshold {
    pub(crate) t: i64,
}

impl Block for IntegerGreaterEqualThreshold {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Integers.GreaterEqualThreshold",
            inputs: &[PortKind::Integer],
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
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit_bool(read_int(inputs, 0) >= self.t, emit);
    }
}

/// `CDL.Integers.Less` — `y = u1 < u2`. Stateless `[A]`, full feedthrough.
#[derive(Clone, Copy, Debug, Default)]
pub struct IntegerLess;

impl Block for IntegerLess {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Integers.Less",
            inputs: &[PortKind::Integer, PortKind::Integer],
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
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit_bool(read_int(inputs, 0) < read_int(inputs, 1), emit);
    }
}

/// `CDL.Integers.LessThreshold` — `y = u < t`. Stateless `[A]`, full feedthrough.
#[derive(Clone, Copy, Debug, Default)]
pub struct IntegerLessThreshold {
    pub(crate) t: i64,
}

impl Block for IntegerLessThreshold {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Integers.LessThreshold",
            inputs: &[PortKind::Integer],
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
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit_bool(read_int(inputs, 0) < self.t, emit);
    }
}

/// `CDL.Integers.LessEqual` — `y = u1 <= u2`. Stateless `[A]`, full feedthrough.
#[derive(Clone, Copy, Debug, Default)]
pub struct IntegerLessEqual;

impl Block for IntegerLessEqual {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Integers.LessEqual",
            inputs: &[PortKind::Integer, PortKind::Integer],
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
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit_bool(read_int(inputs, 0) <= read_int(inputs, 1), emit);
    }
}

/// `CDL.Integers.LessEqualThreshold` — `y = u <= t`. Stateless `[A]`, full feedthrough.
#[derive(Clone, Copy, Debug, Default)]
pub struct IntegerLessEqualThreshold {
    pub(crate) t: i64,
}

impl Block for IntegerLessEqualThreshold {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Integers.LessEqualThreshold",
            inputs: &[PortKind::Integer],
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
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit_bool(read_int(inputs, 0) <= self.t, emit);
    }
}
