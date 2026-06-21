//! Tier-1 exact conformance for CDL.Discrete blocks through the frozen facade.

mod block_harness;

use block_harness::{
    BlockCase, Param, ParamValue, Port, R, assert_cases_are_deterministic,
    assert_cases_match_exact_oracle, case,
};

const U: &[Port] = &[Port { name: "u", kind: R }];
const REAL_Y: &[Port] = &[Port { name: "y", kind: R }];
const UNIT_DELAY_PARAMS: &[Param] = &[Param {
    name: "y_start",
    value: ParamValue::Real("0.0"),
}];
const UNIT_DELAY_NONZERO_START_PARAMS: &[Param] = &[Param {
    name: "y_start",
    value: ParamValue::Real("2.5"),
}];

// The checked-in goldens pin one-sample-delay dynamics, state latching,
// determinism, non-dyadic bit-exact values, and `y_start` parameter binding.
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
];

const STATEFUL_DISCRETE_SLUGS: &[&str] =
    &["discrete_unit_delay", "discrete_unit_delay_nonzero_y_start"];

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
