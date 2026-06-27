//! `CDL.Routing` Real-family algebraic blocks.
//!
//! These classes lower CDL vector and matrix connectors to flattened scalar ports at BUILD time.
//! Matrix outputs use row-major lowering: `y[row, col]` is emitted as `row * nin + col`, with both
//! source dimensions zero-based after lowering. Selector and mask parameters are structural because
//! they change the resolved feedthrough map.

use std::borrow::Cow;

use oce_model::Value;

use crate::{
    Block, BlockKind, BlockSignature, Ctx, MAX_RESOLVED_PORT_WIDTH, PortKind, PortShape,
    ResolvedBlockSignature, emit_real, read_int, read_real,
};

const EXTRACTOR_WARNING: &str = "The extract index is out of the range.";

fn real_kinds(width: usize) -> Vec<PortKind> {
    PortShape::new(PortKind::Real, width).to_kinds()
}

fn bounded_product_width(left: usize, right: usize) -> usize {
    left.checked_mul(right)
        .filter(|width| *width <= MAX_RESOLVED_PORT_WIDTH)
        .unwrap_or(0)
}

/// `CDL.Routing.RealExtractSignal` extracts a parameter-selected Real vector.
///
/// The block is stateless `[A]`. Source validation requires every `extract[i]` to be in `1..=nin`
/// and treats out-of-range selectors as a hard initialization error. The hot path still degrades
/// deterministically for direct, invalid construction by emitting `0.0` when no input exists or by
/// falling back to `u1` for an invalid selector. Duplicate selectors and `nout > nin` are valid
/// when all selectors are in range. No method panics.
#[derive(Clone, Debug, Default)]
pub struct RealExtractSignal {
    inputs: Vec<PortKind>,
    outputs: Vec<PortKind>,
    extract: Vec<usize>,
}

impl RealExtractSignal {
    pub(crate) fn new(nin: usize, nout: usize, extract: Vec<usize>) -> Self {
        let outputs = real_kinds(nout);
        let extract = (0..nout)
            .map(|idx| extract.get(idx).copied().unwrap_or(idx + 1))
            .collect();
        Self {
            inputs: real_kinds(nin),
            outputs,
            extract,
        }
    }

    fn source_input_index(&self, out_idx: usize) -> Option<usize> {
        let one_based = *self.extract.get(out_idx)?;
        let idx = one_based.checked_sub(1)?;
        (idx < self.inputs.len()).then_some(idx)
    }
}

impl Block for RealExtractSignal {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Routing.RealExtractSignal",
            inputs: &[PortKind::Real],
            outputs: &[PortKind::Real],
            stateful: false,
        };
        &SIG
    }

    fn resolved_signature(&self) -> ResolvedBlockSignature<'_> {
        ResolvedBlockSignature {
            class_path: self.signature().class_path,
            inputs: Cow::Borrowed(self.inputs.as_slice()),
            outputs: Cow::Borrowed(self.outputs.as_slice()),
        }
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }

    fn feeds_through(&self, in_idx: usize, out_idx: usize) -> bool {
        self.source_input_index(out_idx) == Some(in_idx)
    }

    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        for out_idx in 0..self.outputs.len() {
            let y = self
                .source_input_index(out_idx)
                .or_else(|| (!self.inputs.is_empty()).then_some(0))
                .map_or(0.0, |in_idx| read_real(inputs, in_idx));
            emit_real(out_idx, y, emit);
        }
    }
}

/// `CDL.Routing.RealExtractor` selects one Real vector element by runtime integer index.
///
/// The block is stateless `[A]`. Source behavior warns, clamps `index` into `1..=nin`, and emits
/// the selected input. Because the selected Real input is runtime-dependent, feedthrough is
/// conservative from `index` and every resolved `u[i]` to `y`. Direct construction with `nin=0`
/// emits `0.0` and warns instead of panicking.
#[derive(Clone, Debug, Default)]
pub struct RealExtractor {
    inputs: Vec<PortKind>,
    nin: usize,
}

impl RealExtractor {
    pub(crate) fn new(nin: usize) -> Self {
        let mut inputs = Vec::with_capacity(nin.saturating_add(1));
        inputs.push(PortKind::Integer);
        PortShape::new(PortKind::Real, nin).extend_kinds(&mut inputs);
        Self { inputs, nin }
    }
}

impl Block for RealExtractor {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Routing.RealExtractor",
            inputs: &[PortKind::Integer, PortKind::Real],
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

    fn step_algebraic(&self, ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        let index = read_int(inputs, 0);
        if self.nin == 0 {
            ctx.warn(self.signature().class_path, EXTRACTOR_WARNING);
            emit_real(0, 0.0, emit);
            return;
        }

        let clamped = if index < 1 {
            1
        } else {
            usize::try_from(index)
                .ok()
                .filter(|idx| *idx <= self.nin)
                .unwrap_or(self.nin)
        };
        if usize::try_from(index).ok() != Some(clamped) {
            ctx.warn(self.signature().class_path, EXTRACTOR_WARNING);
        }
        emit_real(0, read_real(inputs, clamped), emit);
    }
}

