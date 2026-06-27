use oce_model::ParamTable;

use super::{bool_param, int_param, real_param, zero_time_param};
use crate::reals_sources::{MIN_SOURCE_RAMP_DURATION, ZERO_TIME_MEMBERS};
use crate::source_timetable::{EXTRAPOLATION_MEMBERS, SMOOTHNESS_MEMBERS};
use crate::{
    Abs, Acos, Add, AddParameter, Asin, Atan, Atan2, Average, Block, CalendarTime, CivilTime,
    Constant, Cos, Divide, Exp, Greater, GreaterThreshold, Hysteresis, Less, LessThreshold,
    Limiter, Line, Log, Log10, MAX_RESOLVED_PORT_WIDTH, MatrixGain, MatrixMax, MatrixMin,
    MatrixReductionAxis, Max, Min, Modulo, MultiMax, MultiMin, MultiSum, Multiply,
    MultiplyByParameter, ParamRule, RealPulse, RealTimeTable, RegistryEntry, Round, Sin, Sort,
    SourceRamp, SourceSin, Sqrt, Subtract, Switch, Tan, TimeTableValues,
};

pub(super) const ENTRIES: &[RegistryEntry] = &[
    RegistryEntry {
        class_path: "CDL.Reals.Sources.Constant",
        make: make_constant,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Sources.CivilTime",
        make: make_civil_time,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Sources.Pulse",
        make: make_real_pulse,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Sources.Ramp",
        make: make_source_ramp,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Sources.Sin",
        make: make_source_sin,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Sources.CalendarTime",
        make: make_calendar_time,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Sources.TimeTable",
        make: make_real_time_table,
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
        class_path: "CDL.Reals.Sin",
        make: make_sin,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Cos",
        make: make_cos,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Tan",
        make: make_tan,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Asin",
        make: make_asin,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Acos",
        make: make_acos,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Atan",
        make: make_atan,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Atan2",
        make: make_atan2,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Exp",
        make: make_exp,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Log",
        make: make_log,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Log10",
        make: make_log10,
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
        class_path: "CDL.Reals.MultiMax",
        make: make_multi_max,
    },
    RegistryEntry {
        class_path: "CDL.Reals.MultiMin",
        make: make_multi_min,
    },
    RegistryEntry {
        class_path: "CDL.Reals.MultiSum",
        make: make_multi_sum,
    },
    RegistryEntry {
        class_path: "CDL.Reals.MatrixGain",
        make: make_matrix_gain,
    },
    RegistryEntry {
        class_path: "CDL.Reals.MatrixMax",
        make: make_matrix_max,
    },
    RegistryEntry {
        class_path: "CDL.Reals.MatrixMin",
        make: make_matrix_min,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Sort",
        make: make_sort,
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

pub(super) const SOURCE_RAMP_PARAM_RULES: &[ParamRule] = &[
    ParamRule::Required { name: "duration" },
    ParamRule::RealGreaterOrEqual {
        name: "duration",
        min: MIN_SOURCE_RAMP_DURATION,
    },
];

pub(super) const SOURCE_SIN_PARAM_RULES: &[ParamRule] = &[
    ParamRule::Required { name: "freqHz" },
    ParamRule::Real { name: "amplitude" },
    ParamRule::Real { name: "freqHz" },
    ParamRule::Real { name: "phase" },
    ParamRule::Real { name: "offset" },
    ParamRule::Real { name: "startTime" },
];

pub(super) const CALENDAR_TIME_PARAM_RULES: &[ParamRule] = &[
    ParamRule::Required { name: "zerTim" },
    ParamRule::EnumMembers {
        name: "zerTim",
        members: ZERO_TIME_MEMBERS,
    },
    ParamRule::IntegerGreaterOrEqual {
        name: "yearRef",
        min: 2010,
    },
    ParamRule::IntegerLessOrEqualConstant {
        name: "yearRef",
        max: 2031,
    },
    ParamRule::RealFinite { name: "offset" },
];

pub(super) const REAL_TIMETABLE_PARAM_RULES: &[ParamRule] = &[
    ParamRule::TimeTableMatrix {
        base: "table",
        values: TimeTableValues::Real,
        time_scale: "timeScale",
        period: None,
        extrapolation: Some("extrapolation"),
    },
    ParamRule::TimeTableOffset {
        base: "offset",
        table: "table",
    },
    ParamRule::EnumMembers {
        name: "smoothness",
        members: SMOOTHNESS_MEMBERS,
    },
    ParamRule::EnumMembers {
        name: "extrapolation",
        members: EXTRAPOLATION_MEMBERS,
    },
    ParamRule::RealFiniteGreaterThan {
        name: "timeScale",
        min: 0.0,
    },
];

pub(super) const MULTI_REAL_PARAM_RULES: &[ParamRule] = &[
    ParamRule::Structural { name: "nin" },
    ParamRule::IntegerGreaterOrEqual {
        name: "nin",
        min: 0,
    },
    ParamRule::IntegerLessOrEqualConstant {
        name: "nin",
        max: MAX_RESOLVED_PORT_WIDTH as i64,
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
    ParamRule::RealArrayElements {
        base: "k",
        len: "nin",
    },
];

pub(super) const MATRIX_GAIN_PARAM_RULES: &[ParamRule] = &[
    ParamRule::Structural { name: "nout" },
    ParamRule::Structural { name: "nin" },
    ParamRule::IntegerGreaterOrEqual {
        name: "nout",
        min: 0,
    },
    ParamRule::IntegerLessOrEqualConstant {
        name: "nout",
        max: MAX_RESOLVED_PORT_WIDTH as i64,
    },
    ParamRule::IntegerGreaterOrEqual {
        name: "nin",
        min: 0,
    },
    ParamRule::IntegerLessOrEqualConstant {
        name: "nin",
        max: MAX_RESOLVED_PORT_WIDTH as i64,
    },
    ParamRule::IntegerProductLessOrEqualConstant {
        left: "nout",
        right: "nin",
        max: MAX_RESOLVED_PORT_WIDTH as i64,
    },
    ParamRule::RealMatrixElements {
        base: "K",
        rows: "nout",
        default_rows: 2,
        cols: "nin",
        default_cols: 2,
    },
];

pub(super) const MATRIX_MAX_PARAM_RULES: &[ParamRule] = &[
    ParamRule::Required { name: "nRow" },
    ParamRule::Required { name: "nCol" },
    ParamRule::Structural { name: "nRow" },
    ParamRule::Structural { name: "nCol" },
    ParamRule::Structural { name: "rowMax" },
    ParamRule::Boolean { name: "rowMax" },
    ParamRule::IntegerGreaterOrEqual {
        name: "nRow",
        min: 1,
    },
    ParamRule::IntegerLessOrEqualConstant {
        name: "nRow",
        max: MAX_RESOLVED_PORT_WIDTH as i64,
    },
    ParamRule::IntegerGreaterOrEqual {
        name: "nCol",
        min: 1,
    },
    ParamRule::IntegerLessOrEqualConstant {
        name: "nCol",
        max: MAX_RESOLVED_PORT_WIDTH as i64,
    },
    ParamRule::IntegerProductLessOrEqualConstant {
        left: "nRow",
        right: "nCol",
        max: MAX_RESOLVED_PORT_WIDTH as i64,
    },
];

pub(super) const MATRIX_MIN_PARAM_RULES: &[ParamRule] = &[
    ParamRule::Required { name: "nRow" },
    ParamRule::Required { name: "nCol" },
    ParamRule::Structural { name: "nRow" },
    ParamRule::Structural { name: "nCol" },
    ParamRule::Structural { name: "rowMin" },
    ParamRule::Boolean { name: "rowMin" },
    ParamRule::IntegerGreaterOrEqual {
        name: "nRow",
        min: 1,
    },
    ParamRule::IntegerLessOrEqualConstant {
        name: "nRow",
        max: MAX_RESOLVED_PORT_WIDTH as i64,
    },
    ParamRule::IntegerGreaterOrEqual {
        name: "nCol",
        min: 1,
    },
    ParamRule::IntegerLessOrEqualConstant {
        name: "nCol",
        max: MAX_RESOLVED_PORT_WIDTH as i64,
    },
    ParamRule::IntegerProductLessOrEqualConstant {
        left: "nRow",
        right: "nCol",
        max: MAX_RESOLVED_PORT_WIDTH as i64,
    },
];

pub(super) const SORT_PARAM_RULES: &[ParamRule] = &[
    ParamRule::Structural { name: "nin" },
    ParamRule::Boolean { name: "ascending" },
    ParamRule::IntegerGreaterOrEqual {
        name: "nin",
        min: 0,
    },
    ParamRule::IntegerLessOrEqualConstant {
        name: "nin",
        max: MAX_RESOLVED_PORT_WIDTH as i64,
    },
];

fn make_constant(p: &ParamTable) -> Box<dyn Block> {
    Box::new(Constant {
        k: real_param(p, "k", 0.0),
    })
}

fn make_civil_time(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(CivilTime)
}

fn make_real_pulse(p: &ParamTable) -> Box<dyn Block> {
    Box::new(RealPulse {
        amplitude: real_param(p, "amplitude", 1.0),
        width: real_param(p, "width", 0.5),
        period: real_param(p, "period", 1.0),
        shift: real_param(p, "shift", 0.0),
        offset: real_param(p, "offset", 0.0),
    })
}

fn make_source_ramp(p: &ParamTable) -> Box<dyn Block> {
    Box::new(SourceRamp {
        height: real_param(p, "height", 1.0),
        duration: real_param(p, "duration", 1.0),
        offset: real_param(p, "offset", 0.0),
        start_time: real_param(p, "startTime", 0.0),
    })
}

fn make_source_sin(p: &ParamTable) -> Box<dyn Block> {
    Box::new(SourceSin {
        amplitude: real_param(p, "amplitude", 1.0),
        freq_hz: real_param(p, "freqHz", 1.0),
        phase: real_param(p, "phase", 0.0),
        offset: real_param(p, "offset", 0.0),
        start_time: real_param(p, "startTime", 0.0),
    })
}

fn make_calendar_time(p: &ParamTable) -> Box<dyn Block> {
    Box::new(CalendarTime {
        zer_tim: zero_time_param(p, "zerTim", oce_model::ZeroTime::NewYear(2016)),
        year_ref: int_param(p, "yearRef", 2016),
        offset: real_param(p, "offset", 0.0),
    })
}

fn make_real_time_table(p: &ParamTable) -> Box<dyn Block> {
    Box::new(RealTimeTable::from_params(p))
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

fn make_sin(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Sin)
}

fn make_cos(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Cos)
}

fn make_tan(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Tan)
}

fn make_asin(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Asin)
}

