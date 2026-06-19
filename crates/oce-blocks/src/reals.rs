//! `CDL.Reals` starter blocks (`03` §4.1) — all stateless `[A]`, full feedthrough on the math path.
//! Non-finite policy is intentionally local for M2: `Min`/`Max` absorb a single NaN operand to match
//! `oce-expr`, while `Divide` and the `Line` slope/intercept arithmetic preserve IEEE NaN/±Inf
//! behavior. Centralized non-finite validation/diagnostics is deferred to the future seam.

use oce_model::Value;

use crate::{Block, BlockKind, BlockSignature, Ctx, PortKind, read_bool, read_real};

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
    fn step_algebraic(
        &self,
        _ctx: &Ctx<'_>,
        _inputs: &[Value],
        emit: &mut dyn FnMut(usize, Value),
    ) {
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
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
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
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit(0, Value::Real(read_real(inputs, 0) - read_real(inputs, 1)));
    }
}

/// `CDL.Reals.Multiply` — `y = u1·u2` (`03` §4.1). Stateless `[A]`, full feedthrough.
#[derive(Clone, Copy, Debug, Default)]
pub struct Multiply;

impl Block for Multiply {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.Multiply",
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
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit(0, Value::Real(read_real(inputs, 0) * read_real(inputs, 1)));
    }
}

/// `CDL.Reals.Divide` — `y = u1/u2` (`03` §4.1). Stateless `[A]`, full feedthrough; divide-by-zero
/// follows IEEE-754 `f64` semantics and never panics.
#[derive(Clone, Copy, Debug, Default)]
pub struct Divide;

impl Block for Divide {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.Divide",
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
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit(0, Value::Real(read_real(inputs, 0) / read_real(inputs, 1)));
    }
}

/// `CDL.Reals.AddParameter` — `y = u + p`, offset `p` (`03` §4.1). Stateless `[A]`, full
/// feedthrough.
#[derive(Clone, Copy, Debug)]
pub struct AddParameter {
    pub(crate) p: f64,
}

impl Block for AddParameter {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.AddParameter",
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
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit(0, Value::Real(read_real(inputs, 0) + self.p));
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
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit(0, Value::Real(self.k * read_real(inputs, 0)));
    }
}

/// `CDL.Reals.Abs` — `y = |u|` (`03` §4.1). Stateless `[A]`, full feedthrough.
#[derive(Clone, Copy, Debug, Default)]
pub struct Abs;

impl Block for Abs {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.Abs",
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
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit(0, Value::Real(read_real(inputs, 0).abs()));
    }
}

/// `CDL.Reals.Min` — `y = min(u1,u2)` (`03` §4.1). Stateless `[A]`, full feedthrough. NaN handling
/// follows the scalar expression evaluator: `f64::min` returns the non-NaN operand.
#[derive(Clone, Copy, Debug, Default)]
pub struct Min;

impl Block for Min {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.Min",
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
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit(
            0,
            Value::Real(read_real(inputs, 0).min(read_real(inputs, 1))),
        );
    }
}

/// `CDL.Reals.Max` — `y = max(u1,u2)` (`03` §4.1). Stateless `[A]`, full feedthrough. NaN handling
/// follows the scalar expression evaluator: `f64::max` returns the non-NaN operand.
#[derive(Clone, Copy, Debug, Default)]
pub struct Max;

impl Block for Max {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.Max",
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
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit(
            0,
            Value::Real(read_real(inputs, 0).max(read_real(inputs, 1))),
        );
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
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        let y = read_real(inputs, 0).max(self.u_min).min(self.u_max);
        emit(0, Value::Real(y));
    }
}

/// `CDL.Reals.Line` — line through `(x1,f1),(x2,f2)` evaluated at `u`, with `u` clamped into
/// `[x1,x2]` before interpolation (`03` §4.1). Stateless `[A]`, full feedthrough. A degenerate
/// `x1 == x2` follows the same `f64` formula and degrades to IEEE NaN/Inf rather than panicking.
/// A NaN `u` takes the lower-clamp path; an inverted `x1 > x2` domain collapses to `f2`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Line;

impl Block for Line {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.Line",
            inputs: &[
                PortKind::Real,
                PortKind::Real,
                PortKind::Real,
                PortKind::Real,
                PortKind::Real,
            ],
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
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        let x1 = read_real(inputs, 0);
        let f1 = read_real(inputs, 1);
        let x2 = read_real(inputs, 2);
        let f2 = read_real(inputs, 3);
        let u = read_real(inputs, 4);
        let x_lim = u.max(x1).min(x2);
        let b = (f2 - f1) / (x2 - x1);
        // M2-PR-G1 will choose the canonical point-slope vs slope-intercept form.
        let a = f2 - b * x2;
        emit(0, Value::Real(a + b * x_lim));
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
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
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
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        let y = if read_bool(inputs, 1) {
            read_real(inputs, 0)
        } else {
            read_real(inputs, 2)
        };
        emit(0, Value::Real(y));
    }
}
