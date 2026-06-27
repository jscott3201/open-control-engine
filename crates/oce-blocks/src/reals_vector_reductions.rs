//! Vector-reduction `CDL.Reals` algebraic blocks.
//!
//! These classes flatten CDL vector inputs (`u[nin]`) to scalar Real ports at BUILD time. Empty
//! vectors are source-legal: Modelica reduction identities produce `0` for `MultiSum`, `+Inf` for
//! `MultiMin`, and `-Inf` for `MultiMax`.

use std::borrow::Cow;

use oce_model::{
    Value,
    determinism::{det_max, det_min},
};

use crate::{
    Block, BlockKind, BlockSignature, Ctx, PortKind, PortShape, ResolvedBlockSignature, emit_real,
    read_real,
};

/// `CDL.Reals.MultiSum` computes `y = sum(k[i] * u[i])` in declaration order.
///
/// The block is stateless `[A]`, feeds every resolved input through to `y`, uses sparse
/// source-default gains of `1`, and emits `0` for `nin=0`. IEEE overflow, infinities, and NaN
/// propagation are not clamped; emitted NaN bits are canonicalized and the block never panics.
#[derive(Clone, Debug, Default)]
pub struct MultiSum {
    pub(crate) inputs: Vec<PortKind>,
    pub(crate) gains: Vec<f64>,
}

impl MultiSum {
    pub(crate) fn new(gains: Vec<f64>) -> Self {
        Self {
            inputs: PortShape::new(PortKind::Real, gains.len()).to_kinds(),
            gains,
        }
    }
}

impl Block for MultiSum {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.MultiSum",
            inputs: &[],
            outputs: &[PortKind::Real],
            stateful: false,
        };
        &SIG
    }

    fn resolved_signature(&self) -> ResolvedBlockSignature<'_> {
        static OUTPUTS: [PortKind; 1] = [PortKind::Real];
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
            .fold(0.0_f64, |acc, (idx, gain)| {
                acc + gain * read_real(inputs, idx)
            });
        emit_real(0, y, emit);
    }
}

/// `CDL.Reals.MultiMin` computes `y = min(u)` over the resolved vector input.
///
/// The block is stateless `[A]`, feeds every resolved input through to `y`, and emits `+Inf` for
/// `nin=0`, matching the Modelica empty-reduction identity. Non-empty reductions use the same
/// deterministic min policy as scalar `CDL.Reals.Min`: a single NaN operand is dropped, all-NaN
/// inputs emit canonical NaN, and signed-zero results are stable. The block never panics.
#[derive(Clone, Debug, Default)]
pub struct MultiMin {
    pub(crate) inputs: Vec<PortKind>,
}

impl MultiMin {
    pub(crate) fn new(nin: usize) -> Self {
        Self {
            inputs: PortShape::new(PortKind::Real, nin).to_kinds(),
        }
    }
}

impl Block for MultiMin {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.MultiMin",
            inputs: &[],
            outputs: &[PortKind::Real],
            stateful: false,
        };
        &SIG
    }

    fn resolved_signature(&self) -> ResolvedBlockSignature<'_> {
        static OUTPUTS: [PortKind; 1] = [PortKind::Real];
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
        let y = (0..self.inputs.len())
            .map(|idx| read_real(inputs, idx))
            .reduce(det_min)
            .unwrap_or(f64::INFINITY);
        emit_real(0, y, emit);
    }
}

/// `CDL.Reals.MultiMax` computes `y = max(u)` over the resolved vector input.
///
/// The block is stateless `[A]`, feeds every resolved input through to `y`, and emits `-Inf` for
/// `nin=0`, matching the Modelica empty-reduction identity. Non-empty reductions use the same
/// deterministic max policy as scalar `CDL.Reals.Max`: a single NaN operand is dropped, all-NaN
/// inputs emit canonical NaN, and signed-zero results are stable. The block never panics.
#[derive(Clone, Debug, Default)]
pub struct MultiMax {
    pub(crate) inputs: Vec<PortKind>,
}

impl MultiMax {
    pub(crate) fn new(nin: usize) -> Self {
        Self {
            inputs: PortShape::new(PortKind::Real, nin).to_kinds(),
        }
    }
}

impl Block for MultiMax {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.MultiMax",
            inputs: &[],
            outputs: &[PortKind::Real],
            stateful: false,
        };
        &SIG
    }

    fn resolved_signature(&self) -> ResolvedBlockSignature<'_> {
        static OUTPUTS: [PortKind; 1] = [PortKind::Real];
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
        let y = (0..self.inputs.len())
            .map(|idx| read_real(inputs, idx))
            .reduce(det_max)
            .unwrap_or(f64::NEG_INFINITY);
        emit_real(0, y, emit);
    }
}
