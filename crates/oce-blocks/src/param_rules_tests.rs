//! Registry parameter-rule contract tests.

use super::{ParamRule, lookup};

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
