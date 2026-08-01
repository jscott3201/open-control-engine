use oce_model::ParamTable;

use super::{bool_param, int_param, real_param};
use crate::source_timetable::{MIN_TIMETABLE_PERIOD, TIMETABLE_TIME_SCALE_DEFAULT};
use crate::{
    And, Block, Edge, LogicalConstant, LogicalPulse, LogicalSwitch, LogicalTimeTable,
    MAX_RESOLVED_PORT_WIDTH, MultiAnd, MultiOr, Nand, Nor, Not, Or, ParamDefault, ParamRule, Pre,
    RegistryEntry, SampleTrigger, TimeTableValues, Xor,
};

const LOGICAL_CONSTANT_K_FALLBACK: bool = false;
const PULSE_WIDTH_DEFAULT: f64 = 0.5;
const PULSE_PERIOD_FALLBACK: f64 = 1.0;
const PULSE_SHIFT_DEFAULT: f64 = 0.0;
const MULTI_LOGICAL_NIN_DEFAULT: i64 = 0;
const PRE_U_START_DEFAULT: bool = false;
const SAMPLE_TRIGGER_PERIOD_FALLBACK: f64 = 1.0;
const SAMPLE_TRIGGER_SHIFT_DEFAULT: f64 = 0.0;

pub(super) const LOGICAL_CONSTANT_PARAM_DEFAULTS: &[ParamDefault] = &[param_default_required!("k")];
pub(super) const LOGICAL_PULSE_PARAM_DEFAULTS: &[ParamDefault] = &[
    param_default_real!("width", PULSE_WIDTH_DEFAULT),
    param_default_required!("period"),
    param_default_real!("shift", PULSE_SHIFT_DEFAULT),
];
pub(super) const LOGICAL_TIME_TABLE_PARAM_DEFAULTS: &[ParamDefault] = &[
    param_default_real!("timeScale", TIMETABLE_TIME_SCALE_DEFAULT),
    param_default_required!("period"),
];
pub(super) const MULTI_LOGICAL_PARAM_DEFAULTS: &[ParamDefault] =
    &[param_default_integer!("nin", MULTI_LOGICAL_NIN_DEFAULT)];
pub(super) const PRE_PARAM_DEFAULTS: &[ParamDefault] =
    &[param_default_boolean!("pre_u_start", PRE_U_START_DEFAULT)];
pub(super) const SAMPLE_TRIGGER_PARAM_DEFAULTS: &[ParamDefault] = &[
    param_default_required!("period"),
    param_default_real!("shift", SAMPLE_TRIGGER_SHIFT_DEFAULT),
];

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
        class_path: "CDL.Logical.Sources.TimeTable",
        make: make_logical_time_table,
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
    ParamRule::Required {
        name: "period",
        kind: oce_model::ValueType::Real,
    },
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

pub(super) const TIME_TABLE_PARAM_RULES: &[ParamRule] = &[
    ParamRule::Required {
        name: "period",
        kind: oce_model::ValueType::Real,
    },
    ParamRule::TimeTableMatrix {
        base: "table",
        values: TimeTableValues::Boolean,
        time_scale: "timeScale",
        period: Some("period"),
        extrapolation: None,
    },
    ParamRule::RealFiniteGreaterThan {
        name: "timeScale",
        min: 0.0,
    },
    ParamRule::RealFiniteGreaterThan {
        name: "period",
        min: 0.0,
    },
    ParamRule::RealGreaterOrEqual {
        name: "period",
        min: MIN_TIMETABLE_PERIOD,
    },
];

/// Upstream `Logical/Sources/Constant.mo` (pin `a131864`) declares `parameter Boolean k` with NO
/// default value ("Constant output value"), mirroring the defaultless `k` on the `Reals`/`Integers`
/// `Sources.Constant` siblings. Omitting it previously fell through to a silent `k=false` engine
/// default — a silently-defeated enable/interlock constant in a safety-critical model — so `k` is
/// required at load time.
pub(super) const LOGICAL_CONSTANT_PARAM_RULES: &[ParamRule] = &[ParamRule::Required {
    name: "k",
    kind: oce_model::ValueType::Boolean,
}];

fn make_logical_constant(p: &ParamTable) -> Box<dyn Block> {
    Box::new(LogicalConstant {
        k: bool_param(p, "k", LOGICAL_CONSTANT_K_FALLBACK),
    })
}

fn make_logical_pulse(p: &ParamTable) -> Box<dyn Block> {
    Box::new(LogicalPulse {
        width: real_param(p, "width", PULSE_WIDTH_DEFAULT),
        period: real_param(p, "period", PULSE_PERIOD_FALLBACK),
        shift: real_param(p, "shift", PULSE_SHIFT_DEFAULT),
    })
}

fn make_logical_time_table(p: &ParamTable) -> Box<dyn Block> {
    Box::new(LogicalTimeTable::from_params(p))
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
        y_start: bool_param(p, "pre_u_start", PRE_U_START_DEFAULT),
    })
}

fn make_edge(p: &ParamTable) -> Box<dyn Block> {
    Box::new(Edge {
        pre_u_start: bool_param(p, "pre_u_start", PRE_U_START_DEFAULT),
    })
}

fn make_sample_trigger(p: &ParamTable) -> Box<dyn Block> {
    // `period` defaults to 1.0 only for a param-less construction; resolved models must carry the
    // author's required `period > 0` value, enforced by SAMPLE_TRIGGER_PARAM_RULES. `shift`
    // defaults to 0.0.
    Box::new(SampleTrigger {
        period: real_param(p, "period", SAMPLE_TRIGGER_PERIOD_FALLBACK),
        shift: real_param(p, "shift", SAMPLE_TRIGGER_SHIFT_DEFAULT),
    })
}

fn bounded_nin(p: &ParamTable) -> usize {
    usize::try_from(int_param(p, "nin", MULTI_LOGICAL_NIN_DEFAULT).max(0))
        .unwrap_or(MAX_RESOLVED_PORT_WIDTH)
        .min(MAX_RESOLVED_PORT_WIDTH)
}
