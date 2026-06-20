use oce_model::ParamTable;

use super::real_param;
use crate::{Block, IntegratorWithReset, RegistryEntry};

pub(super) const ENTRIES: &[RegistryEntry] = &[RegistryEntry {
    class_path: "CDL.Reals.IntegratorWithReset",
    make: make_integrator_with_reset,
}];

fn make_integrator_with_reset(p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegratorWithReset {
        y_start: real_param(p, "y_start", 0.0),
    })
}
