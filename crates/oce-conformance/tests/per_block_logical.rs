//! Tier-1 exact conformance for CDL.Logical blocks through the frozen facade.

mod block_harness;

use block_harness::{
    B, BlockCase, Param, ParamValue, Port, R, assert_cases_are_deterministic,
    assert_cases_match_exact_oracle, case,
};

const U: &[Port] = &[Port { name: "u", kind: B }];
const U_REAL: &[Port] = &[Port { name: "u", kind: R }];
const U1_U2: &[Port] = &[
    Port {
        name: "u1",
        kind: B,
    },
    Port {
        name: "u2",
        kind: B,
    },
];
const U1_U2_U3: &[Port] = &[
    Port {
        name: "u1",
        kind: B,
    },
    Port {
        name: "u2",
        kind: B,
    },
    Port {
        name: "u3",
        kind: B,
    },
];
const U_CLR: &[Port] = &[
    Port { name: "u", kind: B },
    Port {
        name: "clr",
        kind: B,
    },
];
const U_RESET: &[Port] = &[
    Port { name: "u", kind: B },
    Port {
        name: "reset",
        kind: B,
    },
];
const PROOF_INPUTS: &[Port] = &[
    Port {
        name: "u_s",
        kind: B,
    },
    Port {
        name: "u_m",
        kind: B,
    },
];
const SWITCH_INPUTS: &[Port] = &[
    Port {
        name: "u1",
        kind: B,
    },
    Port {
        name: "u2",
        kind: B,
    },
    Port {
        name: "u3",
        kind: B,
    },
];
const TIMER_OUTPUTS: &[Port] = &[
    Port { name: "y", kind: R },
    Port {
        name: "passed",
        kind: B,
    },
];
const BOOL_Y: &[Port] = &[Port { name: "y", kind: B }];
const PROOF_OUTPUTS: &[Port] = &[
    Port {
        name: "yLocFal",
        kind: B,
    },
    Port {
        name: "yLocTru",
        kind: B,
    },
];

