//! Tier-1 exact conformance for CDL.Routing Real-family blocks.

mod block_harness;

use block_harness::{
    BlockCase, I, Param, ParamValue, Port, R, assert_cases_match_exact_oracle, case,
};

const U1_U2_U3_U4_U5: &[Port] = &[
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
    Port {
        name: "u5",
        kind: R,
    },
];

const INDEX_U1_U2_U3: &[Port] = &[
    Port {
        name: "index",
        kind: I,
    },
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
const U1_U2_U3_U4: &[Port] = &[
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

const Y: &[Port] = &[Port { name: "y", kind: R }];
const Y1_Y2: &[Port] = &[
    Port {
        name: "y1",
        kind: R,
    },
    Port {
        name: "y2",
        kind: R,
    },
];
const Y1_Y2_Y3: &[Port] = &[
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
const Y1_Y2_Y3_Y4: &[Port] = &[
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
];
const Y1_Y2_Y3_Y4_Y5_Y6: &[Port] = &[
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
        name: "y5",
        kind: R,
    },
    Port {
        name: "y6",
        kind: R,
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
        "routing_real_extract_signal",
        "CDL.Routing.RealExtractSignal",
        "RealExtractSignal",
        U1_U2_U3_U4_U5,
        EXTRACT_SIGNAL_PARAMS,
        Y1_Y2_Y3_Y4,
    ),
    case(
        "routing_real_extractor",
        "CDL.Routing.RealExtractor",
        "RealExtractor",
        INDEX_U1_U2_U3,
        NIN_3,
        Y,
    ),
    case(
        "routing_real_scalar_replicator",
        "CDL.Routing.RealScalarReplicator",
        "RealScalarReplicator",
        U,
        NOUT_3,
        Y1_Y2_Y3,
    ),
    case(
        "routing_real_vector_filter",
        "CDL.Routing.RealVectorFilter",
        "RealVectorFilter",
        U1_U2_U3_U4,
        VECTOR_FILTER_PARAMS,
        Y1_Y2,
    ),
    case(
        "routing_real_vector_replicator",
        "CDL.Routing.RealVectorReplicator",
        "RealVectorReplicator",
        U1_U2,
        VECTOR_REPLICATOR_PARAMS,
        Y1_Y2_Y3_Y4_Y5_Y6,
    ),
];

#[test]
fn routing_real_blocks_match_exact_oracle() {
    assert_cases_match_exact_oracle(CASES, "CDL/Routing", "single-block-routing-reals");
}
