//! Real routing parameter tests.

use super::common::*;

#[test]
fn routing_real_extract_signal_selector_parameters_are_checked() {
    let invalid_low = one_block_model(
        "CDL.Routing.RealExtractSignal",
        &[ValueType::Real, ValueType::Real],
        &[ValueType::Real],
        vec![
            (Arc::from("nin"), Value::Integer(2)),
            (Arc::from("nout"), Value::Integer(1)),
            (Arc::from("extract_1"), Value::Integer(0)),
        ],
    );
    let err = validate(&invalid_low).expect_err("extract_1=0 is out of range");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::ParameterOutOfRange]);
    assert!(err.diagnostics[0].message.contains("`extract_1`"));
    assert!(err.diagnostics[0].message.contains("range 1..=2"));

    let invalid_high = one_block_model(
        "CDL.Routing.RealExtractSignal",
        &[ValueType::Real, ValueType::Real],
        &[ValueType::Real],
        vec![
            (Arc::from("nin"), Value::Integer(2)),
            (Arc::from("nout"), Value::Integer(1)),
            (Arc::from("extract_1"), Value::Integer(3)),
        ],
    );
    let err = validate(&invalid_high).expect_err("extract_1>nin is out of range");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::ParameterOutOfRange]);
    assert!(err.diagnostics[0].message.contains("`extract_1`"));

    let default_invalid = one_block_model(
        "CDL.Routing.RealExtractSignal",
        &[],
        &[ValueType::Real],
        vec![
            (Arc::from("nin"), Value::Integer(0)),
            (Arc::from("nout"), Value::Integer(1)),
        ],
    );
    let err = validate(&default_invalid)
        .expect_err("default extract_1=1 is invalid when nin=0 and nout=1");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::ParameterOutOfRange]);
    assert!(err.diagnostics[0].message.contains("`extract_1`"));

    let duplicate_valid = one_block_model(
        "CDL.Routing.RealExtractSignal",
        &[ValueType::Real, ValueType::Real],
        &[ValueType::Real, ValueType::Real, ValueType::Real],
        vec![
            (Arc::from("nin"), Value::Integer(2)),
            (Arc::from("nout"), Value::Integer(3)),
            (Arc::from("extract_1"), Value::Integer(2)),
            (Arc::from("extract_2"), Value::Integer(1)),
            (Arc::from("extract_3"), Value::Integer(2)),
        ],
    );
    assert!(
        validate(&duplicate_valid)
            .expect("duplicate selectors are source-valid when in range")
            .is_empty()
    );
}

#[test]
fn routing_real_vector_filter_mask_parameters_are_checked() {
    let missing = one_block_model("CDL.Routing.RealVectorFilter", &[], &[], vec![]);
    let err = validate(&missing).expect_err("VectorFilter nin and nout are required");
    assert_eq!(
        codes(&err.diagnostics),
        vec![
            DiagCode::MissingRequiredParameter,
            DiagCode::MissingRequiredParameter,
        ]
    );

    let bad_mask_type = one_block_model(
        "CDL.Routing.RealVectorFilter",
        &[ValueType::Real, ValueType::Real],
        &[ValueType::Real, ValueType::Real],
        vec![
            (Arc::from("nin"), Value::Integer(2)),
            (Arc::from("nout"), Value::Integer(2)),
            (Arc::from("msk_1"), Value::Real(1.0)),
        ],
    );
    let err = validate(&bad_mask_type).expect_err("msk_1 must be Boolean");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::ParameterOutOfRange]);
    assert!(err.diagnostics[0].message.contains("`msk_1`"));

    let count_mismatch = one_block_model(
        "CDL.Routing.RealVectorFilter",
        &[ValueType::Real, ValueType::Real, ValueType::Real],
        &[ValueType::Real, ValueType::Real],
        vec![
            (Arc::from("nin"), Value::Integer(3)),
            (Arc::from("nout"), Value::Integer(2)),
            (Arc::from("msk_1"), Value::Boolean(false)),
            (Arc::from("msk_2"), Value::Boolean(true)),
            (Arc::from("msk_3"), Value::Boolean(false)),
        ],
    );
    let err = validate(&count_mismatch).expect_err("mask true count must equal nout");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::ParameterOutOfRange]);
    assert!(err.diagnostics[0].message.contains("true count 1"));
    assert!(err.diagnostics[0].message.contains("`nout` = 2"));

    let sparse_default_valid = one_block_model(
        "CDL.Routing.RealVectorFilter",
        &[ValueType::Real, ValueType::Real, ValueType::Real],
        &[ValueType::Real, ValueType::Real],
        vec![
            (Arc::from("nin"), Value::Integer(3)),
            (Arc::from("nout"), Value::Integer(2)),
            (Arc::from("msk_2"), Value::Boolean(false)),
        ],
    );
    assert!(
        validate(&sparse_default_valid)
            .expect("omitted mask entries default to true")
            .is_empty()
    );
}

#[test]
fn routing_real_vector_replicator_product_width_is_checked() {
    let too_wide = one_block_model(
        "CDL.Routing.RealVectorReplicator",
        &[],
        &[],
        vec![
            (Arc::from("nin"), Value::Integer(1024)),
            (Arc::from("nout"), Value::Integer(1025)),
        ],
    );
    let err = validate(&too_wide).expect_err("nin*nout above max resolved width must fail");
    assert!(
        codes(&err.diagnostics).contains(&DiagCode::ParameterOutOfRange),
        "unexpected diagnostics: {:?}",
        err.diagnostics
    );
    assert!(
        err.diagnostics
            .iter()
            .any(|diag| diag.message.contains("nin*nout")),
        "unexpected diagnostics: {:?}",
        err.diagnostics
    );
}
