//! Aligned-tolerance conformance for scalar `CDL.Psychrometrics` formula blocks.

mod block_harness;

use block_harness::{
    BlockCase, Param, ParamValue, Port, R, assert_cases_match_aligned_tolerance_oracle, case,
};

const T_AND_PHI: &[Port] = &[
    Port {
        name: "TDryBul",
        kind: R,
    },
    Port {
        name: "phi",
        kind: R,
    },
];

const T_DEW: &[Port] = &[Port {
    name: "TDewPoi",
    kind: R,
}];
const ENTHALPY: &[Port] = &[Port { name: "h", kind: R }];
const T_WET: &[Port] = &[Port {
    name: "TWetBul",
    kind: R,
}];

const CUSTOM_P_ATM: &[Param] = &[Param {
    name: "pAtm",
    value: ParamValue::Real("90000.0"),
}];

const CASES: &[BlockCase] = &[
    case(
        "psychrometrics_dew_point_t_dry_bul_phi",
        "CDL.Psychrometrics.DewPoint_TDryBulPhi",
        "DewPoint_TDryBulPhi",
        T_AND_PHI,
        &[],
        T_DEW,
    ),
    case(
        "psychrometrics_specific_enthalpy_t_dry_bul_phi",
        "CDL.Psychrometrics.SpecificEnthalpy_TDryBulPhi",
        "SpecificEnthalpy_TDryBulPhi",
        T_AND_PHI,
        &[],
        ENTHALPY,
    ),
    case(
        "psychrometrics_specific_enthalpy_custom_p_atm_pressure_boundaries",
        "CDL.Psychrometrics.SpecificEnthalpy_TDryBulPhi",
        "SpecificEnthalpy_TDryBulPhi/custom_p_atm_pressure_boundaries",
        T_AND_PHI,
        CUSTOM_P_ATM,
        ENTHALPY,
    ),
    case(
        "psychrometrics_wet_bulb_t_dry_bul_phi",
        "CDL.Psychrometrics.WetBulb_TDryBulPhi",
        "WetBulb_TDryBulPhi",
        T_AND_PHI,
        &[],
        T_WET,
    ),
];

#[test]
fn psychrometric_formula_blocks_match_aligned_tolerance_oracle() {
    assert_cases_match_aligned_tolerance_oracle(
        CASES,
        "CDL/Psychrometrics",
        "single-block-psychrometrics",
    );
}
