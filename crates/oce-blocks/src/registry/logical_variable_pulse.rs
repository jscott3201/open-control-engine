//! Registry entry for `CDL.Logical.VariablePulse`.

use oce_model::ParamTable;

use super::real_param;
use crate::{
    Block, LogicalVariablePulse, ParamRule, RegistryEntry,
    logical_variable_pulse::{MAX_DELTA_U, MIN_DELTA_U, MIN_VARIABLE_PULSE_TIME},
};

pub(super) const ENTRIES: &[RegistryEntry] = &[RegistryEntry {
    class_path: "CDL.Logical.VariablePulse",
    make: make_variable_pulse,
}];

pub(super) const VARIABLE_PULSE_PARAM_RULES: &[ParamRule] = &[
    ParamRule::Required { name: "period" },
    ParamRule::RealGreaterOrEqual {
        name: "deltaU",
        min: MIN_DELTA_U,
    },
    ParamRule::RealLessOrEqualConstant {
        name: "deltaU",
        max: MAX_DELTA_U,
    },
    ParamRule::RealGreaterOrEqual {
        name: "minTruFalHol",
        min: MIN_VARIABLE_PULSE_TIME,
    },
    ParamRule::RealGreaterOrEqualScaledWarning {
        left: "period",
        right: "minTruFalHol",
        factor: 2.0,
    },
];

fn make_variable_pulse(p: &ParamTable) -> Box<dyn Block> {
    let period = real_param(p, "period", 1.0);
    Box::new(LogicalVariablePulse {
        period,
        delta_u: real_param(p, "deltaU", 0.01),
        min_true_false_hold: real_param(p, "minTruFalHol", 0.01 * period),
    })
}
