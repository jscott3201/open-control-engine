//! Unit tests for `element_extents.rs`: the shape each rule gives, element by element.

use alloc::{format, string::String, sync::Arc, vec::Vec};

use oce_model::{ParamTable, Value};

use super::{BoundCells, bound_cells, element_outside_extent, index};

fn table(entries: &[(&str, Value)]) -> ParamTable {
    ParamTable {
        values: entries
            .iter()
            .map(|(name, value)| (Arc::from(*name), value.clone()))
            .collect(),
    }
}

fn reals(names: &[&str]) -> ParamTable {
    table(
        &names
            .iter()
            .map(|name| (*name, Value::Real(0.0)))
            .collect::<Vec<_>>(),
    )
}

#[test]
fn an_index_is_a_canonical_decimal_from_one() {
    assert_eq!(index("1"), Some(Some(1)));
    assert_eq!(index("42"), Some(Some(42)));
    assert_eq!(index("0"), Some(None), "zero is outside every extent");
    assert_eq!(index("99999999999999999999999"), Some(None), "past usize");
    for text in ["", "01", "+1", "-1", "1a", "x", " 1", "\u{FF11}"] {
        assert_eq!(index(text), None, "{text:?} is no element index");
    }
}

#[test]
fn bound_cells_span_their_largest_row_and_column_and_count_distinct_cells() {
    let cells = bound_cells(&reals(&["K_1_1", "K_2_3", "K_1_2", "k_9_9", "K_1"]), "K");
    assert_eq!((cells.rows, cells.cols, cells.count), (2, 3, 3));
    assert!(!cells.is_complete());
    assert_eq!(
        cells.last,
        Some(1),
        "K_2_3 is the cell with the largest row"
    );
    let full = bound_cells(&reals(&["K_1_1", "K_1_2", "K_2_1", "K_2_2"]), "K");
    assert!(full.is_complete());
    assert_eq!(bound_cells(&reals(&[]), "K"), BoundCells::default());
    assert!(
        BoundCells::default().is_complete(),
        "no cell fills the empty rectangle"
    );
    let zero = bound_cells(&reals(&["K_1_1", "K_0_1"]), "K");
    assert_eq!(zero.outside, Some(1));
}

#[test]
fn a_vector_element_lies_inside_its_resolved_length() {
    let sum = |n: Option<i64>, elements: &[&str]| {
        let mut entries: Vec<(&str, Value)> = elements
            .iter()
            .map(|name| (*name, Value::Real(1.0)))
            .collect();
        if let Some(n) = n {
            entries.push(("nin", Value::Integer(n)));
        }
        element_outside_extent("CDL.Reals.MultiSum", &table(&entries))
    };
    assert_eq!(sum(Some(2), &["k_1", "k_2"]), None);
    assert_eq!(sum(Some(2), &["k_1", "k_3"]), Some(1));
    assert_eq!(sum(Some(0), &["k_1"]), Some(0));
    assert_eq!(
        sum(Some(-5), &["k_1"]),
        Some(0),
        "a negative length admits nothing"
    );
    // A length of another kind leaves the elements to the kind check.
    let real_nin = table(&[("nin", Value::Real(2.0)), ("k_5", Value::Real(1.0))]);
    assert_eq!(
        element_outside_extent("CDL.Reals.MultiSum", &real_nin),
        None
    );
}

#[test]
fn a_table_offset_lies_inside_the_tables_outputs_and_the_table_is_complete() {
    let mut names: Vec<String> = Vec::new();
    for row in 1..=2 {
        for col in 1..=3 {
            names.push(format!("table_{row}_{col}"));
        }
    }
    let with = |extra: &[&str]| {
        let mut all: Vec<&str> = names.iter().map(String::as_str).collect();
        all.extend_from_slice(extra);
        element_outside_extent("CDL.Reals.Sources.TimeTable", &reals(&all))
    };
    assert_eq!(with(&[]), None);
    assert_eq!(with(&["offset_1", "offset_2"]), None);
    assert_eq!(with(&["offset_3"]), Some(6));
    assert_eq!(
        with(&["table_3_1"]),
        Some(6),
        "a third row of one cell is unfilled"
    );
}

#[test]
fn an_unregistered_class_or_a_class_with_no_shape_judges_nothing() {
    let anything = reals(&["k_9", "K_9_9", "table_9_9"]);
    assert_eq!(element_outside_extent("CDL.Reals.Nope", &anything), None);
    assert_eq!(element_outside_extent("CDL.Reals.Add", &anything), None);
    let one = table(&[("K_1_1", Value::Real(1.0))]);
    assert_eq!(element_outside_extent("CDL.Reals.MatrixGain", &one), None);
}

#[test]
fn an_element_of_a_required_length_the_block_omits_is_outside() {
    // `VectorFilter.nin` is required: a mask with no `nin` lies inside no length.
    let filter = table(&[
        ("nout", Value::Integer(0)),
        ("msk_1", Value::Boolean(true)),
        ("msk_2", Value::Boolean(true)),
    ]);
    assert_eq!(
        element_outside_extent("CDL.Routing.RealVectorFilter", &filter),
        Some(1)
    );
    // With its length bound, the same mask is inside.
    let mut bound = filter.clone();
    bound.values.push((Arc::from("nin"), Value::Integer(2)));
    assert_eq!(
        element_outside_extent("CDL.Routing.RealVectorFilter", &bound),
        None
    );
}

#[test]
fn a_complete_matrix_wider_than_the_port_width_is_outside() {
    let width = crate::MAX_RESOLVED_PORT_WIDTH;
    let row = |cols: usize| ParamTable {
        values: (1..=cols)
            .map(|col| (Arc::from(format!("K_1_{col}").as_str()), Value::Real(0.0)))
            .collect(),
    };
    assert_eq!(
        element_outside_extent("CDL.Reals.MatrixGain", &row(width)),
        None
    );
    let past = row(width + 1);
    assert_eq!(
        element_outside_extent("CDL.Reals.MatrixGain", &past),
        Some(width),
        "the cell with the largest column is named"
    );
    // A table whose rectangle is past the width, likewise.
    let mut names: Vec<String> = Vec::new();
    names.push(format!("table_{}_2", width / 2 + 1));
    let corner = reals(&names.iter().map(String::as_str).collect::<Vec<_>>());
    assert_eq!(
        element_outside_extent("CDL.Reals.Sources.TimeTable", &corner),
        Some(0)
    );
}
