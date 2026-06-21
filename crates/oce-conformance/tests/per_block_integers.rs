//! Tier-1 exact conformance for CDL.Integers blocks through the frozen facade.

mod block_harness;

use block_harness::{
    B, BlockCase, I, Param, ParamValue, Port, assert_cases_are_deterministic,
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
const K_NEG3: &[Param] = &[Param {
    name: "k",
    value: ParamValue::Integer("-3"),
}];
const T_10: &[Param] = &[Param {
    name: "t",
    value: ParamValue::Integer("10"),
}];

// This table covers registered `CDL.Integers` blocks with checked-in reference tables.
// MultiSum is vector-shaped and remains deferred; Constant and Abs have no goldens yet.
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
    case(
        "integers_multiply_by_parameter",
        "CDL.Integers.MultiplyByParameter",
        "MultiplyByParameter",
        U,
        K_NEG3,
        INTEGER_Y,
    ),
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
        "integers_change",
        "CDL.Integers.Change",
        "Change",
        U,
        &[],
        CHANGE_OUTPUTS,
    ),
];

const STATEFUL_INTEGERS_SLUGS: &[&str] = &["integers_on_counter", "integers_change"];

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