const TIMER_PARAMS: &[Param] = &[Param {
    name: "t",
    value: ParamValue::Real("0.25"),
}];
const MULTI_NIN_3: &[Param] = &[Param {
    name: "nin",
    value: ParamValue::Integer("3"),
}];
const TIMER_ZERO_PARAMS: &[Param] = &[Param {
    name: "t",
    value: ParamValue::Real("0.0"),
}];
const TIMER_ACCUMULATING_LATCH_PARAMS: &[Param] = &[Param {
    name: "t",
    value: ParamValue::Real("0.5"),
}];
const TRUE_DELAY_PARAMS: &[Param] = &[Param {
    name: "delayTime",
    value: ParamValue::Real("75.0"),
}];
const TRUE_FALSE_HOLD_PARAMS: &[Param] = &[
    Param {
        name: "trueHoldDuration",
        value: ParamValue::Real("300.0"),
    },
    Param {
        name: "falseHoldDuration",
        value: ParamValue::Real("300.0"),
    },
];
const TRUE_HOLD_WITH_RESET_PARAMS: &[Param] = &[Param {
    name: "duration",
    value: ParamValue::Real("200.0"),
}];
const PROOF_PARAMS: &[Param] = &[
    Param {
        name: "debounce",
        value: ParamValue::Real("2.0"),
    },
    Param {
        name: "feedbackDelay",
        value: ParamValue::Real("5.0"),
    },
];
const PROOF_INVERTED_DELAY_PARAMS: &[Param] = &[
    Param {
        name: "debounce",
        value: ParamValue::Real("5.0"),
    },
    Param {
        name: "feedbackDelay",
        value: ParamValue::Real("2.0"),
    },
];
const SAMPLE_TRIGGER_PARAMS: &[Param] = &[
    Param {
        name: "period",
        value: ParamValue::Real("2.0"),
    },
    Param {
        name: "shift",
        value: ParamValue::Real("1.0"),
    },
];
const SAMPLE_TRIGGER_SHIFT_AFTER_PERIOD_PARAMS: &[Param] = &[
    Param {
        name: "period",
        value: ParamValue::Real("1.0"),
    },
    Param {
        name: "shift",
        value: ParamValue::Real("2.0"),
    },
];
const SAMPLE_TRIGGER_NEGATIVE_SHIFT_PARAMS: &[Param] = &[
    Param {
        name: "period",
        value: ParamValue::Real("1.0"),
    },
    Param {
        name: "shift",
        value: ParamValue::Real("-0.5"),
    },
];
const SOURCE_PULSE_PARAMS: &[Param] = &[
    Param {
        name: "width",
        value: ParamValue::Real("0.2"),
    },
    Param {
        name: "period",
        value: ParamValue::Real("2.0"),
    },
    Param {
        name: "shift",
        value: ParamValue::Real("0.6"),
    },
];
const SOURCE_PULSE_NEGATIVE_SHIFT_PARAMS: &[Param] = &[
    Param {
        name: "width",
        value: ParamValue::Real("0.2"),
    },
    Param {
        name: "period",
        value: ParamValue::Real("2.0"),
    },
    Param {
        name: "shift",
        value: ParamValue::Real("-1.9"),
    },
];
const SOURCE_PULSE_WIDTH_ONE_PARAMS: &[Param] = &[
    Param {
        name: "width",
        value: ParamValue::Real("1.0"),
    },
    Param {
        name: "period",
        value: ParamValue::Real("2.0"),
    },
    Param {
        name: "shift",
        value: ParamValue::Real("0.6"),
    },
];
const VARIABLE_PULSE_PARAMS: &[Param] = &[
    Param {
        name: "period",
        value: ParamValue::Real("4.0"),
    },
    Param {
        name: "deltaU",
        value: ParamValue::Real("0.01"),
    },
    Param {
        name: "minTruFalHol",
        value: ParamValue::Real("0.04"),
    },
];
const VARIABLE_PULSE_DELTA_RESET_PARAMS: &[Param] = &[
    Param {
        name: "period",
        value: ParamValue::Real("4.0"),
    },
    Param {
        name: "deltaU",
        value: ParamValue::Real("0.125"),
    },
    Param {
        name: "minTruFalHol",
        value: ParamValue::Real("0.04"),
    },
];
const VARIABLE_PULSE_MINIMUM_HOLD_PARAMS: &[Param] = &[
    Param {
        name: "period",
        value: ParamValue::Real("3.0"),
    },
    Param {
        name: "deltaU",
        value: ParamValue::Real("0.01"),
    },
    Param {
        name: "minTruFalHol",
        value: ParamValue::Real("1.0"),
    },
];
const VARIABLE_PULSE_ADJUSTED_PERIOD_PARAMS: &[Param] = &[
    Param {
        name: "period",
        value: ParamValue::Real("1.0"),
    },
    Param {
        name: "deltaU",
        value: ParamValue::Real("0.01"),
    },
    Param {
        name: "minTruFalHol",
        value: ParamValue::Real("1.0"),
    },
];

