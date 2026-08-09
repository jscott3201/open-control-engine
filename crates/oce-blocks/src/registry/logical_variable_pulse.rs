//! Registry entry for `CDL.Logical.VariablePulse`.

use oce_model::ParamTable;

use super::real_param;
use crate::{
    Block, LogicalVariablePulse, ParamDefault, ParamRule, RegistryEntry,
    logical_variable_pulse::{MAX_DELTA_U, MIN_DELTA_U, MIN_VARIABLE_PULSE_TIME},
};

const PERIOD_FALLBACK: f64 = 1.0;
const DELTA_U_DEFAULT: f64 = 0.01;

pub(super) const VARIABLE_PULSE_PARAM_DEFAULTS: &[ParamDefault] = &[
    param_default_required!("period"),
    param_default_real!("deltaU", DELTA_U_DEFAULT),
    param_default_derived!("minTruFalHol", "0.01 * period"),
];

pub(super) const ENTRIES: &[RegistryEntry] = &[RegistryEntry {
    class_path: "CDL.Logical.VariablePulse",
    make: make_variable_pulse,
}];

pub(super) const VARIABLE_PULSE_PARAM_RULES: &[ParamRule] = &[
    ParamRule::Required {
        name: "period",
        kind: oce_model::ValueType::Real,
    },
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
    let period = real_param(p, "period", PERIOD_FALLBACK);
    Box::new(LogicalVariablePulse {
        period,
        delta_u: real_param(p, "deltaU", DELTA_U_DEFAULT),
        min_true_false_hold: real_param(p, "minTruFalHol", 0.01 * period),
    })
}
