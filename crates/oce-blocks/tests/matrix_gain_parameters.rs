//! How `CDL.Reals.MatrixGain` reads its parameters (GitHub #643, issue #773), through the public
//! registry.
//!
//! The constructor reads `K` in one pass over the parameter table. Each table below is built
//! into a block, and its shape and outputs are compared bit for bit (`f64::to_bits`) with a
//! reference written here: the shape is `K`'s own (issue #773; upstream's `nout = size(K, 1)` and
//! `nin = size(K, 2)` are protected), the largest row and column among the cells `K_<row>_<col>`
//! whose indices are canonical decimals from 1, or the default 2x2 when no cell is bound, a
//! product past `MAX_RESOLVED_PORT_WIDTH` giving an empty block; then each cell by building its
//! name and taking its first binding (a Real as itself, an Integer promoted, anything else the
//! identity's 1 or 0), and `y = K * u` folded from 0 in column order. `nin` and `nout` bound
//! beside `K` (in every shape the old reading took them in) are read by nothing. The `K` tables
//! mix hostile spellings, duplicates, row or column 0, missing cells and values of every kind at
//! their edges. A few hand cases with independently written results close the file.

use std::sync::Arc;

use oce_blocks::{Block, Ctx, MAX_RESOLVED_PORT_WIDTH, NoopDiagnostics, lookup};
use oce_model::determinism::canonicalize_real;
use oce_model::{ParamTable, Value};

fn find<'a>(params: &'a ParamTable, name: &str) -> Option<&'a Value> {
    params
        .values
        .iter()
        .find(|(n, _)| n.as_ref() == name)
        .map(|(_, v)| v)
}

