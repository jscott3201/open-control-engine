//! Aligned-tolerance conformance for transcendental `CDL.Reals.Sources` blocks.

mod block_harness;

use block_harness::{
    BlockCase, I, Param, ParamValue, Port, R, assert_cases_match_aligned_tolerance_oracle,
    assert_cases_match_exact_oracle, case,
};

const REAL_Y: &[Port] = &[Port { name: "y", kind: R }];
const CALENDAR_TIME_OUTPUTS: &[Port] = &[
    Port {
        name: "year",
        kind: I,
    },
    Port {
        name: "month",
        kind: I,
    },
    Port {
        name: "day",
        kind: I,
    },
    Port {
        name: "hour",
        kind: I,
    },
    Port {
        name: "minute",
        kind: R,
    },
    Port {
        name: "weekDay",
        kind: I,
    },
];

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

const CALENDAR_TIME_PARAMS: &[Param] = &[
    Param {
        name: "zerTim",
        value: ParamValue::Integer("11"),
    },
    Param {
        name: "yearRef",
        value: ParamValue::Integer("2016"),
    },
    Param {
        name: "offset",
        value: ParamValue::Real("0.0"),
    },
];

const CALENDAR_TIME_ANOMALY_PARAMS: &[Param] = &[
    Param {
        name: "zerTim",
        value: ParamValue::Integer("18"),
    },
    Param {
        name: "yearRef",
        value: ParamValue::Integer("2016"),
    },
    Param {
        name: "offset",
        value: ParamValue::Real("0.0"),
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

const CALENDAR_TIME_CASES: &[BlockCase] = &[
    case(
        "reals_source_calendar_time",
        "CDL.Reals.Sources.CalendarTime",
        "Sources/CalendarTime",
        &[],
        CALENDAR_TIME_PARAMS,
        CALENDAR_TIME_OUTPUTS,
    ),
    case(
        "reals_source_calendar_time_source_year_anomaly",
        "CDL.Reals.Sources.CalendarTime",
        "Sources/CalendarTime/source_year_anomaly",
        &[],
        CALENDAR_TIME_ANOMALY_PARAMS,
        CALENDAR_TIME_OUTPUTS,
    ),
];

#[test]
fn reals_source_transcendental_blocks_match_aligned_tolerance_oracle() {
    assert_cases_match_aligned_tolerance_oracle(
        CASES,
        "CDL/Reals",
        "single-block-reals-source-transcendental",
    );
}

#[test]
fn reals_calendar_time_blocks_match_exact_oracle() {
    assert_cases_match_exact_oracle(
        CALENDAR_TIME_CASES,
        "CDL/Reals",
        "single-block-reals-source-calendar-time",
    );
}
