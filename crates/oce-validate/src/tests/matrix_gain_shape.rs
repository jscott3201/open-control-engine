//! `CDL.Reals.MatrixGain` takes its counts from `K` (issue #773, owner ruling of 2026-11-22):
//! upstream declares `nin = size(K, 2)` and `nout = size(K, 1)` protected, and `K`'s default is
//! the 2x2 identity (`parameter Real K[:,:]=[1, 0; 0, 1]`, `Reals/MatrixGain.mo` line 4). So the
//! load gate refuses `nin` and `nout` as undeclared (`unknown-parameter`), and refuses a `K` whose
//! bound cells do not fill the rectangle their largest row and column span
//! (`parameter-out-of-range`, one finding naming the unbound cells). No `K` is the default 2x2.

use super::common::*;

/// A `MatrixGain` of `ins` inputs and `outs` outputs binding `cells` (`(row, col, value)`).
fn gain(ins: usize, outs: usize, cells: &[(usize, usize, f64)]) -> ModelGraph {
    one_block_model(
        "CDL.Reals.MatrixGain",
        &vec![ValueType::Real; ins],
        &vec![ValueType::Real; outs],
        cells
            .iter()
            .map(|&(row, col, value)| (Arc::from(format!("K_{row}_{col}")), Value::Real(value)))
            .collect(),
    )
}

/// Every cell of a `rows x cols` matrix.
fn full(rows: usize, cols: usize) -> Vec<(usize, usize, f64)> {
    (1..=rows)
        .flat_map(|row| (1..=cols).map(move |col| (row, col, 0.5)))
        .collect()
}

fn refusal(model: &ModelGraph) -> Vec<(DiagCode, String)> {
    validate(model)
        .expect_err("refused")
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message.clone()))
        .collect()
}

#[test]
fn no_cell_is_the_default_two_by_two_identity() {
    assert!(validate(&gain(2, 2, &[])).unwrap().is_empty());
    // Its ports are the default's: three inputs are a mismatch, as for any class.
    assert_eq!(
        codes(&validate(&gain(3, 2, &[])).unwrap_err().diagnostics),
        vec![DiagCode::MalformedDocument]
    );
}

#[test]
fn a_complete_matrix_of_any_shape_loads_with_its_own_counts() {
    for (rows, cols) in [(1, 1), (1, 5), (5, 1), (2, 3), (3, 2), (4, 4)] {
        assert!(
            validate(&gain(cols, rows, &full(rows, cols)))
                .unwrap_or_else(|e| panic!("{rows}x{cols}: {e:?}"))
                .is_empty(),
            "{rows}x{cols}"
        );
    }
}

#[test]
fn nin_and_nout_are_not_parameters_of_the_class() {
    for (name, value) in [("nin", 2), ("nout", 2), ("nin", 7), ("nout", 0)] {
        let mut model = gain(2, 2, &full(2, 2));
        model.blocks[0]
            .params
            .values
            .push((Arc::from(name), Value::Integer(value)));
        let refused = refusal(&model);
        assert_eq!(refused.len(), 1, "{name} = {value}: {refused:?}");
        assert_eq!(refused[0].0, DiagCode::UnknownParameter, "{refused:?}");
        assert!(refused[0].1.contains(&format!("`{name}`")), "{refused:?}");
    }
    // The reproduction of issue #773: `nout = 7` on a 2x2 `K` once loaded with seven outputs.
    let mut seven = gain(2, 7, &full(2, 2));
    seven.blocks[0]
        .params
        .values
        .push((Arc::from("nout"), Value::Integer(7)));
    let codes = codes(&validate(&seven).unwrap_err().diagnostics);
    assert!(codes.contains(&DiagCode::UnknownParameter), "{codes:?}");
}

#[test]
fn a_ragged_or_partial_matrix_is_refused_naming_its_unbound_cells() {
    // A missing cell; an extra cell in one row; a lone corner; a column cell past the others.
    for (what, cells, rows, cols, unbound) in [
        (
            "missing K_2_2",
            vec![(1, 1, 1.0), (1, 2, 0.0), (2, 1, 0.0)],
            2,
            2,
            "K_2_2",
        ),
        (
            "an extra cell in row 1",
            vec![
                (1, 1, 1.0),
                (1, 2, 0.0),
                (1, 3, 0.0),
                (2, 1, 0.0),
                (2, 2, 1.0),
            ],
            2,
            3,
            "K_2_3",
        ),
        ("a lone corner", vec![(3, 3, 1.0)], 3, 3, "K_1_1"),
        (
            "a row past the others in column 1",
            vec![(1, 1, 1.0), (1, 2, 1.0), (3, 1, 1.0)],
            3,
            2,
            "K_2_1",
        ),
    ] {
        let refused = refusal(&gain(cols, rows, &cells));
        assert!(
            refused
                .iter()
                .any(|(code, message)| *code == DiagCode::ParameterOutOfRange
                    && message.contains(&format!("{rows}x{cols}"))
                    && message.contains(unbound)),
            "{what}: {refused:?}"
        );
    }
}

#[test]
fn the_largest_complete_matrix_loads_and_one_cell_past_the_port_width_is_refused() {
    // 1 x 2^20 and 2^20 x 1 are the widest; one more cell's worth is past the bound. Each is
    // judged from the bound cells, never by walking the shape.
    let width = oce_blocks::MAX_RESOLVED_PORT_WIDTH;
    let row = gain(width, 1, &full(1, width));
    assert!(validate(&row).unwrap().is_empty());
    let column = gain(1, width, &full(width, 1));
    assert!(validate(&column).unwrap().is_empty());
    let past = gain(1, 1, &[(width + 1, 1, 1.0)]);
    let refused = refusal(&past);
    assert!(
        refused
            .iter()
            .any(|(code, message)| *code == DiagCode::ParameterOutOfRange
                && message.contains("more than the maximum flattened cell count")),
        "{refused:?}"
    );
    let corner = gain(1, 1, &[(1024, 1025, 1.0)]);
    assert!(
        refusal(&corner)
            .iter()
            .any(|(_, message)| message.contains("more than the maximum")),
    );
}
