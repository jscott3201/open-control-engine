use oce_model::ParamTable;

use super::{int_param, real_param};
use crate::discrete_sampled::{MIN_SAMPLE_PERIOD, valid_sample_period};
use crate::{
    Block, FirstOrderHold, ParamRule, RegistryEntry, Sampler, TriggeredMax, TriggeredMovingMean,
    TriggeredSampler, UnitDelay, ZeroOrderHold,
};

pub(super) const SAMPLED_PARAM_RULES: &[ParamRule] = &[
    ParamRule::Required {
        name: "samplePeriod",
    },
    ParamRule::RealGreaterOrEqual {
        name: "samplePeriod",
        min: MIN_SAMPLE_PERIOD,
    },
];

pub(super) const TRIGGERED_MOVING_MEAN_PARAM_RULES: &[ParamRule] = &[
    ParamRule::Required { name: "n" },
    ParamRule::IntegerGreaterOrEqual { name: "n", min: 1 },
];

pub(super) const ENTRIES: &[RegistryEntry] = &[
    RegistryEntry {
        class_path: "CDL.Discrete.FirstOrderHold",
        make: make_first_order_hold,
    },
    RegistryEntry {
        class_path: "CDL.Discrete.Sampler",
        make: make_sampler,
    },
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
    RegistryEntry {
        class_path: "CDL.Discrete.ZeroOrderHold",
        make: make_zero_order_hold,
    },
];

fn sample_period_param(p: &ParamTable) -> f64 {
    valid_sample_period(real_param(p, "samplePeriod", 1.0))
}

fn make_first_order_hold(p: &ParamTable) -> Box<dyn Block> {
    Box::new(FirstOrderHold {
        sample_period: sample_period_param(p),
    })
}

fn make_sampler(p: &ParamTable) -> Box<dyn Block> {
    Box::new(Sampler {
        sample_period: sample_period_param(p),
    })
}

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
        sample_period: sample_period_param(p),
    })
}

fn make_zero_order_hold(p: &ParamTable) -> Box<dyn Block> {
    Box::new(ZeroOrderHold {
        sample_period: sample_period_param(p),
    })
}
