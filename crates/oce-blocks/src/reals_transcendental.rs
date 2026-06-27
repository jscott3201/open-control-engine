//! Scalar transcendental `CDL.Reals` blocks.
//!
//! The Buildings source files for these blocks are algebraic wrappers over `Modelica.Math`.
//! Open Control Engine evaluates them with the pinned pure-Rust `libm` crate, with default
//! features disabled in the workspace manifest, so outputs are independent of the host platform
//! C math library. Outputs still pass through the shared Real emitter, which canonicalizes NaN
//! payload/sign bits for deterministic snapshots and goldens.

use oce_model::Value;

use crate::{Block, BlockKind, BlockSignature, Ctx, PortKind, emit_real, read_real};

/// `CDL.Reals.Sin` computes `y = sin(u)`.
///
/// `u` is interpreted in radians; CDL's `displayUnit="deg"` metadata is non-computational and
/// does not alter the core value. The block is stateless `[A]`, fully feedthrough, and never
/// panics; non-finite values follow the deterministic `libm` result with canonicalized NaN bits.
#[derive(Clone, Copy, Debug, Default)]
pub struct Sin;

impl Block for Sin {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = unary_signature("CDL.Reals.Sin");
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }

    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        true
    }

    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit_real(0, libm::sin(read_real(inputs, 0)), emit);
    }
}

/// `CDL.Reals.Cos` computes `y = cos(u)`.
///
/// `u` is interpreted in radians; display-unit metadata is non-computational. The block is
/// stateless `[A]`, fully feedthrough, and uses deterministic `libm` math with canonicalized NaNs.
#[derive(Clone, Copy, Debug, Default)]
pub struct Cos;

impl Block for Cos {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = unary_signature("CDL.Reals.Cos");
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }

    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        true
    }

    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit_real(0, libm::cos(read_real(inputs, 0)), emit);
    }
}

/// `CDL.Reals.Tan` computes `y = tan(u)`.
///
/// `u` is interpreted in radians; display-unit metadata is non-computational. The block is
/// stateless `[A]`, fully feedthrough, and does not special-case poles or non-finite values beyond
/// deterministic `libm` evaluation and NaN canonicalization.
#[derive(Clone, Copy, Debug, Default)]
pub struct Tan;

impl Block for Tan {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = unary_signature("CDL.Reals.Tan");
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }

    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        true
    }

    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit_real(0, libm::tan(read_real(inputs, 0)), emit);
    }
}

/// `CDL.Reals.Asin` computes `y = asin(u)`.
///
/// The result is in radians. Inputs outside `[-1, 1]` degrade to NaN per `Modelica.Math.asin` /
/// IEEE behavior; emitted NaN bits are canonicalized and the block never panics.
#[derive(Clone, Copy, Debug, Default)]
pub struct Asin;

impl Block for Asin {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = unary_signature("CDL.Reals.Asin");
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }

    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        true
    }

    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit_real(0, libm::asin(read_real(inputs, 0)), emit);
    }
}

/// `CDL.Reals.Acos` computes `y = acos(u)`.
///
/// The result is in radians. Inputs outside `[-1, 1]` degrade to NaN per `Modelica.Math.acos` /
/// IEEE behavior; emitted NaN bits are canonicalized and the block never panics.
#[derive(Clone, Copy, Debug, Default)]
pub struct Acos;

impl Block for Acos {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = unary_signature("CDL.Reals.Acos");
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }

    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        true
    }

    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit_real(0, libm::acos(read_real(inputs, 0)), emit);
    }
}

/// `CDL.Reals.Atan` computes `y = atan(u)`.
///
/// The result is in radians in the `[-pi/2, pi/2]` branch used by `Modelica.Math.atan`. The block
/// is stateless `[A]`, fully feedthrough, and uses deterministic `libm` math with canonicalized
/// NaN outputs.
#[derive(Clone, Copy, Debug, Default)]
pub struct Atan;

impl Block for Atan {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = unary_signature("CDL.Reals.Atan");
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }

    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        true
    }

    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit_real(0, libm::atan(read_real(inputs, 0)), emit);
    }
}

/// `CDL.Reals.Atan2` computes `y = atan2(u1, u2)`.
///
/// The upstream block documents that `u1` and `u2` shall not both be zero. Open Control Engine
/// keeps execution panic-free; simultaneous-zero inputs emit a warning through [`Ctx`] and still
/// return the deterministic `libm` branch value. Valid inputs preserve the `[-pi, pi]` branch
/// semantics after NaN canonicalization.
#[derive(Clone, Copy, Debug, Default)]
pub struct Atan2;

impl Block for Atan2 {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.Atan2",
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

    fn step_algebraic(&self, ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        let u1 = read_real(inputs, 0);
        let u2 = read_real(inputs, 1);
        if u1 == 0.0 && u2 == 0.0 {
            ctx.warn(
                "CDL.Reals.Atan2",
                "Atan2: inputs u1 and u2 shall not both be zero; returning deterministic libm branch value",
            );
        }
        emit_real(0, libm::atan2(u1, u2), emit);
    }
}

/// `CDL.Reals.Exp` computes `y = exp(u)`.
///
/// Overflow, underflow, infinities, and NaNs follow deterministic `libm` behavior. The block is
/// stateless `[A]`, fully feedthrough, and never panics.
#[derive(Clone, Copy, Debug, Default)]
pub struct Exp;

impl Block for Exp {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = unary_signature("CDL.Reals.Exp");
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }

    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        true
    }

    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit_real(0, libm::exp(read_real(inputs, 0)), emit);
    }
}

/// `CDL.Reals.Log` computes `y = log(u)`.
///
/// Buildings documents `u > 0` as required and reports an error for zero or negative input.
/// Open Control Engine keeps execution panic-free while making invalid runtime values fail-visible:
/// inputs that are not greater than zero emit a warning through [`Ctx`], then return the
/// deterministic `libm` value (`-Inf` for zero, canonical NaN for negative and NaN inputs).
#[derive(Clone, Copy, Debug, Default)]
pub struct Log;

impl Block for Log {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = unary_signature("CDL.Reals.Log");
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }

    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        true
    }

    fn step_algebraic(&self, ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        let u = read_real(inputs, 0);
        warn_if_not_positive(ctx, "CDL.Reals.Log", "Log", u);
        emit_real(0, libm::log(u), emit);
    }
}

/// `CDL.Reals.Log10` computes `y = log10(u)`.
///
/// Buildings documents `u > 0` as required and reports an error for zero or negative input.
/// Invalid runtime values emit a warning through [`Ctx`], then return the deterministic `libm`
/// value (`-Inf` for zero, canonical NaN for negative and NaN inputs).
#[derive(Clone, Copy, Debug, Default)]
pub struct Log10;

impl Block for Log10 {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = unary_signature("CDL.Reals.Log10");
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }

    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        true
    }

    fn step_algebraic(&self, ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        let u = read_real(inputs, 0);
        warn_if_not_positive(ctx, "CDL.Reals.Log10", "Log10", u);
        emit_real(0, libm::log10(u), emit);
    }
}

const fn unary_signature(class_path: &'static str) -> BlockSignature {
    BlockSignature {
        class_path,
        inputs: &[PortKind::Real],
        outputs: &[PortKind::Real],
        stateful: false,
    }
}

fn warn_if_not_positive(ctx: &Ctx<'_>, class_path: &'static str, name: &str, u: f64) {
    if u > 0.0 {
        return;
    }
    ctx.warn(
        class_path,
        &format!("{name}: input u must be greater than zero; returning deterministic libm value"),
    );
}