const CASES: &[BlockCase] = &[
    case("logical_and", "CDL.Logical.And", "And", U1_U2, &[], BOOL_Y),
    case(
        "logical_nand",
        "CDL.Logical.Nand",
        "Nand",
        U1_U2,
        &[],
        BOOL_Y,
    ),
    case("logical_or", "CDL.Logical.Or", "Or", U1_U2, &[], BOOL_Y),
    case(
        "logical_multi_and",
        "CDL.Logical.MultiAnd",
        "MultiAnd",
        U1_U2_U3,
        MULTI_NIN_3,
        BOOL_Y,
    ),
    case(
        "logical_multi_or",
        "CDL.Logical.MultiOr",
        "MultiOr",
        U1_U2_U3,
        MULTI_NIN_3,
        BOOL_Y,
    ),
    case("logical_xor", "CDL.Logical.Xor", "Xor", U1_U2, &[], BOOL_Y),
    case("logical_not", "CDL.Logical.Not", "Not", U, &[], BOOL_Y),
    case(
        "logical_switch",
        "CDL.Logical.Switch",
        "Switch",
        SWITCH_INPUTS,
        &[],
        BOOL_Y,
    ),
    case("logical_edge", "CDL.Logical.Edge", "Edge", U, &[], BOOL_Y),
    case(
        "logical_falling_edge",
        "CDL.Logical.FallingEdge",
        "FallingEdge",
        U,
        &[],
        BOOL_Y,
    ),
    case(
        "logical_change",
        "CDL.Logical.Change",
        "Change",
        U,
        &[],
        BOOL_Y,
    ),
    case(
        "logical_latch",
        "CDL.Logical.Latch",
        "Latch",
        U_CLR,
        &[],
        BOOL_Y,
    ),
    case(
        "logical_toggle",
        "CDL.Logical.Toggle",
        "Toggle",
        U_CLR,
        &[],
        BOOL_Y,
    ),
    case(
        "logical_timer",
        "CDL.Logical.Timer",
        "Timer",
        U,
        TIMER_PARAMS,
        TIMER_OUTPUTS,
    ),
    case(
        "logical_timer_threshold_zero",
        "CDL.Logical.Timer",
        "Timer/threshold_zero",
        U,
        TIMER_ZERO_PARAMS,
        TIMER_OUTPUTS,
    ),
    // Input held false from the start: `pre(passed) = t <= 0` initializes the latch true and no
    // edge ever fires a clearing clause — the oracle-diff scenario for the 2026-07-06 closeout
    // Timer divergence fix.
    case(
        "logical_timer_input_never_rises",
        "CDL.Logical.Timer",
        "Timer/input_never_rises",
        U,
        TIMER_ZERO_PARAMS,
        TIMER_OUTPUTS,
    ),
    case(
        "logical_timer_accumulating_threshold_zero",
        "CDL.Logical.TimerAccumulating",
        "TimerAccumulating/threshold_zero",
        U_RESET,
        TIMER_ZERO_PARAMS,
        TIMER_OUTPUTS,
    ),
    case(
        "logical_timer_accumulating_latch_reset",
        "CDL.Logical.TimerAccumulating",
        "TimerAccumulating/latch_reset",
        U_RESET,
        TIMER_ACCUMULATING_LATCH_PARAMS,
        TIMER_OUTPUTS,
    ),
    case(
        "logical_true_delay",
        "CDL.Logical.TrueDelay",
        "TrueDelay",
        U,
        TRUE_DELAY_PARAMS,
        BOOL_Y,
    ),
    case(
        "logical_true_false_hold",
        "CDL.Logical.TrueFalseHold",
        "TrueFalseHold",
        U,
        TRUE_FALSE_HOLD_PARAMS,
        BOOL_Y,
    ),
    case(
        "logical_true_hold_with_reset",
        "CDL.Logical.TrueHoldWithReset",
        "TrueHoldWithReset",
        U_CLR,
        TRUE_HOLD_WITH_RESET_PARAMS,
        BOOL_Y,
    ),
    case(
        "logical_proof_stable_equal_no_alarm",
        "CDL.Logical.Proof",
        "Proof/stable_equal_no_alarm",
        PROOF_INPUTS,
        PROOF_PARAMS,
        PROOF_OUTPUTS,
    ),
    case(
        "logical_proof_mismatch_latches_clear_on_equal",
        "CDL.Logical.Proof",
        "Proof/mismatch_latches_clear_on_equal",
        PROOF_INPUTS,
        PROOF_PARAMS,
        PROOF_OUTPUTS,
    ),
    case(
        "logical_proof_debounce_before_feedback",
        "CDL.Logical.Proof",
        "Proof/debounce_before_feedback",
        PROOF_INPUTS,
        PROOF_PARAMS,
        PROOF_OUTPUTS,
    ),
    case(
        "logical_proof_feedback_before_debounce_then_unstable_both",
        "CDL.Logical.Proof",
        "Proof/feedback_before_debounce_then_unstable_both",
        PROOF_INPUTS,
        PROOF_PARAMS,
        PROOF_OUTPUTS,
    ),
    case(
        "logical_proof_inverted_delay_warning_only",
        "CDL.Logical.Proof",
        "Proof/inverted_delay_warning_only",
        PROOF_INPUTS,
        PROOF_INVERTED_DELAY_PARAMS,
        PROOF_OUTPUTS,
    ),
    case(
        "logical_variable_pulse",
        "CDL.Logical.VariablePulse",
        "VariablePulse",
        U_REAL,
        VARIABLE_PULSE_PARAMS,
        BOOL_Y,
    ),
    case(
        "logical_variable_pulse_delta_reset",
        "CDL.Logical.VariablePulse",
        "VariablePulse/delta_reset",
        U_REAL,
        VARIABLE_PULSE_DELTA_RESET_PARAMS,
        BOOL_Y,
    ),
    case(
        "logical_variable_pulse_endpoints",
        "CDL.Logical.VariablePulse",
        "VariablePulse/endpoints",
        U_REAL,
        VARIABLE_PULSE_PARAMS,
        BOOL_Y,
    ),
    case(
        "logical_variable_pulse_minimum_hold",
        "CDL.Logical.VariablePulse",
        "VariablePulse/minimum_hold",
        U_REAL,
        VARIABLE_PULSE_MINIMUM_HOLD_PARAMS,
        BOOL_Y,
    ),
    case(
        "logical_variable_pulse_adjusted_period",
        "CDL.Logical.VariablePulse",
        "VariablePulse/adjusted_period",
        U_REAL,
        VARIABLE_PULSE_ADJUSTED_PERIOD_PARAMS,
        BOOL_Y,
    ),
    case(
        "logical_sample_trigger",
        "CDL.Logical.Sources.SampleTrigger",
        "Sources/SampleTrigger",
        &[],
        SAMPLE_TRIGGER_PARAMS,
        BOOL_Y,
    ),
    case(
        "logical_sample_trigger_shift_after_period",
        "CDL.Logical.Sources.SampleTrigger",
        "Sources/SampleTrigger/shift_after_period",
        &[],
        SAMPLE_TRIGGER_SHIFT_AFTER_PERIOD_PARAMS,
        BOOL_Y,
    ),
    case(
        "logical_sample_trigger_negative_shift",
        "CDL.Logical.Sources.SampleTrigger",
        "Sources/SampleTrigger/negative_shift",
        &[],
        SAMPLE_TRIGGER_NEGATIVE_SHIFT_PARAMS,
        BOOL_Y,
    ),
    case(
        "logical_source_pulse",
        "CDL.Logical.Sources.Pulse",
        "Sources/Pulse",
        &[],
        SOURCE_PULSE_PARAMS,
        BOOL_Y,
    ),
    case(
        "logical_source_pulse_negative_shift_folded",
        "CDL.Logical.Sources.Pulse",
        "Sources/Pulse/negative_shift_folded",
        &[],
        SOURCE_PULSE_NEGATIVE_SHIFT_PARAMS,
        BOOL_Y,
    ),
    case(
        "logical_source_pulse_width_one",
        "CDL.Logical.Sources.Pulse",
        "Sources/Pulse/width_one",
        &[],
        SOURCE_PULSE_WIDTH_ONE_PARAMS,
        BOOL_Y,
    ),
];

