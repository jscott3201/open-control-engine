//! Exact conformance for typed `CDL.*.Sources.TimeTable` blocks through the frozen facade.

mod block_harness;

use block_harness::{
    B, BlockCase, I, Param, ParamValue, Port, R, assert_cases_match_exact_oracle, case,
};

const REAL_TABLE_OUTPUTS: &[Port] = &[
    Port {
        name: "y1",
        kind: R,
    },
    Port {
        name: "y2",
        kind: R,
    },
];
const REAL_TABLE_SINGLE_OUTPUT: &[Port] = &[Port { name: "y", kind: R }];
const INTEGER_TABLE_OUTPUTS: &[Port] = &[
    Port {
        name: "y1",
        kind: I,
    },
    Port {
        name: "y2",
        kind: I,
    },
];
const LOGICAL_TABLE_OUTPUTS: &[Port] = &[
    Port {
        name: "y1",
        kind: B,
    },
    Port {
        name: "y2",
        kind: B,
    },
];

const REAL_TABLE_PARAMS: &[Param] = &[
    rp("table_1_1", "0.0"),
    rp("table_1_2", "0.0"),
    rp("table_1_3", "10.0"),
    rp("table_2_1", "1.0"),
    rp("table_2_2", "0.0"),
    rp("table_2_3", "20.0"),
    rp("table_3_1", "1.0"),
    rp("table_3_2", "1.0"),
    rp("table_3_3", "30.0"),
    rp("table_4_1", "2.0"),
    rp("table_4_2", "4.0"),
    rp("table_4_3", "40.0"),
    rp("table_5_1", "3.0"),
    rp("table_5_2", "9.0"),
    rp("table_5_3", "50.0"),
    ip("smoothness", "1"),
    ip("extrapolation", "2"),
    rp("offset_1", "0.5"),
    rp("offset_2", "-1.0"),
];

const REAL_PERIODIC_PARAMS: &[Param] = &[
    rp("table_1_1", "0.0"),
    rp("table_1_2", "0.0"),
    rp("table_2_1", "1.0"),
    rp("table_2_2", "10.0"),
    rp("table_3_1", "2.0"),
    rp("table_3_2", "20.0"),
    rp("timeScale", "2.0"),
];

const INTEGER_TABLE_PARAMS: &[Param] = &[
    rp("table_1_1", "0.0"),
    rp("table_1_2", "-2.0"),
    rp("table_1_3", "7.0"),
    rp("table_2_1", "2.0"),
    rp("table_2_2", "3.0"),
    rp("table_2_3", "8.0"),
    rp("table_3_1", "5.0"),
    rp("table_3_2", "4.0"),
    rp("table_3_3", "9.0"),
    rp("period", "6.0"),
];

const LOGICAL_TABLE_PARAMS: &[Param] = &[
    rp("table_1_1", "0.0"),
    rp("table_1_2", "0.0"),
    rp("table_1_3", "1.0"),
    rp("table_2_1", "1.0"),
    rp("table_2_2", "1.0"),
    rp("table_2_3", "0.0"),
    rp("table_3_1", "3.0"),
    rp("table_3_2", "0.0"),
    rp("table_3_3", "1.0"),
    rp("period", "4.0"),
];

const REAL_CASES: &[BlockCase] = &[
    case(
        "reals_source_time_table",
        "CDL.Reals.Sources.TimeTable",
        "Sources/TimeTable",
        &[],
        REAL_TABLE_PARAMS,
        REAL_TABLE_OUTPUTS,
    ),
    case(
        "reals_source_time_table_periodic_scaled",
        "CDL.Reals.Sources.TimeTable",
        "Sources/TimeTable/periodic_scaled",
        &[],
        REAL_PERIODIC_PARAMS,
        REAL_TABLE_SINGLE_OUTPUT,
    ),
];

const INTEGER_CASES: &[BlockCase] = &[case(
    "integer_source_time_table",
    "CDL.Integers.Sources.TimeTable",
    "Sources/TimeTable",
    &[],
    INTEGER_TABLE_PARAMS,
    INTEGER_TABLE_OUTPUTS,
)];

const LOGICAL_CASES: &[BlockCase] = &[case(
    "logical_source_time_table",
    "CDL.Logical.Sources.TimeTable",
    "Sources/TimeTable",
    &[],
    LOGICAL_TABLE_PARAMS,
    LOGICAL_TABLE_OUTPUTS,
)];

#[test]
fn source_time_tables_match_exact_oracles() {
    assert_cases_match_exact_oracle(REAL_CASES, "CDL/Reals", "single-block-reals-time-table");
    assert_cases_match_exact_oracle(
        INTEGER_CASES,
        "CDL/Integers",
        "single-block-integers-time-table",
    );
    assert_cases_match_exact_oracle(
        LOGICAL_CASES,
        "CDL/Logical",
        "single-block-logical-time-table",
    );
}

const fn rp(name: &'static str, value: &'static str) -> Param {
    Param {
        name,
        value: ParamValue::Real(value),
    }
}

const fn ip(name: &'static str, value: &'static str) -> Param {
    Param {
        name,
        value: ParamValue::Integer(value),
    }
}
