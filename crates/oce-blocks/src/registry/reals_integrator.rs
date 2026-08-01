use oce_model::ParamTable;

use super::real_param;
use crate::{Block, IntegratorWithReset, ParamDefault, RegistryEntry};

const K_DEFAULT: f64 = 1.0;
const Y_START_DEFAULT: f64 = 0.0;

pub(super) const INTEGRATOR_WITH_RESET_PARAM_DEFAULTS: &[ParamDefault] = &[
    param_default_real!("k", K_DEFAULT),
    param_default_real!("y_start", Y_START_DEFAULT),
];

pub(super) const ENTRIES: &[RegistryEntry] = &[RegistryEntry {
    class_path: "CDL.Reals.IntegratorWithReset",
    make: make_integrator_with_reset,
}];

fn make_integrator_with_reset(p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegratorWithReset {
        k: real_param(p, "k", K_DEFAULT),
        y_start: real_param(p, "y_start", Y_START_DEFAULT),
    })
}
