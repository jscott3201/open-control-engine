//! `CDL.Routing` Integer-family algebraic blocks.
//!
//! These classes lower CDL Integer vector and matrix connectors to flattened scalar ports at BUILD
//! time. Matrix outputs use row-major lowering: `y[row, col]` is emitted as `row * nin + col`,
//! with both source dimensions zero-based after lowering.

use std::borrow::Cow;

use oce_model::Value;

use crate::{
    Block, BlockKind, BlockSignature, Ctx, MAX_RESOLVED_PORT_WIDTH, PortKind, PortShape,
    ResolvedBlockSignature, read_int,
};

const EXTRACTOR_WARNING: &str = "The extract index is out of the range.";

fn integer_kinds(width: usize) -> Vec<PortKind> {
    PortShape::new(PortKind::Integer, width).to_kinds()
}

fn bounded_product_width(left: usize, right: usize) -> usize {
    left.checked_mul(right)
        .filter(|width| *width <= MAX_RESOLVED_PORT_WIDTH)
        .unwrap_or(0)
}

/// `CDL.Routing.IntegerExtractSignal` extracts a parameter-selected Integer vector.
///
/// The block is stateless `[A]`. Source validation requires every `extract[i]` to be in `1..=nin`
/// and treats out-of-range selectors as a hard initialization error. The hot path still degrades
/// deterministically for direct invalid construction by emitting `0` when no input exists or by
/// falling back to `u1` for an invalid selector. Duplicate selectors and `nout > nin` are valid
/// when all selectors are in range. No method panics.
#[derive(Clone, Debug, Default)]
pub struct IntegerExtractSignal {
    inputs: Vec<PortKind>,
    outputs: Vec<PortKind>,
    extract: Vec<usize>,
}

impl IntegerExtractSignal {
    pub(crate) fn new(nin: usize, nout: usize, extract: Vec<usize>) -> Self {
        let extract = (0..nout)
            .map(|idx| extract.get(idx).copied().unwrap_or(idx + 1))
            .collect();
        Self {
            inputs: integer_kinds(nin),
            outputs: integer_kinds(nout),
            extract,
        }
    }

    fn source_input_index(&self, out_idx: usize) -> Option<usize> {
        let one_based = *self.extract.get(out_idx)?;
        let idx = one_based.checked_sub(1)?;
        (idx < self.inputs.len()).then_some(idx)
    }
}

impl Block for IntegerExtractSignal {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Routing.IntegerExtractSignal",
            inputs: &[PortKind::Integer],
            outputs: &[PortKind::Integer],
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
                .map_or(0, |in_idx| read_int(inputs, in_idx));
            emit(out_idx, Value::Integer(y));
        }
    }
}

/// `CDL.Routing.IntegerExtractor` selects one Integer vector element by runtime integer index.
///
/// The block is stateless `[A]`. Source behavior warns, clamps `index` into `1..=nin`, and emits
/// the selected input without Real coercion. Because the selected Integer input is
/// runtime-dependent, feedthrough is conservative from `index` and every resolved `u[i]` to `y`.
/// Direct construction with `nin=0` emits `0` and warns instead of panicking; load-time validation
/// rejects that source-invalid shape.
#[derive(Clone, Debug, Default)]
pub struct IntegerExtractor {
    inputs: Vec<PortKind>,
    nin: usize,
}

impl IntegerExtractor {
    pub(crate) fn new(nin: usize) -> Self {
        let mut inputs = Vec::with_capacity(nin.saturating_add(1));
        inputs.push(PortKind::Integer);
        PortShape::new(PortKind::Integer, nin).extend_kinds(&mut inputs);
        Self { inputs, nin }
    }
}

impl Block for IntegerExtractor {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Routing.IntegerExtractor",
            inputs: &[PortKind::Integer, PortKind::Integer],
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

    fn step_algebraic(&self, ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        let index = read_int(inputs, 0);
        if self.nin == 0 {
            ctx.warn(self.signature().class_path, EXTRACTOR_WARNING);
            emit(0, Value::Integer(0));
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
        emit(0, Value::Integer(read_int(inputs, clamped)));
    }
}

/// `CDL.Routing.IntegerScalarReplicator` fills an Integer vector with one scalar input.
///
/// The block is stateless `[A]`, feeds the scalar input through to every resolved output, preserves
/// exact `i64` values without Real coercion, and emits no outputs when `nout=0`. No method panics.
#[derive(Clone, Debug, Default)]
pub struct IntegerScalarReplicator {
    outputs: Vec<PortKind>,
}

impl IntegerScalarReplicator {
    pub(crate) fn new(nout: usize) -> Self {
        Self {
            outputs: integer_kinds(nout),
        }
    }
}

impl Block for IntegerScalarReplicator {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Routing.IntegerScalarReplicator",
            inputs: &[PortKind::Integer],
            outputs: &[PortKind::Integer],
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
        let u = read_int(inputs, 0);
        for out_idx in 0..self.outputs.len() {
            emit(out_idx, Value::Integer(u));
        }
    }
}

/// `CDL.Routing.IntegerVectorFilter` emits Integer vector elements whose structural mask is true.
///
/// Source validation requires `nout == count_true(msk)`. Missing `msk_i` values default to `true`.
/// The block preserves input declaration order and feeds only the selected `u[i]` through to each
/// corresponding output. Direct invalid construction emits `0` for outputs without a selected
/// source input rather than panicking.
#[derive(Clone, Debug, Default)]
pub struct IntegerVectorFilter {
    inputs: Vec<PortKind>,
    outputs: Vec<PortKind>,
    selected: Vec<usize>,
}

impl IntegerVectorFilter {
    pub(crate) fn new(nin: usize, nout: usize, mask: Vec<bool>) -> Self {
        let selected = (0..nin)
            .filter(|idx| mask.get(*idx).copied().unwrap_or(true))
            .collect();
        Self {
            inputs: integer_kinds(nin),
            outputs: integer_kinds(nout),
            selected,
        }
    }
}

impl Block for IntegerVectorFilter {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Routing.IntegerVectorFilter",
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
                .map_or(0, |idx| read_int(inputs, *idx));
            emit(out_idx, Value::Integer(y));
        }
    }
}

/// `CDL.Routing.IntegerVectorReplicator` fills each output matrix row with the input vector.
///
/// The block is stateless `[A]`. `y[nout,nin]` is flattened row-major, so output port
/// `row * nin + col` depends on input `u[col]`. `nin=0` or `nout=0` resolves to no outputs.
/// Product widths above [`MAX_RESOLVED_PORT_WIDTH`] resolve to zero outputs for direct construction;
/// load-time validation rejects those parameters before scheduling.
#[derive(Clone, Debug, Default)]
pub struct IntegerVectorReplicator {
    inputs: Vec<PortKind>,
    outputs: Vec<PortKind>,
    nin: usize,
}

impl IntegerVectorReplicator {
    pub(crate) fn new(nin: usize, nout: usize) -> Self {
        let output_width = bounded_product_width(nin, nout);
        Self {
            inputs: integer_kinds(nin),
            outputs: integer_kinds(output_width),
            nin,
        }
    }
}

impl Block for IntegerVectorReplicator {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Routing.IntegerVectorReplicator",
            inputs: &[PortKind::Integer],
            outputs: &[PortKind::Integer],
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
            emit(
                out_idx,
                Value::Integer(read_int(inputs, out_idx % self.nin)),
            );
        }
    }
}
