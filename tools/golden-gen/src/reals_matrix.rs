//! CDL.Reals matrix and sort goldens.
//!
//! References are re-derived from the source equations: `MatrixGain` matrix-vector multiply,
//! row/column min/max reductions, and Modelica 4.1.0 `Math.Vectors.sort` shellsort.

use crate::oracle::{Golden, InputSeries, Sample, ValueKind};

fn ticks(n: usize) -> Vec<f64> {
    (0..n).map(|k| (k as f64) * 60.0).collect()
}

fn r(x: f64) -> Sample {
    Sample::Real(x)
}

fn i(x: i64) -> Sample {
    Sample::Integer(x)
}

fn input_r(name: &'static str, values: impl IntoIterator<Item = f64>) -> InputSeries {
    InputSeries::new(name, ValueKind::Real, values.into_iter().map(r).collect())
}

/// Build CDL.Reals matrix and sort goldens.
pub fn goldens() -> Vec<Golden> {
    let mut out = Vec::new();

    // MatrixGain: y = K * u, K = [0.5, 2, -1; 3, 0, 0.25].
    {
        let time = ticks(4);
        let u1 = [4.0_f64, 2.0e16, -0.0, 4.0];
        let u2 = [-1.0_f64, -5.0e15, 5.0, f64::INFINITY];
        let u3 = [8.0_f64, -1.0, 0.0, 8.0];
        let y1: Vec<Sample> = u1
            .iter()
            .zip(u2)
            .zip(u3)
            .map(|((&a, b), c)| r(((0.0 + 0.5 * a) + 2.0 * b) + -c))
            .collect();
        let y2: Vec<Sample> = u1
            .iter()
            .zip(u2)
            .zip(u3)
            .map(|((&a, b), c)| r(((0.0 + 3.0 * a) + 0.0 * b) + 0.25 * c))
            .collect();
        let inputs = vec![input_r("u1", u1), input_r("u2", u2), input_r("u3", u3)];
        for (signal, samples) in [("y1", y1), ("y2", y2)] {
            out.push(
                Golden::new(
                    "CDL.Reals.MatrixGain",
                    signal,
                    ValueKind::Real,
                    time.clone(),
                    samples,
                    "params nout=2,nin=3,K=[0.5,2,-1;3,0,0.25]; rows include order-sensitive and zero-times-Inf probes",
                    "y[row] = sum_col K[row,col] * u[col] in row-major declaration order",
                )
                .with_inputs(inputs.clone()),
            );
        }
    }

    // MatrixMax row-wise and column-wise scenarios over a 2x3 matrix.
    {
        let (time, inputs) = matrix_inputs();
        let row_y1 = [3.0_f64, 4.0, 6.0, 8.0];
        let row_y2 = [5.0_f64, 7.0, 9.0, 8.0];
        for (signal, values) in [("y1", row_y1), ("y2", row_y2)] {
            out.push(
                Golden::new(
                    "CDL.Reals.MatrixMax",
                    signal,
                    ValueKind::Real,
                    time.clone(),
                    values.into_iter().map(r).collect(),
                    "params nRow=2,nCol=3,rowMax=true; u[2,3] flattened row-major",
                    "y[row] = max(u[row,:])",
                )
                .with_inputs(inputs.clone()),
            );
        }
        let col_y1 = [5.0_f64, 2.0, 9.0, 8.0];
        let col_y2 = [0.0_f64, 7.0, 0.0, 8.0];
        let col_y3 = [4.0_f64, 4.0, -0.0, 8.0];
        for (signal, values) in [("y1", col_y1), ("y2", col_y2), ("y3", col_y3)] {
            out.push(
                Golden::new(
                    "CDL.Reals.MatrixMax",
                    signal,
                    ValueKind::Real,
                    time.clone(),
                    values.into_iter().map(r).collect(),
                    "params nRow=2,nCol=3,rowMax=false; u[2,3] flattened row-major",
                    "y[col] = max(u[:,col])",
                )
                .with_scenario("columns")
                .with_inputs(inputs.clone()),
            );
        }
    }

    // MatrixMin row-wise and column-wise scenarios over the same 2x3 matrix.
    {
        let (time, inputs) = matrix_inputs();
        let row_y1 = [0.0_f64, -1.0, -3.0, 8.0];
        let row_y2 = [-0.0_f64, -2.0, -0.0, 8.0];
        for (signal, values) in [("y1", row_y1), ("y2", row_y2)] {
            out.push(
                Golden::new(
                    "CDL.Reals.MatrixMin",
                    signal,
                    ValueKind::Real,
                    time.clone(),
                    values.into_iter().map(r).collect(),
                    "params nRow=2,nCol=3,rowMin=true; u[2,3] flattened row-major",
                    "y[row] = min(u[row,:])",
                )
                .with_inputs(inputs.clone()),
            );
        }
        let col_y1 = [3.0_f64, -1.0, 6.0, 8.0];
        let col_y2 = [-0.0_f64, 2.0, -0.0, 8.0];
        let col_y3 = [2.0_f64, -2.0, -3.0, 8.0];
        for (signal, values) in [("y1", col_y1), ("y2", col_y2), ("y3", col_y3)] {
            out.push(
                Golden::new(
                    "CDL.Reals.MatrixMin",
                    signal,
                    ValueKind::Real,
                    time.clone(),
                    values.into_iter().map(r).collect(),
                    "params nRow=2,nCol=3,rowMin=false; u[2,3] flattened row-major",
                    "y[col] = min(u[:,col])",
                )
                .with_scenario("columns")
                .with_inputs(inputs.clone()),
            );
        }
    }

    // Sort ascending and descending. Indices are 1-based with respect to the original vector.
    {
        let time = ticks(4);
        let u1 = [3.0_f64, -0.0, 2.0, 3.0];
        let u2 = [1.0_f64, 4.0, 2.0, 3.0];
        let u3 = [2.0_f64, -0.0, -1.0, 3.0];
        let inputs = vec![input_r("u1", u1), input_r("u2", u2), input_r("u3", u3)];
        let rows = u1
            .iter()
            .zip(u2)
            .zip(u3)
            .map(|((&a, b), c)| modelica_vectors_sort(vec![a, b, c], true))
            .collect::<Vec<_>>();
        for output_idx in 0..3 {
            out.push(
                Golden::new(
                    "CDL.Reals.Sort",
                    match output_idx {
                        0 => "y1",
                        1 => "y2",
                        _ => "y3",
                    },
                    ValueKind::Real,
                    time.clone(),
                    rows.iter().map(|row| r(row.0[output_idx])).collect(),
                    "params nin=3,ascending=true; rows cover ordinary, signed-zero, negative, and equal values",
                    "Modelica.Math.Vectors.sort shellsort ascending; y contains sorted values",
                )
                .with_inputs(inputs.clone()),
            );
        }
        for output_idx in 0..3 {
            out.push(
                Golden::new(
                    "CDL.Reals.Sort",
                    match output_idx {
                        0 => "yIdx1",
                        1 => "yIdx2",
                        _ => "yIdx3",
                    },
                    ValueKind::Integer,
                    time.clone(),
                    rows.iter().map(|row| i(row.1[output_idx])).collect(),
                    "params nin=3,ascending=true; rows cover ordinary, signed-zero, negative, and equal values",
                    "Modelica.Math.Vectors.sort shellsort ascending; yIdx contains 1-based source indices",
                )
                .with_inputs(inputs.clone()),
            );
        }

        let descending_rows = u1
            .iter()
            .zip(u2)
            .zip(u3)
            .map(|((&a, b), c)| modelica_vectors_sort(vec![a, b, c], false))
            .collect::<Vec<_>>();
        for output_idx in 0..3 {
            out.push(
                Golden::new(
                    "CDL.Reals.Sort",
                    match output_idx {
                        0 => "y1",
                        1 => "y2",
                        _ => "y3",
                    },
                    ValueKind::Real,
                    time.clone(),
                    descending_rows
                        .iter()
                        .map(|row| r(row.0[output_idx]))
                        .collect(),
                    "params nin=3,ascending=false; same rows as ascending scenario",
                    "Modelica.Math.Vectors.sort shellsort descending; y contains sorted values",
                )
                .with_scenario("descending")
                .with_inputs(inputs.clone()),
            );
        }
        for output_idx in 0..3 {
            out.push(
                Golden::new(
                    "CDL.Reals.Sort",
                    match output_idx {
                        0 => "yIdx1",
                        1 => "yIdx2",
                        _ => "yIdx3",
                    },
                    ValueKind::Integer,
                    time.clone(),
                    descending_rows
                        .iter()
                        .map(|row| i(row.1[output_idx]))
                        .collect(),
                    "params nin=3,ascending=false; same rows as ascending scenario",
                    "Modelica.Math.Vectors.sort shellsort descending; yIdx contains 1-based source indices",
                )
                .with_scenario("descending")
                .with_inputs(inputs.clone()),
            );
        }

        let u1 = [4.0_f64, 1.0, -0.0, 9.0];
        let u2 = [1.0_f64, 3.0, 5.0, 9.0];
        let u3 = [3.0_f64, 2.0, -0.0, 1.0];
        let u4 = [2.0_f64, 4.0, -1.0, 9.0];
        let inputs = vec![
            input_r("u1", u1),
            input_r("u2", u2),
            input_r("u3", u3),
            input_r("u4", u4),
        ];
        let wide_rows = u1
            .iter()
            .zip(u2)
            .zip(u3)
            .zip(u4)
            .map(|(((&a, b), c), d)| modelica_vectors_sort(vec![a, b, c, d], true))
            .collect::<Vec<_>>();
        for output_idx in 0..4 {
            out.push(
                Golden::new(
                    "CDL.Reals.Sort",
                    match output_idx {
                        0 => "y1",
                        1 => "y2",
                        2 => "y3",
                        _ => "y4",
                    },
                    ValueKind::Real,
                    time.clone(),
                    wide_rows.iter().map(|row| r(row.0[output_idx])).collect(),
                    "params nin=4,ascending=true; exercises shellsort gap-halving path",
                    "Modelica.Math.Vectors.sort shellsort ascending; y contains sorted values",
                )
                .with_scenario("wide")
                .with_inputs(inputs.clone()),
            );
        }
        for output_idx in 0..4 {
            out.push(
                Golden::new(
                    "CDL.Reals.Sort",
                    match output_idx {
                        0 => "yIdx1",
                        1 => "yIdx2",
                        2 => "yIdx3",
                        _ => "yIdx4",
                    },
                    ValueKind::Integer,
                    time.clone(),
                    wide_rows.iter().map(|row| i(row.1[output_idx])).collect(),
                    "params nin=4,ascending=true; exercises shellsort gap-halving path",
                    "Modelica.Math.Vectors.sort shellsort ascending; yIdx contains 1-based source indices",
                )
                .with_scenario("wide")
                .with_inputs(inputs.clone()),
            );
        }
    }

    out
}

