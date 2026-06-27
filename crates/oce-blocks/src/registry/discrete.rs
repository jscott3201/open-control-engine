use oce_model::ParamTable;

use super::real_param;
use crate::{Block, RegistryEntry, TriggeredSampler, UnitDelay};

pub(super) const ENTRIES: &[RegistryEntry] = &[
    RegistryEntry {
        class_path: "CDL.Discrete.TriggeredSampler",
        make: make_triggered_sampler,
    },
    RegistryEntry {
        class_path: "CDL.Discrete.UnitDelay",
        make: make_unit_delay,
    },
];

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
