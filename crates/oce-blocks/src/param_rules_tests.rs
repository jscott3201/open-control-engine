//! Registry parameter-rule contract tests.

use super::{
    MAX_RESOLVED_PORT_WIDTH, ParamRule, TimeTableValues, lookup, reals_sources::ZERO_TIME_MEMBERS,
};

#[test]
fn registry_exposes_block_param_rules() {
    assert_eq!(
        lookup("CDL.Logical.Sources.SampleTrigger")
            .unwrap()
            .param_rules(),
        &[
            ParamRule::Required { name: "period" },
            ParamRule::RealGreaterThan {
                name: "period",
                min: 0.0,
            },
        ]
    );
    assert_eq!(
        lookup("CDL.Logical.Sources.TimeTable")
            .unwrap()
            .param_rules(),
        &[
            ParamRule::Required { name: "period" },
            ParamRule::TimeTableMatrix {
                base: "table",
                values: TimeTableValues::Boolean,
                time_scale: "timeScale",
                period: Some("period"),
                extrapolation: None,
            },
            ParamRule::RealFiniteGreaterThan {
                name: "timeScale",
                min: 0.0,
            },
            ParamRule::RealFiniteGreaterThan {
                name: "period",
                min: 0.0,
            },
            ParamRule::RealGreaterOrEqual {
                name: "period",
                min: 1.0e-6,
            },
        ]
    );
    assert_eq!(
        lookup("CDL.Reals.Limiter").unwrap().param_rules(),
        &[
            ParamRule::RealLessOrEqual {
                lower: "uMin",
                upper: "uMax",
            },
            ParamRule::RealEqualWarning {
                left: "uMin",
                right: "uMax",
            },
        ]
    );
    assert_eq!(
        lookup("CDL.Logical.Proof").unwrap().param_rules(),
        &[
            ParamRule::Required { name: "debounce" },
            ParamRule::Required {
                name: "feedbackDelay",
            },
            ParamRule::RealLessOrEqualWarning {
                lower: "debounce",
                upper: "feedbackDelay",
            },
        ]
    );
    let multi_width_rules = &[
        ParamRule::Structural { name: "nin" },
        ParamRule::IntegerGreaterOrEqual {
            name: "nin",
            min: 0,
        },
        ParamRule::IntegerLessOrEqualConstant {
            name: "nin",
            max: MAX_RESOLVED_PORT_WIDTH as i64,
        },
    ];
    for path in ["CDL.Logical.MultiAnd", "CDL.Logical.MultiOr"] {
        assert_eq!(
            lookup(path).unwrap().param_rules(),
            multi_width_rules,
            "{path}"
        );
    }
    for path in ["CDL.Reals.MultiMax", "CDL.Reals.MultiMin"] {
        assert_eq!(
            lookup(path).unwrap().param_rules(),
            multi_width_rules,
            "{path}"
        );
    }
    assert_eq!(
        lookup("CDL.Integers.MultiSum").unwrap().param_rules(),
        &[
            ParamRule::Structural { name: "nin" },
            ParamRule::IntegerGreaterOrEqual {
                name: "nin",
                min: 0,
            },
            ParamRule::IntegerLessOrEqualConstant {
                name: "nin",
                max: MAX_RESOLVED_PORT_WIDTH as i64,
            },
            ParamRule::IntegerArrayElements {
                base: "k",
                len: "nin",
            },
        ]
    );
    assert_eq!(
        lookup("CDL.Integers.Sources.TimeTable")
            .unwrap()
            .param_rules(),
        &[
            ParamRule::Required { name: "period" },
            ParamRule::TimeTableMatrix {
                base: "table",
                values: TimeTableValues::Integer,
                time_scale: "timeScale",
                period: Some("period"),
                extrapolation: None,
            },
            ParamRule::RealFiniteGreaterThan {
                name: "timeScale",
                min: 0.0,
            },
            ParamRule::RealFiniteGreaterThan {
                name: "period",
                min: 0.0,
            },
            ParamRule::RealGreaterOrEqual {
                name: "period",
                min: 1.0e-6,
            },
        ]
    );
    assert_eq!(
        lookup("CDL.Reals.MultiSum").unwrap().param_rules(),
        &[
            ParamRule::Structural { name: "nin" },
            ParamRule::IntegerGreaterOrEqual {
                name: "nin",
                min: 0,
            },
            ParamRule::IntegerLessOrEqualConstant {
                name: "nin",
                max: MAX_RESOLVED_PORT_WIDTH as i64,
            },
            ParamRule::RealArrayElements {
                base: "k",
                len: "nin",
            },
        ]
    );
    assert_eq!(
        lookup("CDL.Reals.MatrixGain").unwrap().param_rules(),
        &[
            ParamRule::Structural { name: "nout" },
            ParamRule::Structural { name: "nin" },
            ParamRule::IntegerGreaterOrEqual {
                name: "nout",
                min: 0,
            },
            ParamRule::IntegerLessOrEqualConstant {
                name: "nout",
                max: MAX_RESOLVED_PORT_WIDTH as i64,
            },
            ParamRule::IntegerGreaterOrEqual {
                name: "nin",
                min: 0,
            },
            ParamRule::IntegerLessOrEqualConstant {
                name: "nin",
                max: MAX_RESOLVED_PORT_WIDTH as i64,
            },
            ParamRule::IntegerProductLessOrEqualConstant {
                left: "nout",
                right: "nin",
                max: MAX_RESOLVED_PORT_WIDTH as i64,
            },
            ParamRule::RealMatrixElements {
                base: "K",
                rows: "nout",
                default_rows: 2,
                cols: "nin",
                default_cols: 2,
            },
        ]
    );
    assert_eq!(
        lookup("CDL.Reals.Sort").unwrap().param_rules(),
        &[
            ParamRule::Structural { name: "nin" },
            ParamRule::Boolean { name: "ascending" },
            ParamRule::IntegerGreaterOrEqual {
                name: "nin",
                min: 0,
            },
            ParamRule::IntegerLessOrEqualConstant {
                name: "nin",
                max: MAX_RESOLVED_PORT_WIDTH as i64,
            },
        ]
    );
    for (path, row_param) in [
        ("CDL.Reals.MatrixMax", "rowMax"),
        ("CDL.Reals.MatrixMin", "rowMin"),
    ] {
        assert_eq!(
            lookup(path).unwrap().param_rules(),
            &[
                ParamRule::Required { name: "nRow" },
                ParamRule::Required { name: "nCol" },
                ParamRule::Structural { name: "nRow" },
                ParamRule::Structural { name: "nCol" },
                ParamRule::Structural { name: row_param },
                ParamRule::Boolean { name: row_param },
                ParamRule::IntegerGreaterOrEqual {
                    name: "nRow",
                    min: 1,
                },
                ParamRule::IntegerLessOrEqualConstant {
                    name: "nRow",
                    max: MAX_RESOLVED_PORT_WIDTH as i64,
                },
                ParamRule::IntegerGreaterOrEqual {
                    name: "nCol",
                    min: 1,
                },
                ParamRule::IntegerLessOrEqualConstant {
                    name: "nCol",
                    max: MAX_RESOLVED_PORT_WIDTH as i64,
                },
                ParamRule::IntegerProductLessOrEqualConstant {
                    left: "nRow",
                    right: "nCol",
                    max: MAX_RESOLVED_PORT_WIDTH as i64,
                },
            ],
            "{path}"
        );
    }
    let extract_signal_rules = &[
        ParamRule::Structural { name: "nin" },
        ParamRule::Structural { name: "nout" },
        ParamRule::StructuralArrayElements { base: "extract" },
        ParamRule::IntegerGreaterOrEqual {
            name: "nin",
            min: 0,
        },
        ParamRule::IntegerLessOrEqualConstant {
            name: "nin",
            max: MAX_RESOLVED_PORT_WIDTH as i64,
        },
        ParamRule::IntegerGreaterOrEqual {
            name: "nout",
            min: 0,
        },
        ParamRule::IntegerLessOrEqualConstant {
            name: "nout",
            max: MAX_RESOLVED_PORT_WIDTH as i64,
        },
        ParamRule::IntegerArrayElementsInRange {
            base: "extract",
            len: "nout",
            len_default: 1,
            min: 1,
            max: "nin",
            max_default: 1,
            default_to_index: true,
        },
    ];
    for path in [
        "CDL.Routing.BooleanExtractSignal",
        "CDL.Routing.IntegerExtractSignal",
        "CDL.Routing.RealExtractSignal",
    ] {
        assert_eq!(
            lookup(path).unwrap().param_rules(),
            extract_signal_rules,
            "{path}"
        );
    }

    let extractor_rules = &[
        ParamRule::Structural { name: "nin" },
        ParamRule::IntegerGreaterOrEqual {
            name: "nin",
            min: 1,
        },
        ParamRule::IntegerLessOrEqualConstant {
            name: "nin",
            max: MAX_RESOLVED_PORT_WIDTH as i64,
        },
    ];
    for path in [
        "CDL.Routing.BooleanExtractor",
        "CDL.Routing.IntegerExtractor",
        "CDL.Routing.RealExtractor",
    ] {
        assert_eq!(
            lookup(path).unwrap().param_rules(),
            extractor_rules,
            "{path}"
        );
    }

    let scalar_replicator_rules = &[
        ParamRule::Structural { name: "nout" },
        ParamRule::IntegerGreaterOrEqual {
            name: "nout",
            min: 0,
        },
        ParamRule::IntegerLessOrEqualConstant {
            name: "nout",
            max: MAX_RESOLVED_PORT_WIDTH as i64,
        },
    ];
    for path in [
        "CDL.Routing.BooleanScalarReplicator",
        "CDL.Routing.IntegerScalarReplicator",
        "CDL.Routing.RealScalarReplicator",
    ] {
        assert_eq!(
            lookup(path).unwrap().param_rules(),
            scalar_replicator_rules,
            "{path}"
        );
    }

    let vector_filter_rules = &[
        ParamRule::Required { name: "nin" },
        ParamRule::Required { name: "nout" },
        ParamRule::Structural { name: "nin" },
        ParamRule::Structural { name: "nout" },
        ParamRule::StructuralArrayElements { base: "msk" },
        ParamRule::IntegerGreaterOrEqual {
            name: "nin",
            min: 0,
        },
        ParamRule::IntegerLessOrEqualConstant {
            name: "nin",
            max: MAX_RESOLVED_PORT_WIDTH as i64,
        },
        ParamRule::IntegerGreaterOrEqual {
            name: "nout",
            min: 0,
        },
        ParamRule::IntegerLessOrEqualConstant {
            name: "nout",
            max: MAX_RESOLVED_PORT_WIDTH as i64,
        },
        ParamRule::BooleanArrayElements {
            base: "msk",
            len: "nin",
        },
        ParamRule::BooleanArrayTrueCountEquals {
            base: "msk",
            len: "nin",
            count: "nout",
            default: true,
        },
    ];
    for path in [
        "CDL.Routing.BooleanVectorFilter",
        "CDL.Routing.IntegerVectorFilter",
        "CDL.Routing.RealVectorFilter",
    ] {
        assert_eq!(
            lookup(path).unwrap().param_rules(),
            vector_filter_rules,
            "{path}"
        );
    }

    let vector_replicator_rules = &[
        ParamRule::Structural { name: "nin" },
        ParamRule::Structural { name: "nout" },
        ParamRule::IntegerGreaterOrEqual {
            name: "nin",
            min: 0,
        },
        ParamRule::IntegerLessOrEqualConstant {
            name: "nin",
            max: MAX_RESOLVED_PORT_WIDTH as i64,
        },
        ParamRule::IntegerGreaterOrEqual {
            name: "nout",
            min: 0,
        },
        ParamRule::IntegerLessOrEqualConstant {
            name: "nout",
            max: MAX_RESOLVED_PORT_WIDTH as i64,
        },
        ParamRule::IntegerProductLessOrEqualConstant {
            left: "nin",
            right: "nout",
            max: MAX_RESOLVED_PORT_WIDTH as i64,
        },
    ];
    for path in [
        "CDL.Routing.BooleanVectorReplicator",
        "CDL.Routing.IntegerVectorReplicator",
        "CDL.Routing.RealVectorReplicator",
    ] {
        assert_eq!(
            lookup(path).unwrap().param_rules(),
            vector_replicator_rules,
            "{path}"
        );
    }
    assert_eq!(
        lookup("CDL.Integers.Stage").unwrap().param_rules(),
        &[
            ParamRule::Required { name: "n" },
            ParamRule::Required {
                name: "holdDuration",
            },
            ParamRule::IntegerGreaterOrEqual { name: "n", min: 1 },
            ParamRule::RealGreaterOrEqual {
                name: "holdDuration",
                min: 0.0,
            },
            ParamRule::RealTimesIntegerInclusiveRange {
                real: "h",
                integer: "n",
                min: 0.001,
                max: 0.5,
            },
        ]
    );
    assert_eq!(
        lookup("CDL.Reals.Derivative").unwrap().param_rules(),
        &[ParamRule::RealGreaterThan {
            name: "T",
            min: 0.0,
        }]
    );
    assert_eq!(
        lookup("CDL.Reals.LimitSlewRate").unwrap().param_rules(),
        &[ParamRule::RealGreaterThan {
            name: "Td",
            min: 0.0,
        }]
    );
    assert_eq!(
        lookup("CDL.Reals.Ramp").unwrap().param_rules(),
        &[
            ParamRule::Required {
                name: "raisingSlewRate",
            },
            ParamRule::RealGreaterOrEqual {
                name: "raisingSlewRate",
                min: 1e-37,
            },
            ParamRule::RealLessOrEqualConstant {
                name: "fallingSlewRate",
                max: -1e-37,
            },
            ParamRule::RealGreaterOrEqual {
                name: "Td",
                min: 1e-15,
            },
        ]
    );
    assert_eq!(
        lookup("CDL.Reals.Sources.CalendarTime")
            .unwrap()
            .param_rules(),
        &[
            ParamRule::Required { name: "zerTim" },
            ParamRule::EnumMembers {
                name: "zerTim",
                members: ZERO_TIME_MEMBERS,
            },
            ParamRule::IntegerGreaterOrEqual {
                name: "yearRef",
                min: 2010,
            },
            ParamRule::IntegerLessOrEqualConstant {
                name: "yearRef",
                max: 2031,
            },
            ParamRule::RealFinite { name: "offset" },
        ]
    );
    assert_eq!(
        lookup("CDL.Reals.Sources.TimeTable").unwrap().param_rules(),
        &[
            ParamRule::TimeTableMatrix {
                base: "table",
                values: TimeTableValues::Real,
                time_scale: "timeScale",
                period: None,
                extrapolation: Some("extrapolation"),
            },
            ParamRule::TimeTableOffset {
                base: "offset",
                table: "table",
            },
            ParamRule::EnumMembers {
                name: "smoothness",
                members: &["LinearSegments", "ConstantSegments"],
            },
            ParamRule::EnumMembers {
                name: "extrapolation",
                members: &["HoldLastPoint", "LastTwoPoints", "Periodic"],
            },
            ParamRule::RealFiniteGreaterThan {
                name: "timeScale",
                min: 0.0,
            },
        ]
    );
    assert_eq!(
        lookup("CDL.Reals.MovingAverage").unwrap().param_rules(),
        &[ParamRule::RealGreaterThan {
            name: "delta",
            min: 0.0,
        }]
    );
    assert_eq!(
        lookup("CDL.Reals.PID").unwrap().param_rules(),
        &[
            ParamRule::RealGreaterThan {
                name: "Td",
                min: 0.0,
            },
            ParamRule::RealGreaterThan {
                name: "Nd",
                min: 0.0,
            },
        ]
    );
    assert_eq!(
        lookup("CDL.Reals.PIDWithReset").unwrap().param_rules(),
        &[
            ParamRule::RealGreaterThan {
                name: "Td",
                min: 0.0,
            },
            ParamRule::RealGreaterThan {
                name: "Nd",
                min: 0.0,
            },
        ]
    );
    assert_eq!(
        lookup("CDL.Psychrometrics.SpecificEnthalpy_TDryBulPhi")
            .unwrap()
            .param_rules(),
        &[ParamRule::RealFiniteGreaterThan {
            name: "pAtm",
            min: 0.0,
        }]
    );
    assert_eq!(
        lookup("CDL.Utilities.SunRiseSet").unwrap().param_rules(),
        &[
            ParamRule::Required { name: "lat" },
            ParamRule::Required { name: "lon" },
            ParamRule::Required { name: "timZon" },
            ParamRule::RealGreaterOrEqual {
                name: "lat",
                min: -std::f64::consts::FRAC_PI_2,
            },
            ParamRule::RealLessOrEqualConstant {
                name: "lat",
                max: std::f64::consts::FRAC_PI_2,
            },
            ParamRule::RealGreaterOrEqual {
                name: "lon",
                min: -std::f64::consts::PI,
            },
            ParamRule::RealLessOrEqualConstant {
                name: "lon",
                max: std::f64::consts::PI,
            },
            ParamRule::RealFinite { name: "timZon" },
        ]
    );
    assert_eq!(
        lookup("CDL.Discrete.TriggeredMovingMean")
            .unwrap()
            .param_rules(),
        &[
            ParamRule::Required { name: "n" },
            ParamRule::IntegerGreaterOrEqual { name: "n", min: 1 },
        ]
    );
    let sampled_rules = &[
        ParamRule::Required {
            name: "samplePeriod",
        },
        ParamRule::RealGreaterOrEqual {
            name: "samplePeriod",
            min: 1e-3,
        },
    ];
    for path in [
        "CDL.Discrete.FirstOrderHold",
        "CDL.Discrete.Sampler",
        "CDL.Discrete.ZeroOrderHold",
    ] {
        assert_eq!(lookup(path).unwrap().param_rules(), sampled_rules, "{path}");
    }
    assert!(
        lookup("CDL.Reals.Sources.Constant")
            .unwrap()
            .param_rules()
            .is_empty()
    );
}
