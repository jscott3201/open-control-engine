//! Block-parameter rules: positivity/range errors and equal-range warnings. The
//! required-parameter (missing-param) rules live in `params_required`.

use super::common::*;

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
fn vector_width_and_flattened_gain_parameters_are_checked() {
    let negative = one_block_model(
        "CDL.Logical.MultiAnd",
        &[],
        &[ValueType::Boolean],
        vec![(Arc::from("nin"), Value::Integer(-1))],
    );
    let err = validate(&negative).expect_err("negative nin must fail");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::ParameterOutOfRange]);
    assert!(err.diagnostics[0].message.contains("`nin`"));

    let bad_gain = one_block_model(
        "CDL.Integers.MultiSum",
        &[ValueType::Integer, ValueType::Integer],
        &[ValueType::Integer],
        vec![
            (Arc::from("nin"), Value::Integer(2)),
            (Arc::from("k_1"), Value::Integer(2)),
            (Arc::from("k_2"), Value::Real(3.0)),
        ],
    );
    let err = validate(&bad_gain).expect_err("supplied k_2 must be an Integer");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::ParameterOutOfRange]);
    assert!(err.diagnostics[0].message.contains("`k_2`"));

    let omitted_gains = one_block_model(
        "CDL.Integers.MultiSum",
        &[ValueType::Integer, ValueType::Integer],
        &[ValueType::Integer],
        vec![(Arc::from("nin"), Value::Integer(2))],
    );
    assert!(
        validate(&omitted_gains)
            .expect("omitted k_i elements default to fill(1,nin)")
            .is_empty()
    );

    let bad_real_gain = one_block_model(
        "CDL.Reals.MultiSum",
        &[ValueType::Real, ValueType::Real],
        &[ValueType::Real],
        vec![
            (Arc::from("nin"), Value::Integer(2)),
            (Arc::from("k_1"), Value::Real(0.5)),
            (Arc::from("k_2"), Value::Boolean(true)),
        ],
    );
    let err = validate(&bad_real_gain).expect_err("supplied k_2 must be numeric for Real gains");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::ParameterOutOfRange]);
    assert!(err.diagnostics[0].message.contains("`k_2`"));

    let integer_promoted_gain = one_block_model(
        "CDL.Reals.MultiSum",
        &[ValueType::Real, ValueType::Real],
        &[ValueType::Real],
        vec![
            (Arc::from("nin"), Value::Integer(2)),
            (Arc::from("k_1"), Value::Integer(2)),
        ],
    );
    assert!(
        validate(&integer_promoted_gain)
            .expect("integer literal k_i promotes to Real parameter")
            .is_empty()
    );

    let bad_matrix_gain = one_block_model(
        "CDL.Reals.MatrixGain",
        &[ValueType::Real, ValueType::Real],
        &[ValueType::Real, ValueType::Real],
        vec![
            (Arc::from("nout"), Value::Integer(2)),
            (Arc::from("nin"), Value::Integer(2)),
            (Arc::from("K_1_1"), Value::Real(1.0)),
            (Arc::from("K_1_2"), Value::Boolean(true)),
        ],
    );
    let err = validate(&bad_matrix_gain).expect_err("K_1_2 must be numeric");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::ParameterOutOfRange]);
    assert!(err.diagnostics[0].message.contains("`K_1_2`"));
}

#[test]
fn matrix_and_sort_dimensions_are_required_or_bounded() {
    let missing_matrix_dims = one_block_model(
        "CDL.Reals.MatrixMax",
        &[ValueType::Real],
        &[ValueType::Real],
        vec![],
    );
    let err = validate(&missing_matrix_dims).expect_err("nRow and nCol are required");
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
            .any(|diag| diag.message.contains("`nRow`"))
    );
    assert!(
        err.diagnostics
            .iter()
            .any(|diag| diag.message.contains("`nCol`"))
    );

    let zero_row = one_block_model(
        "CDL.Reals.MatrixMin",
        &[],
        &[],
        vec![
            (Arc::from("nRow"), Value::Integer(0)),
            (Arc::from("nCol"), Value::Integer(1)),
        ],
    );
    let err = validate(&zero_row).expect_err("nRow=0 must fail");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::ParameterOutOfRange]);
    assert!(err.diagnostics[0].message.contains("`nRow`"));

    let bad_sort = one_block_model(
        "CDL.Reals.Sort",
        &[],
        &[],
        vec![(Arc::from("nin"), Value::Integer(-1))],
    );
    let err = validate(&bad_sort).expect_err("negative Sort nin must fail");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::ParameterOutOfRange]);
    assert!(err.diagnostics[0].message.contains("`nin`"));
}

