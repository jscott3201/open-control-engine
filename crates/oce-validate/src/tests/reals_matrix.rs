//! Matrix and sort resolved-signature validation tests.

use super::common::*;

#[test]
fn reals_matrix_gain_resolved_signature_accepts_matrix_dimensions() {
    let m = one_block_model(
        "CDL.Reals.MatrixGain",
        &[ValueType::Real, ValueType::Real, ValueType::Real],
        &[ValueType::Real, ValueType::Real],
        vec![
            (Arc::from("nout"), Value::Integer(2)),
            (Arc::from("nin"), Value::Integer(3)),
        ],
    );
    assert!(
        validate(&m)
            .expect("MatrixGain nout=2,nin=3 resolves 3 inputs and 2 outputs")
            .is_empty()
    );
}

#[test]
fn reals_matrix_reducer_rejects_arity_mismatch() {
    let m = one_block_model(
        "CDL.Reals.MatrixMax",
        &[ValueType::Real, ValueType::Real, ValueType::Real],
        &[ValueType::Real, ValueType::Real],
        vec![
            (Arc::from("nRow"), Value::Integer(2)),
            (Arc::from("nCol"), Value::Integer(2)),
            (Arc::from("rowMax"), Value::Boolean(true)),
        ],
    );
    let err = validate(&m).expect_err("MatrixMax 2x2 row-wise requires four inputs");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::MalformedDocument]);
    assert!(
        err.diagnostics[0].message.contains("class requires 4/2"),
        "unexpected diagnostic: {:?}",
        err.diagnostics
    );
}

#[test]
fn reals_sort_resolved_signature_checks_mixed_output_kinds() {
    let m = one_block_model(
        "CDL.Reals.Sort",
        &[ValueType::Real, ValueType::Real],
        &[
            ValueType::Real,
            ValueType::Real,
            ValueType::Integer,
            ValueType::Integer,
        ],
        vec![(Arc::from("nin"), Value::Integer(2))],
    );
    assert!(
        validate(&m)
            .expect("Sort nin=2 emits two Real y ports and two Integer yIdx ports")
            .is_empty()
    );

    let wrong_y_idx = one_block_model(
        "CDL.Reals.Sort",
        &[ValueType::Real, ValueType::Real],
        &[
            ValueType::Real,
            ValueType::Real,
            ValueType::Real,
            ValueType::Integer,
        ],
        vec![(Arc::from("nin"), Value::Integer(2))],
    );
    let err = validate(&wrong_y_idx).expect_err("Sort yIdx ports must be Integer");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::PortKindMismatch]);
    assert_eq!(err.diagnostics[0].subject.as_deref(), Some("connector#4"));
}

#[test]
fn reals_matrix_gain_rejects_bad_defaulted_and_malformed_cells() {
    let bad_default_cell = one_block_model(
        "CDL.Reals.MatrixGain",
        &[ValueType::Real, ValueType::Real],
        &[ValueType::Real, ValueType::Real],
        vec![(Arc::from("K_1_2"), Value::Boolean(true))],
    );
    let err =
        validate(&bad_default_cell).expect_err("default 2x2 MatrixGain still validates K cells");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::ParameterOutOfRange]);
    assert!(err.diagnostics[0].message.contains("`K_1_2`"));

    let malformed_cells = one_block_model(
        "CDL.Reals.MatrixGain",
        &[ValueType::Real, ValueType::Real],
        &[ValueType::Real, ValueType::Real],
        vec![
            (Arc::from("nout"), Value::Integer(2)),
            (Arc::from("nin"), Value::Integer(2)),
            (Arc::from("K_1"), Value::Real(1.0)),
            (Arc::from("K_1_2_3"), Value::Real(1.0)),
            (Arc::from("K_0_1"), Value::Real(1.0)),
            (Arc::from("K_3_1"), Value::Real(1.0)),
        ],
    );
    let err = validate(&malformed_cells).expect_err("malformed and out-of-shape K names must fail");
    assert_eq!(
        codes(&err.diagnostics),
        vec![
            DiagCode::ParameterOutOfRange,
            DiagCode::ParameterOutOfRange,
            DiagCode::ParameterOutOfRange,
            DiagCode::ParameterOutOfRange,
        ]
    );
    assert!(
        err.diagnostics
            .iter()
            .any(|diag| diag.message.contains("`K_1`"))
    );
    assert!(
        err.diagnostics
            .iter()
            .any(|diag| diag.message.contains("`K_3_1`"))
    );
}

#[test]
fn reals_matrix_and_sort_boolean_params_must_be_boolean() {
    let bad_row_max = one_block_model(
        "CDL.Reals.MatrixMax",
        &[ValueType::Real, ValueType::Real],
        &[ValueType::Real],
        vec![
            (Arc::from("nRow"), Value::Integer(1)),
            (Arc::from("nCol"), Value::Integer(2)),
            (Arc::from("rowMax"), Value::String(Arc::from("true"))),
        ],
    );
    let err = validate(&bad_row_max).expect_err("rowMax must be Boolean");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::ParameterOutOfRange]);
    assert!(err.diagnostics[0].message.contains("`rowMax`"));

    let bad_row_min = one_block_model(
        "CDL.Reals.MatrixMin",
        &[ValueType::Real, ValueType::Real],
        &[ValueType::Real],
        vec![
            (Arc::from("nRow"), Value::Integer(1)),
            (Arc::from("nCol"), Value::Integer(2)),
            (Arc::from("rowMin"), Value::Integer(1)),
        ],
    );
    let err = validate(&bad_row_min).expect_err("rowMin must be Boolean");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::ParameterOutOfRange]);
    assert!(err.diagnostics[0].message.contains("`rowMin`"));

    let bad_ascending = one_block_model(
        "CDL.Reals.Sort",
        &[ValueType::Real, ValueType::Real],
        &[
            ValueType::Real,
            ValueType::Real,
            ValueType::Integer,
            ValueType::Integer,
        ],
        vec![
            (Arc::from("nin"), Value::Integer(2)),
            (Arc::from("ascending"), Value::Integer(1)),
        ],
    );
    let err = validate(&bad_ascending).expect_err("ascending must be Boolean");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::ParameterOutOfRange]);
    assert!(err.diagnostics[0].message.contains("`ascending`"));
}
