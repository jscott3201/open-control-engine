//! Tier-1 exact conformance for CDL.Routing Boolean and Integer blocks.

mod block_harness;

use block_harness::{
    B, BlockCase, I, Param, ParamValue, Port, assert_cases_match_exact_oracle, case,
};

const BOOLEAN_U1_U2_U3_U4_U5: &[Port] = &[
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
    Port {
        name: "u4",
        kind: B,
    },
    Port {
        name: "u5",
        kind: B,
    },
];

const BOOLEAN_INDEX_U1_U2_U3: &[Port] = &[
    Port {
        name: "index",
        kind: I,
    },
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

const BOOLEAN_U: &[Port] = &[Port { name: "u", kind: B }];
const BOOLEAN_U1_U2: &[Port] = &[
    Port {
        name: "u1",
        kind: B,
    },
    Port {
        name: "u2",
        kind: B,
    },
];
const BOOLEAN_U1_U2_U3_U4: &[Port] = &[
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
    Port {
        name: "u4",
        kind: B,
    },
];

const BOOLEAN_Y: &[Port] = &[Port { name: "y", kind: B }];
const BOOLEAN_Y1_Y2: &[Port] = &[
    Port {
        name: "y1",
        kind: B,
    },
    Port {
        name: "y2",
        kind: B,
    },
];
const BOOLEAN_Y1_Y2_Y3: &[Port] = &[
    Port {
        name: "y1",
        kind: B,
    },
    Port {
        name: "y2",
        kind: B,
    },
    Port {
        name: "y3",
        kind: B,
    },
];
const BOOLEAN_Y1_Y2_Y3_Y4: &[Port] = &[
    Port {
        name: "y1",
        kind: B,
    },
    Port {
        name: "y2",
        kind: B,
    },
    Port {
        name: "y3",
        kind: B,
    },
    Port {
        name: "y4",
        kind: B,
    },
];
const BOOLEAN_Y1_Y2_Y3_Y4_Y5_Y6: &[Port] = &[
    Port {
        name: "y1",
        kind: B,
    },
    Port {
        name: "y2",
        kind: B,
    },
    Port {
        name: "y3",
        kind: B,
    },
    Port {
        name: "y4",
        kind: B,
    },
    Port {
        name: "y5",
        kind: B,
    },
    Port {
        name: "y6",
        kind: B,
    },
];

const INTEGER_U1_U2_U3_U4_U5: &[Port] = &[
    Port {
        name: "u1",
        kind: I,
    },
    Port {
        name: "u2",
        kind: I,
    },
    Port {
        name: "u3",
        kind: I,
    },
    Port {
        name: "u4",
        kind: I,
    },
    Port {
        name: "u5",
        kind: I,
    },
];

const INTEGER_INDEX_U1_U2_U3: &[Port] = &[
    Port {
        name: "index",
        kind: I,
    },
    Port {
        name: "u1",
        kind: I,
    },
    Port {
        name: "u2",
        kind: I,
    },
    Port {
        name: "u3",
        kind: I,
    },
];

const INTEGER_U: &[Port] = &[Port { name: "u", kind: I }];
const INTEGER_U1_U2: &[Port] = &[
    Port {
        name: "u1",
        kind: I,
    },
    Port {
        name: "u2",
        kind: I,
    },
];
const INTEGER_U1_U2_U3_U4: &[Port] = &[
    Port {
        name: "u1",
        kind: I,
    },
    Port {
        name: "u2",
        kind: I,
    },
    Port {
        name: "u3",
        kind: I,
    },
    Port {
        name: "u4",
        kind: I,
    },
];

const INTEGER_Y: &[Port] = &[Port { name: "y", kind: I }];
const INTEGER_Y1_Y2: &[Port] = &[
    Port {
        name: "y1",
        kind: I,
    },
    Port {
        name: "y2",
        kind: I,
    },
];
const INTEGER_Y1_Y2_Y3: &[Port] = &[
    Port {
        name: "y1",
        kind: I,
    },
    Port {
        name: "y2",
        kind: I,
    },
    Port {
        name: "y3",
        kind: I,
    },
];
const INTEGER_Y1_Y2_Y3_Y4: &[Port] = &[
    Port {
        name: "y1",
        kind: I,
    },
    Port {
        name: "y2",
        kind: I,
    },
    Port {
        name: "y3",
        kind: I,
    },
    Port {
        name: "y4",
        kind: I,
    },
];
const INTEGER_Y1_Y2_Y3_Y4_Y5_Y6: &[Port] = &[
    Port {
        name: "y1",
        kind: I,
    },
    Port {
        name: "y2",
        kind: I,
    },
    Port {
        name: "y3",
        kind: I,
    },
    Port {
        name: "y4",
        kind: I,
    },
    Port {
        name: "y5",
        kind: I,
    },
    Port {
        name: "y6",
        kind: I,
    },
];

const EXTRACT_SIGNAL_PARAMS: &[Param] = &[
    Param {
        name: "nin",
        value: ParamValue::Integer("5"),
    },
    Param {
        name: "nout",
        value: ParamValue::Integer("4"),
    },
    Param {
        name: "extract_1",
        value: ParamValue::Integer("5"),
    },
    Param {
        name: "extract_2",
        value: ParamValue::Integer("2"),
    },
    Param {
        name: "extract_3",
        value: ParamValue::Integer("2"),
    },
    Param {
        name: "extract_4",
        value: ParamValue::Integer("1"),
    },
];

const NIN_3: &[Param] = &[Param {
    name: "nin",
    value: ParamValue::Integer("3"),
}];

const NOUT_3: &[Param] = &[Param {
    name: "nout",
    value: ParamValue::Integer("3"),
}];

const VECTOR_FILTER_PARAMS: &[Param] = &[
    Param {
        name: "nin",
        value: ParamValue::Integer("4"),
    },
    Param {
        name: "nout",
        value: ParamValue::Integer("2"),
    },
    Param {
        name: "msk_1",
        value: ParamValue::Boolean("false"),
    },
    Param {
        name: "msk_2",
        value: ParamValue::Boolean("true"),
    },
    Param {
        name: "msk_3",
        value: ParamValue::Boolean("true"),
    },
    Param {
        name: "msk_4",
        value: ParamValue::Boolean("false"),
    },
];

const VECTOR_REPLICATOR_PARAMS: &[Param] = &[
    Param {
        name: "nin",
        value: ParamValue::Integer("2"),
    },
    Param {
        name: "nout",
        value: ParamValue::Integer("3"),
    },
];

const CASES: &[BlockCase] = &[
    case(
        "routing_boolean_extract_signal",
        "CDL.Routing.BooleanExtractSignal",
        "BooleanExtractSignal",
        BOOLEAN_U1_U2_U3_U4_U5,
        EXTRACT_SIGNAL_PARAMS,
        BOOLEAN_Y1_Y2_Y3_Y4,
    ),
    case(
        "routing_boolean_extractor",
        "CDL.Routing.BooleanExtractor",
        "BooleanExtractor",
        BOOLEAN_INDEX_U1_U2_U3,
        NIN_3,
        BOOLEAN_Y,
    ),
    case(
        "routing_boolean_scalar_replicator",
        "CDL.Routing.BooleanScalarReplicator",
        "BooleanScalarReplicator",
        BOOLEAN_U,
        NOUT_3,
        BOOLEAN_Y1_Y2_Y3,
    ),
    case(
        "routing_boolean_vector_filter",
        "CDL.Routing.BooleanVectorFilter",
        "BooleanVectorFilter",
        BOOLEAN_U1_U2_U3_U4,
        VECTOR_FILTER_PARAMS,
        BOOLEAN_Y1_Y2,
    ),
    case(
        "routing_boolean_vector_replicator",
        "CDL.Routing.BooleanVectorReplicator",
        "BooleanVectorReplicator",
        BOOLEAN_U1_U2,
        VECTOR_REPLICATOR_PARAMS,
        BOOLEAN_Y1_Y2_Y3_Y4_Y5_Y6,
    ),
    case(
        "routing_integer_extract_signal",
        "CDL.Routing.IntegerExtractSignal",
        "IntegerExtractSignal",
        INTEGER_U1_U2_U3_U4_U5,
        EXTRACT_SIGNAL_PARAMS,
        INTEGER_Y1_Y2_Y3_Y4,
    ),
    case(
        "routing_integer_extractor",
        "CDL.Routing.IntegerExtractor",
        "IntegerExtractor",
        INTEGER_INDEX_U1_U2_U3,
        NIN_3,
        INTEGER_Y,
    ),
    case(
        "routing_integer_scalar_replicator",
        "CDL.Routing.IntegerScalarReplicator",
        "IntegerScalarReplicator",
        INTEGER_U,
        NOUT_3,
        INTEGER_Y1_Y2_Y3,
    ),
    case(
        "routing_integer_vector_filter",
        "CDL.Routing.IntegerVectorFilter",
        "IntegerVectorFilter",
        INTEGER_U1_U2_U3_U4,
        VECTOR_FILTER_PARAMS,
        INTEGER_Y1_Y2,
    ),
    case(
        "routing_integer_vector_replicator",
        "CDL.Routing.IntegerVectorReplicator",
        "IntegerVectorReplicator",
        INTEGER_U1_U2,
        VECTOR_REPLICATOR_PARAMS,
        INTEGER_Y1_Y2_Y3_Y4_Y5_Y6,
    ),
];

#[test]
fn routing_typed_blocks_match_exact_oracle() {
    assert_cases_match_exact_oracle(CASES, "CDL/Routing", "single-block-routing-typed");
}
