use oce_model::ParamTable;

use super::{bool_param, real_param};
use crate::{Block, Derivative, LimitSlewRate, MovingAverage, ParamRule, RegistryEntry};

pub(super) const ENTRIES: &[RegistryEntry] = &[
    RegistryEntry {
        class_path: "CDL.Reals.Derivative",
        make: make_derivative,
    },
    RegistryEntry {
        class_path: "CDL.Reals.LimitSlewRate",
        make: make_limit_slew_rate,
    },
    RegistryEntry {
        class_path: "CDL.Reals.MovingAverage",
        make: make_moving_average,
    },
];

/// Upstream `LimitSlewRate.mo` (pin `a131864`) declares `raisingSlewRate` with NO default
/// (`fallingSlewRate` and `Td` default derived from it). Exact-bound alignment of the range
/// rules to the upstream `min=` annotations is tracked separately.
pub(super) const LIMIT_SLEW_RATE_PARAM_RULES: &[ParamRule] = &[
    ParamRule::Required {
        name: "raisingSlewRate",
    },
    ParamRule::RealGreaterThan {
        name: "Td",
        min: 0.0,
    },
];

/// Upstream `MovingAverage.mo` (pin `a131864`) declares `delta(min=1E-5)` with NO default.
pub(super) const MOVING_AVERAGE_PARAM_RULES: &[ParamRule] = &[
    ParamRule::Required { name: "delta" },
    ParamRule::RealGreaterThan {
        name: "delta",
        min: 0.0,
    },
];

fn make_derivative(p: &ParamTable) -> Box<dyn Block> {
    // `k` and `T` are RealInput connectors upstream (declaration order k, T, u), not parameters;
    // `y_start` is the block's only parameter.
    Box::new(Derivative {
        y_start: real_param(p, "y_start", 0.0),
    })
}

fn make_limit_slew_rate(p: &ParamTable) -> Box<dyn Block> {
    let raising_slew_rate = real_param(p, "raisingSlewRate", 1.0);
    Box::new(LimitSlewRate {
        raising_slew_rate,
        falling_slew_rate: real_param(p, "fallingSlewRate", -raising_slew_rate),
        td: real_param(p, "Td", raising_slew_rate * 10.0),
        enable: bool_param(p, "enable", true),
    })
}

fn make_moving_average(p: &ParamTable) -> Box<dyn Block> {
    Box::new(MovingAverage {
        delta: real_param(p, "delta", 1.0),
    })
}
