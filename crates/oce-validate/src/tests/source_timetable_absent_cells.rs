//! A `TimeTable` whose shape has cells no parameter binds (issue #726): one finding for the
//! table, the count of absent cells and the first four in row order, found from the cells bound.

use super::common::*;
use crate::validate_block_parameters;

const CLASS: &str = "CDL.Reals.Sources.TimeTable";

/// A parameter table binding `table_<row>_<col>` for each cell, in the order given.
fn cells(bound: &[(usize, usize)]) -> ParamTable {
    ParamTable {
        values: bound
            .iter()
            .map(|(row, col)| rp(&format!("table_{row}_{col}"), *row as f64))
            .collect(),
    }
}

/// The parameter-rule diagnostics of one block binding `bound`.
fn findings(bound: &[(usize, usize)]) -> Vec<oce_diag::Diagnostic> {
    match validate_block_parameters(CLASS, &cells(bound)) {
        Ok(warnings) => warnings,
        Err(error) => error.diagnostics,
    }
}

fn messages(bound: &[(usize, usize)]) -> Vec<String> {
    findings(bound).into_iter().map(|d| d.message).collect()
}

#[test]
fn a_table_short_one_cell_names_that_cell_as_it_always_has() {
    // The text before issue #726, unchanged for one absent cell.
    assert_eq!(
        messages(&[(1, 1), (1, 2), (2, 1)]),
        ["table `table` on block `CDL.Reals.Sources.TimeTable` is missing required cell table_2_2"]
    );
    assert_eq!(
        messages(&[(1, 2), (2, 1), (2, 2)]),
        ["table `table` on block `CDL.Reals.Sources.TimeTable` is missing required cell table_1_1"]
    );
    let found = findings(&[(1, 1), (1, 2), (2, 1)]);
    assert_eq!(found[0].code, DiagCode::ParameterOutOfRange);
    assert_eq!(found[0].severity, Severity::Error);
    assert_eq!(found[0].subject.as_deref(), Some("block#0"));
}

#[test]
fn a_table_short_several_cells_is_one_finding_with_the_count_shape_and_cells() {
    assert_eq!(
        messages(&[(1, 1), (2, 2)]),
        [
            "table `table` on block `CDL.Reals.Sources.TimeTable` is missing 2 required cells of \
             its 2x2 shape: table_1_2, table_2_1"
        ]
    );
    // Four absent: all named, none counted.
    assert_eq!(
        messages(&[(1, 1), (3, 2)]),
        [
            "table `table` on block `CDL.Reals.Sources.TimeTable` is missing 4 required cells of \
             its 3x2 shape: table_1_2, table_2_1, table_2_2, table_3_1"
        ]
    );
    // Five absent: four named, one counted.
    assert_eq!(
        messages(&[(3, 3), (1, 1)]),
        [
            "table `table` on block `CDL.Reals.Sources.TimeTable` is missing 7 required cells of \
             its 3x3 shape: table_1_2, table_1_3, table_2_1, table_2_2 and 3 more"
        ]
    );
}

#[test]
fn the_named_cells_are_the_first_absent_in_row_order_whatever_order_the_cells_are_bound_in() {
    let bound = [(1, 1), (1, 3), (2, 2), (3, 1), (3, 3), (4, 2)];
    let expected = "table `table` on block `CDL.Reals.Sources.TimeTable` is missing 6 required \
                    cells of its 4x3 shape: table_1_2, table_2_1, table_2_3, table_3_2 and 2 more";
    let mut order = bound.to_vec();
    for _ in 0..order.len() {
        assert_eq!(messages(&order), [expected], "bound in the order {order:?}");
        order.reverse();
        assert_eq!(messages(&order), [expected], "bound in the order {order:?}");
        order.rotate_left(1);
    }
}

#[test]
fn one_cell_at_the_far_corner_of_the_widest_shape_is_one_finding() {
    // 524,288 x 2 is the widest table (`MAX_RESOLVED_PORT_WIDTH` cells): its last cell alone.
    assert_eq!(
        messages(&[(524_288, 2)]),
        [
            "table `table` on block `CDL.Reals.Sources.TimeTable` is missing 1048575 required \
             cells of its 524288x2 shape: table_1_1, table_1_2, table_2_1, table_2_2 and 1048571 \
             more"
        ]
    );
    // Its first cell alone is a 1 x 1 table: refused for its width, nothing absent.
    assert_eq!(
        messages(&[(1, 1)]),
        [
            "table `table` on block `CDL.Reals.Sources.TimeTable` must have at least time plus one \
          output column"
        ]
    );
    // One row more is past the widest: refused for its size, its absent cells not counted.
    assert_eq!(
        messages(&[(524_289, 2)]),
        [
            "table `table` on block `CDL.Reals.Sources.TimeTable` exceeds maximum flattened cell \
          count 1048576"
        ]
    );
}

#[test]
fn a_bound_cell_is_still_judged_beside_the_absent_ones() {
    let mut params = cells(&[(1, 1), (3, 2)]);
    params
        .values
        .push((Arc::from("table_2_2"), Value::Boolean(true)));
    let err = validate_block_parameters(CLASS, &params).expect_err("absent and non-numeric");
    let messages: Vec<&str> = err.diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(
        messages,
        [
            "parameter `table_2_2` on block `CDL.Reals.Sources.TimeTable` must be numeric",
            "table `table` on block `CDL.Reals.Sources.TimeTable` is missing 3 required cells of \
             its 3x2 shape: table_1_2, table_2_1, table_3_1",
        ]
    );
}

#[test]
fn a_whole_model_and_the_block_alone_agree_on_an_absent_cell_finding() {
    let model = one_block_model(
        CLASS,
        &[],
        &[ValueType::Real],
        cells(&[(1, 1), (3, 2)]).values,
    );
    let whole: Vec<String> = validate(&model)
        .expect_err("absent cells")
        .diagnostics
        .into_iter()
        .filter(|d| d.message.starts_with("table "))
        .map(|d| d.message)
        .collect();
    assert_eq!(whole, messages(&[(1, 1), (3, 2)]));
}
