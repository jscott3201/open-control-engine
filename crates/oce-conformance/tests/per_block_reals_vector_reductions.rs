//! Tier-1 exact conformance for CDL.Reals vector-reduction blocks.

mod block_harness;

use block_harness::{BlockCase, Param, ParamValue, Port, R, assert_cases_match_exact_oracle, case};

const U1_U2_U3: &[Port] = &[
    Port {
        name: "u1",
        kind: R,
    },
    Port {
        name: "u2",
        kind: R,
    },
    Port {
        name: "u3",
        kind: R,
    },
];
const REAL_Y: &[Port] = &[Port { name: "y", kind: R }];

const NIN_0: &[Param] = &[Param {
    name: "nin",
    value: ParamValue::Integer("0"),
}];
const NIN_3: &[Param] = &[Param {
    name: "nin",
    value: ParamValue::Integer("3"),
}];
const MULTI_SUM_NIN_3_GAINS: &[Param] = &[
    Param {
        name: "nin",
        value: ParamValue::Integer("3"),
    },
    Param {
        name: "k_1",
        value: ParamValue::Real("0.1"),
    },
    Param {
        name: "k_2",
        value: ParamValue::Real("1.0"),
    },
    Param {
        name: "k_3",
        value: ParamValue::Real("-0.1"),
    },
];

const CASES: &[BlockCase] = &[
    case(
        "reals_multi_sum",
        "CDL.Reals.MultiSum",
        "MultiSum",
        U1_U2_U3,
        MULTI_SUM_NIN_3_GAINS,
        REAL_Y,
    ),
    case(
        "reals_multi_sum_empty",
        "CDL.Reals.MultiSum",
        "MultiSum/empty",
        &[],
        NIN_0,
        REAL_Y,
    ),
    case(
        "reals_multi_min",
        "CDL.Reals.MultiMin",
        "MultiMin",
        U1_U2_U3,
        NIN_3,
        REAL_Y,
    ),
    case(
        "reals_multi_min_empty",
        "CDL.Reals.MultiMin",
        "MultiMin/empty",
        &[],
        NIN_0,
        REAL_Y,
    ),
    case(
        "reals_multi_max",
        "CDL.Reals.MultiMax",
        "MultiMax",
        U1_U2_U3,
        NIN_3,
        REAL_Y,
    ),
    case(
        "reals_multi_max_empty",
        "CDL.Reals.MultiMax",
        "MultiMax/empty",
        &[],
        NIN_0,
        REAL_Y,
    ),
];

#[test]
fn reals_vector_reduction_blocks_match_exact_oracle() {
    assert_cases_match_exact_oracle(CASES, "CDL/Reals", "single-block-reals-vector-reductions");
}
