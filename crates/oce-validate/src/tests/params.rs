//! Block-parameter rules: required params, positivity/range errors, and equal-range warnings.

use super::common::*;

fn one_block_model(
    class: &str,
    inputs: &[ValueType],
    outputs: &[ValueType],
    params: Vec<(Arc<str>, Value)>,
) -> ModelGraph {
    let mut connectors = Vec::with_capacity(inputs.len() + outputs.len());
    let mut input_ids = Vec::with_capacity(inputs.len());
    let mut output_ids = Vec::with_capacity(outputs.len());
    for (idx, value_type) in inputs.iter().copied().enumerate() {
        let id = idx as u32;
        connectors.push(conn(id, 0, Dir::In, value_type));
        input_ids.push(id);
    }
    for (offset, value_type) in outputs.iter().copied().enumerate() {
        let id = (inputs.len() + offset) as u32;
        connectors.push(conn(id, 0, Dir::Out, value_type));
        output_ids.push(id);
    }
    ModelGraph {
        blocks: vec![block_with_params(0, class, &input_ids, &output_ids, params)],
        connectors,
        connections: vec![],
        external_inputs: input_ids.into_iter().map(ConnectorId).collect(),
    }
}

fn real_to_real_model(class: &str, params: Vec<(Arc<str>, Value)>) -> ModelGraph {
    one_block_model(class, &[ValueType::Real], &[ValueType::Real], params)
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
fn sample_trigger_period_zero_rejection_is_pinned() {
    let model = one_block_model(
        "CDL.Logical.Sources.SampleTrigger",
        &[],
        &[ValueType::Boolean],
        vec![rp("period", 0.0)],
    );
    let err = validate(&model).expect_err("period=0 is a behavior-changing shall-error");
    let golden: Vec<String> = err
        .diagnostics
        .iter()
        .map(|d| {
            format!(
                "{}|{}|{}|{}",
                d.severity.as_str(),
                d.code.as_str(),
                d.subject.as_deref().unwrap_or("<none>"),
                d.message
            )
        })
        .collect();
    assert_eq!(
        golden,
        vec![
            "error|parameter-out-of-range|block#0|parameter `period` on block \
             `CDL.Logical.Sources.SampleTrigger` must be > 0; got 0"
                .to_string()
        ]
    );
}

#[test]
fn strict_positive_param_rules_reject_zero() {
    let cases: &[(&str, &[ValueType], &[ValueType], &str)] = &[
        (
            "CDL.Reals.Derivative",
            &[ValueType::Real],
            &[ValueType::Real],
            "T",
        ),
        (
            "CDL.Reals.LimitSlewRate",
            &[ValueType::Real],
            &[ValueType::Real],
            "Td",
        ),
        (
            "CDL.Reals.MovingAverage",
            &[ValueType::Real],
            &[ValueType::Real],
            "delta",
        ),
        (
            "CDL.Reals.PID",
            &[ValueType::Real, ValueType::Real],
            &[ValueType::Real],
            "Td",
        ),
        (
            "CDL.Reals.PID",
            &[ValueType::Real, ValueType::Real],
            &[ValueType::Real],
            "Nd",
        ),
        (
            "CDL.Reals.PIDWithReset",
            &[
                ValueType::Real,
                ValueType::Real,
                ValueType::Boolean,
                ValueType::Real,
            ],
            &[ValueType::Real],
            "Td",
        ),
        (
            "CDL.Reals.PIDWithReset",
            &[
                ValueType::Real,
                ValueType::Real,
                ValueType::Boolean,
                ValueType::Real,
            ],
            &[ValueType::Real],
            "Nd",
        ),
    ];
    for (class, inputs, outputs, param) in cases {
        let model = one_block_model(class, inputs, outputs, vec![rp(param, 0.0)]);
        let err = match validate(&model) {
            Ok(warnings) => panic!(
                "{class}.{param}=0 must fail the strict-positive rule, got warnings: {warnings:?}"
            ),
            Err(err) => err,
        };
        assert_eq!(
            codes(&err.diagnostics),
            vec![DiagCode::ParameterOutOfRange],
            "{class}.{param}"
        );
        assert_eq!(err.diagnostics[0].severity, Severity::Error);
        assert!(
            err.diagnostics[0].message.contains(param),
            "{class}.{param}: {:?}",
            err.diagnostics
        );
    }
}

#[test]
fn pid_with_reset_zero_td_is_rejected() {
    let model = one_block_model(
        "CDL.Reals.PIDWithReset",
        &[
            ValueType::Real,
            ValueType::Real,
            ValueType::Boolean,
            ValueType::Real,
        ],
        &[ValueType::Real],
        vec![rp("Td", 0.0)],
    );
    let err = validate(&model).expect_err("PIDWithReset.Td=0 must fail");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::ParameterOutOfRange]);
    assert_eq!(err.diagnostics[0].severity, Severity::Error);
    assert!(
        err.diagnostics[0].message.contains("`Td`"),
        "unexpected diagnostic: {:?}",
        err.diagnostics
    );
}

#[test]
fn limiter_inverted_bounds_are_an_error() {
    let model = real_to_real_model("CDL.Reals.Limiter", vec![rp("uMin", 2.0), rp("uMax", 1.0)]);
    let err = validate(&model).expect_err("uMin > uMax must fail");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::ParameterOutOfRange]);
    assert_eq!(err.diagnostics[0].severity, Severity::Error);
    assert!(err.diagnostics[0].message.contains("uMin <= uMax"));
}

#[test]
fn limiter_equal_bounds_are_a_warning_only() {
    let model = real_to_real_model("CDL.Reals.Limiter", vec![rp("uMin", 1.0), rp("uMax", 1.0)]);
    let warnings = validate(&model).expect("uMin == uMax is a safe deterministic degrade");
    assert_eq!(codes(&warnings), vec![DiagCode::ParameterOutOfRange]);
    assert_eq!(warnings[0].severity, Severity::Warning);
    assert!(warnings[0].message.contains("clamp to a constant"));
}

#[test]
fn integer_literals_are_valid_for_real_param_rules() {
    let model = one_block_model(
        "CDL.Logical.Sources.SampleTrigger",
        &[],
        &[ValueType::Boolean],
        vec![(Arc::from("period"), Value::Integer(1))],
    );
    assert!(
        validate(&model)
            .expect("integer literal promotes for Real parameter rules")
            .is_empty()
    );
}
