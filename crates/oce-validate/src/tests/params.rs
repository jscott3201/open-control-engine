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
fn triggered_moving_mean_n_parameter_is_required_and_positive() {
    let missing = one_block_model(
        "CDL.Discrete.TriggeredMovingMean",
        &[ValueType::Real, ValueType::Boolean],
        &[ValueType::Real],
        vec![],
    );
    let err = validate(&missing).expect_err("TriggeredMovingMean n is required");
    assert_eq!(
        codes(&err.diagnostics),
        vec![DiagCode::MissingRequiredParameter]
    );
    assert_eq!(err.diagnostics[0].severity, Severity::Error);
    assert!(err.diagnostics[0].message.contains("`n`"));

    let invalid = one_block_model(
        "CDL.Discrete.TriggeredMovingMean",
        &[ValueType::Real, ValueType::Boolean],
        &[ValueType::Real],
        vec![(Arc::from("n"), Value::Integer(0))],
    );
    let err = validate(&invalid).expect_err("TriggeredMovingMean n=0 must fail");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::ParameterOutOfRange]);
    assert_eq!(err.diagnostics[0].severity, Severity::Error);
    assert!(
        err.diagnostics[0]
            .message
            .contains("`CDL.Discrete.TriggeredMovingMean`")
    );
    assert!(err.diagnostics[0].message.contains("`n`"));

    assert!(
        validate(&one_block_model(
            "CDL.Discrete.TriggeredMovingMean",
            &[ValueType::Real, ValueType::Boolean],
            &[ValueType::Real],
            vec![(Arc::from("n"), Value::Integer(1))],
        ))
        .expect("TriggeredMovingMean n=1 is valid")
        .is_empty()
    );
}

#[test]
fn sampled_discrete_sample_period_is_required_and_at_least_one_millisecond() {
    for class in [
        "CDL.Discrete.FirstOrderHold",
        "CDL.Discrete.Sampler",
        "CDL.Discrete.ZeroOrderHold",
    ] {
        let missing = real_to_real_model(class, vec![]);
        let err = validate(&missing).expect_err("samplePeriod is required");
        assert_eq!(
            codes(&err.diagnostics),
            vec![DiagCode::MissingRequiredParameter],
            "{class}"
        );
        assert_eq!(err.diagnostics[0].severity, Severity::Error);
        assert!(err.diagnostics[0].message.contains("`samplePeriod`"));

        let invalid = real_to_real_model(class, vec![rp("samplePeriod", 0.0005)]);
        let err = validate(&invalid).expect_err("samplePeriod below 1E-3 must fail");
        assert_eq!(
            codes(&err.diagnostics),
            vec![DiagCode::ParameterOutOfRange],
            "{class}"
        );
        assert_eq!(err.diagnostics[0].severity, Severity::Error);
        assert!(err.diagnostics[0].message.contains("`samplePeriod`"));

        assert!(
            validate(&real_to_real_model(class, vec![rp("samplePeriod", 0.001)]))
                .expect("samplePeriod boundary is inclusive")
                .is_empty(),
            "{class}"
        );
    }
}

#[test]
fn real_source_ramp_duration_is_required_and_at_least_small() {
    let missing = one_block_model("CDL.Reals.Sources.Ramp", &[], &[ValueType::Real], vec![]);
    let err = validate(&missing).expect_err("Sources.Ramp duration is required");
    assert_eq!(
        codes(&err.diagnostics),
        vec![DiagCode::MissingRequiredParameter]
    );
    assert_eq!(err.diagnostics[0].severity, Severity::Error);
    assert!(err.diagnostics[0].message.contains("`duration`"));

    let invalid = one_block_model(
        "CDL.Reals.Sources.Ramp",
        &[],
        &[ValueType::Real],
        vec![rp("duration", 0.0)],
    );
    let err = validate(&invalid).expect_err("Sources.Ramp duration=0 must fail");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::ParameterOutOfRange]);
    assert_eq!(err.diagnostics[0].severity, Severity::Error);
    assert!(err.diagnostics[0].message.contains("`duration`"));
    assert!(
        err.diagnostics[0]
            .message
            .contains("`CDL.Reals.Sources.Ramp`")
    );

    assert!(
        validate(&one_block_model(
            "CDL.Reals.Sources.Ramp",
            &[],
            &[ValueType::Real],
            vec![rp("duration", 1e-37)],
        ))
        .expect("Sources.Ramp duration lower bound is inclusive")
        .is_empty()
    );
}

