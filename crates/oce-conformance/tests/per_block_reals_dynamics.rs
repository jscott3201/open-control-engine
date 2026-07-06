//! Tier-1 exact conformance for the dynamic `CDL.Reals` controller and integrator blocks
//! (PID, PIDWithReset, IntegratorWithReset) through the frozen facade. Split from
//! `per_block_reals.rs` along the reals-dynamics behavioral topic; the checked-in goldens are
//! independent Tier-A recurrence oracles from `tools/golden-gen` (never engine self-output).

mod block_harness;

use block_harness::{
    B, BlockCase, Param, ParamValue, Port, R, assert_cases_are_deterministic,
    assert_cases_match_exact_oracle, case,
};

const PID_INPUTS: &[Port] = &[
    Port {
        name: "u_s",
        kind: R,
    },
    Port {
        name: "u_m",
        kind: R,
    },
];
const PID_RESET_INPUTS: &[Port] = &[
    Port {
        name: "u_s",
        kind: R,
    },
    Port {
        name: "u_m",
        kind: R,
    },
    Port {
        name: "trigger",
        kind: B,
    },
];
const INTEGRATOR_WITH_RESET_INPUTS: &[Port] = &[
    Port { name: "u", kind: R },
    Port {
        name: "y_reset_in",
        kind: R,
    },
    Port {
        name: "trigger",
        kind: B,
    },
];

const REAL_Y: &[Port] = &[Port { name: "y", kind: R }];

const PID_PI_RECURRENCE_PARAMS: &[Param] = &[
    Param {
        name: "k",
        value: ParamValue::Real("0.37"),
    },
    Param {
        name: "Ti",
        value: ParamValue::Real("0.3"),
    },
    Param {
        name: "xi_start",
        value: ParamValue::Real("0.19"),
    },
    Param {
        name: "yMin",
        value: ParamValue::Real("-0.36"),
    },
    Param {
        name: "yMax",
        value: ParamValue::Real("0.42"),
    },
];
const PID_RESET_PI_RECURRENCE_PARAMS: &[Param] = &[
    Param {
        name: "k",
        value: ParamValue::Real("0.37"),
    },
    Param {
        name: "Ti",
        value: ParamValue::Real("0.3"),
    },
    Param {
        name: "xi_start",
        value: ParamValue::Real("0.11"),
    },
    Param {
        name: "y_reset",
        value: ParamValue::Real("0.275"),
    },
    Param {
        name: "yMin",
        value: ParamValue::Real("-0.35"),
    },
    Param {
        name: "yMax",
        value: ParamValue::Real("0.45"),
    },
];
const INTEGRATOR_WITH_RESET_PARAMS: &[Param] = &[Param {
    name: "y_start",
    value: ParamValue::Real("1.0"),
}];
const INTEGRATOR_WITH_RESET_GAIN_PARAMS: &[Param] = &[
    Param {
        name: "k",
        value: ParamValue::Real("2.5"),
    },
    Param {
        name: "y_start",
        value: ParamValue::Real("1.0"),
    },
];

const CASES: &[BlockCase] = &[
    case(
        "reals_pid_pi_recurrence",
        "CDL.Reals.PID",
        "PID/pi_recurrence",
        PID_INPUTS,
        PID_PI_RECURRENCE_PARAMS,
        REAL_Y,
    ),
    case(
        "reals_pid_with_reset_pi_recurrence",
        "CDL.Reals.PIDWithReset",
        "PIDWithReset/pi_reset_recurrence",
        PID_RESET_INPUTS,
        PID_RESET_PI_RECURRENCE_PARAMS,
        REAL_Y,
    ),
    case(
        "reals_integrator_with_reset",
        "CDL.Reals.IntegratorWithReset",
        "IntegratorWithReset",
        INTEGRATOR_WITH_RESET_INPUTS,
        INTEGRATOR_WITH_RESET_PARAMS,
        REAL_Y,
    ),
    // k=2.5 oracle-diff golden for the 2026-07-06 closeout divergence fix: the engine previously
    // dropped the upstream gain `k` (der(y)=k*u), so any k != 1 model mis-integrated silently.
    case(
        "reals_integrator_with_reset_gain",
        "CDL.Reals.IntegratorWithReset",
        "IntegratorWithReset/gain",
        INTEGRATOR_WITH_RESET_INPUTS,
        INTEGRATOR_WITH_RESET_GAIN_PARAMS,
        REAL_Y,
    ),
];

const DYNAMICS_SLUGS: &[&str] = &[
    "reals_pid_pi_recurrence",
    "reals_pid_with_reset_pi_recurrence",
    "reals_integrator_with_reset",
    "reals_integrator_with_reset_gain",
];

#[test]
fn reals_dynamics_blocks_match_exact_oracle() {
    assert_cases_match_exact_oracle(CASES, "CDL/Reals", "single-block-reals-dynamics");
}

#[test]
fn reals_dynamics_exact_runs_are_deterministic() {
    assert_cases_are_deterministic(
        CASES,
        DYNAMICS_SLUGS,
        "CDL/Reals",
        "single-block-reals-dynamics",
    );
}
