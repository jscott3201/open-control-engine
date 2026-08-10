//! Source and psychrometric block-parameter validation cases.

use super::common::*;

fn calendar_time_outputs() -> [ValueType; 6] {
    [
        ValueType::Integer,
        ValueType::Integer,
        ValueType::Integer,
        ValueType::Integer,
        ValueType::Real,
        ValueType::Integer,
    ]
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
fn calendar_time_parameters_bind_zero_time_and_source_year_bounds() {
    let outputs = calendar_time_outputs();
    let missing = one_block_model("CDL.Reals.Sources.CalendarTime", &[], &outputs, vec![]);
    let err = validate(&missing).expect_err("CalendarTime zerTim is required");
    assert_eq!(
        codes(&err.diagnostics),
        vec![DiagCode::MissingRequiredParameter]
    );
    assert_eq!(err.diagnostics[0].severity, Severity::Error);
    assert!(err.diagnostics[0].message.contains("`zerTim`"));

    for (value, expected_codes) in [
        (Value::Integer(0), vec![DiagCode::ParameterOutOfRange]),
        (Value::Integer(45), vec![DiagCode::ParameterOutOfRange]),
        (
            Value::String(Arc::from(
                "Buildings.Controls.OBC.CDL.Types.ZeroTime.NY2051",
            )),
            vec![DiagCode::ParameterOutOfRange],
        ),
        (
            Value::Boolean(true),
            vec![
                DiagCode::ParameterKindMismatch,
                DiagCode::ParameterOutOfRange,
            ],
        ),
    ] {
        let invalid = one_block_model(
            "CDL.Reals.Sources.CalendarTime",
            &[],
            &outputs,
            vec![(Arc::from("zerTim"), value)],
        );
        let err = validate(&invalid).expect_err("invalid ZeroTime value must fail");
        assert_eq!(codes(&err.diagnostics), expected_codes);
        assert!(
            err.diagnostics
                .iter()
                .all(|diag| diag.severity == Severity::Error)
        );
        assert!(
            err.diagnostics
                .iter()
                .all(|diag| diag.message.contains("`zerTim`"))
        );
    }

    for value in [
        Value::Integer(2009),
        Value::Integer(2032),
        Value::Real(2016.0),
    ] {
        let invalid = one_block_model(
            "CDL.Reals.Sources.CalendarTime",
            &[],
            &outputs,
            vec![
                (Arc::from("zerTim"), Value::Integer(11)),
                (Arc::from("yearRef"), value),
            ],
        );
        let err = validate(&invalid).expect_err("invalid CalendarTime yearRef must fail");
        assert!(
            codes(&err.diagnostics)
                .iter()
                .all(|code| *code == DiagCode::ParameterOutOfRange)
        );
        assert!(
            err.diagnostics
                .iter()
                .all(|diag| diag.severity == Severity::Error)
        );
        assert!(
            err.diagnostics
                .iter()
                .all(|diag| diag.message.contains("`yearRef`"))
        );
    }

    for value in [
        Value::Real(f64::NAN),
        Value::Real(f64::INFINITY),
        Value::Real(f64::NEG_INFINITY),
        Value::Boolean(false),
    ] {
        let invalid = one_block_model(
            "CDL.Reals.Sources.CalendarTime",
            &[],
            &outputs,
            vec![
                (Arc::from("zerTim"), Value::Integer(11)),
                (Arc::from("offset"), value),
            ],
        );
        let err = validate(&invalid).expect_err("invalid CalendarTime offset must fail");
        assert_eq!(codes(&err.diagnostics), vec![DiagCode::ParameterOutOfRange]);
        assert_eq!(err.diagnostics[0].severity, Severity::Error);
        assert!(err.diagnostics[0].message.contains("`offset`"));
    }

    for zer_tim in [
        Value::Integer(11),
        Value::Enum {
            class: oce_model::EnumClassId::ZERO_TIME,
            ordinal: 11,
        },
        Value::String(Arc::from(
            "Buildings.Controls.OBC.CDL.Types.ZeroTime.NY2017",
        )),
    ] {
        assert!(
            validate(&one_block_model(
                "CDL.Reals.Sources.CalendarTime",
                &[],
                &outputs,
                vec![
                    (Arc::from("zerTim"), zer_tim),
                    (Arc::from("yearRef"), Value::Integer(2031)),
                    (Arc::from("offset"), Value::Integer(0)),
                ],
            ))
            .expect("valid CalendarTime parameters pass")
            .is_empty()
        );
    }
}

#[test]
fn real_source_sin_frequency_is_required() {
    let missing = one_block_model("CDL.Reals.Sources.Sin", &[], &[ValueType::Real], vec![]);
    let err = validate(&missing).expect_err("Sources.Sin freqHz is required");
    assert_eq!(
        codes(&err.diagnostics),
        vec![DiagCode::MissingRequiredParameter]
    );
    assert_eq!(err.diagnostics[0].severity, Severity::Error);
    assert!(err.diagnostics[0].message.contains("`freqHz`"));

    let wrong_frequency_type = one_block_model(
        "CDL.Reals.Sources.Sin",
        &[],
        &[ValueType::Real],
        vec![(Arc::from("freqHz"), Value::Boolean(false))],
    );
    let err = validate(&wrong_frequency_type).expect_err("Sources.Sin freqHz must be Real");
    assert_eq!(
        codes(&err.diagnostics),
        vec![
            DiagCode::ParameterKindMismatch,
            DiagCode::ParameterOutOfRange,
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
            .all(|diag| diag.message.contains("`freqHz`"))
    );
    assert!(
        err.diagnostics[0]
            .message
            .contains("`CDL.Reals.Sources.Sin`")
    );

    let wrong_optional_type = one_block_model(
        "CDL.Reals.Sources.Sin",
        &[],
        &[ValueType::Real],
        vec![
            rp("freqHz", 0.25),
            (Arc::from("amplitude"), Value::Boolean(true)),
        ],
    );
    let err = validate(&wrong_optional_type).expect_err("Sources.Sin amplitude must be Real");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::ParameterOutOfRange]);
    assert_eq!(err.diagnostics[0].severity, Severity::Error);
    assert!(err.diagnostics[0].message.contains("`amplitude`"));

    assert!(
        validate(&one_block_model(
            "CDL.Reals.Sources.Sin",
            &[],
            &[ValueType::Real],
            vec![rp("freqHz", 0.25)],
        ))
        .expect("Sources.Sin has no source assertion on finite frequency")
        .is_empty()
    );
}

