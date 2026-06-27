//! Aligned-tolerance conformance for scalar `CDL.Utilities` blocks.

mod block_harness;

use block_harness::{
    B, BlockCase, Param, ParamValue, Port, R, assert_cases_match_aligned_tolerance_oracle, case,
};

const SUN_OUTPUTS: &[Port] = &[
    Port {
        name: "nextSunRise",
        kind: R,
    },
    Port {
        name: "nextSunSet",
        kind: R,
    },
    Port {
        name: "sunUp",
        kind: B,
    },
];

const DEFAULT_PARAMS: &[Param] = &[
    Param {
        name: "lat",
        value: ParamValue::Real("0.0"),
    },
    Param {
        name: "lon",
        value: ParamValue::Real("0.0"),
    },
    Param {
        name: "timZon",
        value: ParamValue::Real("0.0"),
    },
];

const SAN_FRANCISCO_PARAMS: &[Param] = &[
    Param {
        name: "lat",
        value: ParamValue::Real("0.6457718232379"),
    },
    Param {
        name: "lon",
        value: ParamValue::Real("-2.1293016874331"),
    },
    Param {
        name: "timZon",
        value: ParamValue::Real("-28800.0"),
    },
];

const ARCTIC_PARAMS: &[Param] = &[
    Param {
        name: "lat",
        value: ParamValue::Real("1.2566370614359"),
    },
    Param {
        name: "lon",
        value: ParamValue::Real("-1.2566370614359"),
    },
    Param {
        name: "timZon",
        value: ParamValue::Real("-18000.0"),
    },
];

const CASES: &[BlockCase] = &[
    case(
        "utilities_sun_rise_set_default_equator",
        "CDL.Utilities.SunRiseSet",
        "SunRiseSet",
        &[],
        DEFAULT_PARAMS,
        SUN_OUTPUTS,
    ),
    case(
        "utilities_sun_rise_set_san_francisco",
        "CDL.Utilities.SunRiseSet",
        "SunRiseSet/san_francisco_validation",
        &[],
        SAN_FRANCISCO_PARAMS,
        SUN_OUTPUTS,
    ),
    case(
        "utilities_sun_rise_set_arctic_polar_day",
        "CDL.Utilities.SunRiseSet",
        "SunRiseSet/arctic_polar_day",
        &[],
        ARCTIC_PARAMS,
        SUN_OUTPUTS,
    ),
];

#[test]
fn utility_blocks_match_aligned_tolerance_oracle() {
    assert_cases_match_aligned_tolerance_oracle(CASES, "CDL/Utilities", "single-block-utilities");
}
