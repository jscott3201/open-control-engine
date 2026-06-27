//! Matrix and sorting `CDL.Reals` algebraic blocks.
//!
//! These classes lower CDL vector and matrix connectors to flattened scalar ports at BUILD time.
//! Matrix inputs and `K[:,:]` parameters are row-major after lowering. `Sort` emits all sorted Real
//! values first, followed by the 1-based Integer indices returned by Modelica's vector sort.

use std::borrow::Cow;

use oce_model::{
    Value,
    determinism::{det_max, det_min},
};

use crate::{
    Block, BlockKind, BlockSignature, Ctx, MAX_RESOLVED_PORT_WIDTH, PortKind, PortShape,
    ResolvedBlockSignature, emit_real, read_real,
};

fn real_kinds(width: usize) -> Vec<PortKind> {
    PortShape::new(PortKind::Real, width).to_kinds()
}

fn real_integer_kinds(width: usize) -> Vec<PortKind> {
    let mut out = Vec::with_capacity(width.saturating_mul(2));
    PortShape::new(PortKind::Real, width).extend_kinds(&mut out);
    PortShape::new(PortKind::Integer, width).extend_kinds(&mut out);
    out
}

fn bounded_product_width(left: usize, right: usize) -> usize {
    left.checked_mul(right)
        .filter(|width| *width <= MAX_RESOLVED_PORT_WIDTH)
        .unwrap_or(0)
}

/// `CDL.Reals.MatrixGain` computes `y = K * u` for a Real matrix parameter `K`.
///
/// The block is stateless `[A]`. `K` is flattened row-major as `K_1_1`, `K_1_2`, ... while the
/// lowered size facts are `nout=size(K,1)` and `nin=size(K,2)`. With no parameters the source
/// default is the 2x2 identity matrix. Missing sparse entries default to that generalized identity;
/// source-generated CXF arrays expand every element. No method panics.
#[derive(Clone, Debug)]
pub struct MatrixGain {
    inputs: Vec<PortKind>,
    outputs: Vec<PortKind>,
    gains: Vec<f64>,
    nin: usize,
}

impl Default for MatrixGain {
    fn default() -> Self {
        Self::new(2, 2, Vec::new())
    }
}

impl MatrixGain {
    pub(crate) fn new(nin: usize, nout: usize, gains: Vec<f64>) -> Self {
        if nin > MAX_RESOLVED_PORT_WIDTH || nout > MAX_RESOLVED_PORT_WIDTH {
            return Self {
                inputs: Vec::new(),
                outputs: Vec::new(),
                gains: Vec::new(),
                nin: 0,
            };
        }
        let product = bounded_product_width(nin, nout);
        if product == 0 && nin.saturating_mul(nout) != 0 {
            return Self {
                inputs: Vec::new(),
                outputs: Vec::new(),
                gains: Vec::new(),
                nin: 0,
            };
        }

        let gains = (0..nout)
            .flat_map(|row| {
                let gains = &gains;
                (0..nin).map(move |col| {
                    gains
                        .get(row * nin + col)
                        .copied()
                        .unwrap_or(if row == col { 1.0 } else { 0.0 })
                })
            })
            .collect();
        Self {
            inputs: real_kinds(nin),
            outputs: real_kinds(nout),
            gains,
            nin,
        }
    }
}

impl Block for MatrixGain {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.MatrixGain",
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
        in_idx < self.inputs.len() && out_idx < self.outputs.len()
    }

    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        for row in 0..self.outputs.len() {
            let base = row * self.nin;
            let y = (0..self.nin).fold(0.0_f64, |acc, col| {
                acc + self.gains[base + col] * read_real(inputs, col)
            });
            emit_real(row, y, emit);
        }
    }
}

/// Reduction direction for row-wise or column-wise matrix reducers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatrixReductionAxis {
    /// Reduce each row independently.
    Rows,
    /// Reduce each column independently.
    Columns,
}

/// `CDL.Reals.MatrixMax` emits row-wise or column-wise maximum values.
///
/// The source requires `nRow >= 1` and `nCol >= 1`; load-time validation enforces that. Direct
/// invalid construction resolves to no ports rather than panicking. Matrix inputs are flattened
/// row-major as `u[row, col] -> row * nCol + col`.
#[derive(Clone, Debug)]
pub struct MatrixMax {
    inputs: Vec<PortKind>,
    outputs: Vec<PortKind>,
    n_row: usize,
    n_col: usize,
    axis: MatrixReductionAxis,
}

impl MatrixMax {
    pub(crate) fn new(n_row: usize, n_col: usize, axis: MatrixReductionAxis) -> Self {
        let width = bounded_product_width(n_row, n_col);
        if width == 0 {
            return Self {
                inputs: Vec::new(),
                outputs: Vec::new(),
                n_row: 0,
                n_col: 0,
                axis,
            };
        }
        let output_width = match axis {
            MatrixReductionAxis::Rows => n_row,
            MatrixReductionAxis::Columns => n_col,
        };
        Self {
            inputs: real_kinds(width),
            outputs: real_kinds(output_width),
            n_row,
            n_col,
            axis,
        }
    }
}

impl Block for MatrixMax {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.MatrixMax",
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
        matrix_reduction_feeds_through(in_idx, out_idx, self.n_row, self.n_col, self.axis)
    }

    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        for out_idx in 0..self.outputs.len() {
            let y = match self.axis {
                MatrixReductionAxis::Rows => (0..self.n_col)
                    .map(|col| read_real(inputs, out_idx * self.n_col + col))
                    .reduce(det_max)
                    .unwrap_or(f64::NEG_INFINITY),
                MatrixReductionAxis::Columns => (0..self.n_row)
                    .map(|row| read_real(inputs, row * self.n_col + out_idx))
                    .reduce(det_max)
                    .unwrap_or(f64::NEG_INFINITY),
            };
            emit_real(out_idx, y, emit);
        }
    }
}

