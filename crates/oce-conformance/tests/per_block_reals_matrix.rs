//! Tier-1 exact conformance for CDL.Reals matrix and sort blocks.

mod block_harness;

use block_harness::{
    BlockCase, I, Param, ParamValue, Port, R, assert_cases_match_exact_oracle, case,
};

const MATRIX_GAIN_INPUTS: &[Port] = &[
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
const MATRIX_GAIN_OUTPUTS: &[Port] = &[
    Port {
        name: "y1",
        kind: R,
    },
    Port {
        name: "y2",
        kind: R,
    },
];
const MATRIX_GAIN_PARAMS: &[Param] = &[
    Param {
        name: "nout",
        value: ParamValue::Integer("2"),
    },
    Param {
        name: "nin",
        value: ParamValue::Integer("3"),
    },
    Param {
        name: "K_1_1",
        value: ParamValue::Real("0.5"),
    },
    Param {
        name: "K_1_2",
        value: ParamValue::Real("2.0"),
    },
    Param {
        name: "K_1_3",
        value: ParamValue::Real("-1.0"),
    },
    Param {
        name: "K_2_1",
        value: ParamValue::Real("3.0"),
    },
    Param {
        name: "K_2_2",
        value: ParamValue::Real("0.0"),
    },
    Param {
        name: "K_2_3",
        value: ParamValue::Real("0.25"),
    },
];

const MATRIX_INPUTS: &[Port] = &[
    Port {
        name: "u11",
        kind: R,
    },
    Port {
        name: "u12",
        kind: R,
    },
    Port {
        name: "u13",
        kind: R,
    },
    Port {
        name: "u21",
        kind: R,
    },
    Port {
        name: "u22",
        kind: R,
    },
    Port {
        name: "u23",
        kind: R,
    },
];
const MATRIX_ROW_OUTPUTS: &[Port] = &[
    Port {
        name: "y1",
        kind: R,
    },
    Port {
        name: "y2",
        kind: R,
    },
];
const MATRIX_COLUMN_OUTPUTS: &[Port] = &[
    Port {
        name: "y1",
        kind: R,
    },
    Port {
        name: "y2",
        kind: R,
    },
    Port {
        name: "y3",
        kind: R,
    },
];
const MATRIX_MAX_ROW_PARAMS: &[Param] = &[
    Param {
        name: "nRow",
        value: ParamValue::Integer("2"),
    },
    Param {
        name: "nCol",
        value: ParamValue::Integer("3"),
    },
    Param {
        name: "rowMax",
        value: ParamValue::Boolean("true"),
    },
];
const MATRIX_MAX_COLUMN_PARAMS: &[Param] = &[
    Param {
        name: "nRow",
        value: ParamValue::Integer("2"),
    },
    Param {
        name: "nCol",
        value: ParamValue::Integer("3"),
    },
    Param {
        name: "rowMax",
        value: ParamValue::Boolean("false"),
    },
];
const MATRIX_MIN_ROW_PARAMS: &[Param] = &[
    Param {
        name: "nRow",
        value: ParamValue::Integer("2"),
    },
    Param {
        name: "nCol",
        value: ParamValue::Integer("3"),
    },
    Param {
        name: "rowMin",
        value: ParamValue::Boolean("true"),
    },
];
const MATRIX_MIN_COLUMN_PARAMS: &[Param] = &[
    Param {
        name: "nRow",
        value: ParamValue::Integer("2"),
    },
    Param {
        name: "nCol",
        value: ParamValue::Integer("3"),
    },
    Param {
        name: "rowMin",
        value: ParamValue::Boolean("false"),
    },
];

const SORT_INPUTS: &[Port] = &[
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
const SORT_OUTPUTS: &[Port] = &[
    Port {
        name: "y1",
        kind: R,
    },
    Port {
        name: "y2",
        kind: R,
    },
    Port {
        name: "y3",
        kind: R,
    },
    Port {
        name: "yIdx1",
        kind: I,
    },
    Port {
        name: "yIdx2",
        kind: I,
    },
    Port {
        name: "yIdx3",
        kind: I,
    },
];
const SORT_WIDE_INPUTS: &[Port] = &[
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
    Port {
        name: "u4",
        kind: R,
    },
];
const SORT_WIDE_OUTPUTS: &[Port] = &[
    Port {
        name: "y1",
        kind: R,
    },
    Port {
        name: "y2",
        kind: R,
    },
    Port {
        name: "y3",
        kind: R,
    },
    Port {
        name: "y4",
        kind: R,
    },
    Port {
        name: "yIdx1",
        kind: I,
    },
    Port {
        name: "yIdx2",
        kind: I,
    },
    Port {
        name: "yIdx3",
        kind: I,
    },
    Port {
        name: "yIdx4",
        kind: I,
    },
];
const SORT_ASCENDING_PARAMS: &[Param] = &[
    Param {
        name: "nin",
        value: ParamValue::Integer("3"),
    },
    Param {
        name: "ascending",
        value: ParamValue::Boolean("true"),
    },
];
const SORT_DESCENDING_PARAMS: &[Param] = &[
    Param {
        name: "nin",
        value: ParamValue::Integer("3"),
    },
    Param {
        name: "ascending",
        value: ParamValue::Boolean("false"),
    },
];
const SORT_WIDE_PARAMS: &[Param] = &[
    Param {
        name: "nin",
        value: ParamValue::Integer("4"),
    },
    Param {
        name: "ascending",
        value: ParamValue::Boolean("true"),
    },
];

const CASES: &[BlockCase] = &[
    case(
        "reals_matrix_gain",
        "CDL.Reals.MatrixGain",
        "MatrixGain",
        MATRIX_GAIN_INPUTS,
        MATRIX_GAIN_PARAMS,
        MATRIX_GAIN_OUTPUTS,
    ),
    case(
        "reals_matrix_max_rows",
        "CDL.Reals.MatrixMax",
        "MatrixMax",
        MATRIX_INPUTS,
        MATRIX_MAX_ROW_PARAMS,
        MATRIX_ROW_OUTPUTS,
    ),
    case(
        "reals_matrix_max_columns",
        "CDL.Reals.MatrixMax",
        "MatrixMax/columns",
        MATRIX_INPUTS,
        MATRIX_MAX_COLUMN_PARAMS,
        MATRIX_COLUMN_OUTPUTS,
    ),
    case(
        "reals_matrix_min_rows",
        "CDL.Reals.MatrixMin",
        "MatrixMin",
        MATRIX_INPUTS,
        MATRIX_MIN_ROW_PARAMS,
        MATRIX_ROW_OUTPUTS,
    ),
    case(
        "reals_matrix_min_columns",
        "CDL.Reals.MatrixMin",
        "MatrixMin/columns",
        MATRIX_INPUTS,
        MATRIX_MIN_COLUMN_PARAMS,
        MATRIX_COLUMN_OUTPUTS,
    ),
    case(
        "reals_sort_ascending",
        "CDL.Reals.Sort",
        "Sort",
        SORT_INPUTS,
        SORT_ASCENDING_PARAMS,
        SORT_OUTPUTS,
    ),
    case(
        "reals_sort_descending",
        "CDL.Reals.Sort",
        "Sort/descending",
        SORT_INPUTS,
        SORT_DESCENDING_PARAMS,
        SORT_OUTPUTS,
    ),
    case(
        "reals_sort_wide",
        "CDL.Reals.Sort",
        "Sort/wide",
        SORT_WIDE_INPUTS,
        SORT_WIDE_PARAMS,
        SORT_WIDE_OUTPUTS,
    ),
];

#[test]
fn reals_matrix_and_sort_blocks_match_exact_oracle() {
    assert_cases_match_exact_oracle(CASES, "CDL/Reals", "single-block-reals-matrix");
}
