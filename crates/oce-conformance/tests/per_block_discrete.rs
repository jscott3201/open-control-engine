//! Tier-1 exact conformance for CDL.Discrete blocks through the frozen facade.

mod block_harness;

use block_harness::{
    B, BlockCase, Param, ParamValue, Port, R, assert_cases_are_deterministic,
    assert_cases_match_exact_oracle, case,
};

const U: &[Port] = &[Port { name: "u", kind: R }];
const U_TRIGGER: &[Port] = &[
    Port { name: "u", kind: R },
    Port {
        name: "trigger",
        kind: B,
    },
];
const REAL_Y: &[Port] = &[Port { name: "y", kind: R }];
const UNIT_DELAY_PARAMS: &[Param] = &[
    Param {
        name: "y_start",
        value: ParamValue::Real("0.0"),
    },
    Param {
        name: "samplePeriod",
        value: ParamValue::Real("1.0"),
    },
];
const UNIT_DELAY_NONZERO_START_PARAMS: &[Param] = &[
    Param {
        name: "y_start",
        value: ParamValue::Real("2.5"),
    },
    Param {
        name: "samplePeriod",
        value: ParamValue::Real("1.0"),
    },
];
const TRIGGERED_SAMPLER_PARAMS: &[Param] = &[Param {
    name: "y_start",
    value: ParamValue::Real("2.5"),
}];
const TRIGGERED_SAMPLER_INITIAL_TRUE_PARAMS: &[Param] = &[Param {
    name: "y_start",
    value: ParamValue::Real("-7.0"),
}];
const TRIGGERED_MOVING_MEAN_PARAMS: &[Param] = &[Param {
    name: "n",
    value: ParamValue::Integer("3"),
}];
const TRIGGERED_MOVING_MEAN_N_ONE_PARAMS: &[Param] = &[Param {
    name: "n",
    value: ParamValue::Integer("1"),
}];
const SAMPLED_PARAMS: &[Param] = &[Param {
    name: "samplePeriod",
    value: ParamValue::Real("1.0"),
}];
const NO_PARAMS: &[Param] = &[];

// The checked-in goldens pin one-sample-delay dynamics, state latching,
// triggered sampling, determinism, non-dyadic bit-exact values, and `y_start` parameter binding.
const CASES: &[BlockCase] = &[
    case(
        "discrete_unit_delay",
        "CDL.Discrete.UnitDelay",
        "UnitDelay",
        U,
        UNIT_DELAY_PARAMS,
        REAL_Y,
    ),
    case(
        "discrete_unit_delay_nonzero_y_start",
        "CDL.Discrete.UnitDelay",
        "UnitDelay/y_start_nonzero",
        U,
        UNIT_DELAY_NONZERO_START_PARAMS,
        REAL_Y,
    ),
    case(
        "discrete_sampler",
        "CDL.Discrete.Sampler",
        "Sampler",
        U,
        SAMPLED_PARAMS,
        REAL_Y,
    ),
    case(
        "discrete_zero_order_hold",
        "CDL.Discrete.ZeroOrderHold",
        "ZeroOrderHold",
        U,
        SAMPLED_PARAMS,
        REAL_Y,
    ),
    case(
        "discrete_first_order_hold",
        "CDL.Discrete.FirstOrderHold",
        "FirstOrderHold",
        U,
        SAMPLED_PARAMS,
        REAL_Y,
    ),
    case(
        "discrete_first_order_hold_negative_start",
        "CDL.Discrete.FirstOrderHold",
        "FirstOrderHold/negative_start",
        U,
        SAMPLED_PARAMS,
        REAL_Y,
    ),
    case(
        "discrete_triggered_sampler",
        "CDL.Discrete.TriggeredSampler",
        "TriggeredSampler",
        U_TRIGGER,
        TRIGGERED_SAMPLER_PARAMS,
        REAL_Y,
    ),
    case(
        "discrete_triggered_sampler_initial_true",
        "CDL.Discrete.TriggeredSampler",
        "TriggeredSampler/trigger_initially_true",
        U_TRIGGER,
        TRIGGERED_SAMPLER_INITIAL_TRUE_PARAMS,
        REAL_Y,
    ),
    case(
        "discrete_triggered_max",
        "CDL.Discrete.TriggeredMax",
        "TriggeredMax",
        U_TRIGGER,
        NO_PARAMS,
        REAL_Y,
    ),
    case(
        "discrete_triggered_max_initial_true",
        "CDL.Discrete.TriggeredMax",
        "TriggeredMax/trigger_initially_true",
        U_TRIGGER,
        NO_PARAMS,
        REAL_Y,
    ),
    case(
        "discrete_triggered_moving_mean",
        "CDL.Discrete.TriggeredMovingMean",
        "TriggeredMovingMean",
        U_TRIGGER,
        TRIGGERED_MOVING_MEAN_PARAMS,
        REAL_Y,
    ),
    case(
        "discrete_triggered_moving_mean_initial_true",
        "CDL.Discrete.TriggeredMovingMean",
        "TriggeredMovingMean/trigger_initially_true",
        U_TRIGGER,
        TRIGGERED_MOVING_MEAN_PARAMS,
        REAL_Y,
    ),
    case(
        "discrete_triggered_moving_mean_n_one",
        "CDL.Discrete.TriggeredMovingMean",
        "TriggeredMovingMean/n_one",
        U_TRIGGER,
        TRIGGERED_MOVING_MEAN_N_ONE_PARAMS,
        REAL_Y,
    ),
];

const STATEFUL_DISCRETE_SLUGS: &[&str] = &[
    "discrete_unit_delay",
    "discrete_unit_delay_nonzero_y_start",
    "discrete_sampler",
    "discrete_zero_order_hold",
    "discrete_first_order_hold",
    "discrete_first_order_hold_negative_start",
    "discrete_triggered_sampler",
    "discrete_triggered_sampler_initial_true",
    "discrete_triggered_max",
    "discrete_triggered_max_initial_true",
    "discrete_triggered_moving_mean",
    "discrete_triggered_moving_mean_initial_true",
    "discrete_triggered_moving_mean_n_one",
];

#[test]
fn discrete_reference_blocks_match_exact_oracle() {
    assert_cases_match_exact_oracle(CASES, "CDL/Discrete", "single-block-discrete");
}

#[test]
fn stateful_discrete_blocks_exact_runs_are_deterministic() {
    assert_cases_are_deterministic(
        CASES,
        STATEFUL_DISCRETE_SLUGS,
        "CDL/Discrete",
        "single-block-discrete",
    );
}
