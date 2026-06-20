use oce_model::ParamTable;

use super::{int_param, real_param};
use crate::{Block, BooleanToInteger, BooleanToReal, IntegerToReal, RealToInteger, RegistryEntry};

pub(super) const ENTRIES: &[RegistryEntry] = &[
    RegistryEntry {
        class_path: "CDL.Conversions.BooleanToInteger",
        make: make_boolean_to_integer,
    },
    RegistryEntry {
        class_path: "CDL.Conversions.BooleanToReal",
        make: make_boolean_to_real,
    },
    RegistryEntry {
        class_path: "CDL.Conversions.IntegerToReal",
        make: make_integer_to_real,
    },
    RegistryEntry {
        class_path: "CDL.Conversions.RealToInteger",
        make: make_real_to_integer,
    },
];

fn make_boolean_to_integer(p: &ParamTable) -> Box<dyn Block> {
    Box::new(BooleanToInteger {
        integer_true: int_param(p, "integerTrue", 1),
        integer_false: int_param(p, "integerFalse", 0),
    })
}

fn make_boolean_to_real(p: &ParamTable) -> Box<dyn Block> {
    Box::new(BooleanToReal {
        real_true: real_param(p, "realTrue", 1.0),
        real_false: real_param(p, "realFalse", 0.0),
    })
}

fn make_integer_to_real(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerToReal)
}

fn make_real_to_integer(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(RealToInteger)
}
