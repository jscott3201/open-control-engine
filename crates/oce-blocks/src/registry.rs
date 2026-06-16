//! The static class-path → constructor registry (`03` R-IMPL-2). `oce-cxf`/`oce-api` resolve a
//! flattened block instance's `class_iri` to a native [`Block`] impl via [`lookup`], then construct
//! it from the instance's resolved [`ParamTable`]. The catalog holds the **canonical** v9+
//! `Reals`/`Integers`/`Logical` class paths; resolving deprecated `Continuous.*` aliases to these
//! is `oce-validate`/`oce-cxf`'s job (M1, `03` §4), not a second alias table here.

use oce_model::{ParamTable, Value};

use crate::{
    Add, And, Block, Constant, Greater, Limiter, MultiplyByParameter, Not, Pre, RegistryEntry,
    Subtract, Switch, UnitDelay,
};

/// Look up an elementary-block constructor by canonical class path. Unknown paths return `None`
/// (an unresolved external / extension block — never a panic; R-IMPL-2).
#[must_use]
pub fn lookup(class_path: &str) -> Option<&'static RegistryEntry> {
    CATALOG.iter().find(|e| e.class_path == class_path)
}

/// The M0 starter catalog (`03` §7 Phase-1 core + the two loop-breakers).
static CATALOG: &[RegistryEntry] = &[
    RegistryEntry {
        class_path: "CDL.Reals.Sources.Constant",
        make: make_constant,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Add",
        make: make_add,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Subtract",
        make: make_subtract,
    },
    RegistryEntry {
        class_path: "CDL.Reals.MultiplyByParameter",
        make: make_multiply_by_parameter,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Limiter",
        make: make_limiter,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Greater",
        make: make_greater,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Switch",
        make: make_switch,
    },
    RegistryEntry {
        class_path: "CDL.Logical.And",
        make: make_and,
    },
    RegistryEntry {
        class_path: "CDL.Logical.Not",
        make: make_not,
    },
    RegistryEntry {
        class_path: "CDL.Logical.Pre",
        make: make_pre,
    },
    RegistryEntry {
        class_path: "CDL.Discrete.UnitDelay",
        make: make_unit_delay,
    },
];

// ---- parameter readers ----------------------------------------------------------------------

fn find<'a>(params: &'a ParamTable, name: &str) -> Option<&'a Value> {
    params
        .values
        .iter()
        .find(|(n, _)| n.as_ref() == name)
        .map(|(_, v)| v)
}

fn real_param(params: &ParamTable, name: &str, default: f64) -> f64 {
    match find(params, name) {
        Some(Value::Real(x)) => *x,
        // Modelica/CDL Int→Real promotion (§7.3.4): an integer literal bound to a `Real` parameter
        // is its real value. CXF may carry a bare integer (e.g. `y_start: 0`) for a Real parameter
        // when no `isOfDataType` re-types it; WITHOUT this arm such a binding would silently fall
        // through to `default`, discarding the author's value (a safety-critical silent wrong value
        // — e.g. a non-zero `UnitDelay.y_start` initial state). `i64 as f64` is the lossless/CDL
        // promotion for the ±2³¹ Integer domain.
        Some(Value::Integer(i)) => *i as f64,
        _ => default,
    }
}

fn bool_param(params: &ParamTable, name: &str, default: bool) -> bool {
    match find(params, name) {
        Some(Value::Boolean(b)) => *b,
        _ => default,
    }
}

// ---- constructors (one per catalog entry) ---------------------------------------------------

fn make_constant(p: &ParamTable) -> Box<dyn Block> {
    Box::new(Constant {
        k: real_param(p, "k", 0.0),
    })
}

fn make_add(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Add)
}

fn make_subtract(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Subtract)
}

fn make_multiply_by_parameter(p: &ParamTable) -> Box<dyn Block> {
    Box::new(MultiplyByParameter {
        k: real_param(p, "k", 1.0),
    })
}

fn make_limiter(p: &ParamTable) -> Box<dyn Block> {
    Box::new(Limiter {
        u_min: real_param(p, "uMin", f64::NEG_INFINITY),
        u_max: real_param(p, "uMax", f64::INFINITY),
    })
}

fn make_greater(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Greater)
}

fn make_switch(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Switch)
}

fn make_and(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(And)
}

fn make_not(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Not)
}

fn make_pre(p: &ParamTable) -> Box<dyn Block> {
    Box::new(Pre {
        y_start: bool_param(p, "pre_u_start", false),
    })
}

fn make_unit_delay(p: &ParamTable) -> Box<dyn Block> {
    Box::new(UnitDelay {
        y_start: real_param(p, "y_start", 0.0),
    })
}
