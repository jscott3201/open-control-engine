use oce_model::ParamTable;

use super::{bool_param, int_param, real_param};
use crate::{
    Abs, Add, AddParameter, Average, Block, Constant, Divide, Greater, GreaterThreshold,
    Hysteresis, Less, LessThreshold, Limiter, Line, Max, Min, Modulo, Multiply,
    MultiplyByParameter, ParamRule, RegistryEntry, Round, Sqrt, Subtract, Switch,
};

pub(super) const ENTRIES: &[RegistryEntry] = &[
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
        class_path: "CDL.Reals.Multiply",
        make: make_multiply,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Divide",
        make: make_divide,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Sqrt",
        make: make_sqrt,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Average",
        make: make_average,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Modulo",
        make: make_modulo,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Round",
        make: make_round,
    },
    RegistryEntry {
        class_path: "CDL.Reals.AddParameter",
        make: make_add_parameter,
    },
    RegistryEntry {
        class_path: "CDL.Reals.MultiplyByParameter",
        make: make_multiply_by_parameter,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Abs",
        make: make_abs,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Min",
        make: make_min,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Max",
        make: make_max,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Limiter",
        make: make_limiter,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Line",
        make: make_line,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Greater",
        make: make_greater,
    },
    RegistryEntry {
        class_path: "CDL.Reals.GreaterThreshold",
        make: make_greater_threshold,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Hysteresis",
        make: make_hysteresis,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Less",
        make: make_less,
    },
    RegistryEntry {
        class_path: "CDL.Reals.LessThreshold",
        make: make_less_threshold,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Switch",
        make: make_switch,
    },
];

pub(super) const LIMITER_PARAM_RULES: &[ParamRule] = &[
    ParamRule::RealLessOrEqual {
        lower: "uMin",
        upper: "uMax",
    },
    ParamRule::RealEqualWarning {
        left: "uMin",
        right: "uMax",
    },
];

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

fn make_multiply(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Multiply)
}

fn make_divide(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Divide)
}

fn make_sqrt(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Sqrt)
}

fn make_average(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Average)
}

fn make_modulo(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Modulo)
}

fn make_round(p: &ParamTable) -> Box<dyn Block> {
    Box::new(Round {
        n: int_param(p, "n", 0),
    })
}

fn make_add_parameter(p: &ParamTable) -> Box<dyn Block> {
    Box::new(AddParameter {
        p: real_param(p, "p", 0.0),
    })
}

fn make_multiply_by_parameter(p: &ParamTable) -> Box<dyn Block> {
    Box::new(MultiplyByParameter {
        k: real_param(p, "k", 1.0),
    })
}

fn make_abs(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Abs)
}

fn make_min(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Min)
}

fn make_max(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Max)
}

fn make_limiter(p: &ParamTable) -> Box<dyn Block> {
    Box::new(Limiter {
        u_min: real_param(p, "uMin", f64::NEG_INFINITY),
        u_max: real_param(p, "uMax", f64::INFINITY),
    })
}

fn make_line(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Line)
}

fn make_greater(p: &ParamTable) -> Box<dyn Block> {
    Box::new(Greater {
        h: real_param(p, "h", 0.0),
        pre_y_start: bool_param(p, "pre_y_start", false),
    })
}

fn make_greater_threshold(p: &ParamTable) -> Box<dyn Block> {
    Box::new(GreaterThreshold {
        t: real_param(p, "t", 0.0),
        h: real_param(p, "h", 0.0),
        pre_y_start: bool_param(p, "pre_y_start", false),
    })
}

fn make_hysteresis(p: &ParamTable) -> Box<dyn Block> {
    Box::new(Hysteresis {
        u_low: real_param(p, "uLow", 0.0),
        u_high: real_param(p, "uHigh", 1.0),
        pre_y_start: bool_param(p, "pre_y_start", false),
    })
}

fn make_less(p: &ParamTable) -> Box<dyn Block> {
    Box::new(Less {
        h: real_param(p, "h", 0.0),
        pre_y_start: bool_param(p, "pre_y_start", false),
    })
}

fn make_less_threshold(p: &ParamTable) -> Box<dyn Block> {
    Box::new(LessThreshold {
        t: real_param(p, "t", 0.0),
        h: real_param(p, "h", 0.0),
        pre_y_start: bool_param(p, "pre_y_start", false),
    })
}

fn make_switch(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Switch)
}