/// `CDL.Reals.MatrixMin` emits row-wise or column-wise minimum values.
///
/// The source requires `nRow >= 1` and `nCol >= 1`; load-time validation enforces that. Direct
/// invalid construction resolves to no ports rather than panicking. Matrix inputs are flattened
/// row-major as `u[row, col] -> row * nCol + col`.
#[derive(Clone, Debug)]
pub struct MatrixMin {
    inputs: Vec<PortKind>,
    outputs: Vec<PortKind>,
    n_row: usize,
    n_col: usize,
    axis: MatrixReductionAxis,
}

impl MatrixMin {
    pub(crate) fn new(n_row: usize, n_col: usize, axis: MatrixReductionAxis) -> Self {
        let width = bounded_product_width(n_row, n_col);
        if width == 0 {
            return Self {
                inputs: Vec::new(),
                outputs: Vec::new(),
                n_row: 0,
                n_col: 0,
                axis,
            };
        }
        let output_width = match axis {
            MatrixReductionAxis::Rows => n_row,
            MatrixReductionAxis::Columns => n_col,
        };
        Self {
            inputs: real_kinds(width),
            outputs: real_kinds(output_width),
            n_row,
            n_col,
            axis,
        }
    }
}

impl Block for MatrixMin {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.MatrixMin",
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
        matrix_reduction_feeds_through(in_idx, out_idx, self.n_row, self.n_col, self.axis)
    }

    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        for out_idx in 0..self.outputs.len() {
            let y = match self.axis {
                MatrixReductionAxis::Rows => (0..self.n_col)
                    .map(|col| read_real(inputs, out_idx * self.n_col + col))
                    .reduce(det_min)
                    .unwrap_or(f64::INFINITY),
                MatrixReductionAxis::Columns => (0..self.n_row)
                    .map(|row| read_real(inputs, row * self.n_col + out_idx))
                    .reduce(det_min)
                    .unwrap_or(f64::INFINITY),
            };
            emit_real(out_idx, y, emit);
        }
    }
}

fn matrix_reduction_feeds_through(
    in_idx: usize,
    out_idx: usize,
    n_row: usize,
    n_col: usize,
    axis: MatrixReductionAxis,
) -> bool {
    if n_row == 0 || n_col == 0 || in_idx >= n_row * n_col {
        return false;
    }
    match axis {
        MatrixReductionAxis::Rows => out_idx < n_row && in_idx / n_col == out_idx,
        MatrixReductionAxis::Columns => out_idx < n_col && in_idx % n_col == out_idx,
    }
}

/// `CDL.Reals.Sort` sorts a Real vector and emits 1-based source indices.
///
/// The implementation mirrors `Modelica.Math.Vectors.sort` from Modelica 4.1.0: shellsort with
/// `gap = div(n, 2)` and strict `>`/`<` comparisons. Equal values are not swapped by a direct
/// comparison, and NaN comparisons are false. `yIdx` ports are emitted after all Real `y` ports.
#[derive(Clone, Debug, Default)]
pub struct Sort {
    inputs: Vec<PortKind>,
    outputs: Vec<PortKind>,
    ascending: bool,
}

impl Sort {
    pub(crate) fn new(nin: usize, ascending: bool) -> Self {
        if nin > MAX_RESOLVED_PORT_WIDTH {
            return Self {
                inputs: Vec::new(),
                outputs: Vec::new(),
                ascending,
            };
        }
        Self {
            inputs: real_kinds(nin),
            outputs: real_integer_kinds(nin),
            ascending,
        }
    }
}

impl Block for Sort {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.Sort",
            inputs: &[PortKind::Real],
            outputs: &[PortKind::Real, PortKind::Integer],
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
        in_idx < self.inputs.len() && out_idx < self.outputs.len()
    }

    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        let (sorted, indices) = modelica_vectors_sort(
            (0..self.inputs.len())
                .map(|idx| read_real(inputs, idx))
                .collect(),
            self.ascending,
        );
        for (idx, value) in sorted.into_iter().enumerate() {
            emit_real(idx, value, emit);
        }
        let offset = indices.len();
        for (idx, source_idx) in indices.into_iter().enumerate() {
            emit(offset + idx, Value::Integer(source_idx));
        }
    }
}

fn modelica_vectors_sort(mut sorted: Vec<f64>, ascending: bool) -> (Vec<f64>, Vec<i64>) {
    let mut indices = (1..=sorted.len())
        .map(|idx| i64::try_from(idx).unwrap_or(i64::MAX))
        .collect::<Vec<_>>();
    let n = sorted.len();
    let mut gap = n / 2;
    while gap > 0 {
        let mut i = gap;
        while i < n {
            let mut j = i - gap;
            let mut can_compare = true;
            while can_compare && should_swap(sorted[j], sorted[j + gap], ascending) {
                sorted.swap(j, j + gap);
                indices.swap(j, j + gap);
                match j.checked_sub(gap) {
                    Some(next) => j = next,
                    None => can_compare = false,
                }
            }
            i += 1;
        }
        gap /= 2;
    }
    (sorted, indices)
}

fn should_swap(left: f64, right: f64, ascending: bool) -> bool {
    if ascending {
        left > right
    } else {
        left < right
    }
}