#[test]
fn sampled_discrete_sample_period_is_required_and_at_least_one_millisecond() {
    for class in [
        "CDL.Discrete.FirstOrderHold",
        "CDL.Discrete.Sampler",
        "CDL.Discrete.UnitDelay",
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
    // The PID-family zero cases moved to the inclusive upstream 100*eps floor
    // (`pid_range_rules_floor_at_the_upstream_min_annotation`); the strict `> 0` rules that
    // remain are LimitSlewRate.Td and MovingAverage.delta.
    let slew = one_block_model(
        "CDL.Reals.LimitSlewRate",
        &[ValueType::Real],
        &[ValueType::Real],
        vec![rp("raisingSlewRate", 1.0), rp("Td", 0.0)],
    );
    let err = validate(&slew).expect_err("LimitSlewRate.Td=0 must fail the strict-positive rule");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::ParameterOutOfRange]);
    assert_eq!(err.diagnostics[0].severity, Severity::Error);
    assert!(
        err.diagnostics[0].message.contains("Td"),
        "unexpected diagnostic: {:?}",
        err.diagnostics
    );

    let moving_average = one_block_model(
        "CDL.Reals.MovingAverage",
        &[ValueType::Real],
        &[ValueType::Real],
        vec![rp("delta", 0.0)],
    );
    let err =
        validate(&moving_average).expect_err("MovingAverage.delta=0 must fail the strict rule");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::ParameterOutOfRange]);
    assert_eq!(err.diagnostics[0].severity, Severity::Error);
    assert!(
        err.diagnostics[0].message.contains("delta"),
        "unexpected diagnostic: {:?}",
        err.diagnostics
    );
}

#[test]
fn pid_range_rules_floor_at_the_upstream_min_annotation() {
    // Upstream PID.mo/PIDWithReset.mo (pin a131864) annotate `min=100*Constants.eps` — an
    // INCLUSIVE Modelica bound — on exactly {k, Ti, Td, r, Ni, Nd}: zero and sub-floor values
    // are rejected, the exact floor value is legal, and NaN never satisfies the annotation.
    const FLOOR: f64 = 100.0 * 1e-15;
    let pid_io: (&[ValueType], &[ValueType]) =
        (&[ValueType::Real, ValueType::Real], &[ValueType::Real]);
    let reset_io: (&[ValueType], &[ValueType]) = (
        &[ValueType::Real, ValueType::Real, ValueType::Boolean],
        &[ValueType::Real],
    );
    for (class, io) in [
        ("CDL.Reals.PID", pid_io),
        ("CDL.Reals.PIDWithReset", reset_io),
    ] {
        for param in ["k", "Ti", "Td", "r", "Ni", "Nd"] {
            for bad in [0.0, 5e-14] {
                let model = one_block_model(class, io.0, io.1, vec![rp(param, bad)]);
                let err = match validate(&model) {
                    Ok(warnings) => panic!(
                        "{class}.{param}={bad} must fail the 100*eps floor, got {warnings:?}"
                    ),
                    Err(err) => err,
                };
                assert_eq!(
                    codes(&err.diagnostics),
                    vec![DiagCode::ParameterOutOfRange],
                    "{class}.{param}={bad}"
                );
                assert_eq!(err.diagnostics[0].severity, Severity::Error);
                assert!(
                    err.diagnostics[0].message.contains(param),
                    "{class}.{param}: {:?}",
                    err.diagnostics
                );
            }
            let at_floor = one_block_model(class, io.0, io.1, vec![rp(param, FLOOR)]);
            let warnings = validate(&at_floor)
                .unwrap_or_else(|err| panic!("{class}.{param}=100*eps is legal: {err:?}"));
            assert!(warnings.is_empty(), "{class}.{param}: {warnings:?}");
        }
        let nan_gain = one_block_model(class, io.0, io.1, vec![rp("k", f64::NAN)]);
        let err = validate(&nan_gain)
            .expect_err("a NaN gain never satisfies the upstream min annotation");
        assert_eq!(
            codes(&err.diagnostics),
            vec![DiagCode::ParameterOutOfRange],
            "{class}.k=NaN"
        );
    }
}

