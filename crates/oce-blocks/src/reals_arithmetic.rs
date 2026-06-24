//! Scalar algebraic `CDL.Reals` blocks (`03` §4.1).
//!
//! Non-finite policy is intentionally local: `Min`/`Max` absorb a single NaN operand to match
//! `oce-expr`, arithmetic follows IEEE NaN/±Inf value behavior, and every Real output canonicalizes
//! NaN bits for cross-architecture determinism. Centralized non-finite validation/diagnostics is
//! deferred to the future seam.

use oce_model::{
    Value,
    determinism::{det_max, det_min},
};

use crate::{Block, BlockKind, BlockSignature, Ctx, PortKind, emit_real, read_bool, read_real};

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
        emit_real(0, self.k, emit);
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
        emit_real(0, read_real(inputs, 0) + read_real(inputs, 1), emit);
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
        emit_real(0, read_real(inputs, 0) - read_real(inputs, 1), emit);
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
        emit_real(0, read_real(inputs, 0) * read_real(inputs, 1), emit);
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
        emit_real(0, read_real(inputs, 0) / read_real(inputs, 1), emit);
    }
}

/// `CDL.Reals.Sqrt` computes `y = sqrt(u)`.
///
/// The block is stateless `[A]`, fully feedthrough, and uses IEEE-754 `f64::sqrt`, which is
/// correctly rounded and bit-portable for the determinism matrix. Domain violations such as
/// `u < 0` produce NaN; the engine canonicalizes emitted NaN bits and never panics.
#[derive(Clone, Copy, Debug, Default)]
pub struct Sqrt;

impl Block for Sqrt {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.Sqrt",
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
        emit_real(0, read_real(inputs, 0).sqrt(), emit);
    }
}

/// `CDL.Reals.Average` computes `y = 0.5 * (u1 + u2)`.
///
/// The block is stateless `[A]`, fully feedthrough, has no parameters, and follows ordinary
/// IEEE-754 addition and multiplication semantics. Overflow, infinities, and NaN propagation are
/// not clamped; emitted NaN bits are canonicalized and the block never panics.
#[derive(Clone, Copy, Debug, Default)]
pub struct Average;

impl Block for Average {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.Average",
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
        emit_real(0, 0.5 * (read_real(inputs, 0) + read_real(inputs, 1)), emit);
    }
}

/// `CDL.Reals.Modulo` computes Modelica `mod(u1, u2)`.
///
/// The block is stateless `[A]`, fully feedthrough, and intentionally uses the floored Modelica
/// definition `u1 - floor(u1 / u2) * u2`, so the result sign follows the divisor rather than Rust's
/// truncated-remainder `%` behavior. A zero divisor degrades through IEEE NaN and never panics.
#[derive(Clone, Copy, Debug, Default)]
pub struct Modulo;

impl Block for Modulo {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.Modulo",
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
        let u1 = read_real(inputs, 0);
        let u2 = read_real(inputs, 1);
        emit_real(0, u1 - (u1 / u2).floor() * u2, emit);
    }
}

/// `CDL.Reals.Round` rounds `u` to decimal digit count `n`.
///
/// The block is stateless `[A]`, fully feedthrough, and implements the Buildings formula:
/// `fac = 10^n`; positive `u` uses `floor(u * fac + 0.5) / fac`, and all other values use
/// `ceil(u * fac - 0.5) / fac`, so exact zero yields `-0.0` through `ceil(-0.5)`. The factor is
/// built with deterministic repeated `* 10.0` operations, then reciprocated for negative `n`; no
/// `powi`/`powf` is used. Non-finite values and overflow follow IEEE behavior, emitted NaN bits
/// are canonicalized, and the block never panics.
#[derive(Clone, Copy, Debug)]
pub struct Round {
    pub(crate) n: i64,
}

impl Block for Round {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.Round",
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
        let u = read_real(inputs, 0);
        let fac = decimal_factor(self.n);
        let y = if u > 0.0 {
            (u * fac + 0.5).floor() / fac
        } else {
            (u * fac - 0.5).ceil() / fac
        };
        emit_real(0, y, emit);
    }
}

fn decimal_factor(n: i64) -> f64 {
    let mut factor = 1.0_f64;
    let mut remaining = n.unsigned_abs();
    while remaining > 0 && factor.is_finite() {
        factor *= 10.0;
        remaining -= 1;
    }
    if n >= 0 { factor } else { 1.0 / factor }
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
        emit_real(0, read_real(inputs, 0) + self.p, emit);
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
        emit_real(0, self.k * read_real(inputs, 0), emit);
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
        emit_real(0, read_real(inputs, 0).abs(), emit);
    }
}

/// `CDL.Reals.Min` — `y = min(u1,u2)` (`03` §4.1). Stateless `[A]`, full feedthrough. NaN handling
/// follows the scalar expression evaluator: a single NaN operand is dropped.
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
        emit_real(0, det_min(read_real(inputs, 0), read_real(inputs, 1)), emit);
    }
}

/// `CDL.Reals.Max` — `y = max(u1,u2)` (`03` §4.1). Stateless `[A]`, full feedthrough. NaN handling
/// follows the scalar expression evaluator: a single NaN operand is dropped.
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
        emit_real(0, det_max(read_real(inputs, 0), read_real(inputs, 1)), emit);
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
        let y = det_min(det_max(read_real(inputs, 0), self.u_min), self.u_max);
        emit_real(0, y, emit);
    }
}

/// `CDL.Reals.Line` — line through `(x1,f1),(x2,f2)` evaluated at `u`, with the input limit gates
/// from Buildings `Line.mo` (`03` §4.1). `limitBelow` clamps `u` to be at least `x1`, `limitAbove`
/// clamps `u` to be at most `x2`, both enabled clamps to `[x1,x2]`, and both disabled extrapolates
/// freely. Stateless `[A]`, full feedthrough. A degenerate `x1 == x2` follows the same `f64` formula
/// and degrades to IEEE NaN/Inf rather than panicking. A NaN `u` follows the deterministic min/max
/// policy in whichever limit gates are enabled; an inverted `x1 > x2` domain follows the same IEEE
/// formula without adding a runtime assertion.
#[derive(Clone, Copy, Debug)]
pub struct Line {
    pub(crate) limit_below: bool,
    pub(crate) limit_above: bool,
}

impl Default for Line {
    fn default() -> Self {
        Self {
            limit_below: true,
            limit_above: true,
        }
    }
}

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
        let x_lim = match (self.limit_below, self.limit_above) {
            (true, true) => det_min(det_max(u, x1), x2),
            (true, false) => det_max(u, x1),
            (false, true) => det_min(u, x2),
            (false, false) => u,
        };
        let b = (f2 - f1) / (x2 - x1);
        // The canonical point-slope vs slope-intercept form is still a compatibility decision.
        let a = f2 - b * x2;
        emit_real(0, a + b * x_lim, emit);
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
        emit_real(0, y, emit);
    }
}