/// A canonical decimal index from 1 (`[1-9][0-9]*` that fits a `usize`).
fn canonical_index(text: &str) -> Option<usize> {
    let first = *text.as_bytes().first()?;
    ((b'1'..=b'9').contains(&first) && text.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}

/// `K`'s shape, `(nout, nin)`: its cells' largest row and column, or 2x2 with no cell.
fn shape_of_k(params: &ParamTable) -> (usize, usize) {
    let cells: Vec<(usize, usize)> = params
        .values
        .iter()
        .filter_map(|(name, _)| {
            let (row, col) = name.strip_prefix("K_")?.split_once('_')?;
            Some((canonical_index(row)?, canonical_index(col)?))
        })
        .collect();
    if cells.is_empty() {
        return (2, 2);
    }
    (
        cells.iter().map(|cell| cell.0).max().unwrap_or(0),
        cells.iter().map(|cell| cell.1).max().unwrap_or(0),
    )
}

/// The block the per-cell reading builds: `(nin, nout, gains)` row-major.
fn reference(params: &ParamTable) -> (usize, usize, Vec<f64>) {
    let (nout, nin) = shape_of_k(params);
    if nout
        .checked_mul(nin)
        .is_none_or(|cells| cells > MAX_RESOLVED_PORT_WIDTH)
    {
        return (0, 0, Vec::new());
    }
    let mut gains = Vec::with_capacity(nout * nin);
    for row in 1..=nout {
        for col in 1..=nin {
            gains.push(match find(params, &format!("K_{row}_{col}")) {
                Some(Value::Real(x)) => *x,
                Some(Value::Integer(n)) => *n as f64,
                _ if row == col => 1.0,
                _ => 0.0,
            });
        }
    }
    (nin, nout, gains)
}

/// `y = K * u`, each output's NaN canonicalized as every Real output is.
fn reference_outputs(nin: usize, nout: usize, gains: &[f64], u: &[f64]) -> Vec<u64> {
    (0..nout)
        .map(|row| {
            let y = (0..nin).fold(0.0_f64, |acc, col| acc + gains[row * nin + col] * u[col]);
            canonicalize_real(y).to_bits()
        })
        .collect()
}

fn block_outputs(block: &dyn Block, u: &[f64]) -> Vec<u64> {
    let diag = NoopDiagnostics;
    let cx = Ctx::new(0.0, &diag);
    let inputs: Vec<Value> = u.iter().copied().map(Value::Real).collect();
    let mut out = Vec::new();
    block.step_algebraic(&cx, &inputs, &mut |idx, value| {
        assert_eq!(idx, out.len(), "outputs are emitted in order");
        match value {
            Value::Real(y) => out.push(y.to_bits()),
            other => panic!("a Real output, got {other:?}"),
        }
    });
    out
}

/// Input vectors of width `nin`, one per tick: counting, signed zeros, non-finite and huge.
fn ticks(nin: usize) -> Vec<Vec<f64>> {
    let edges = [
        -0.0,
        0.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::MIN_POSITIVE / 4.0,
        1.0e308,
        -3.5,
    ];
    vec![
        (0..nin).map(|i| i as f64 + 1.0).collect(),
        vec![-0.0; nin],
        (0..nin).map(|i| edges[i % edges.len()]).collect(),
        (0..nin).map(|i| edges[(i + 3) % edges.len()]).collect(),
    ]
}

fn assert_matches_reference(params: &ParamTable) {
    let entry = lookup("CDL.Reals.MatrixGain").expect("registered");
    let block = (entry.make)(params);
    let (nin, nout, gains) = reference(params);
    let signature = block.resolved_signature();
    assert_eq!(
        (signature.inputs.len(), signature.outputs.len()),
        (nin, nout),
        "{params:?}"
    );
    for u in ticks(nin) {
        assert_eq!(
            block_outputs(block.as_ref(), &u),
            reference_outputs(nin, nout, &gains, &u),
            "{params:?} u={u:?}"
        );
    }
}

/// Values a cell can be bound to: every kind, and Reals and Integers at their edges.
fn cell_values() -> Vec<Value> {
    vec![
        Value::Real(1.5),
        Value::Real(-0.0),
        Value::Real(f64::NAN),
        Value::Real(f64::INFINITY),
        Value::Real(f64::NEG_INFINITY),
        Value::Real(f64::MIN_POSITIVE / 2.0),
        Value::Real(1.0e300),
        Value::Integer(i64::MIN),
        Value::Integer(i64::MAX),
        Value::Integer(7),
        Value::Boolean(true),
        Value::String(Arc::from("2.0")),
    ]
}

/// Cell names, canonical or not, inside and outside every shape below.
const CELLS: [&str; 24] = [
    "K_1_1",
    "K_1_2",
    "K_2_1",
    "K_2_2",
    "K_1_3",
    "K_3_1",
    "K_3_3",
    "K_4_1",
    "K_1_4",
    "K_0_1",
    "K_1_0",
    "K_01_1",
    "K_1_01",
    "K_+1_1",
    "K_1",
    "K_1_1_1",
    "k_1_1",
    "K_\u{FF11}_1",
    "K__1_1",
    "K_1_1 ",
    "K_18446744073709551616_1",
    "K_1_18446744073709551616",
    "K_1_1",
    "K_2_2",
];

/// `nin`/`nout` bindings, every one the old reading took a shape from: omitted, each alone, empty
/// either way, square and non-square, wrong kinds, negative, repeated, past the product bound. The
/// class does not declare them (issue #773), so none changes the block.
fn ignored_counts() -> Vec<Vec<(&'static str, Value)>> {
    let int = Value::Integer;
    vec![
        vec![],
        vec![("nout", int(3))],
        vec![("nin", int(3))],
        vec![("nout", int(0)), ("nin", int(3))],
        vec![("nout", int(3)), ("nin", int(0))],
        vec![("nout", int(1)), ("nin", int(1))],
        vec![("nout", int(2)), ("nin", int(3))],
        vec![("nout", int(3)), ("nin", int(2))],
        vec![("nout", int(3)), ("nin", int(3))],
        vec![("nout", int(4)), ("nin", int(1))],
        vec![("nout", int(-1)), ("nin", int(2))],
        vec![("nout", Value::Real(3.0)), ("nin", int(3))],
        vec![("nout", int(3)), ("nin", Value::Boolean(true))],
        vec![("nout", int(1)), ("nout", int(3)), ("nin", int(2))],
        vec![("nout", int(i64::MAX)), ("nin", int(2))],
        vec![("nout", int(1 << 20)), ("nin", int(2))],
    ]
}

#[test]
fn the_constructor_builds_the_block_the_per_cell_reading_built() {
    let values = cell_values();
    let mut tables = 0;
    for shape in ignored_counts() {
        // Every cell name with each value in turn, so each name meets each kind; then each
        // cell alone, then the empty table.
        let mut k_tables: Vec<Vec<(&str, Value)>> = (0..values.len())
            .map(|turn| {
                CELLS
                    .iter()
                    .enumerate()
                    .map(|(i, name)| (*name, values[(i + turn) % values.len()].clone()))
                    .collect()
            })
            .collect();
        k_tables.extend(CELLS.iter().map(|name| vec![(*name, Value::Real(5.0))]));
        k_tables.push(Vec::new());
        for k in k_tables {
            // The shape before the cells, then after them: neither order may matter.
            for shape_first in [true, false] {
                let mut values: Vec<(Arc<str>, Value)> = Vec::new();
                let shape_rows = shape.iter().map(|(n, v)| (Arc::from(*n), v.clone()));
                let cells = k.iter().map(|(n, v)| (Arc::from(*n), v.clone()));
                if shape_first {
                    values.extend(shape_rows);
                    values.extend(cells);
                } else {
                    values.extend(cells);
                    values.extend(shape_rows);
                }
                assert_matches_reference(&ParamTable { values });
                tables += 1;
            }
        }
    }
    assert_eq!(tables, 16 * (12 + 24 + 1) * 2);
}

#[test]
fn the_widest_shape_the_product_bound_admits_is_built_and_a_wider_one_is_empty() {
    // `K` spanning `MAX_RESOLVED_PORT_WIDTH` cells is a block (the load gate refuses its unbound
    // cells first); one more row, or a second column, puts the product past the bound and builds
    // the empty block.
    let width = MAX_RESOLVED_PORT_WIDTH;
    for (corner, outputs) in [
        (format!("K_{width}_1"), MAX_RESOLVED_PORT_WIDTH),
        (format!("K_{}_1", width + 1), 0),
        (format!("K_{width}_2"), 0),
    ] {
        let params = ParamTable {
            values: vec![
                (Arc::from(corner.as_str()), Value::Real(0.0)),
                (Arc::from("K_1_1"), Value::Real(3.0)),
                (Arc::from("K_2_1"), Value::Real(-2.0)),
            ],
        };
        let block = (lookup("CDL.Reals.MatrixGain").expect("registered").make)(&params);
        assert_eq!(
            block.resolved_signature().outputs.len(),
            outputs,
            "{corner}"
        );
        if outputs > 0 {
            let y = block_outputs(block.as_ref(), &[2.0]);
            assert_eq!(y[0], 6.0_f64.to_bits());
            assert_eq!(y[1], (-4.0_f64).to_bits());
            // Row 3 onwards: column 1 is off the diagonal, so the default 0.
            assert!(y[2..].iter().all(|bits| *bits == 0.0_f64.to_bits()));
        }
    }
}

fn hand(params: &[(&str, Value)], u: &[f64]) -> Vec<f64> {
    let params = ParamTable {
        values: params
            .iter()
            .map(|(name, value)| (Arc::from(*name), value.clone()))
            .collect(),
    };
    let block = (lookup("CDL.Reals.MatrixGain").expect("registered").make)(&params);
    block_outputs(block.as_ref(), u)
        .into_iter()
        .map(f64::from_bits)
        .collect()
}

#[test]
fn hand_worked_gains_give_their_products() {
    let (real, int) = (Value::Real, Value::Integer);
    // K = [[1, 2, 3], [4, 5, 6]], u = (1, 10, 100): y = (321, 654).
    let full = [
        ("K_1_1", real(1.0)),
        ("K_1_2", real(2.0)),
        ("K_1_3", real(3.0)),
        ("K_2_1", real(4.0)),
        ("K_2_2", real(5.0)),
        ("K_2_3", real(6.0)),
    ];
    assert_eq!(hand(&full, &[1.0, 10.0, 100.0]), [321.0, 654.0]);
    // No parameters: the 2x2 identity.
    assert_eq!(hand(&[], &[3.0, 4.0]), [3.0, 4.0]);
    // `nin`/`nout` are read by nothing (issue #773): still the 2x2 identity.
    assert_eq!(
        hand(&[("nout", int(2)), ("nin", int(3))], &[7.0, 8.0]),
        [7.0, 8.0]
    );
    // `K_1_2` alone spans a 1x2 matrix; its unbound cell is the identity's: K = [[1, 5]],
    // u = (2, 3): y = (17).
    assert_eq!(hand(&[("K_1_2", real(5.0))], &[2.0, 3.0]), [17.0]);
    // An Integer cell is promoted; a Boolean or String cell is the default.
    assert_eq!(
        hand(
            &[
                ("K_1_1", int(3)),
                ("K_2_2", Value::Boolean(false)),
                ("K_2_1", Value::String(Arc::from("9")))
            ],
            &[2.0, 5.0]
        ),
        [6.0, 5.0]
    );
    // The first of a repeated cell is read; a non-canonical or index-0 spelling is no cell and
    // sets no shape: K = [[2, 0], [0, 1]].
    assert_eq!(
        hand(
            &[
                ("K_1_1", real(2.0)),
                ("K_1_1", real(3.0)),
                ("K_2_2", real(1.0)),
                ("K_01_1", real(9.0)),
                ("K_1_+2", real(9.0)),
                ("K_0_1", real(9.0)),
                ("K_1", real(9.0)),
                ("k_3_3", real(9.0)),
            ],
            &[1.0, 1.0]
        ),
        [2.0, 1.0]
    );
    // A cell past the others sets the shape: `K_3_1` makes K 3x1 = [[2], [0], [9]].
    assert_eq!(
        hand(&[("K_1_1", real(2.0)), ("K_3_1", real(9.0))], &[1.0]),
        [2.0, 0.0, 9.0]
    );
}
