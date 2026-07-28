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
    OnCounter, ParamDefault, ParamRule, RegistryEntry, TimeTableValues,
};

const INTEGER_CONSTANT_K_FALLBACK: i64 = 0;
const PULSE_AMPLITUDE_DEFAULT: i64 = 1;
const PULSE_WIDTH_DEFAULT: f64 = 0.5;
const PULSE_PERIOD_FALLBACK: f64 = 1.0;
const PULSE_SHIFT_DEFAULT: f64 = 0.0;
const PULSE_OFFSET_DEFAULT: i64 = 0;
const ADD_PARAMETER_P_FALLBACK: i64 = 0;
const MULTI_SUM_NIN_DEFAULT: i64 = 0;
const MULTI_SUM_K_DEFAULT: i64 = 1;
const THRESHOLD_DEFAULT: i64 = 0;
const ON_COUNTER_Y_START_DEFAULT: i64 = 0;
const CHANGE_PRE_U_START_DEFAULT: i64 = 0;
const STAGE_N_FALLBACK: i64 = 1;
const STAGE_HOLD_DURATION_FALLBACK: f64 = 0.0;
const STAGE_PRE_Y_START_DEFAULT: i64 = 0;

pub(super) const INTEGER_CONSTANT_PARAM_DEFAULTS: &[ParamDefault] = &[param_default_required!("k")];
pub(super) const INTEGER_PULSE_PARAM_DEFAULTS: &[ParamDefault] = &[
    param_default_integer!("amplitude", PULSE_AMPLITUDE_DEFAULT),
    param_default_real!("width", PULSE_WIDTH_DEFAULT),
    param_default_required!("period"),
    param_default_real!("shift", PULSE_SHIFT_DEFAULT),
    param_default_integer!("offset", PULSE_OFFSET_DEFAULT),
];
pub(super) const INTEGER_TIME_TABLE_PARAM_DEFAULTS: &[ParamDefault] =
    &[param_default_required!("period")];
pub(super) const INTEGER_ADD_PARAMETER_PARAM_DEFAULTS: &[ParamDefault] =
    &[param_default_required!("p")];
pub(super) const INTEGER_MULTI_SUM_PARAM_DEFAULTS: &[ParamDefault] = &[
    param_default_integer!("nin", MULTI_SUM_NIN_DEFAULT),
    param_default_integer!("k_<i>", MULTI_SUM_K_DEFAULT),
];
pub(super) const INTEGER_THRESHOLD_PARAM_DEFAULTS: &[ParamDefault] =
    &[param_default_integer!("t", THRESHOLD_DEFAULT)];
pub(super) const INTEGER_ON_COUNTER_PARAM_DEFAULTS: &[ParamDefault] = &[param_default_integer!(
    "y_start",
    ON_COUNTER_Y_START_DEFAULT
)];
pub(super) const INTEGER_CHANGE_PARAM_DEFAULTS: &[ParamDefault] = &[param_default_integer!(
    "pre_u_start",
    CHANGE_PRE_U_START_DEFAULT
)];
pub(super) const INTEGER_STAGE_PARAM_DEFAULTS: &[ParamDefault] = &[
    param_default_required!("n"),
    param_default_required!("holdDuration"),
    param_default_derived!("h", "0.02 / n"),
    param_default_integer!("pre_y_start", STAGE_PRE_Y_START_DEFAULT),
];

/// Upstream declares these parameters with NO default value (pin `a131864`):
/// `Integers/Sources/Constant.k` and `Integers/AddParameter.p`. Omitting one previously fell
/// through to a silent engine default (k=0 / p=0).
pub(super) const INTEGER_CONSTANT_PARAM_RULES: &[ParamRule] = &[ParamRule::Required { name: "k" }];

pub(super) const INTEGER_ADD_PARAMETER_PARAM_RULES: &[ParamRule] =
    &[ParamRule::Required { name: "p" }];

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
        k: int_param(p, "k", INTEGER_CONSTANT_K_FALLBACK),
    })
}

fn make_integer_pulse(p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerPulse {
        amplitude: int_param(p, "amplitude", PULSE_AMPLITUDE_DEFAULT),
        width: real_param(p, "width", PULSE_WIDTH_DEFAULT),
        period: real_param(p, "period", PULSE_PERIOD_FALLBACK),
        shift: real_param(p, "shift", PULSE_SHIFT_DEFAULT),
        offset: int_param(p, "offset", PULSE_OFFSET_DEFAULT),
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
        p: int_param(p, "p", ADD_PARAMETER_P_FALLBACK),
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
            int_param(p, &key, MULTI_SUM_K_DEFAULT)
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
        t: int_param(p, "t", THRESHOLD_DEFAULT),
    })
}

fn make_integer_greater_equal(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerGreaterEqual)
}

fn make_integer_greater_equal_threshold(p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerGreaterEqualThreshold {
        t: int_param(p, "t", THRESHOLD_DEFAULT),
    })
}

fn make_integer_less(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerLess)
}

fn make_integer_less_threshold(p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerLessThreshold {
        t: int_param(p, "t", THRESHOLD_DEFAULT),
    })
}

fn make_integer_less_equal(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerLessEqual)
}

fn make_integer_less_equal_threshold(p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerLessEqualThreshold {
        t: int_param(p, "t", THRESHOLD_DEFAULT),
    })
}

fn make_integer_on_counter(p: &ParamTable) -> Box<dyn Block> {
    Box::new(OnCounter {
        y_start: int_param(p, "y_start", ON_COUNTER_Y_START_DEFAULT),
    })
}

fn make_integer_change(p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerChange {
        pre_u_start: int_param(p, "pre_u_start", CHANGE_PRE_U_START_DEFAULT),
    })
}

fn make_integer_stage(p: &ParamTable) -> Box<dyn Block> {
    let n = int_param(p, "n", STAGE_N_FALLBACK).max(1);
    Box::new(IntegerStage {
        n,
        hold_duration: real_param(p, "holdDuration", STAGE_HOLD_DURATION_FALLBACK),
        h: real_param(p, "h", 0.02 / n as f64),
        pre_y_start: int_param(p, "pre_y_start", STAGE_PRE_Y_START_DEFAULT),
    })
}

fn bounded_nin(p: &ParamTable) -> usize {
    usize::try_from(int_param(p, "nin", MULTI_SUM_NIN_DEFAULT).max(0))
        .unwrap_or(MAX_RESOLVED_PORT_WIDTH)
        .min(MAX_RESOLVED_PORT_WIDTH)
}
