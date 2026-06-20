use oce_model::ParamTable;

use super::real_param;
use crate::{Block, RegistryEntry, UnitDelay};

pub(super) const ENTRIES: &[RegistryEntry] = &[RegistryEntry {
    class_path: "CDL.Discrete.UnitDelay",
    make: make_unit_delay,
}];

fn make_unit_delay(p: &ParamTable) -> Box<dyn Block> {
    Box::new(UnitDelay {
        y_start: real_param(p, "y_start", 0.0),
    })
}
