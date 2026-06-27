use oce_model::ParamTable;

use super::{int_param, real_param};
use crate::{
    Block, ParamRule, RegistryEntry, TriggeredMax, TriggeredMovingMean, TriggeredSampler, UnitDelay,
};

pub(super) const TRIGGERED_MOVING_MEAN_PARAM_RULES: &[ParamRule] = &[
    ParamRule::Required { name: "n" },
    ParamRule::IntegerGreaterOrEqual { name: "n", min: 1 },
];

pub(super) const ENTRIES: &[RegistryEntry] = &[
    RegistryEntry {
        class_path: "CDL.Discrete.TriggeredMax",
        make: make_triggered_max,
    },
    RegistryEntry {
        class_path: "CDL.Discrete.TriggeredMovingMean",
        make: make_triggered_moving_mean,
    },
    RegistryEntry {
        class_path: "CDL.Discrete.TriggeredSampler",
        make: make_triggered_sampler,
    },
    RegistryEntry {
        class_path: "CDL.Discrete.UnitDelay",
        make: make_unit_delay,
    },
];

fn make_triggered_max(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(TriggeredMax)
}

fn make_triggered_moving_mean(p: &ParamTable) -> Box<dyn Block> {
    Box::new(TriggeredMovingMean {
        n: int_param(p, "n", 1).max(1) as usize,
    })
}

fn make_triggered_sampler(p: &ParamTable) -> Box<dyn Block> {
    Box::new(TriggeredSampler {
        y_start: real_param(p, "y_start", 0.0),
    })
}

fn make_unit_delay(p: &ParamTable) -> Box<dyn Block> {
    Box::new(UnitDelay {
        y_start: real_param(p, "y_start", 0.0),
    })
}
