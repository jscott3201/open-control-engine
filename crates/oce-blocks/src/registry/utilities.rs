use oce_model::ParamTable;

use super::string_param;
use crate::{Assert, Block, RegistryEntry};

pub(super) const ENTRIES: &[RegistryEntry] = &[RegistryEntry {
    class_path: "CDL.Utilities.Assert",
    make: make_assert,
}];

fn make_assert(p: &ParamTable) -> Box<dyn Block> {
    Box::new(Assert {
        message: string_param(p, "message", ""),
    })
}
