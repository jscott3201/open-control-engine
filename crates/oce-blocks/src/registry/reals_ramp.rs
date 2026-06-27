use oce_model::ParamTable;

use super::real_param;
use crate::{Block, ParamRule, Ramp, RegistryEntry};

const CDL_EPS: f64 = 1e-15;
const CDL_SMALL: f64 = 1e-37;

pub(super) const ENTRIES: &[RegistryEntry] = &[RegistryEntry {
    class_path: "CDL.Reals.Ramp",
    make: make_ramp,
}];

pub(super) const RAMP_PARAM_RULES: &[ParamRule] = &[
    ParamRule::Required {
        name: "raisingSlewRate",
    },
    ParamRule::RealGreaterOrEqual {
        name: "raisingSlewRate",
        min: CDL_SMALL,
    },
    ParamRule::RealLessOrEqualConstant {
        name: "fallingSlewRate",
        max: -CDL_SMALL,
    },
    ParamRule::RealGreaterOrEqual {
        name: "Td",
        min: CDL_EPS,
    },
];

fn make_ramp(p: &ParamTable) -> Box<dyn Block> {
    let raising_slew_rate = real_param(p, "raisingSlewRate", 1.0);
    Box::new(Ramp {
        raising_slew_rate,
        falling_slew_rate: real_param(p, "fallingSlewRate", -raising_slew_rate),
        td: real_param(p, "Td", raising_slew_rate * 0.001),
    })
}
