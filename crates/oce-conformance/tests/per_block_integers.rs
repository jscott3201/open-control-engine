//! Tier-1 exact conformance for CDL.Integers blocks through the frozen facade.

mod block_harness;

use block_harness::{
    B, BlockCase, I, Param, ParamValue, Port, R, assert_cases_are_deterministic,
    assert_cases_match_exact_oracle, case,
};

const U: &[Port] = &[Port { name: "u", kind: I }];
const U1_U2: &[Port] = &[
    Port {
        name: "u1",
        kind: I,
    },
    Port {
        name: "u2",
        kind: I,
    },
];
const SWITCH_INPUTS: &[Port] = &[
    Port {
        name: "u1",
        kind: I,
    },
    Port {
        name: "u2",
        kind: B,
    },
    Port {
        name: "u3",
        kind: I,
    },
];
const COUNTER_INPUTS: &[Port] = &[
    Port {
        name: "trigger",
        kind: B,
    },
    Port {
        name: "reset",
        kind: B,
    },
];
const STAGE_INPUTS: &[Port] = &[Port { name: "u", kind: R }];

const INTEGER_Y: &[Port] = &[Port { name: "y", kind: I }];
const BOOL_Y: &[Port] = &[Port { name: "y", kind: B }];
const CHANGE_OUTPUTS: &[Port] = &[
    Port { name: "y", kind: B },
    Port {
        name: "up",
        kind: B,
    },
    Port {
        name: "down",
        kind: B,
    },
];

const P_1000: &[Param] = &[Param {
    name: "p",
    value: ParamValue::Integer("1000"),
}];
const K_NEG_12345: &[Param] = &[Param {
    name: "k",
    value: ParamValue::Integer("-12345"),
}];
const SOURCE_PULSE_NEGATIVE_SHIFT_PARAMS: &[Param] = &[
    Param {
        name: "amplitude",
        value: ParamValue::Integer("3"),
    },
    Param {
        name: "width",
        value: ParamValue::Real("0.5"),
    },
    Param {
        name: "period",
        value: ParamValue::Real("1.0"),
    },
    Param {
        name: "shift",
        value: ParamValue::Real("-1.25"),
    },
    Param {
        name: "offset",
        value: ParamValue::Integer("-2"),
    },
];
const T_10: &[Param] = &[Param {
    name: "t",
    value: ParamValue::Integer("10"),
}];
const COUNTER_Y_START_TWO: &[Param] = &[Param {
    name: "y_start",
    value: ParamValue::Integer("2"),
}];
const COUNTER_Y_START_THREE: &[Param] = &[Param {
    name: "y_start",
    value: ParamValue::Integer("3"),
}];
const STAGE_N1: &[Param] = &[
    Param {
        name: "n",
        value: ParamValue::Integer("1"),
    },
    Param {
        name: "holdDuration",
        value: ParamValue::Real("0.0"),
    },
    Param {
        name: "h",
        value: ParamValue::Real("0.02"),
    },
];
const STAGE_THRESHOLD_PARAMS: &[Param] = &[
    Param {
        name: "n",
        value: ParamValue::Integer("4"),
    },
    Param {
        name: "holdDuration",
        value: ParamValue::Real("0.0"),
    },
    Param {
        name: "h",
        value: ParamValue::Real("0.001"),
    },
];
const STAGE_N4_HOLD_ZERO: &[Param] = &[
    Param {
        name: "n",
        value: ParamValue::Integer("4"),
    },
    Param {
        name: "holdDuration",
        value: ParamValue::Real("0.0"),
    },
    Param {
        name: "h",
        value: ParamValue::Real("0.05"),
    },
];
const STAGE_HOLD_DURATION: &[Param] = &[
    Param {
        name: "n",
        value: ParamValue::Integer("4"),
    },
    Param {
        name: "holdDuration",
        value: ParamValue::Real("2.0"),
    },
    Param {
        name: "h",
        value: ParamValue::Real("0.05"),
    },
];
const STAGE_UNCLAMPED_ZERO: &[Param] = &[
    Param {
        name: "n",
        value: ParamValue::Integer("4"),
    },
    Param {
        name: "holdDuration",
        value: ParamValue::Real("0.0"),
    },
    Param {
        name: "h",
        value: ParamValue::Real("0.05"),
    },
    Param {
        name: "pre_y_start",
        value: ParamValue::Integer("3"),
    },
];

