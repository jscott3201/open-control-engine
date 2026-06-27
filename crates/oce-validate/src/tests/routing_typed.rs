//! Boolean and Integer routing parameter tests.

use super::common::*;

#[test]
fn routing_typed_extract_signal_selector_parameters_are_checked() {
    let boolean_invalid = one_block_model(
        "CDL.Routing.BooleanExtractSignal",
        &[ValueType::Boolean, ValueType::Boolean],
        &[ValueType::Boolean],
        vec![
            (Arc::from("nin"), Value::Integer(2)),
            (Arc::from("nout"), Value::Integer(1)),
            (Arc::from("extract_1"), Value::Integer(3)),
        ],
    );
    let err = validate(&boolean_invalid).expect_err("Boolean extract_1>nin is out of range");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::ParameterOutOfRange]);
    assert!(err.diagnostics[0].message.contains("`extract_1`"));

    let integer_duplicate_valid = one_block_model(
        "CDL.Routing.IntegerExtractSignal",
        &[ValueType::Integer, ValueType::Integer],
        &[ValueType::Integer, ValueType::Integer, ValueType::Integer],
        vec![
            (Arc::from("nin"), Value::Integer(2)),
            (Arc::from("nout"), Value::Integer(3)),
            (Arc::from("extract_1"), Value::Integer(2)),
            (Arc::from("extract_2"), Value::Integer(1)),
            (Arc::from("extract_3"), Value::Integer(2)),
        ],
    );
    assert!(
        validate(&integer_duplicate_valid)
            .expect("Integer duplicate selectors are source-valid when in range")
            .is_empty()
    );
}

#[test]
fn routing_typed_vector_filter_mask_parameters_are_checked() {
    let boolean_count_mismatch = one_block_model(
        "CDL.Routing.BooleanVectorFilter",
        &[ValueType::Boolean, ValueType::Boolean, ValueType::Boolean],
        &[ValueType::Boolean, ValueType::Boolean],
        vec![
            (Arc::from("nin"), Value::Integer(3)),
            (Arc::from("nout"), Value::Integer(2)),
            (Arc::from("msk_1"), Value::Boolean(false)),
            (Arc::from("msk_2"), Value::Boolean(true)),
            (Arc::from("msk_3"), Value::Boolean(false)),
        ],
    );
    let err =
        validate(&boolean_count_mismatch).expect_err("Boolean mask true count must equal nout");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::ParameterOutOfRange]);
    assert!(err.diagnostics[0].message.contains("true count 1"));

    let integer_sparse_default_valid = one_block_model(
        "CDL.Routing.IntegerVectorFilter",
        &[ValueType::Integer, ValueType::Integer, ValueType::Integer],
        &[ValueType::Integer, ValueType::Integer],
        vec![
            (Arc::from("nin"), Value::Integer(3)),
            (Arc::from("nout"), Value::Integer(2)),
            (Arc::from("msk_2"), Value::Boolean(false)),
        ],
    );
    assert!(
        validate(&integer_sparse_default_valid)
            .expect("Integer omitted mask entries default to true")
            .is_empty()
    );
}

#[test]
fn routing_typed_vector_replicator_product_width_is_checked() {
    let boolean_too_wide = one_block_model(
        "CDL.Routing.BooleanVectorReplicator",
        &[],
        &[],
        vec![
            (Arc::from("nin"), Value::Integer(1024)),
            (Arc::from("nout"), Value::Integer(1025)),
        ],
    );
    let err = validate(&boolean_too_wide)
        .expect_err("Boolean nin*nout above max resolved width must fail");
    assert!(
        codes(&err.diagnostics).contains(&DiagCode::ParameterOutOfRange),
        "unexpected diagnostics: {:?}",
        err.diagnostics
    );
}