#[test]
fn psychrometric_specific_enthalpy_atmospheric_pressure_must_be_finite_positive_when_present() {
    let wrong_type = one_block_model(
        "CDL.Psychrometrics.SpecificEnthalpy_TDryBulPhi",
        &[ValueType::Real, ValueType::Real],
        &[ValueType::Real],
        vec![(Arc::from("pAtm"), Value::Boolean(true))],
    );
    let err = validate(&wrong_type).expect_err("pAtm must be numeric when present");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::ParameterOutOfRange]);
    assert_eq!(err.diagnostics[0].severity, Severity::Error);
    assert!(err.diagnostics[0].message.contains("`pAtm`"));
    assert!(
        err.diagnostics[0]
            .message
            .contains("`CDL.Psychrometrics.SpecificEnthalpy_TDryBulPhi`")
    );

    for value in [
        Value::Real(0.0),
        Value::Real(-1.0),
        Value::Real(f64::NAN),
        Value::Real(f64::INFINITY),
        Value::Real(f64::NEG_INFINITY),
    ] {
        let invalid = one_block_model(
            "CDL.Psychrometrics.SpecificEnthalpy_TDryBulPhi",
            &[ValueType::Real, ValueType::Real],
            &[ValueType::Real],
            vec![(Arc::from("pAtm"), value)],
        );
        let err = validate(&invalid).expect_err("pAtm must be finite and positive");
        assert_eq!(codes(&err.diagnostics), vec![DiagCode::ParameterOutOfRange]);
        assert_eq!(err.diagnostics[0].severity, Severity::Error);
        assert!(err.diagnostics[0].message.contains("`pAtm`"));
    }

    assert!(
        validate(&one_block_model(
            "CDL.Psychrometrics.SpecificEnthalpy_TDryBulPhi",
            &[ValueType::Real, ValueType::Real],
            &[ValueType::Real],
            vec![(Arc::from("pAtm"), Value::Integer(101_325))],
        ))
        .expect("Integer literal can bind to Real pAtm by CDL numeric promotion")
        .is_empty()
    );
}

#[test]
fn sun_rise_set_parameters_are_required_bounded_and_finite() {
    let missing = one_block_model(
        "CDL.Utilities.SunRiseSet",
        &[],
        &[ValueType::Real, ValueType::Real, ValueType::Boolean],
        vec![],
    );
    let err = validate(&missing).expect_err("SunRiseSet coordinates and timezone are required");
    assert_eq!(
        codes(&err.diagnostics),
        vec![
            DiagCode::MissingRequiredParameter,
            DiagCode::MissingRequiredParameter,
            DiagCode::MissingRequiredParameter,
        ]
    );
    assert!(
        err.diagnostics
            .iter()
            .all(|diag| diag.severity == Severity::Error)
    );
    assert!(err.diagnostics[0].message.contains("`lat`"));
    assert!(err.diagnostics[1].message.contains("`lon`"));
    assert!(err.diagnostics[2].message.contains("`timZon`"));

    for (name, value) in [
        ("lat", std::f64::consts::FRAC_PI_2 + 0.001),
        ("lat", -std::f64::consts::FRAC_PI_2 - 0.001),
        ("lon", std::f64::consts::PI + 0.001),
        ("lon", -std::f64::consts::PI - 0.001),
        ("timZon", f64::NAN),
        ("timZon", f64::INFINITY),
        ("timZon", f64::NEG_INFINITY),
    ] {
        let params = match name {
            "lat" => vec![rp("lat", value), rp("lon", 0.0), rp("timZon", 0.0)],
            "lon" => vec![rp("lat", 0.0), rp("lon", value), rp("timZon", 0.0)],
            "timZon" => vec![rp("lat", 0.0), rp("lon", 0.0), rp("timZon", value)],
            _ => unreachable!("test table only uses known SunRiseSet params"),
        };
        let invalid = one_block_model(
            "CDL.Utilities.SunRiseSet",
            &[],
            &[ValueType::Real, ValueType::Real, ValueType::Boolean],
            params,
        );
        let err = validate(&invalid).expect_err(&format!(
            "SunRiseSet parameter {name}={value} must be rejected"
        ));
        assert_eq!(codes(&err.diagnostics), vec![DiagCode::ParameterOutOfRange]);
        assert_eq!(err.diagnostics[0].severity, Severity::Error);
        assert!(err.diagnostics[0].message.contains(&format!("`{name}`")));
    }

    assert!(
        validate(&one_block_model(
            "CDL.Utilities.SunRiseSet",
            &[],
            &[ValueType::Real, ValueType::Real, ValueType::Boolean],
            vec![
                rp("lat", std::f64::consts::FRAC_PI_2),
                rp("lon", -std::f64::consts::PI),
                rp("timZon", -18_000.0),
            ],
        ))
        .expect("source physical coordinate bounds are inclusive")
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
             `CDL.Logical.Sources.SampleTrigger` must be finite and > 0; got 0"
                .to_string()
        ]
    );
}