// This table covers registered `CDL.Integers` blocks with checked-in reference tables.
// MultiSum is vector-shaped and remains deferred.
const CASES: &[BlockCase] = &[
    case(
        "integers_add",
        "CDL.Integers.Add",
        "Add",
        U1_U2,
        &[],
        INTEGER_Y,
    ),
    case(
        "integers_subtract",
        "CDL.Integers.Subtract",
        "Subtract",
        U1_U2,
        &[],
        INTEGER_Y,
    ),
    case(
        "integers_multiply",
        "CDL.Integers.Multiply",
        "Multiply",
        U1_U2,
        &[],
        INTEGER_Y,
    ),
    case(
        "integers_add_parameter",
        "CDL.Integers.AddParameter",
        "AddParameter",
        U,
        P_1000,
        INTEGER_Y,
    ),
    case("integers_abs", "CDL.Integers.Abs", "Abs", U, &[], INTEGER_Y),
    case(
        "integers_max",
        "CDL.Integers.Max",
        "Max",
        U1_U2,
        &[],
        INTEGER_Y,
    ),
    case(
        "integers_min",
        "CDL.Integers.Min",
        "Min",
        U1_U2,
        &[],
        INTEGER_Y,
    ),
    case(
        "integers_switch",
        "CDL.Integers.Switch",
        "Switch",
        SWITCH_INPUTS,
        &[],
        INTEGER_Y,
    ),
    case(
        "integers_greater",
        "CDL.Integers.Greater",
        "Greater",
        U1_U2,
        &[],
        BOOL_Y,
    ),
    case(
        "integers_equal",
        "CDL.Integers.Equal",
        "Equal",
        U1_U2,
        &[],
        BOOL_Y,
    ),
    case(
        "integers_greater_equal",
        "CDL.Integers.GreaterEqual",
        "GreaterEqual",
        U1_U2,
        &[],
        BOOL_Y,
    ),
    case(
        "integers_greater_threshold",
        "CDL.Integers.GreaterThreshold",
        "GreaterThreshold",
        U,
        T_10,
        BOOL_Y,
    ),
    case(
        "integers_greater_equal_threshold",
        "CDL.Integers.GreaterEqualThreshold",
        "GreaterEqualThreshold",
        U,
        T_10,
        BOOL_Y,
    ),
    case(
        "integers_less",
        "CDL.Integers.Less",
        "Less",
        U1_U2,
        &[],
        BOOL_Y,
    ),
    case(
        "integers_less_equal",
        "CDL.Integers.LessEqual",
        "LessEqual",
        U1_U2,
        &[],
        BOOL_Y,
    ),
    case(
        "integers_less_threshold",
        "CDL.Integers.LessThreshold",
        "LessThreshold",
        U,
        T_10,
        BOOL_Y,
    ),
    case(
        "integers_less_equal_threshold",
        "CDL.Integers.LessEqualThreshold",
        "LessEqualThreshold",
        U,
        T_10,
        BOOL_Y,
    ),
    case(
        "integers_on_counter",
        "CDL.Integers.OnCounter",
        "OnCounter",
        COUNTER_INPUTS,
        &[],
        INTEGER_Y,
    ),
    case(
        "integers_on_counter_held_reset",
        "CDL.Integers.OnCounter",
        "OnCounter/held_reset",
        COUNTER_INPUTS,
        COUNTER_Y_START_THREE,
        INTEGER_Y,
    ),
    case(
        "integers_on_counter_trigger_initially_true",
        "CDL.Integers.OnCounter",
        "OnCounter/trigger_initially_true",
        COUNTER_INPUTS,
        COUNTER_Y_START_TWO,
        INTEGER_Y,
    ),
    case(
        "integers_change",
        "CDL.Integers.Change",
        "Change",
        U,
        &[],
        CHANGE_OUTPUTS,
    ),
    case(
        "integers_stage_n1_initial_zero",
        "CDL.Integers.Stage",
        "Stage/n1_initial_zero",
        STAGE_INPUTS,
        STAGE_N1,
        INTEGER_Y,
    ),
    case(
        "integers_stage_threshold_boundaries",
        "CDL.Integers.Stage",
        "Stage/threshold_boundaries",
        STAGE_INPUTS,
        STAGE_THRESHOLD_PARAMS,
        INTEGER_Y,
    ),
    case(
        "integers_stage_hysteresis_hold",
        "CDL.Integers.Stage",
        "Stage/hysteresis_hold",
        STAGE_INPUTS,
        STAGE_N4_HOLD_ZERO,
        INTEGER_Y,
    ),
    case(
        "integers_stage_hold_duration",
        "CDL.Integers.Stage",
        "Stage/hold_duration",
        STAGE_INPUTS,
        STAGE_HOLD_DURATION,
        INTEGER_Y,
    ),
    case(
        "integers_stage_unclamped_zero",
        "CDL.Integers.Stage",
        "Stage/unclamped_zero",
        STAGE_INPUTS,
        STAGE_UNCLAMPED_ZERO,
        INTEGER_Y,
    ),
    case(
        "integers_constant",
        "CDL.Integers.Sources.Constant",
        "Sources/Constant",
        &[],
        K_NEG_12345,
        INTEGER_Y,
    ),
    case(
        "integers_source_pulse_negative_shift",
        "CDL.Integers.Sources.Pulse",
        "Sources/Pulse",
        &[],
        SOURCE_PULSE_NEGATIVE_SHIFT_PARAMS,
        INTEGER_Y,
    ),
];

const STATEFUL_INTEGERS_SLUGS: &[&str] = &[
    "integers_on_counter",
    "integers_on_counter_held_reset",
    "integers_on_counter_trigger_initially_true",
    "integers_change",
    "integers_stage_n1_initial_zero",
    "integers_stage_threshold_boundaries",
    "integers_stage_hysteresis_hold",
    "integers_stage_hold_duration",
    "integers_stage_unclamped_zero",
];

#[test]
fn integers_reference_blocks_match_exact_oracle() {
    assert_cases_match_exact_oracle(CASES, "CDL/Integers", "single-block-integers");
}

#[test]
fn stateful_integers_blocks_exact_runs_are_deterministic() {
    assert_cases_are_deterministic(
        CASES,
        STATEFUL_INTEGERS_SLUGS,
        "CDL/Integers",
        "single-block-integers",
    );
}
