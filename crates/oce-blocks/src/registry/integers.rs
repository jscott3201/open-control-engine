use oce_model::ParamTable;

use super::int_param;
use super::real_param;
use crate::source_timetable::MIN_TIMETABLE_PERIOD;
use crate::{
    Block, IntegerAbs, IntegerAdd, IntegerAddParameter, IntegerChange, IntegerConstant,
    IntegerEqual, IntegerGreater, IntegerGreaterEqual, IntegerGreaterEqualThreshold,
    IntegerGreaterThreshold, IntegerLess, IntegerLessEqual, IntegerLessEqualThreshold,
    IntegerLessThreshold, IntegerMax, IntegerMin, IntegerMultiSum, IntegerMultiply, IntegerPulse,
    IntegerStage, IntegerSubtract, IntegerSwitch, IntegerTimeTable, MAX_RESOLVED_PORT_WIDTH,
    OnCounter, ParamRule, RegistryEntry, TimeTableValues,
};

pub(super) const STAGE_PARAM_RULES: &[ParamRule] = &[
    ParamRule::Required { name: "n" },
    ParamRule::Required {
        name: "holdDuration",
    },
    ParamRule::IntegerGreaterOrEqual { name: "n", min: 1 },
    ParamRule::RealGreaterOrEqual {
        name: "holdDuration",
        min: 0.0,
    },
    ParamRule::RealTimesIntegerInclusiveRange {
        real: "h",
        integer: "n",
        min: 0.001,
        max: 0.5,
    },
];

pub(super) const MULTI_SUM_PARAM_RULES: &[ParamRule] = &[
    ParamRule::Structural { name: "nin" },
    ParamRule::IntegerGreaterOrEqual {
        name: "nin",
        min: 0,
    },
    ParamRule::IntegerLessOrEqualConstant {
        name: "nin",
        max: MAX_RESOLVED_PORT_WIDTH as i64,
    },
    ParamRule::IntegerArrayElements {
        base: "k",
        len: "nin",
    },
];

pub(super) const TIME_TABLE_PARAM_RULES: &[ParamRule] = &[
    ParamRule::Required { name: "period" },
    ParamRule::TimeTableMatrix {
        base: "table",
        values: TimeTableValues::Integer,
        time_scale: "timeScale",
        period: Some("period"),
        extrapolation: None,
    },
    ParamRule::RealFiniteGreaterThan {
        name: "timeScale",
        min: 0.0,
    },
    ParamRule::RealFiniteGreaterThan {
        name: "period",
        min: 0.0,
    },
    ParamRule::RealGreaterOrEqual {
        name: "period",
        min: MIN_TIMETABLE_PERIOD,
    },
];

pub(super) const ENTRIES: &[RegistryEntry] = &[
    RegistryEntry {
        class_path: "CDL.Integers.Sources.Constant",
        make: make_integer_constant,
    },
    RegistryEntry {
        class_path: "CDL.Integers.Sources.Pulse",
        make: make_integer_pulse,
    },
    RegistryEntry {
        class_path: "CDL.Integers.Sources.TimeTable",
        make: make_integer_time_table,
    },
    RegistryEntry {
        class_path: "CDL.Integers.Abs",
        make: make_integer_abs,
    },
    RegistryEntry {
        class_path: "CDL.Integers.Add",
        make: make_integer_add,
    },
    RegistryEntry {
        class_path: "CDL.Integers.Subtract",
        make: make_integer_subtract,
    },
    RegistryEntry {
        class_path: "CDL.Integers.Multiply",
        make: make_integer_multiply,
    },
    RegistryEntry {
        class_path: "CDL.Integers.AddParameter",
        make: make_integer_add_parameter,
    },
    RegistryEntry {
        class_path: "CDL.Integers.Max",
        make: make_integer_max,
    },
    RegistryEntry {
        class_path: "CDL.Integers.Min",
        make: make_integer_min,
    },
    RegistryEntry {
        class_path: "CDL.Integers.MultiSum",
        make: make_integer_multi_sum,
    },
    RegistryEntry {
        class_path: "CDL.Integers.Switch",
        make: make_integer_switch,
    },
    RegistryEntry {
        class_path: "CDL.Integers.Equal",
        make: make_integer_equal,
    },
    RegistryEntry {
        class_path: "CDL.Integers.Greater",
        make: make_integer_greater,
    },
    RegistryEntry {
        class_path: "CDL.Integers.GreaterThreshold",
        make: make_integer_greater_threshold,
    },
    RegistryEntry {
        class_path: "CDL.Integers.GreaterEqual",
        make: make_integer_greater_equal,
    },
    RegistryEntry {
        class_path: "CDL.Integers.GreaterEqualThreshold",
        make: make_integer_greater_equal_threshold,
    },
    RegistryEntry {
        class_path: "CDL.Integers.Less",
        make: make_integer_less,
    },
    RegistryEntry {
        class_path: "CDL.Integers.LessThreshold",
        make: make_integer_less_threshold,
    },
    RegistryEntry {
        class_path: "CDL.Integers.LessEqual",
        make: make_integer_less_equal,
    },
    RegistryEntry {
        class_path: "CDL.Integers.LessEqualThreshold",
        make: make_integer_less_equal_threshold,
    },
    RegistryEntry {
        class_path: "CDL.Integers.OnCounter",
        make: make_integer_on_counter,
    },
    RegistryEntry {
        class_path: "CDL.Integers.Change",
        make: make_integer_change,
    },
    RegistryEntry {
        class_path: "CDL.Integers.Stage",
        make: make_integer_stage,
    },
];

