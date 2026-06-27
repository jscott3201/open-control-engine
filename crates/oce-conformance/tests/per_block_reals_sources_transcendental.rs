//! Aligned-tolerance conformance for transcendental `CDL.Reals.Sources` blocks.

mod block_harness;

use block_harness::{
    BlockCase, Param, ParamValue, Port, R, assert_cases_match_aligned_tolerance_oracle, case,
};

const REAL_Y: &[Port] = &[Port { name: "y", kind: R }];

const SOURCE_SIN_PARAMS: &[Param] = &[
    Param {
        name: "amplitude",
        value: ParamValue::Real("2.0"),
    },
    Param {
        name: "freqHz",
        value: ParamValue::Real("0.25"),
    },
    Param {
        name: "phase",
        value: ParamValue::Real("1.5707963267948966"),
    },
    Param {
        name: "offset",
        value: ParamValue::Real("0.5"),
    },
    Param {
        name: "startTime",
        value: ParamValue::Real("1.0"),
    },
];

const CASES: &[BlockCase] = &[case(
    "reals_source_sin",
    "CDL.Reals.Sources.Sin",
    "Sources/Sin",
    &[],
    SOURCE_SIN_PARAMS,
    REAL_Y,
)];

#[test]
fn reals_source_transcendental_blocks_match_aligned_tolerance_oracle() {
    assert_cases_match_aligned_tolerance_oracle(
        CASES,
        "CDL/Reals",
        "single-block-reals-source-transcendental",
    );
}
