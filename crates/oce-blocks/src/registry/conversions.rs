use oce_model::ParamTable;

use super::{int_param, real_param};
use crate::{
    Block, BooleanToInteger, BooleanToReal, IntegerToReal, ParamDefault, RealToInteger,
    RegistryEntry,
};

const INTEGER_TRUE_DEFAULT: i64 = 1;
const INTEGER_FALSE_DEFAULT: i64 = 0;
const REAL_TRUE_DEFAULT: f64 = 1.0;
const REAL_FALSE_DEFAULT: f64 = 0.0;

pub(super) const BOOLEAN_TO_INTEGER_PARAM_DEFAULTS: &[ParamDefault] = &[
    param_default_integer!("integerTrue", INTEGER_TRUE_DEFAULT),
    param_default_integer!("integerFalse", INTEGER_FALSE_DEFAULT),
];
pub(super) const BOOLEAN_TO_REAL_PARAM_DEFAULTS: &[ParamDefault] = &[
    param_default_real!("realTrue", REAL_TRUE_DEFAULT),
    param_default_real!("realFalse", REAL_FALSE_DEFAULT),
];

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
        integer_true: int_param(p, "integerTrue", INTEGER_TRUE_DEFAULT),
        integer_false: int_param(p, "integerFalse", INTEGER_FALSE_DEFAULT),
    })
}

fn make_boolean_to_real(p: &ParamTable) -> Box<dyn Block> {
    Box::new(BooleanToReal {
        real_true: real_param(p, "realTrue", REAL_TRUE_DEFAULT),
        real_false: real_param(p, "realFalse", REAL_FALSE_DEFAULT),
    })
}

fn make_integer_to_real(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerToReal)
}

fn make_real_to_integer(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(RealToInteger)
}
