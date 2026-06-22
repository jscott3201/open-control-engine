//! `CDL.Conversions` (`03` §4.4): legal explicit crossings between Boolean, Integer, and Real.
//! Real-to-Integer intentionally uses the sign-branched Buildings/CDL rule, so half values round
//! away from zero (`-2.5 -> -3`). Non-finite and out-of-range casts pin Rust's current saturating
//! `f64 as i64` behavior and join the deferred centralized non-finite diagnostics seam.

use oce_model::Value;

use crate::{
    Block, BlockKind, BlockSignature, Ctx, PortKind, emit_real, read_bool, read_int, read_real,
};

/// `CDL.Conversions.BooleanToInteger` — true→`integerTrue`, false→`integerFalse`.
/// Stateless `[A]`, full feedthrough.
#[derive(Clone, Copy, Debug)]
pub struct BooleanToInteger {
    pub(crate) integer_true: i64,
    pub(crate) integer_false: i64,
}

impl Default for BooleanToInteger {
    fn default() -> Self {
        Self {
            integer_true: 1,
            integer_false: 0,
        }
    }
}

impl Block for BooleanToInteger {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Conversions.BooleanToInteger",
            inputs: &[PortKind::Boolean],
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
        let y = if read_bool(inputs, 0) {
            self.integer_true
        } else {
            self.integer_false
        };
        emit(0, Value::Integer(y));
    }
}

/// `CDL.Conversions.BooleanToReal` — true→`realTrue`, false→`realFalse`.
/// Stateless `[A]`, full feedthrough.
#[derive(Clone, Copy, Debug)]
pub struct BooleanToReal {
    pub(crate) real_true: f64,
    pub(crate) real_false: f64,
}

impl Default for BooleanToReal {
    fn default() -> Self {
        Self {
            real_true: 1.0,
            real_false: 0.0,
        }
    }
}

impl Block for BooleanToReal {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Conversions.BooleanToReal",
            inputs: &[PortKind::Boolean],
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
        let y = if read_bool(inputs, 0) {
            self.real_true
        } else {
            self.real_false
        };
        emit_real(0, y, emit);
    }
}

/// `CDL.Conversions.IntegerToReal` — widen `i64` to `f64`. Stateless `[A]`, full feedthrough.
#[derive(Clone, Copy, Debug, Default)]
pub struct IntegerToReal;

impl Block for IntegerToReal {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Conversions.IntegerToReal",
            inputs: &[PortKind::Integer],
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
        emit_real(0, read_int(inputs, 0) as f64, emit);
    }
}

/// `CDL.Conversions.RealToInteger` — sign-branched round half away from zero.
///
/// Matches Buildings `Controls/OBC/CDL/Conversions/RealToInteger.mo`:
/// `integer(floor(u + 0.5))` for positive `u`, otherwise `integer(ceil(u - 0.5))`.
/// Stateless `[A]`, full feedthrough.
#[derive(Clone, Copy, Debug, Default)]
pub struct RealToInteger;

impl Block for RealToInteger {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Conversions.RealToInteger",
            inputs: &[PortKind::Real],
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
        let u = read_real(inputs, 0);
        let y = if u > 0.0 {
            (u + 0.5).floor()
        } else {
            (u - 0.5).ceil()
        };
        emit(0, Value::Integer(y as i64));
    }
}
