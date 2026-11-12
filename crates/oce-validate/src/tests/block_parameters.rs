//! `validate_block_parameters`: the load gate's parameter checks for one model-less block
//! (GitHub #440). Each case is cross-checked against a full [`validate`] run of a model holding
//! the same block, so the standalone entry cannot drift from the gate it mirrors.

use std::sync::Arc;

use oce_diag::DiagCode;
use oce_model::{BlockId, BlockInstance, ModelGraph, ParamTable, Value};

use crate::{validate, validate_block_parameters};

fn table(values: &[(&str, Value)]) -> ParamTable {
    ParamTable {
        values: values
            .iter()
            .map(|(name, value)| (Arc::from(*name), value.clone()))
            .collect(),
    }
}

/// The parameter diagnostic codes a whole-model validation reports for the same block, with its
/// connector-arity and port-type findings filtered out (the model has no ports).
fn whole_model_param_codes(class_path: &str, params: &ParamTable) -> Vec<DiagCode> {
    let mut model = ModelGraph::default();
    model.blocks.push(BlockInstance {
        id: BlockId(0),
        class_iri: Arc::from(class_path),
        inputs: Vec::new(),
        outputs: Vec::new(),
        params: params.clone(),
        decl_order: 0,
        instance_iri: None,
    });
    let diags = match validate(&model) {
        Ok(warnings) => warnings,
        Err(error) => error.diagnostics,
    };
    diags
        .into_iter()
        .map(|diag| diag.code)
        .filter(|code| {
            !matches!(
                code,
                DiagCode::MalformedDocument | DiagCode::PortKindMismatch
            )
        })
        .collect()
}

fn codes(class_path: &str, params: &ParamTable) -> Vec<DiagCode> {
    match validate_block_parameters(class_path, params) {
        Ok(warnings) => warnings.into_iter().map(|diag| diag.code).collect(),
        Err(error) => error
            .diagnostics
            .into_iter()
            .map(|diag| diag.code)
            .collect(),
    }
}

#[test]
fn valid_tables_pass_with_their_warnings() {
    let params = table(&[("nin", Value::Integer(3))]);
    assert_eq!(
        validate_block_parameters("CDL.Reals.MultiSum", &params).unwrap(),
        []
    );
    let equal_bounds = table(&[("uLow", Value::Real(1.0)), ("uHigh", Value::Real(1.0))]);
    let warnings = validate_block_parameters("CDL.Reals.Hysteresis", &equal_bounds).unwrap();
    assert!(!warnings.is_empty(), "equal hysteresis bounds warn");
    assert!(warnings.iter().all(|diag| !diag.is_error()));
}

#[test]
fn each_refusal_carries_the_load_gates_code() {
    let cases: [(&str, ParamTable, DiagCode); 5] = [
        (
            "CDL.Reals.MatrixMax",
            table(&[("nCol", Value::Integer(2))]),
            DiagCode::MissingRequiredParameter,
        ),
        (
            "CDL.Reals.MultiSum",
            table(&[("nin", Value::Integer(-1))]),
            DiagCode::ParameterOutOfRange,
        ),
        (
            "CDL.Reals.MultiSum",
            table(&[("nin", Value::Integer(1 << 21))]),
            DiagCode::ParameterOutOfRange,
        ),
        (
            "CDL.Reals.MultiSum",
            table(&[("k_1", Value::Real(f64::NAN)), ("nin", Value::Integer(1))]),
            DiagCode::NonFiniteParameter,
        ),
        (
            "CDL.Reals.MultiSum",
            table(&[("nin", Value::Integer(1)), ("nin", Value::Integer(2))]),
            DiagCode::DuplicateParameter,
        ),
    ];
    for (class_path, params, code) in cases {
        let error = validate_block_parameters(class_path, &params).unwrap_err();
        assert!(
            error.diagnostics.iter().any(|diag| diag.code == code),
            "{class_path}: expected {code:?}, got {:?}",
            error.diagnostics
        );
        assert_eq!(
            codes(class_path, &params),
            whole_model_param_codes(class_path, &params)
        );
    }
}

#[test]
fn unknown_classes_get_only_the_class_independent_checks() {
    assert_eq!(
        validate_block_parameters("CDL.Reals.NoSuchBlock", &table(&[])).unwrap(),
        []
    );
    let error = validate_block_parameters(
        "CDL.Reals.NoSuchBlock",
        &table(&[("k", Value::Real(f64::INFINITY))]),
    )
    .unwrap_err();
    assert_eq!(error.diagnostics[0].code, DiagCode::NonFiniteParameter);
}

#[test]
fn the_standalone_entry_agrees_with_whole_model_validation_on_every_class() {
    for entry in oce_blocks::catalog() {
        for params in [
            table(&[]),
            table(&[("nin", Value::Integer(2)), ("nout", Value::Integer(2))]),
            table(&[("nin", Value::Real(2.0))]),
        ] {
            assert_eq!(
                codes(entry.class_path, &params),
                whole_model_param_codes(entry.class_path, &params),
                "{}",
                entry.class_path
            );
        }
    }
}
