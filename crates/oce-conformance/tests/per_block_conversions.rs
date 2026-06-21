//! Tier-1 exact conformance for CDL.Conversions blocks through the frozen facade.

mod block_harness;

use block_harness::{
    B, BlockCase, I, Param, ParamValue, Port, R, assert_cases_are_deterministic,
    assert_cases_match_exact_oracle, case,
};

const BOOL_U: &[Port] = &[Port { name: "u", kind: B }];
const INT_U: &[Port] = &[Port { name: "u", kind: I }];
const REAL_U: &[Port] = &[Port { name: "u", kind: R }];

const REAL_Y: &[Port] = &[Port { name: "y", kind: R }];
const INTEGER_Y: &[Port] = &[Port { name: "y", kind: I }];

const BOOLEAN_TO_REAL_PARAMS: &[Param] = &[
    Param {
        name: "realTrue",
        value: ParamValue::Real("0.1"),
    },
    Param {
        name: "realFalse",
        value: ParamValue::Real("-0.3"),
    },
];
const BOOLEAN_TO_INTEGER_PARAMS: &[Param] = &[
    Param {
        name: "integerTrue",
        value: ParamValue::Integer("7"),
    },
    Param {
        name: "integerFalse",
        value: ParamValue::Integer("-2"),
    },
];

const CASES: &[BlockCase] = &[
    case(
        "conversions_boolean_to_real",
        "CDL.Conversions.BooleanToReal",
        "BooleanToReal",
        BOOL_U,
        BOOLEAN_TO_REAL_PARAMS,
        REAL_Y,
    ),
    case(
        "conversions_boolean_to_integer",
        "CDL.Conversions.BooleanToInteger",
        "BooleanToInteger",
        BOOL_U,
        BOOLEAN_TO_INTEGER_PARAMS,
        INTEGER_Y,
    ),
    case(
        "conversions_integer_to_real",
        "CDL.Conversions.IntegerToReal",
        "IntegerToReal",
        INT_U,
        &[],
        REAL_Y,
    ),
    case(
        "conversions_real_to_integer",
        "CDL.Conversions.RealToInteger",
        "RealToInteger",
        REAL_U,
        &[],
        INTEGER_Y,
    ),
];

// All Conversions blocks are stateless [A], but running every case twice cheaply pins
// facade-level determinism. Note: the IntegerToReal reference row for the original
// 2^53+1 probe is already serialized as 2^53 by the f64-backed CSV writer, so this
// facade test drives the representable 2^53 identity. The unit/oracle tier carries the
// i64-to-f64 round-to-even intent; here the discriminating rows are -7 and 2147483647.
const CONVERSION_SLUGS: &[&str] = &[
    "conversions_boolean_to_real",
    "conversions_boolean_to_integer",
    "conversions_integer_to_real",
    "conversions_real_to_integer",
];

#[test]
fn conversions_reference_blocks_match_exact_oracle() {
    assert_cases_match_exact_oracle(CASES, "CDL/Conversions", "single-block-conversions");
}

#[test]
fn conversions_blocks_exact_runs_are_deterministic() {
    assert_cases_are_deterministic(
        CASES,
        CONVERSION_SLUGS,
        "CDL/Conversions",
        "single-block-conversions",
    );
}
