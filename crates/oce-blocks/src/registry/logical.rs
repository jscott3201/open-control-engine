use oce_model::ParamTable;

use super::{bool_param, int_param, real_param};
use crate::{
    And, Block, Edge, LogicalConstant, LogicalPulse, LogicalSwitch, MAX_RESOLVED_PORT_WIDTH,
    MultiAnd, MultiOr, Nand, Nor, Not, Or, ParamRule, Pre, RegistryEntry, SampleTrigger, Xor,
};

pub(super) const ENTRIES: &[RegistryEntry] = &[
    RegistryEntry {
        class_path: "CDL.Logical.Sources.Constant",
        make: make_logical_constant,
    },
    RegistryEntry {
        class_path: "CDL.Logical.Sources.Pulse",
        make: make_logical_pulse,
    },
    RegistryEntry {
        class_path: "CDL.Logical.And",
        make: make_and,
    },
    RegistryEntry {
        class_path: "CDL.Logical.Or",
        make: make_or,
    },
    RegistryEntry {
        class_path: "CDL.Logical.MultiAnd",
        make: make_multi_and,
    },
    RegistryEntry {
        class_path: "CDL.Logical.MultiOr",
        make: make_multi_or,
    },
    RegistryEntry {
        class_path: "CDL.Logical.Not",
        make: make_not,
    },
    RegistryEntry {
        class_path: "CDL.Logical.Nand",
        make: make_nand,
    },
    RegistryEntry {
        class_path: "CDL.Logical.Nor",
        make: make_nor,
    },
    RegistryEntry {
        class_path: "CDL.Logical.Xor",
        make: make_xor,
    },
    RegistryEntry {
        class_path: "CDL.Logical.Switch",
        make: make_logical_switch,
    },
    RegistryEntry {
        class_path: "CDL.Logical.Pre",
        make: make_pre,
    },
    RegistryEntry {
        class_path: "CDL.Logical.Edge",
        make: make_edge,
    },
    RegistryEntry {
        class_path: "CDL.Logical.Sources.SampleTrigger",
        make: make_sample_trigger,
    },
];

pub(super) const SAMPLE_TRIGGER_PARAM_RULES: &[ParamRule] = &[
    ParamRule::Required { name: "period" },
    ParamRule::RealGreaterThan {
        name: "period",
        min: 0.0,
    },
];

pub(super) const MULTI_LOGICAL_PARAM_RULES: &[ParamRule] = &[
    ParamRule::Structural { name: "nin" },
    ParamRule::IntegerGreaterOrEqual {
        name: "nin",
        min: 0,
    },
    ParamRule::IntegerLessOrEqualConstant {
        name: "nin",
        max: MAX_RESOLVED_PORT_WIDTH as i64,
    },
];

fn make_logical_constant(p: &ParamTable) -> Box<dyn Block> {
    Box::new(LogicalConstant {
        k: bool_param(p, "k", false),
    })
}

fn make_logical_pulse(p: &ParamTable) -> Box<dyn Block> {
    Box::new(LogicalPulse {
        width: real_param(p, "width", 0.5),
        period: real_param(p, "period", 1.0),
        shift: real_param(p, "shift", 0.0),
    })
}

fn make_and(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(And)
}

fn make_or(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Or)
}

fn make_multi_and(p: &ParamTable) -> Box<dyn Block> {
    Box::new(MultiAnd::new(bounded_nin(p)))
}

fn make_multi_or(p: &ParamTable) -> Box<dyn Block> {
    Box::new(MultiOr::new(bounded_nin(p)))
}

fn make_not(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Not)
}

fn make_nand(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Nand)
}

fn make_nor(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Nor)
}

fn make_xor(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Xor)
}

fn make_logical_switch(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(LogicalSwitch)
}

fn make_pre(p: &ParamTable) -> Box<dyn Block> {
    Box::new(Pre {
        y_start: bool_param(p, "pre_u_start", false),
    })
}

fn make_edge(p: &ParamTable) -> Box<dyn Block> {
    Box::new(Edge {
        pre_u_start: bool_param(p, "pre_u_start", false),
    })
}

fn make_sample_trigger(p: &ParamTable) -> Box<dyn Block> {
    // `period` defaults to 1.0 only for a param-less construction; resolved models must carry the
    // author's required `period > 0` value, enforced by SAMPLE_TRIGGER_PARAM_RULES. `shift`
    // defaults to 0.0.
    Box::new(SampleTrigger {
        period: real_param(p, "period", 1.0),
        shift: real_param(p, "shift", 0.0),
    })
}

fn bounded_nin(p: &ParamTable) -> usize {
    usize::try_from(int_param(p, "nin", 0).max(0))
        .unwrap_or(MAX_RESOLVED_PORT_WIDTH)
        .min(MAX_RESOLVED_PORT_WIDTH)
}
