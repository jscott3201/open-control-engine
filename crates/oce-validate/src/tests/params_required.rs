//! Required-parameter rules: every parameter upstream declares with NO default value must be
//! present in the model — omitting one is an authoring error, never a silent engine default.

use super::common::*;
use oce_blocks::{ParamRule, PortKind, catalog, lookup};

fn value_type(kind: PortKind) -> ValueType {
    match kind {
        PortKind::Real => ValueType::Real,
        PortKind::Integer => ValueType::Integer,
        PortKind::Boolean => ValueType::Boolean,
    }
}

fn representative_value(kind: ValueType) -> Value {
    match kind {
        ValueType::Real => Value::Real(1.0),
        ValueType::Integer => Value::Integer(1),
        ValueType::Boolean => Value::Boolean(true),
        ValueType::String => Value::String(Arc::from("required parameter")),
        ValueType::Enum(class) => Value::Enum { class, ordinal: 1 },
    }
}

fn deliberately_wrong_value(kind: ValueType) -> Value {
    match kind {
        ValueType::Real | ValueType::Integer | ValueType::String | ValueType::Enum(_) => {
            Value::Boolean(false)
        }
        ValueType::Boolean => Value::Integer(1),
    }
}

fn validation_diagnostics(model: &ModelGraph) -> Vec<oce_diag::Diagnostic> {
    match validate(model) {
        Ok(warnings) => warnings,
        Err(err) => err.diagnostics,
    }
}

fn requires_flattened_table_fixture(class: &str, name: &str) -> bool {
    matches!(
        (class, name),
        (
            "CDL.Logical.Sources.TimeTable" | "CDL.Integers.Sources.TimeTable",
            "period"
        )
    )
}

#[test]
fn every_required_catalog_kind_accepts_a_matching_value_and_rejects_a_wrong_value() {
    let mut exercised = 0;
    let mut excluded = Vec::new();
    for entry in catalog() {
        let required: Vec<_> = entry
            .param_rules
            .iter()
            .filter_map(|rule| match rule {
                ParamRule::Required { name, kind } => Some((*name, *kind)),
                _ => None,
            })
            .collect();
        if required.is_empty() {
            continue;
        }

        let matching_params: Vec<_> = required
            .iter()
            .map(|(name, kind)| (Arc::from(*name), representative_value(*kind)))
            .collect();

        for &(name, kind) in &required {
            // TimeTable validation requires a consistent flattened `table` matrix in addition to
            // the scalar `period`. The generic one-block scalar harness cannot express that shape;
            // dedicated TimeTable tests cover it with real flattened fixtures.
            if requires_flattened_table_fixture(entry.class_path, name) {
                excluded.push(format!("{}.{}", entry.class_path, name));
                continue;
            }
            exercised += 1;
            let params = ParamTable {
                values: matching_params.clone(),
            };
            let block = (lookup(entry.class_path)
                .expect("catalog class is registered")
                .make)(&params);
            let signature = block.resolved_signature();
            let inputs: Vec<_> = signature.inputs.iter().copied().map(value_type).collect();
            let outputs: Vec<_> = signature.outputs.iter().copied().map(value_type).collect();
            let matching =
                one_block_model(entry.class_path, &inputs, &outputs, matching_params.clone());
            let matching_diags = validation_diagnostics(&matching);
            assert!(
                matching_diags.iter().all(|diag| !diag.is_error()),
                "matching {kind:?} rejected for {}.{name}: {matching_diags:?}",
                entry.class_path
            );

            let mut wrong_params = matching_params.clone();
            let (_, value) = wrong_params
                .iter_mut()
                .find(|(param_name, _)| param_name.as_ref() == name)
                .expect("target required parameter was collected above");
            *value = deliberately_wrong_value(kind);
            let wrong = one_block_model(entry.class_path, &inputs, &outputs, wrong_params);
            let wrong_diags = validation_diagnostics(&wrong);
            assert!(
                wrong_diags.iter().any(|diag| {
                    diag.code == DiagCode::ParameterKindMismatch
                        && diag.message.contains(&format!("parameter `{name}`"))
                }),
                "wrong value was not rejected for {}.{name} declared {kind:?}: {wrong_diags:?}",
                entry.class_path
            );
        }
    }
    assert_eq!(exercised, 47);
    assert_eq!(
        excluded,
        [
            "CDL.Logical.Sources.TimeTable.period",
            "CDL.Integers.Sources.TimeTable.period",
        ]
    );
}