fn matrix_inputs() -> (Vec<f64>, Vec<InputSeries>) {
    let time = ticks(4);
    let u11 = [3.0_f64, -1.0, 6.0, 8.0];
    let u12 = [0.0_f64, 2.0, -0.0, 8.0];
    let u13 = [2.0_f64, 4.0, -3.0, 8.0];
    let u21 = [5.0_f64, 2.0, 9.0, 8.0];
    let u22 = [-0.0_f64, 7.0, 0.0, 8.0];
    let u23 = [4.0_f64, -2.0, -0.0, 8.0];
    (
        time,
        vec![
            input_r("u11", u11),
            input_r("u12", u12),
            input_r("u13", u13),
            input_r("u21", u21),
            input_r("u22", u22),
            input_r("u23", u23),
        ],
    )
}

fn modelica_vectors_sort(mut sorted: Vec<f64>, ascending: bool) -> (Vec<f64>, Vec<i64>) {
    let mut indices = (1..=sorted.len()).map(|idx| idx as i64).collect::<Vec<_>>();
    let n = sorted.len();
    let mut gap = n / 2;
    while gap > 0 {
        let mut idx = gap;
        while idx < n {
            let mut j = idx - gap;
            let mut can_compare = true;
            while can_compare && should_swap(sorted[j], sorted[j + gap], ascending) {
                sorted.swap(j, j + gap);
                indices.swap(j, j + gap);
                match j.checked_sub(gap) {
                    Some(next) => j = next,
                    None => can_compare = false,
                }
            }
            idx += 1;
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