fn make_acos(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Acos)
}

fn make_atan(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Atan)
}

fn make_atan2(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Atan2)
}

fn make_exp(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Exp)
}

fn make_log(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Log)
}

fn make_log10(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Log10)
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

fn make_multi_max(p: &ParamTable) -> Box<dyn Block> {
    Box::new(MultiMax::new(bounded_nin(p)))
}

fn make_multi_min(p: &ParamTable) -> Box<dyn Block> {
    Box::new(MultiMin::new(bounded_nin(p)))
}

fn make_multi_sum(p: &ParamTable) -> Box<dyn Block> {
    let nin = bounded_nin(p);
    let gains = (1..=nin)
        .map(|idx| real_param(p, &format!("k_{idx}"), 1.0))
        .collect();
    Box::new(MultiSum::new(gains))
}

fn make_matrix_gain(p: &ParamTable) -> Box<dyn Block> {
    let nout = bounded_usize_param(p, "nout", 2);
    let nin = bounded_usize_param(p, "nin", 2);
    let Some(product) = nout.checked_mul(nin) else {
        return Box::new(MatrixGain::new(0, 0, Vec::new()));
    };
    if product > MAX_RESOLVED_PORT_WIDTH {
        return Box::new(MatrixGain::new(0, 0, Vec::new()));
    }
    let gains = (1..=nout)
        .flat_map(|row| (1..=nin).map(move |col| (row, col)))
        .map(|(row, col)| {
            real_param(
                p,
                &format!("K_{row}_{col}"),
                if row == col { 1.0 } else { 0.0 },
            )
        })
        .collect();
    Box::new(MatrixGain::new(nin, nout, gains))
}

