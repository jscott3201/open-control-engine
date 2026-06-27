//! TimeTable source parameter and resolved-signature validation tests.

use super::common::*;

fn table(cells: &[(usize, usize, f64)]) -> Vec<(Arc<str>, Value)> {
    cells
        .iter()
        .map(|(row, col, value)| (Arc::from(format!("table_{row}_{col}")), Value::Real(*value)))
        .collect()
}

fn valid_real_table() -> Vec<(Arc<str>, Value)> {
    table(&[
        (1, 1, 0.0),
        (1, 2, 1.0),
        (1, 3, 10.0),
        (2, 1, 1.0),
        (2, 2, 2.0),
        (2, 3, 20.0),
        (3, 1, 2.0),
        (3, 2, 3.0),
        (3, 3, 30.0),
    ])
}

#[test]
fn time_table_sources_resolve_vector_outputs_from_table_columns() {
    let real = one_block_model(
        "CDL.Reals.Sources.TimeTable",
        &[],
        &[ValueType::Real, ValueType::Real],
        valid_real_table(),
    );
    assert!(validate(&real).expect("valid Real TimeTable").is_empty());

    let mut integer_params = table(&[
        (1, 1, 0.0),
        (1, 2, 1.0),
        (1, 3, -2.0),
        (2, 1, 2.0),
        (2, 2, 2.0),
        (2, 3, -3.0),
    ]);
    integer_params.push(rp("period", 4.0));
    let integer = one_block_model(
        "CDL.Integers.Sources.TimeTable",
        &[],
        &[ValueType::Integer, ValueType::Integer],
        integer_params,
    );
    assert!(
        validate(&integer)
            .expect("valid Integer TimeTable")
            .is_empty()
    );

    let mut logical_params = table(&[
        (1, 1, 0.0),
        (1, 2, 0.0),
        (1, 3, 1.0),
        (2, 1, 2.0),
        (2, 2, 1.0),
        (2, 3, 0.0),
    ]);
    logical_params.push(rp("period", 4.0));
    let logical = one_block_model(
        "CDL.Logical.Sources.TimeTable",
        &[],
        &[ValueType::Boolean, ValueType::Boolean],
        logical_params,
    );
    assert!(
        validate(&logical)
            .expect("valid Logical TimeTable")
            .is_empty()
    );
}

#[test]
fn time_table_rejects_missing_incomplete_and_malformed_table_params() {
    let missing = one_block_model(
        "CDL.Reals.Sources.TimeTable",
        &[],
        &[ValueType::Real],
        vec![],
    );
    let err = validate(&missing).expect_err("table is required");
    assert_eq!(
        codes(&err.diagnostics),
        vec![DiagCode::MissingRequiredParameter]
    );

    let incomplete = one_block_model(
        "CDL.Reals.Sources.TimeTable",
        &[],
        &[ValueType::Real],
        vec![
            rp("table_1_1", 0.0),
            rp("table_1_2", 1.0),
            rp("table_2_1", 1.0),
        ],
    );
    let err = validate(&incomplete).expect_err("rectangular table is required");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::ParameterOutOfRange]);
    assert!(err.diagnostics[0].message.contains("table_2_2"));

    let malformed = one_block_model(
        "CDL.Reals.Sources.TimeTable",
        &[],
        &[ValueType::Real],
        vec![
            rp("table_1", 0.0),
            rp("table_1_2_3", 1.0),
            rp("table_0_1", 0.0),
            rp("table_1_1", 0.0),
            rp("table_1_2", 1.0),
        ],
    );
    let err = validate(&malformed).expect_err("bad table element names fail");
    assert_eq!(
        codes(&err.diagnostics),
        vec![
            DiagCode::ParameterOutOfRange,
            DiagCode::ParameterOutOfRange,
            DiagCode::ParameterOutOfRange,
        ]
    );
}

#[test]
fn real_time_table_rejects_bad_time_shape_enums_and_offsets() {
    let mut nonmonotonic = valid_real_table();
    nonmonotonic.push(rp("table_3_1", 0.5));
    let m = one_block_model(
        "CDL.Reals.Sources.TimeTable",
        &[],
        &[ValueType::Real, ValueType::Real],
        nonmonotonic,
    );
    let err = validate(&m).expect_err("time column must be nondecreasing");
    assert!(err.diagnostics[0].message.contains("nondecreasing"));

    let mut bad_periodic = table(&[(1, 1, 0.0), (1, 2, 1.0), (2, 1, 0.0), (2, 2, 2.0)]);
    bad_periodic.push(rp("offset_2", 0.0));
    bad_periodic.push((Arc::from("offset_bad"), Value::Real(0.0)));
    let m = one_block_model(
        "CDL.Reals.Sources.TimeTable",
        &[],
        &[ValueType::Real],
        bad_periodic,
    );
    let err = validate(&m).expect_err("degenerate periodic range and bad offset fail");
    assert!(
        err.diagnostics
            .iter()
            .any(|diag| diag.message.contains("positive time range"))
    );
    assert!(
        err.diagnostics
            .iter()
            .any(|diag| diag.message.contains("offset_2"))
    );
    assert!(
        err.diagnostics
            .iter()
            .any(|diag| diag.message.contains("offset_bad"))
    );

    let mut bad_enum = valid_real_table();
    bad_enum.push((Arc::from("smoothness"), Value::Integer(99)));
    let m = one_block_model(
        "CDL.Reals.Sources.TimeTable",
        &[],
        &[ValueType::Real, ValueType::Real],
        bad_enum,
    );
    let err = validate(&m).expect_err("bad smoothness enum fails");
    assert!(err.diagnostics[0].message.contains("smoothness"));
}

#[test]
fn integer_and_logical_time_tables_reject_typed_value_and_period_errors() {
    let mut bad_integer = table(&[(1, 1, 0.0), (1, 2, 1.5), (2, 1, 2.0), (2, 2, 2.0)]);
    bad_integer.push(rp("period", 4.0));
    let m = one_block_model(
        "CDL.Integers.Sources.TimeTable",
        &[],
        &[ValueType::Integer],
        bad_integer,
    );
    let err = validate(&m).expect_err("Integer TimeTable values must encode integers");
    assert!(err.diagnostics[0].message.contains("Integer"));

    let mut bad_logical = table(&[(1, 1, 0.0), (1, 2, 0.5), (2, 1, 2.0), (2, 2, 1.0)]);
    bad_logical.push(rp("period", 4.0));
    let m = one_block_model(
        "CDL.Logical.Sources.TimeTable",
        &[],
        &[ValueType::Boolean],
        bad_logical,
    );
    let err = validate(&m).expect_err("Logical TimeTable values must encode 0 or 1");
    assert!(err.diagnostics[0].message.contains("Boolean"));

    let mut bad_period = table(&[(1, 1, 1.0), (1, 2, 1.0), (2, 1, 4.0), (2, 2, 2.0)]);
    bad_period.push(rp("period", 4.0));
    let m = one_block_model(
        "CDL.Integers.Sources.TimeTable",
        &[],
        &[ValueType::Integer],
        bad_period,
    );
    let err = validate(&m).expect_err("periodic step table must start at zero and fit period");
    assert!(
        err.diagnostics
            .iter()
            .any(|diag| diag.message.contains("start at time 0"))
    );
    assert!(
        err.diagnostics
            .iter()
            .any(|diag| diag.message.contains("smaller than `period`"))
    );
}
