//! Every refusal of the port-naming seam, reached with hand-built signatures (GitHub #440).
//!
//! The registry's constructors never build a signature the layouts cannot name (the fuzz in
//! `tests/hostile_parameters.rs` checks that), so `LayoutMismatch` and `NoLayout` are reachable
//! only through [`name_signature`]. These tests drive it with arities no constructor produces and
//! require the specific refusal, never a guessed name.

use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use oce_model::{ParamTable, Value};

use super::{
    Connector, Declared, EXTRACTOR, Layout, MATRIX_MAX, MATRIX_MIN, MatrixCols, PortSide,
    ResolvedPortsError, SORT, VECTOR_IN_SCALAR_OUT, VECTOR_REPLICATOR, layout, name_signature,
};
use crate::PortKind;

fn reals(n: usize) -> Vec<PortKind> {
    vec![PortKind::Real; n]
}

fn flag(name: &str, value: bool) -> ParamTable {
    ParamTable {
        values: vec![(Arc::from(name), Value::Boolean(value))],
    }
}

fn mismatch(side: PortSide, resolved: usize) -> ResolvedPortsError {
    ResolvedPortsError::LayoutMismatch { side, resolved }
}

fn name(
    class: &'static str,
    declared: Declared,
    params: &ParamTable,
    inputs: usize,
    outputs: usize,
) -> Result<(Vec<alloc::string::String>, Vec<alloc::string::String>), ResolvedPortsError> {
    name_signature(class, declared, params, &reals(inputs), &reals(outputs)).map(|(i, o)| {
        (
            i.into_iter().map(|p| p.name).collect(),
            o.into_iter().map(|p| p.name).collect(),
        )
    })
}

#[test]
fn a_class_with_neither_names_nor_a_layout_is_no_layout() {
    let empty = ParamTable::default();
    assert_eq!(
        name("CDL.Reals.Add", Declared::Layout(None), &empty, 2, 1),
        Err(ResolvedPortsError::NoLayout)
    );
    assert!(
        layout("CDL.Reals.Add").is_none(),
        "named classes carry no layout"
    );
}

#[test]
fn sort_refuses_an_odd_output_count() {
    let empty = ParamTable::default();
    let sort = Declared::Layout(Some(&SORT));
    assert_eq!(
        name("CDL.Reals.Sort", sort, &empty, 1, 3),
        Err(mismatch(PortSide::Output, 3))
    );
    assert_eq!(
        name("CDL.Reals.Sort", sort, &empty, 2, 1),
        Err(mismatch(PortSide::Output, 1))
    );
    assert!(name("CDL.Reals.Sort", sort, &empty, 2, 4).is_ok());
}

#[test]
fn a_matrix_reduction_refuses_inputs_without_outputs_and_a_ragged_matrix() {
    let max = Declared::Layout(Some(&MATRIX_MAX));
    let min = Declared::Layout(Some(&MATRIX_MIN));
    for (class, declared, flag_name) in [
        ("CDL.Reals.MatrixMax", max, "rowMax"),
        ("CDL.Reals.MatrixMin", min, "rowMin"),
    ] {
        for by_row in [true, false] {
            let params = flag(flag_name, by_row);
            // Inputs with no outputs: no column count can be inferred.
            assert_eq!(
                name(class, declared, &params, 4, 0),
                Err(mismatch(PortSide::Input, 4)),
                "{class} by_row={by_row}"
            );
            // Five cells cannot be two rows (by row) or two columns (by column).
            assert_eq!(
                name(class, declared, &params, 5, 2),
                Err(mismatch(PortSide::Input, 5)),
                "{class} by_row={by_row}"
            );
            // Nothing at all is a legal empty matrix.
            assert_eq!(name(class, declared, &params, 0, 0), Ok((vec![], vec![])));
        }
    }
}

#[test]
fn a_replicator_matrix_needs_a_known_input_width() {
    let empty = ParamTable::default();
    // A test layout with two input vectors: their common width is not one vector's width, so the
    // output matrix has no column count and must be refused, not guessed.
    static TWO_VECTORS: Layout = Layout {
        inputs: &[Connector::Vector("u"), Connector::Vector("v")],
        outputs: &[Connector::Matrix {
            base: "y",
            cols: MatrixCols::InputVectorWidth,
        }],
    };
    assert_eq!(
        name(
            "test.TwoVectors",
            Declared::Layout(Some(&TWO_VECTORS)),
            &empty,
            4,
            4
        ),
        Err(mismatch(PortSide::Output, 4))
    );
    // The real replicator: six outputs over a width-4 input vector is ragged.
    let replicator = Declared::Layout(Some(&VECTOR_REPLICATOR));
    assert_eq!(
        name("CDL.Routing.RealVectorReplicator", replicator, &empty, 4, 6),
        Err(mismatch(PortSide::Output, 6))
    );
    // Outputs over an empty input vector cannot be laid out.
    assert_eq!(
        name("CDL.Routing.RealVectorReplicator", replicator, &empty, 0, 2),
        Err(mismatch(PortSide::Output, 2))
    );
}

#[test]
fn scalar_connectors_refuse_too_few_or_unaccounted_ports() {
    let empty = ParamTable::default();
    let extractor = Declared::Layout(Some(&EXTRACTOR));
    // `index` is declared, so zero inputs is too few.
    assert_eq!(
        name("CDL.Routing.RealExtractor", extractor, &empty, 0, 1),
        Err(mismatch(PortSide::Input, 0))
    );
    // A scalar-only output side cannot absorb a second output.
    let multi = Declared::Layout(Some(&VECTOR_IN_SCALAR_OUT));
    assert_eq!(
        name("CDL.Reals.MultiSum", multi, &empty, 2, 2),
        Err(mismatch(PortSide::Output, 2))
    );
    assert_eq!(
        name("CDL.Reals.MultiSum", multi, &empty, 2, 0),
        Err(mismatch(PortSide::Output, 0))
    );
}

#[test]
fn fixed_names_refuse_a_different_arity() {
    let empty = ParamTable::default();
    let names = Declared::Names(&["u1", "u2"], &["y"]);
    assert_eq!(
        name("CDL.Reals.Add", names, &empty, 3, 1),
        Err(mismatch(PortSide::Input, 3))
    );
    assert_eq!(
        name("CDL.Reals.Add", names, &empty, 2, 0),
        Err(mismatch(PortSide::Output, 0))
    );
    assert_eq!(
        name("CDL.Reals.Add", names, &empty, 2, 1),
        Ok((vec!["u1".into(), "u2".into()], vec!["y".into()]))
    );
}
