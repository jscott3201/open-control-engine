//! Aligned-tolerance conformance for scalar `CDL.Reals` transcendental blocks.

mod block_harness;

use block_harness::{BlockCase, Port, R, assert_cases_match_aligned_tolerance_oracle, case};

const U: &[Port] = &[Port { name: "u", kind: R }];
const U1_U2: &[Port] = &[
    Port {
        name: "u1",
        kind: R,
    },
    Port {
        name: "u2",
        kind: R,
    },
];
const REAL_Y: &[Port] = &[Port { name: "y", kind: R }];

const CASES: &[BlockCase] = &[
    case("reals_sin", "CDL.Reals.Sin", "Sin", U, &[], REAL_Y),
    case("reals_cos", "CDL.Reals.Cos", "Cos", U, &[], REAL_Y),
    case("reals_tan", "CDL.Reals.Tan", "Tan", U, &[], REAL_Y),
    case("reals_asin", "CDL.Reals.Asin", "Asin", U, &[], REAL_Y),
    case("reals_acos", "CDL.Reals.Acos", "Acos", U, &[], REAL_Y),
    case("reals_atan", "CDL.Reals.Atan", "Atan", U, &[], REAL_Y),
    case(
        "reals_atan2",
        "CDL.Reals.Atan2",
        "Atan2",
        U1_U2,
        &[],
        REAL_Y,
    ),
    case("reals_exp", "CDL.Reals.Exp", "Exp", U, &[], REAL_Y),
    case("reals_log", "CDL.Reals.Log", "Log", U, &[], REAL_Y),
    case("reals_log10", "CDL.Reals.Log10", "Log10", U, &[], REAL_Y),
];

#[test]
fn reals_transcendental_blocks_match_aligned_tolerance_oracle() {
    assert_cases_match_aligned_tolerance_oracle(CASES, "CDL/Reals", "single-block-reals");
}