const STATEFUL_LOGICAL_SLUGS: &[&str] = &[
    "logical_edge",
    "logical_falling_edge",
    "logical_change",
    "logical_latch",
    "logical_toggle",
    "logical_timer",
    "logical_timer_threshold_zero",
    "logical_timer_accumulating_threshold_zero",
    "logical_timer_accumulating_latch_reset",
    "logical_true_delay",
    "logical_true_false_hold",
    "logical_true_hold_with_reset",
    "logical_proof_stable_equal_no_alarm",
    "logical_proof_mismatch_latches_clear_on_equal",
    "logical_proof_debounce_before_feedback",
    "logical_proof_feedback_before_debounce_then_unstable_both",
    "logical_proof_inverted_delay_warning_only",
    "logical_variable_pulse",
    "logical_variable_pulse_delta_reset",
    "logical_variable_pulse_endpoints",
    "logical_variable_pulse_minimum_hold",
    "logical_variable_pulse_adjusted_period",
    "logical_sample_trigger",
    "logical_sample_trigger_shift_after_period",
    "logical_sample_trigger_negative_shift",
];

#[test]
fn logical_reference_blocks_match_exact_oracle() {
    assert_cases_match_exact_oracle(CASES, "CDL/Logical", "single-block-logical");
}

#[test]
fn stateful_logical_blocks_exact_runs_are_deterministic() {
    assert_cases_are_deterministic(
        CASES,
        STATEFUL_LOGICAL_SLUGS,
        "CDL/Logical",
        "single-block-logical",
    );
}
