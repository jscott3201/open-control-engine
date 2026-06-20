use oce_model::ParamTable;

use super::{bool_param, real_param};
use crate::{
    And, Block, Edge, LogicalConstant, LogicalSwitch, Nand, Nor, Not, Or, Pre, RegistryEntry,
    SampleTrigger, Xor,
};

pub(super) const ENTRIES: &[RegistryEntry] = &[
    RegistryEntry {
        class_path: "CDL.Logical.Sources.Constant",
        make: make_logical_constant,
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

fn make_logical_constant(p: &ParamTable) -> Box<dyn Block> {
    Box::new(LogicalConstant {
        k: bool_param(p, "k", false),
    })
}

fn make_and(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(And)
}

fn make_or(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Or)
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
    // `period` defaults to 1.0 only for a param-less construction; a resolved model carries the
    // author's value (CDL requires `period > 0`; the oce-validate rule enforcing it is pending —
    // SampleTrigger degrades safely until then). `shift` defaults to 0.0.
    Box::new(SampleTrigger {
        period: real_param(p, "period", 1.0),
        shift: real_param(p, "shift", 0.0),
    })
}
