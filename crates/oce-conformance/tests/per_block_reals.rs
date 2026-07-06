//! Tier-1 exact conformance for CDL.Reals blocks through the frozen facade.

mod block_harness;

use block_harness::{
    B, BlockCase, Param, ParamValue, Port, R, assert_cases_are_deterministic,
    assert_cases_match_exact_oracle, case,
};

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
const LINE_INPUTS: &[Port] = &[
    Port {
        name: "x1",
        kind: R,
    },
    Port {
        name: "f1",
        kind: R,
    },
    Port {
        name: "x2",
        kind: R,
    },
    Port {
        name: "f2",
        kind: R,
    },
    Port { name: "u", kind: R },
];
const SWITCH_INPUTS: &[Port] = &[
    Port {
        name: "u1",
        kind: R,
    },
    Port {
        name: "u2",
        kind: B,
    },
    Port {
        name: "u3",
        kind: R,
    },
];
const RAMP_INPUTS: &[Port] = &[
    Port { name: "u", kind: R },
    Port {
        name: "active",
        kind: B,
    },
];
const REAL_Y: &[Port] = &[Port { name: "y", kind: R }];
const BOOL_Y: &[Port] = &[Port { name: "y", kind: B }];

const P_02: &[Param] = &[Param {
    name: "p",
    value: ParamValue::Real("0.2"),
}];
const K_01: &[Param] = &[Param {
    name: "k",
    value: ParamValue::Real("0.1"),
}];
const CONSTANT_K: &[Param] = &[Param {
    name: "k",
    value: ParamValue::Real("21.5"),
}];
const SOURCE_RAMP_PARAMS: &[Param] = &[
    Param {
        name: "height",
        value: ParamValue::Real("2.0"),
    },
    Param {
        name: "duration",
        value: ParamValue::Real("3.0"),
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
const SOURCE_RAMP_NEGATIVE_HEIGHT_PARAMS: &[Param] = &[
    Param {
        name: "height",
        value: ParamValue::Real("-2.0"),
    },
    Param {
        name: "duration",
        value: ParamValue::Real("2.0"),
    },
    Param {
        name: "offset",
        value: ParamValue::Real("10.0"),
    },
    Param {
        name: "startTime",
        value: ParamValue::Real("-1.0"),
    },
];
const SOURCE_PULSE_PARAMS: &[Param] = &[
    Param {
        name: "amplitude",
        value: ParamValue::Real("2.0"),
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
        value: ParamValue::Real("0.0"),
    },
    Param {
        name: "offset",
        value: ParamValue::Real("0.2"),
    },
];
const ROUND_N_TWO: &[Param] = &[Param {
    name: "n",
    value: ParamValue::Integer("2"),
}];
const LIMITER_PARAMS: &[Param] = &[
    Param {
        name: "uMin",
        value: ParamValue::Real("0.0"),
    },
    Param {
        name: "uMax",
        value: ParamValue::Real("5.5"),
    },
];
const LINE_LIMIT_BELOW_ONLY_PARAMS: &[Param] = &[
    Param {
        name: "limitBelow",
        value: ParamValue::Boolean("true"),
    },
    Param {
        name: "limitAbove",
        value: ParamValue::Boolean("false"),
    },
];
const LINE_LIMIT_ABOVE_ONLY_PARAMS: &[Param] = &[
    Param {
        name: "limitBelow",
        value: ParamValue::Boolean("false"),
    },
    Param {
        name: "limitAbove",
        value: ParamValue::Boolean("true"),
    },
];
const LINE_UNLIMITED_PARAMS: &[Param] = &[
    Param {
        name: "limitBelow",
        value: ParamValue::Boolean("false"),
    },
    Param {
        name: "limitAbove",
        value: ParamValue::Boolean("false"),
    },
];
const THRESHOLD_PARAMS: &[Param] = &[Param {
    name: "t",
    value: ParamValue::Real("4.5"),
}];
const HYSTERETIC_COMPARATOR_PARAMS: &[Param] = &[Param {
    name: "h",
    value: ParamValue::Real("1.0"),
}];
const HYSTERETIC_COMPARATOR_PRESET_PARAMS: &[Param] = &[
    Param {
        name: "h",
        value: ParamValue::Real("1.0"),
    },
    Param {
        name: "pre_y_start",
        value: ParamValue::Boolean("true"),
    },
];
const HYSTERETIC_THRESHOLD_PARAMS: &[Param] = &[
    Param {
        name: "t",
        value: ParamValue::Real("4.5"),
    },
    Param {
        name: "h",
        value: ParamValue::Real("1.0"),
    },
];
const HYSTERETIC_THRESHOLD_PRESET_PARAMS: &[Param] = &[
    Param {
        name: "t",
        value: ParamValue::Real("4.5"),
    },
    Param {
        name: "h",
        value: ParamValue::Real("1.0"),
    },
    Param {
        name: "pre_y_start",
        value: ParamValue::Boolean("true"),
    },
];
const HYSTERESIS_PARAMS: &[Param] = &[
    Param {
        name: "uLow",
        value: ParamValue::Real("1.5"),
    },
    Param {
        name: "uHigh",
        value: ParamValue::Real("3.5"),
    },
];
const RAMP_CLAMPED_PARAMS: &[Param] = &[
    Param {
        name: "raisingSlewRate",
        value: ParamValue::Real("2.0"),
    },
    Param {
        name: "fallingSlewRate",
        value: ParamValue::Real("-3.0"),
    },
    Param {
        name: "Td",
        value: ParamValue::Real("0.1"),
    },
];
const RAMP_INACTIVE_PARAMS: &[Param] = &[
    Param {
        name: "raisingSlewRate",
        value: ParamValue::Real("2.0"),
    },
    Param {
        name: "fallingSlewRate",
        value: ParamValue::Real("-2.0"),
    },
    Param {
        name: "Td",
        value: ParamValue::Real("1.0"),
    },
];
const RAMP_ACTIVE_REINIT_PARAMS: &[Param] = &[
    Param {
        name: "raisingSlewRate",
        value: ParamValue::Real("0.5"),
    },
    Param {
        name: "fallingSlewRate",
        value: ParamValue::Real("-0.5"),
    },
    Param {
        name: "Td",
        value: ParamValue::Real("1.0"),
    },
];
const RAMP_IMPLICIT_RESIDUE_PARAMS: &[Param] = &[
    Param {
        name: "raisingSlewRate",
        value: ParamValue::Real("100.0"),
    },
    Param {
        name: "fallingSlewRate",
        value: ParamValue::Real("-100.0"),
    },
    Param {
        name: "Td",
        value: ParamValue::Real("1.0"),
    },
];
const CASES: &[BlockCase] = &[
    case(
        "reals_constant",
        "CDL.Reals.Sources.Constant",
        "Sources/Constant",
        &[],
        CONSTANT_K,
        REAL_Y,
    ),
    case(
        "reals_civil_time",
        "CDL.Reals.Sources.CivilTime",
        "Sources/CivilTime",
        &[],
        &[],
        REAL_Y,
    ),
    case(
        "reals_source_pulse",
        "CDL.Reals.Sources.Pulse",
        "Sources/Pulse",
        &[],
        SOURCE_PULSE_PARAMS,
        REAL_Y,
    ),
    case(
        "reals_source_ramp",
        "CDL.Reals.Sources.Ramp",
        "Sources/Ramp",
        &[],
        SOURCE_RAMP_PARAMS,
        REAL_Y,
    ),
    case(
        "reals_source_ramp_negative_height_start",
        "CDL.Reals.Sources.Ramp",
        "Sources/Ramp/negative_height_start",
        &[],
        SOURCE_RAMP_NEGATIVE_HEIGHT_PARAMS,
        REAL_Y,
    ),
    case("reals_add", "CDL.Reals.Add", "Add", U1_U2, &[], REAL_Y),
    case(
        "reals_subtract",
        "CDL.Reals.Subtract",
        "Subtract",
        U1_U2,
        &[],
        REAL_Y,
    ),
    case(
        "reals_multiply",
        "CDL.Reals.Multiply",
        "Multiply",
        U1_U2,
        &[],
        REAL_Y,
    ),
    case(
        "reals_divide",
        "CDL.Reals.Divide",
        "Divide",
        U1_U2,
        &[],
        REAL_Y,
    ),
    case("reals_sqrt", "CDL.Reals.Sqrt", "Sqrt", U, &[], REAL_Y),
    case(
        "reals_average",
        "CDL.Reals.Average",
        "Average",
        U1_U2,
        &[],
        REAL_Y,
    ),
    case(
        "reals_modulo",
        "CDL.Reals.Modulo",
        "Modulo",
        U1_U2,
        &[],
        REAL_Y,
    ),
    case(
        "reals_round",
        "CDL.Reals.Round",
        "Round",
        U,
        ROUND_N_TWO,
        REAL_Y,
    ),
    case(
        "reals_add_parameter",
        "CDL.Reals.AddParameter",
        "AddParameter",
        U,
        P_02,
        REAL_Y,
    ),
    case(
        "reals_multiply_by_parameter",
        "CDL.Reals.MultiplyByParameter",
        "MultiplyByParameter",
        U,
        K_01,
        REAL_Y,
    ),
    case("reals_abs", "CDL.Reals.Abs", "Abs", U, &[], REAL_Y),
    case("reals_min", "CDL.Reals.Min", "Min", U1_U2, &[], REAL_Y),
    case("reals_max", "CDL.Reals.Max", "Max", U1_U2, &[], REAL_Y),
    case(
        "reals_limiter",
        "CDL.Reals.Limiter",
        "Limiter",
        U,
        LIMITER_PARAMS,
        REAL_Y,
    ),
    case(
        "reals_line",
        "CDL.Reals.Line",
        "Line",
        LINE_INPUTS,
        &[],
        REAL_Y,
    ),
    case(
        "reals_line_limit_below_only",
        "CDL.Reals.Line",
        "Line/limit_below_only",
        LINE_INPUTS,
        LINE_LIMIT_BELOW_ONLY_PARAMS,
        REAL_Y,
    ),
    case(
        "reals_line_limit_above_only",
        "CDL.Reals.Line",
        "Line/limit_above_only",
        LINE_INPUTS,
        LINE_LIMIT_ABOVE_ONLY_PARAMS,
        REAL_Y,
    ),
    case(
        "reals_line_unlimited",
        "CDL.Reals.Line",
        "Line/unlimited",
        LINE_INPUTS,
        LINE_UNLIMITED_PARAMS,
        REAL_Y,
    ),
    case(
        "reals_greater",
        "CDL.Reals.Greater",
        "Greater",
        U1_U2,
        &[],
        BOOL_Y,
    ),
    case(
        "reals_greater_threshold",
        "CDL.Reals.GreaterThreshold",
        "GreaterThreshold",
        U,
        THRESHOLD_PARAMS,
        BOOL_Y,
    ),
    case(
        "reals_greater_hysteretic",
        "CDL.Reals.Greater",
        "Greater/hysteretic",
        U1_U2,
        HYSTERETIC_COMPARATOR_PARAMS,
        BOOL_Y,
    ),
    case(
        "reals_greater_hysteretic_preset",
        "CDL.Reals.Greater",
        "Greater/hysteretic_preset",
        U1_U2,
        HYSTERETIC_COMPARATOR_PRESET_PARAMS,
        BOOL_Y,
    ),
    case(
        "reals_greater_threshold_hysteretic",
        "CDL.Reals.GreaterThreshold",
        "GreaterThreshold/hysteretic",
        U,
        HYSTERETIC_THRESHOLD_PARAMS,
        BOOL_Y,
    ),
    case(
        "reals_greater_threshold_hysteretic_preset",
        "CDL.Reals.GreaterThreshold",
        "GreaterThreshold/hysteretic_preset",
        U,
        HYSTERETIC_THRESHOLD_PRESET_PARAMS,
        BOOL_Y,
    ),
    case(
        "reals_hysteresis",
        "CDL.Reals.Hysteresis",
        "Hysteresis",
        U,
        HYSTERESIS_PARAMS,
        BOOL_Y,
    ),
    case("reals_less", "CDL.Reals.Less", "Less", U1_U2, &[], BOOL_Y),
    case(
        "reals_less_threshold",
        "CDL.Reals.LessThreshold",
        "LessThreshold",
        U,
        THRESHOLD_PARAMS,
        BOOL_Y,
    ),
    case(
        "reals_less_hysteretic",
        "CDL.Reals.Less",
        "Less/hysteretic",
        U1_U2,
        HYSTERETIC_COMPARATOR_PARAMS,
        BOOL_Y,
    ),
    case(
        "reals_less_hysteretic_preset",
        "CDL.Reals.Less",
        "Less/hysteretic_preset",
        U1_U2,
        HYSTERETIC_COMPARATOR_PRESET_PARAMS,
        BOOL_Y,
    ),
    case(
        "reals_less_threshold_hysteretic",
        "CDL.Reals.LessThreshold",
        "LessThreshold/hysteretic",
        U,
        HYSTERETIC_THRESHOLD_PARAMS,
        BOOL_Y,
    ),
    case(
        "reals_less_threshold_hysteretic_preset",
        "CDL.Reals.LessThreshold",
        "LessThreshold/hysteretic_preset",
        U,
        HYSTERETIC_THRESHOLD_PRESET_PARAMS,
        BOOL_Y,
    ),
    case(
        "reals_switch",
        "CDL.Reals.Switch",
        "Switch",
        SWITCH_INPUTS,
        &[],
        REAL_Y,
    ),
    case(
        "reals_ramp_inactive_passthrough",
        "CDL.Reals.Ramp",
        "Ramp/inactive_passthrough",
        RAMP_INPUTS,
        RAMP_INACTIVE_PARAMS,
        REAL_Y,
    ),
    case(
        "reals_ramp_active_reinitialize",
        "CDL.Reals.Ramp",
        "Ramp/active_reinitialize",
        RAMP_INPUTS,
        RAMP_ACTIVE_REINIT_PARAMS,
        REAL_Y,
    ),
    case(
        "reals_ramp_clamped_edges",
        "CDL.Reals.Ramp",
        "Ramp/clamped_edges",
        RAMP_INPUTS,
        RAMP_CLAMPED_PARAMS,
        REAL_Y,
    ),
    case(
        "reals_ramp_implicit_residue",
        "CDL.Reals.Ramp",
        "Ramp/implicit_residue",
        RAMP_INPUTS,
        RAMP_IMPLICIT_RESIDUE_PARAMS,
        REAL_Y,
    ),
];

const STATEFUL_REALS_SLUGS: &[&str] = &[
    "reals_hysteresis",
    "reals_greater_hysteretic",
    "reals_greater_hysteretic_preset",
    "reals_greater_threshold_hysteretic",
    "reals_greater_threshold_hysteretic_preset",
    "reals_less_hysteretic",
    "reals_less_hysteretic_preset",
    "reals_less_threshold_hysteretic",
    "reals_less_threshold_hysteretic_preset",
    "reals_ramp_inactive_passthrough",
    "reals_ramp_active_reinitialize",
    "reals_ramp_clamped_edges",
    "reals_ramp_implicit_residue",
];

#[test]
fn reals_reference_blocks_match_exact_oracle() {
    assert_cases_match_exact_oracle(CASES, "CDL/Reals", "single-block-reals");
}

#[test]
fn stateful_reals_blocks_exact_runs_are_deterministic() {
    assert_cases_are_deterministic(
        CASES,
        STATEFUL_REALS_SLUGS,
        "CDL/Reals",
        "single-block-reals",
    );
}