#[test]
fn required_real_parameter_rejects_boolean_value() {
    let model = one_block_model(
        "CDL.Reals.Sources.Constant",
        &[],
        &[ValueType::Real],
        vec![(Arc::from("k"), Value::Boolean(true))],
    );
    let err = validate(&model).expect_err("Boolean k must not execute as the Real fallback");
    assert_eq!(
        codes(&err.diagnostics),
        vec![DiagCode::ParameterKindMismatch]
    );
    assert_eq!(err.diagnostics[0].severity, Severity::Error);
    assert_eq!(
        err.diagnostics[0].message,
        "parameter `k` on block `CDL.Reals.Sources.Constant` must have kind Real; got Boolean"
    );
}

#[test]
fn required_integer_and_boolean_parameters_reject_wrong_kinds() {
    for (class, output, value, expected, actual) in [
        (
            "CDL.Integers.Sources.Constant",
            ValueType::Integer,
            Value::Boolean(true),
            "Integer",
            "Boolean",
        ),
        (
            "CDL.Logical.Sources.Constant",
            ValueType::Boolean,
            Value::Real(3.5),
            "Boolean",
            "Real",
        ),
    ] {
        let model = one_block_model(class, &[], &[output], vec![(Arc::from("k"), value)]);
        let err = validate(&model).expect_err("wrong-kind required parameter must fail");
        assert_eq!(
            codes(&err.diagnostics),
            vec![DiagCode::ParameterKindMismatch]
        );
        assert!(err.diagnostics[0].message.contains(expected));
        assert!(err.diagnostics[0].message.contains(actual));
        assert!(err.diagnostics[0].message.contains("`k`"));
    }
}

#[test]
fn required_scalar_parameters_accept_declared_kinds_and_real_widening() {
    for (class, output, value) in [
        (
            "CDL.Reals.Sources.Constant",
            ValueType::Real,
            Value::Real(3.5),
        ),
        (
            "CDL.Integers.Sources.Constant",
            ValueType::Integer,
            Value::Integer(7),
        ),
        (
            "CDL.Logical.Sources.Constant",
            ValueType::Boolean,
            Value::Boolean(true),
        ),
        (
            "CDL.Reals.Sources.Constant",
            ValueType::Real,
            Value::Integer(3),
        ),
    ] {
        let model = one_block_model(class, &[], &[output], vec![(Arc::from("k"), value)]);
        assert!(
            validate(&model)
                .expect("declared kind must load")
                .is_empty()
        );
    }
}

#[test]
fn missing_required_sample_trigger_period_is_an_error() {
    let model = one_block_model(
        "CDL.Logical.Sources.SampleTrigger",
        &[],
        &[ValueType::Boolean],
        vec![],
    );
    let err = validate(&model).expect_err("missing required period must fail");
    assert_eq!(
        codes(&err.diagnostics),
        vec![DiagCode::MissingRequiredParameter]
    );
    assert_eq!(err.diagnostics[0].severity, Severity::Error);
    assert_eq!(err.diagnostics[0].subject.as_deref(), Some("block#0"));
    assert!(err.diagnostics[0].message.contains("`period`"));
}

#[test]
fn missing_required_proof_parameters_are_errors() {
    let model = one_block_model(
        "CDL.Logical.Proof",
        &[ValueType::Boolean, ValueType::Boolean],
        &[ValueType::Boolean, ValueType::Boolean],
        vec![],
    );
    let err = validate(&model).expect_err("Proof debounce and feedbackDelay are required");
    assert_eq!(
        codes(&err.diagnostics),
        vec![
            DiagCode::MissingRequiredParameter,
            DiagCode::MissingRequiredParameter,
        ]
    );
    assert!(
        err.diagnostics
            .iter()
            .all(|diag| diag.severity == Severity::Error)
    );
    assert!(err.diagnostics[0].message.contains("`debounce`"));
    assert!(err.diagnostics[1].message.contains("`feedbackDelay`"));
}

#[test]
fn missing_required_stage_parameters_are_errors() {
    let model = one_block_model(
        "CDL.Integers.Stage",
        &[ValueType::Real],
        &[ValueType::Integer],
        vec![],
    );
    let err = validate(&model).expect_err("Stage n and holdDuration are required");
    assert_eq!(
        codes(&err.diagnostics),
        vec![
            DiagCode::MissingRequiredParameter,
            DiagCode::MissingRequiredParameter,
        ]
    );
    assert!(
        err.diagnostics
            .iter()
            .all(|diag| diag.severity == Severity::Error)
    );
    assert!(
        err.diagnostics
            .iter()
            .any(|diag| diag.message.contains("`n`"))
    );
    assert!(
        err.diagnostics
            .iter()
            .any(|diag| diag.message.contains("`holdDuration`"))
    );
}

