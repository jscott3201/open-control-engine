use oce_model::ParamTable;

use super::{bool_param, real_param};
use crate::{Block, Derivative, LimitSlewRate, MovingAverage, RegistryEntry};

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

fn make_derivative(p: &ParamTable) -> Box<dyn Block> {
    Box::new(Derivative {
        k: real_param(p, "k", 1.0),
        t: real_param(p, "T", 0.1),
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