#[test]
fn pid_output_bound_pair_mirrors_the_limiter_rule() {
    // Upstream constrains yMin/yMax only transitively, through the instantiated
    // `Limiter lim(final uMax=yMax, final uMin=yMin)` and its `assert(uMin < uMax)`; the
    // engine mirrors its own Limiter precedent — error on inversion, warning on equality.
    let inverted = one_block_model(
        "CDL.Reals.PID",
        &[ValueType::Real, ValueType::Real],
        &[ValueType::Real],
        vec![rp("yMin", 0.5), rp("yMax", 0.2)],
    );
    let err = validate(&inverted).expect_err("yMin > yMax must fail");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::ParameterOutOfRange]);
    assert_eq!(err.diagnostics[0].severity, Severity::Error);
    assert!(err.diagnostics[0].message.contains("yMin <= yMax"));

    let equal = one_block_model(
        "CDL.Reals.PIDWithReset",
        &[ValueType::Real, ValueType::Real, ValueType::Boolean],
        &[ValueType::Real],
        vec![rp("yMin", 0.4), rp("yMax", 0.4)],
    );
    let warnings = validate(&equal).expect("yMin == yMax is a safe deterministic degrade");
    assert_eq!(codes(&warnings), vec![DiagCode::ParameterOutOfRange]);
    assert_eq!(warnings[0].severity, Severity::Warning);
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
fn hysteresis_threshold_pair_mirrors_the_limiter_rule() {
    // Upstream Hysteresis.mo has the initial-equation `assert(uHigh > uLow)`; the engine
    // errors on inversion and softens equality to a warning, like its Limiter precedent.
    let inverted = one_block_model(
        "CDL.Reals.Hysteresis",
        &[ValueType::Real],
        &[ValueType::Boolean],
        vec![rp("uLow", 2.0), rp("uHigh", 1.0)],
    );
    let err = validate(&inverted).expect_err("uLow > uHigh must fail");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::ParameterOutOfRange]);
    assert_eq!(err.diagnostics[0].severity, Severity::Error);
    assert!(err.diagnostics[0].message.contains("uLow <= uHigh"));

    let equal = one_block_model(
        "CDL.Reals.Hysteresis",
        &[ValueType::Real],
        &[ValueType::Boolean],
        vec![rp("uLow", 1.0), rp("uHigh", 1.0)],
    );
    let warnings = validate(&equal).expect("uLow == uHigh degrades deterministically");
    assert_eq!(codes(&warnings), vec![DiagCode::ParameterOutOfRange]);
    assert_eq!(warnings[0].severity, Severity::Warning);

    let healthy = one_block_model(
        "CDL.Reals.Hysteresis",
        &[ValueType::Real],
        &[ValueType::Boolean],
        vec![rp("uLow", 0.5), rp("uHigh", 1.0)],
    );
    let warnings = validate(&healthy).expect("a healthy threshold pair validates");
    assert!(warnings.is_empty(), "{warnings:?}");
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
fn variable_pulse_invalid_bounded_params_are_errors() {
    let missing = one_block_model(
        "CDL.Logical.VariablePulse",
        &[ValueType::Real],
        &[ValueType::Boolean],
        vec![],
    );
    let err = validate(&missing).expect_err("VariablePulse period is required");
    assert_eq!(
        codes(&err.diagnostics),
        vec![DiagCode::MissingRequiredParameter]
    );

    let bad_delta = one_block_model(
        "CDL.Logical.VariablePulse",
        &[ValueType::Real],
        &[ValueType::Boolean],
        vec![rp("period", 10.0), rp("deltaU", 0.000_1)],
    );
    let err = validate(&bad_delta).expect_err("deltaU below source min must fail");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::ParameterOutOfRange]);
    assert_eq!(err.diagnostics[0].severity, Severity::Error);
    assert!(err.diagnostics[0].message.contains("`deltaU`"));

    let bad_hold = one_block_model(
        "CDL.Logical.VariablePulse",
        &[ValueType::Real],
        &[ValueType::Boolean],
        vec![rp("period", 10.0), rp("minTruFalHol", 0.0)],
    );
    let err = validate(&bad_hold).expect_err("minTruFalHol below Constants.small must fail");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::ParameterOutOfRange]);
    assert_eq!(err.diagnostics[0].severity, Severity::Error);
    assert!(err.diagnostics[0].message.contains("`minTruFalHol`"));
}

#[test]
fn variable_pulse_short_period_is_warning_only() {
    let model = one_block_model(
        "CDL.Logical.VariablePulse",
        &[ValueType::Real],
        &[ValueType::Boolean],
        vec![rp("period", 0.0), rp("minTruFalHol", 1.0)],
    );
    let warnings = validate(&model).expect("Buildings source assert is warning-only");
    assert_eq!(codes(&warnings), vec![DiagCode::ParameterOutOfRange]);
    assert_eq!(warnings[0].severity, Severity::Warning);
    assert!(warnings[0].message.contains("period >= 2"));
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