#[test]
fn missing_upstream_no_default_parameters_are_errors() {
    // Upstream (pin a131864) declares each of these with NO default value: omitting one is an
    // authoring error that previously fell through to a silent engine default
    // (n=0 / p=0 / k=1 or 0 / k=false / delayTime=0 / trueHoldDuration=0 / raisingSlewRate=1 /
    // delta=1), silently changing behavior.
    let cases: &[(&str, &[ValueType], &[ValueType], &str)] = &[
        (
            "CDL.Reals.Round",
            &[ValueType::Real],
            &[ValueType::Real],
            "n",
        ),
        (
            "CDL.Logical.Sources.Constant",
            &[],
            &[ValueType::Boolean],
            "k",
        ),
        (
            "CDL.Logical.TrueDelay",
            &[ValueType::Boolean],
            &[ValueType::Boolean],
            "delayTime",
        ),
        (
            "CDL.Logical.TrueFalseHold",
            &[ValueType::Boolean],
            &[ValueType::Boolean],
            "trueHoldDuration",
        ),
        (
            "CDL.Reals.AddParameter",
            &[ValueType::Real],
            &[ValueType::Real],
            "p",
        ),
        (
            "CDL.Reals.MultiplyByParameter",
            &[ValueType::Real],
            &[ValueType::Real],
            "k",
        ),
        ("CDL.Reals.Sources.Constant", &[], &[ValueType::Real], "k"),
        (
            "CDL.Integers.Sources.Constant",
            &[],
            &[ValueType::Integer],
            "k",
        ),
        (
            "CDL.Integers.AddParameter",
            &[ValueType::Integer],
            &[ValueType::Integer],
            "p",
        ),
        (
            "CDL.Reals.LimitSlewRate",
            &[ValueType::Real],
            &[ValueType::Real],
            "raisingSlewRate",
        ),
        (
            "CDL.Reals.MovingAverage",
            &[ValueType::Real],
            &[ValueType::Real],
            "delta",
        ),
    ];
    for (class, inputs, outputs, param) in cases {
        let model = one_block_model(class, inputs, outputs, vec![]);
        let err = match validate(&model) {
            Ok(warnings) => panic!("{class} without `{param}` must fail, got {warnings:?}"),
            Err(err) => err,
        };
        assert_eq!(
            codes(&err.diagnostics),
            vec![DiagCode::MissingRequiredParameter],
            "{class}"
        );
        assert_eq!(err.diagnostics[0].severity, Severity::Error);
        assert!(
            err.diagnostics[0].message.contains(&format!("`{param}`")),
            "{class}: {:?}",
            err.diagnostics
        );
    }
}

#[test]
fn missing_required_assert_message_is_an_error() {
    // Upstream `Utilities/Assert.mo` declares `parameter String message` with NO default; omitting
    // it previously fell through to a silent empty-string engine default, emitting a blank
    // diagnostic when the assertion trips. Assert has a Boolean input and NO output connectors.
    let model = one_block_model("CDL.Utilities.Assert", &[ValueType::Boolean], &[], vec![]);
    let err = validate(&model).expect_err("Assert.message is required");
    assert_eq!(
        codes(&err.diagnostics),
        vec![DiagCode::MissingRequiredParameter]
    );
    assert_eq!(err.diagnostics[0].severity, Severity::Error);
    assert!(err.diagnostics[0].message.contains("`message`"));
}

#[test]
fn missing_bound_pair_parameters_report_both_errors() {
    // Limiter uMax/uMin and Hysteresis uLow/uHigh all lack upstream defaults: omitting both
    // reports two missing-required errors (order-independent assertion — diagnostics sort by
    // message within a code).
    struct BoundPair {
        class: &'static str,
        inputs: &'static [ValueType],
        outputs: &'static [ValueType],
        params: [&'static str; 2],
    }
    let cases = [
        BoundPair {
            class: "CDL.Reals.Limiter",
            inputs: &[ValueType::Real],
            outputs: &[ValueType::Real],
            params: ["uMin", "uMax"],
        },
        BoundPair {
            class: "CDL.Reals.Hysteresis",
            inputs: &[ValueType::Real],
            outputs: &[ValueType::Boolean],
            params: ["uLow", "uHigh"],
        },
    ];
    for BoundPair {
        class,
        inputs,
        outputs,
        params,
    } in cases
    {
        let model = one_block_model(class, inputs, outputs, vec![]);
        let err = match validate(&model) {
            Ok(warnings) => panic!("{class} without bounds must fail, got {warnings:?}"),
            Err(err) => err,
        };
        assert_eq!(
            codes(&err.diagnostics),
            vec![
                DiagCode::MissingRequiredParameter,
                DiagCode::MissingRequiredParameter,
            ],
            "{class}"
        );
        for param in params {
            assert!(
                err.diagnostics
                    .iter()
                    .any(|diag| diag.message.contains(&format!("`{param}`"))),
                "{class} must name `{param}`: {:?}",
                err.diagnostics
            );
        }
    }
}