fn make_integer_constant(p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerConstant {
        k: int_param(p, "k", 0),
    })
}

fn make_integer_pulse(p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerPulse {
        amplitude: int_param(p, "amplitude", 1),
        width: real_param(p, "width", 0.5),
        period: real_param(p, "period", 1.0),
        shift: real_param(p, "shift", 0.0),
        offset: int_param(p, "offset", 0),
    })
}

fn make_integer_time_table(p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerTimeTable::from_params(p))
}

fn make_integer_abs(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerAbs)
}

fn make_integer_add(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerAdd)
}

fn make_integer_subtract(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerSubtract)
}

fn make_integer_multiply(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerMultiply)
}

fn make_integer_add_parameter(p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerAddParameter {
        p: int_param(p, "p", 0),
    })
}

fn make_integer_max(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerMax)
}

fn make_integer_min(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerMin)
}

fn make_integer_multi_sum(p: &ParamTable) -> Box<dyn Block> {
    let nin = bounded_nin(p);
    let gains = (1..=nin)
        .map(|idx| {
            let key = format!("k_{idx}");
            int_param(p, &key, 1)
        })
        .collect();
    Box::new(IntegerMultiSum::new(gains))
}

fn make_integer_switch(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerSwitch)
}

fn make_integer_equal(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerEqual)
}

fn make_integer_greater(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerGreater)
}

fn make_integer_greater_threshold(p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerGreaterThreshold {
        t: int_param(p, "t", 0),
    })
}

fn make_integer_greater_equal(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerGreaterEqual)
}

fn make_integer_greater_equal_threshold(p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerGreaterEqualThreshold {
        t: int_param(p, "t", 0),
    })
}

fn make_integer_less(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerLess)
}

fn make_integer_less_threshold(p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerLessThreshold {
        t: int_param(p, "t", 0),
    })
}

fn make_integer_less_equal(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerLessEqual)
}

fn make_integer_less_equal_threshold(p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerLessEqualThreshold {
        t: int_param(p, "t", 0),
    })
}

fn make_integer_on_counter(p: &ParamTable) -> Box<dyn Block> {
    Box::new(OnCounter {
        y_start: int_param(p, "y_start", 0),
    })
}

fn make_integer_change(p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerChange {
        pre_u_start: int_param(p, "pre_u_start", 0),
    })
}

fn make_integer_stage(p: &ParamTable) -> Box<dyn Block> {
    let n = int_param(p, "n", 1).max(1);
    Box::new(IntegerStage {
        n,
        hold_duration: real_param(p, "holdDuration", 0.0),
        h: real_param(p, "h", 0.02 / n as f64),
        pre_y_start: int_param(p, "pre_y_start", 0),
    })
}

fn bounded_nin(p: &ParamTable) -> usize {
    usize::try_from(int_param(p, "nin", 0).max(0))
        .unwrap_or(MAX_RESOLVED_PORT_WIDTH)
        .min(MAX_RESOLVED_PORT_WIDTH)
}