/// `CDL.Routing.RealScalarReplicator` fills a Real vector with one scalar input.
///
/// The block is stateless `[A]`, feeds the scalar input through to every resolved output, and emits
/// no outputs when `nout=0`. IEEE non-finite values pass through with deterministic NaN
/// canonicalization. No method panics.
#[derive(Clone, Debug, Default)]
pub struct RealScalarReplicator {
    outputs: Vec<PortKind>,
}

impl RealScalarReplicator {
    pub(crate) fn new(nout: usize) -> Self {
        Self {
            outputs: real_kinds(nout),
        }
    }
}

impl Block for RealScalarReplicator {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Routing.RealScalarReplicator",
            inputs: &[PortKind::Real],
            outputs: &[PortKind::Real],
            stateful: false,
        };
        &SIG
    }

    fn resolved_signature(&self) -> ResolvedBlockSignature<'_> {
        ResolvedBlockSignature {
            class_path: self.signature().class_path,
            inputs: Cow::Borrowed(self.signature().inputs),
            outputs: Cow::Borrowed(self.outputs.as_slice()),
        }
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }

    fn feeds_through(&self, in_idx: usize, out_idx: usize) -> bool {
        in_idx == 0 && out_idx < self.outputs.len()
    }

    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        let u = read_real(inputs, 0);
        for out_idx in 0..self.outputs.len() {
            emit_real(out_idx, u, emit);
        }
    }
}

/// `CDL.Routing.RealVectorFilter` emits Real vector elements whose structural mask is true.
///
/// Source validation requires `nout == count_true(msk)`. Missing `msk_i` values default to `true`.
/// The block preserves input declaration order and feeds only the selected `u[i]` through to each
/// corresponding output. Direct invalid construction emits zeros for outputs without a selected
/// source input rather than panicking.
#[derive(Clone, Debug, Default)]
pub struct RealVectorFilter {
    inputs: Vec<PortKind>,
    outputs: Vec<PortKind>,
    selected: Vec<usize>,
}

impl RealVectorFilter {
    pub(crate) fn new(nin: usize, nout: usize, mask: Vec<bool>) -> Self {
        let selected = (0..nin)
            .filter(|idx| mask.get(*idx).copied().unwrap_or(true))
            .collect();
        Self {
            inputs: real_kinds(nin),
            outputs: real_kinds(nout),
            selected,
        }
    }
}

impl Block for RealVectorFilter {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Routing.RealVectorFilter",
            inputs: &[],
            outputs: &[],
            stateful: false,
        };
        &SIG
    }

    fn resolved_signature(&self) -> ResolvedBlockSignature<'_> {
        ResolvedBlockSignature {
            class_path: self.signature().class_path,
            inputs: Cow::Borrowed(self.inputs.as_slice()),
            outputs: Cow::Borrowed(self.outputs.as_slice()),
        }
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }

    fn feeds_through(&self, in_idx: usize, out_idx: usize) -> bool {
        self.selected.get(out_idx).copied() == Some(in_idx)
    }

    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        for out_idx in 0..self.outputs.len() {
            let y = self
                .selected
                .get(out_idx)
                .filter(|idx| **idx < self.inputs.len())
                .map_or(0.0, |idx| read_real(inputs, *idx));
            emit_real(out_idx, y, emit);
        }
    }
}

/// `CDL.Routing.RealVectorReplicator` fills each output matrix row with the input vector.
///
/// The block is stateless `[A]`. `y[nout,nin]` is flattened row-major, so output port
/// `row * nin + col` depends on input `u[col]`. `nin=0` or `nout=0` resolves to no outputs.
/// Product widths above [`MAX_RESOLVED_PORT_WIDTH`] resolve to zero outputs for direct construction;
/// load-time validation rejects those parameters before scheduling.
#[derive(Clone, Debug, Default)]
pub struct RealVectorReplicator {
    inputs: Vec<PortKind>,
    outputs: Vec<PortKind>,
    nin: usize,
}

impl RealVectorReplicator {
    pub(crate) fn new(nin: usize, nout: usize) -> Self {
        let output_width = bounded_product_width(nin, nout);
        Self {
            inputs: real_kinds(nin),
            outputs: real_kinds(output_width),
            nin,
        }
    }
}

impl Block for RealVectorReplicator {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Routing.RealVectorReplicator",
            inputs: &[PortKind::Real],
            outputs: &[PortKind::Real],
            stateful: false,
        };
        &SIG
    }

    fn resolved_signature(&self) -> ResolvedBlockSignature<'_> {
        ResolvedBlockSignature {
            class_path: self.signature().class_path,
            inputs: Cow::Borrowed(self.inputs.as_slice()),
            outputs: Cow::Borrowed(self.outputs.as_slice()),
        }
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }

    fn feeds_through(&self, in_idx: usize, out_idx: usize) -> bool {
        self.nin != 0 && out_idx < self.outputs.len() && out_idx % self.nin == in_idx
    }

    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        if self.nin == 0 {
            return;
        }
        for out_idx in 0..self.outputs.len() {
            emit_real(out_idx, read_real(inputs, out_idx % self.nin), emit);
        }
    }
}