fn make_matrix_max(p: &ParamTable) -> Box<dyn Block> {
    Box::new(MatrixMax::new(
        bounded_usize_param(p, "nRow", 1),
        bounded_usize_param(p, "nCol", 1),
        if bool_param(p, "rowMax", true) {
            MatrixReductionAxis::Rows
        } else {
            MatrixReductionAxis::Columns
        },
    ))
}

fn make_matrix_min(p: &ParamTable) -> Box<dyn Block> {
    Box::new(MatrixMin::new(
        bounded_usize_param(p, "nRow", 1),
        bounded_usize_param(p, "nCol", 1),
        if bool_param(p, "rowMin", true) {
            MatrixReductionAxis::Rows
        } else {
            MatrixReductionAxis::Columns
        },
    ))
}

fn make_sort(p: &ParamTable) -> Box<dyn Block> {
    Box::new(Sort::new(
        bounded_usize_param(p, "nin", 0),
        bool_param(p, "ascending", true),
    ))
}

fn make_limiter(p: &ParamTable) -> Box<dyn Block> {
    Box::new(Limiter {
        u_min: real_param(p, "uMin", f64::NEG_INFINITY),
        u_max: real_param(p, "uMax", f64::INFINITY),
    })
}

fn make_line(p: &ParamTable) -> Box<dyn Block> {
    Box::new(Line {
        limit_below: bool_param(p, "limitBelow", true),
        limit_above: bool_param(p, "limitAbove", true),
    })
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

fn bounded_nin(p: &ParamTable) -> usize {
    let raw = int_param(p, "nin", 0);
    usize::try_from(raw)
        .unwrap_or(0)
        .min(MAX_RESOLVED_PORT_WIDTH)
}

fn bounded_usize_param(p: &ParamTable, name: &str, default: i64) -> usize {
    usize::try_from(int_param(p, name, default))
        .unwrap_or(0)
        .min(MAX_RESOLVED_PORT_WIDTH)
}
