//! `matrix_elements` against the per-cell reading it replaced (GitHub #643): for every table of
//! up to three bindings drawn from the spellings a hostile document may use, and every shape
//! around them, it returns the very binding (by reference) that `find(params, "K_<row>_<col>")`
//! returned for each cell, row-major, so `MatrixGain` reads the same gains it read before.

use alloc::format;
use alloc::sync::Arc;
use alloc::vec::Vec;

use oce_model::{ParamTable, Value};

use super::{find, matrix_elements};

/// The reading `matrix_elements` replaced: build each cell's name, scan the table for it.
fn per_cell<'a>(
    params: &'a ParamTable,
    base: &str,
    rows: usize,
    cols: usize,
) -> Vec<Option<&'a Value>> {
    (1..=rows)
        .flat_map(|row| (1..=cols).map(move |col| (row, col)))
        .map(|(row, col)| find(params, &format!("{base}_{row}_{col}")))
        .collect()
}

/// Names a cell of `K` or not: canonical cells inside and outside the shapes below, row or
/// column 0, a leading zero, a sign, one or three index fields, another case and base, an empty
/// field, non-ASCII digits (full-width, Arabic-Indic), a trailing space and an index too large
/// for `usize` in either field.
const NAMES: [&str; 28] = [
    "K_1_1",
    "K_1_2",
    "K_2_1",
    "K_2_2",
    "K_3_1",
    "K_1_3",
    "K_10_1",
    "K_1_10",
    "K_0_1",
    "K_1_0",
    "K_01_1",
    "K_1_01",
    "K_+1_1",
    "K_1_+1",
    "K_-1_1",
    "K_1",
    "K_1_1_1",
    "k_1_1",
    "KK_1_1",
    "K",
    "K__1_1",
    "K_1_",
    "K_\u{FF11}_1",
    "K_1_\u{0661}",
    "K_1_1 ",
    "K_340282366920938463463374607431768211456_1",
    "K_1_340282366920938463463374607431768211456",
    "K_2_10",
];

/// Shapes `rows × cols`: empty either way, one cell, square, non-square both ways, a single row
/// or column reaching index 10, and one wider than every name.
const SHAPES: [(usize, usize); 11] = [
    (0, 0),
    (0, 3),
    (3, 0),
    (1, 1),
    (2, 2),
    (2, 3),
    (3, 2),
    (1, 10),
    (10, 1),
    (3, 3),
    (10, 10),
];

fn value(i: usize) -> Value {
    match i % 3 {
        0 => Value::Real(i as f64),
        1 => Value::Integer(i as i64),
        _ => Value::Boolean(i.is_multiple_of(2)),
    }
}

fn assert_same(params: &ParamTable) {
    for (rows, cols) in SHAPES {
        let ours = matrix_elements(params, "K", rows, cols);
        let theirs = per_cell(params, "K", rows, cols);
        assert_eq!(ours.len(), rows * cols, "{params:?} {rows}x{cols}");
        assert_eq!(ours.len(), theirs.len());
        for (cell, (ours, theirs)) in ours.iter().zip(&theirs).enumerate() {
            let same = match (ours, theirs) {
                (Some(ours), Some(theirs)) => core::ptr::eq(*ours, *theirs),
                (None, None) => true,
                _ => false,
            };
            assert!(same, "{params:?} {rows}x{cols} cell {cell}");
        }
    }
}

#[test]
fn matrix_elements_returns_the_binding_every_per_cell_lookup_returned() {
    assert_same(&ParamTable { values: Vec::new() });
    let mut tables = 0;
    for a in 0..NAMES.len() {
        for b in 0..NAMES.len() {
            for c in 0..NAMES.len() {
                let params = ParamTable {
                    values: [a, b, c]
                        .iter()
                        .enumerate()
                        .map(|(slot, &name)| (Arc::from(NAMES[name]), value(slot + name)))
                        .collect(),
                };
                assert_same(&params);
                tables += 1;
            }
        }
    }
    assert_eq!(tables, 28 * 28 * 28);
}

#[test]
fn a_shape_whose_cell_count_overflows_reads_no_cell() {
    let params = ParamTable {
        values: alloc::vec![(Arc::from("K_1_1"), Value::Real(2.0))],
    };
    assert!(matrix_elements(&params, "K", usize::MAX, 2).is_empty());
    assert!(matrix_elements(&params, "K", 2, usize::MAX).is_empty());
    // A zero dimension against a huge one is an empty shape, not an overflow.
    assert!(matrix_elements(&params, "K", usize::MAX, 0).is_empty());
}

#[test]
fn every_cell_lands_in_its_row_major_slot() {
    // Hand-written: a 2x3 table bound in reverse order, each value naming its own cell.
    let names = ["K_2_3", "K_2_2", "K_2_1", "K_1_3", "K_1_2", "K_1_1"];
    let params = ParamTable {
        values: names
            .iter()
            .map(|name| {
                let tag: i64 = name[2..].replace('_', "").parse().expect("digits");
                (Arc::from(*name), Value::Integer(tag))
            })
            .collect(),
    };
    let read: Vec<Option<i64>> = matrix_elements(&params, "K", 2, 3)
        .into_iter()
        .map(|value| match value {
            Some(Value::Integer(tag)) => Some(*tag),
            _ => None,
        })
        .collect();
    assert_eq!(
        read,
        [Some(11), Some(12), Some(13), Some(21), Some(22), Some(23)]
    );
    // The same table read as 3x2 keeps only the cells inside that shape.
    let read: Vec<Option<i64>> = matrix_elements(&params, "K", 3, 2)
        .into_iter()
        .map(|value| match value {
            Some(Value::Integer(tag)) => Some(*tag),
            _ => None,
        })
        .collect();
    assert_eq!(read, [Some(11), Some(12), Some(21), Some(22), None, None]);
}
