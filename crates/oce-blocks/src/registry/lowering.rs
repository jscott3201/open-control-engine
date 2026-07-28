//! Registry entries for importer-only pass-through lowering blocks.

use oce_model::ParamTable;

use crate::{Block, BooleanPassThrough, IntegerPassThrough, RealPassThrough, RegistryEntry};

pub(super) const ENTRIES: &[RegistryEntry] = &[
    RegistryEntry {
        class_path: "urn:oce:lowering#PassThrough.Real",
        make: make_real_pass_through,
    },
    RegistryEntry {
        class_path: "urn:oce:lowering#PassThrough.Integer",
        make: make_integer_pass_through,
    },
    RegistryEntry {
        class_path: "urn:oce:lowering#PassThrough.Boolean",
        make: make_boolean_pass_through,
    },
];

fn make_real_pass_through(_params: &ParamTable) -> Box<dyn Block> {
    Box::new(RealPassThrough)
}

fn make_integer_pass_through(_params: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerPassThrough)
}

fn make_boolean_pass_through(_params: &ParamTable) -> Box<dyn Block> {
    Box::new(BooleanPassThrough)
}