#[test]
fn typed_source_pulse_period_and_width_rules_match_cdl_bounds() {
    for (class, output) in [
        ("CDL.Logical.Sources.Pulse", ValueType::Boolean),
        ("CDL.Reals.Sources.Pulse", ValueType::Real),
        ("CDL.Integers.Sources.Pulse", ValueType::Integer),
    ] {
        let missing = one_block_model(class, &[], &[output], vec![]);
        let err = validate(&missing).expect_err("Sources.Pulse period is required");
        assert_eq!(
            codes(&err.diagnostics),
            vec![DiagCode::MissingRequiredParameter],
            "{class}"
        );
        assert_eq!(err.diagnostics[0].severity, Severity::Error);
        assert!(err.diagnostics[0].message.contains("`period`"));

        for params in [
            vec![rp("period", 0.0)],
            vec![rp("period", 1e-38)],
            vec![rp("period", 1.0), rp("width", 0.0)],
            vec![rp("period", 1.0), rp("width", 1.1)],
        ] {
            let invalid = one_block_model(class, &[], &[output], params);
            let err = validate(&invalid).expect_err("invalid pulse timing parameter must fail");
            assert_eq!(
                codes(&err.diagnostics),
                vec![DiagCode::ParameterOutOfRange],
                "{class}"
            );
            assert_eq!(err.diagnostics[0].severity, Severity::Error);
            assert!(
                err.diagnostics[0].message.contains("`period`")
                    || err.diagnostics[0].message.contains("`width`"),
                "{class}: {}",
                err.diagnostics[0].message
            );
        }

        assert!(
            validate(&one_block_model(
                class,
                &[],
                &[output],
                vec![rp("period", 1e-37), rp("width", 1e-37)],
            ))
            .expect("lower bounds are inclusive")
            .is_empty(),
            "{class}"
        );
        assert!(
            validate(&one_block_model(
                class,
                &[],
                &[output],
                vec![rp("period", 1.0), rp("width", 1.0)],
            ))
            .expect("width upper bound is inclusive")
            .is_empty(),
            "{class}"
        );
    }
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
fn stage_dependent_parameter_bounds_are_pinned() {
    let valid_base = || {
        vec![
            (Arc::from("n"), Value::Integer(4)),
            (Arc::from("holdDuration"), Value::Real(0.0)),
        ]
    };

    assert!(
        validate(&one_block_model(
            "CDL.Integers.Stage",
            &[ValueType::Real],
            &[ValueType::Integer],
            valid_base(),
        ))
        .expect("Stage n=4, holdDuration=0, default h is valid")
        .is_empty()
    );

    for h in [0.001 / 4.0, 0.5 / 4.0] {
        let mut params = valid_base();
        params.push(rp("h", h));
        assert!(
            validate(&one_block_model(
                "CDL.Integers.Stage",
                &[ValueType::Real],
                &[ValueType::Integer],
                params,
            ))
            .expect("Stage h boundary is inclusive")
            .is_empty(),
            "h={h}"
        );
    }

    let cases = [
        (
            vec![
                (Arc::from("n"), Value::Integer(0)),
                (Arc::from("holdDuration"), Value::Real(0.0)),
            ],
            "`n`",
        ),
        (
            vec![
                (Arc::from("n"), Value::Integer(4)),
                (Arc::from("holdDuration"), Value::Real(-1.0)),
            ],
            "`holdDuration`",
        ),
        (
            vec![
                (Arc::from("n"), Value::Integer(4)),
                (Arc::from("holdDuration"), Value::Real(0.0)),
                rp("h", 0.0002),
            ],
            "`h`",
        ),
        (
            vec![
                (Arc::from("n"), Value::Integer(4)),
                (Arc::from("holdDuration"), Value::Real(0.0)),
                rp("h", 0.126),
            ],
            "`h`",
        ),
    ];

    for (params, expected_param) in cases {
        let err = validate(&one_block_model(
            "CDL.Integers.Stage",
            &[ValueType::Real],
            &[ValueType::Integer],
            params,
        ))
        .expect_err("invalid Stage params must fail");
        assert_eq!(codes(&err.diagnostics), vec![DiagCode::ParameterOutOfRange]);
        assert_eq!(err.diagnostics[0].severity, Severity::Error);
        assert!(
            err.diagnostics[0].message.contains(expected_param),
            "unexpected diagnostic: {:?}",
            err.diagnostics
        );
    }
}

#[test]
fn ramp_parameter_bounds_are_pinned() {
    let valid_base = || {
        vec![
            rp("raisingSlewRate", 1e-37),
            rp("fallingSlewRate", -1e-37),
            rp("Td", 1e-15),
        ]
    };

    assert!(
        validate(&one_block_model(
            "CDL.Reals.Ramp",
            &[ValueType::Real, ValueType::Boolean],
            &[ValueType::Real],
            valid_base(),
        ))
        .expect("Ramp lower/upper boundaries are inclusive")
        .is_empty()
    );

    assert!(
        validate(&one_block_model(
            "CDL.Reals.Ramp",
            &[ValueType::Real, ValueType::Boolean],
            &[ValueType::Real],
            vec![rp("raisingSlewRate", 2.0)],
        ))
        .expect("fallingSlewRate and Td may be omitted and default from raisingSlewRate")
        .is_empty()
    );

    for (params, golden) in [
        (
            Vec::new(),
            vec![
                "error|missing-required-parameter|block#0|block `CDL.Reals.Ramp` is missing required parameter `raisingSlewRate`",
            ],
        ),
        (
            vec![
                rp("raisingSlewRate", 0.0),
                rp("fallingSlewRate", -1.0),
                rp("Td", 1e-15),
            ],
            vec![
                "error|parameter-out-of-range|block#0|parameter `raisingSlewRate` on block `CDL.Reals.Ramp` must be >= 0.0000000000000000000000000000000000001; got 0",
            ],
        ),
        (
            vec![
                rp("raisingSlewRate", 1.0),
                rp("fallingSlewRate", -1e-38),
                rp("Td", 1e-15),
            ],
            vec![
                "error|parameter-out-of-range|block#0|parameter `fallingSlewRate` on block `CDL.Reals.Ramp` must be <= -0.0000000000000000000000000000000000001; got -0.00000000000000000000000000000000000001",
            ],
        ),
        (
            vec![
                rp("raisingSlewRate", 1.0),
                rp("fallingSlewRate", -1.0),
                rp("Td", 0.0),
            ],
            vec![
                "error|parameter-out-of-range|block#0|parameter `Td` on block `CDL.Reals.Ramp` must be >= 0.000000000000001; got 0",
            ],
        ),
    ] {
        let err = validate(&one_block_model(
            "CDL.Reals.Ramp",
            &[ValueType::Real, ValueType::Boolean],
            &[ValueType::Real],
            params,
        ))
        .expect_err("invalid Ramp params must fail");
        let got: Vec<String> = err
            .diagnostics
            .iter()
            .map(|diag| {
                format!(
                    "{}|{}|{}|{}",
                    diag.severity.as_str(),
                    diag.code.as_str(),
                    diag.subject.as_deref().unwrap_or("<none>"),
                    diag.message
                )
            })
            .collect();
        assert_eq!(got, golden, "{:?}", err.diagnostics);
    }
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
            &[ValueType::Real, ValueType::Real, ValueType::Boolean],
            &[ValueType::Real],
            "Td",
        ),
        (
            "CDL.Reals.PIDWithReset",
            &[ValueType::Real, ValueType::Real, ValueType::Boolean],
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
        &[ValueType::Real, ValueType::Real, ValueType::Boolean],
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
fn proof_feedback_delay_less_than_debounce_is_warning_only() {
    let model = one_block_model(
        "CDL.Logical.Proof",
        &[ValueType::Boolean, ValueType::Boolean],
        &[ValueType::Boolean, ValueType::Boolean],
        vec![rp("debounce", 5.0), rp("feedbackDelay", 2.0)],
    );
    let warnings = validate(&model).expect("Proof source assert is warning-only");
    assert_eq!(codes(&warnings), vec![DiagCode::ParameterOutOfRange]);
    assert_eq!(warnings[0].severity, Severity::Warning);
    assert!(warnings[0].message.contains("debounce <= feedbackDelay"));
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
