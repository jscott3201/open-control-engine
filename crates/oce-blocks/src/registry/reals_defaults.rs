//! Constructor constants and authored defaults for the `CDL.Reals` registry family.

use oce_model::ZeroTime;

use crate::ParamDefault;

pub(super) const CONSTANT_K_FALLBACK: f64 = 0.0;
pub(super) const PULSE_AMPLITUDE_DEFAULT: f64 = 1.0;
pub(super) const PULSE_WIDTH_DEFAULT: f64 = 0.5;
pub(super) const PULSE_PERIOD_FALLBACK: f64 = 1.0;
pub(super) const PULSE_SHIFT_DEFAULT: f64 = 0.0;
pub(super) const PULSE_OFFSET_DEFAULT: f64 = 0.0;
pub(super) const SOURCE_RAMP_HEIGHT_DEFAULT: f64 = 1.0;
pub(super) const SOURCE_RAMP_DURATION_FALLBACK: f64 = 1.0;
pub(super) const SOURCE_RAMP_OFFSET_DEFAULT: f64 = 0.0;
pub(super) const SOURCE_RAMP_START_TIME_DEFAULT: f64 = 0.0;
pub(super) const SOURCE_SIN_AMPLITUDE_DEFAULT: f64 = 1.0;
pub(super) const SOURCE_SIN_FREQ_HZ_FALLBACK: f64 = 1.0;
pub(super) const SOURCE_SIN_PHASE_DEFAULT: f64 = 0.0;
pub(super) const SOURCE_SIN_OFFSET_DEFAULT: f64 = 0.0;
pub(super) const SOURCE_SIN_START_TIME_DEFAULT: f64 = 0.0;
pub(super) const CALENDAR_ZERO_TIME_FALLBACK: ZeroTime = ZeroTime::NewYear(2016);
pub(super) const CALENDAR_YEAR_REF_DEFAULT: i64 = 2016;
pub(super) const CALENDAR_OFFSET_DEFAULT: f64 = 0.0;
pub(super) const ROUND_N_FALLBACK: i64 = 0;
pub(super) const ADD_PARAMETER_P_FALLBACK: f64 = 0.0;
pub(super) const MULTIPLY_BY_PARAMETER_K_FALLBACK: f64 = 1.0;
pub(super) const MULTI_NIN_DEFAULT: i64 = 0;
pub(super) const MULTI_SUM_K_DEFAULT: f64 = 1.0;
pub(super) const MATRIX_GAIN_NOUT_DEFAULT: i64 = 2;
pub(super) const MATRIX_GAIN_NIN_DEFAULT: i64 = 2;
pub(super) const MATRIX_EXTREME_NROW_FALLBACK: i64 = 1;
pub(super) const MATRIX_EXTREME_NCOL_FALLBACK: i64 = 1;
pub(super) const MATRIX_MAX_ROW_DEFAULT: bool = true;
pub(super) const MATRIX_MIN_ROW_DEFAULT: bool = true;
pub(super) const SORT_NIN_DEFAULT: i64 = 0;
pub(super) const SORT_ASCENDING_DEFAULT: bool = true;
pub(super) const LIMITER_U_MIN_FALLBACK: f64 = f64::NEG_INFINITY;
pub(super) const LIMITER_U_MAX_FALLBACK: f64 = f64::INFINITY;
pub(super) const LINE_LIMIT_BELOW_DEFAULT: bool = true;
pub(super) const LINE_LIMIT_ABOVE_DEFAULT: bool = true;
pub(super) const COMPARATOR_H_DEFAULT: f64 = 0.0;
pub(super) const COMPARATOR_T_DEFAULT: f64 = 0.0;
pub(super) const COMPARATOR_PRE_Y_START_DEFAULT: bool = false;
pub(super) const HYSTERESIS_U_LOW_FALLBACK: f64 = 0.0;
pub(super) const HYSTERESIS_U_HIGH_FALLBACK: f64 = 1.0;
pub(super) const HYSTERESIS_PRE_Y_START_DEFAULT: bool = false;

pub(super) const CONSTANT_PARAM_DEFAULTS: &[ParamDefault] = &[param_default_required!("k")];
pub(super) const PULSE_PARAM_DEFAULTS: &[ParamDefault] = &[
    param_default_real!("amplitude", PULSE_AMPLITUDE_DEFAULT),
    param_default_real!("width", PULSE_WIDTH_DEFAULT),
    param_default_required!("period"),
    param_default_real!("shift", PULSE_SHIFT_DEFAULT),
    param_default_real!("offset", PULSE_OFFSET_DEFAULT),
];
pub(super) const SOURCE_RAMP_PARAM_DEFAULTS: &[ParamDefault] = &[
    param_default_real!("height", SOURCE_RAMP_HEIGHT_DEFAULT),
    param_default_required!("duration"),
    param_default_real!("offset", SOURCE_RAMP_OFFSET_DEFAULT),
    param_default_real!("startTime", SOURCE_RAMP_START_TIME_DEFAULT),
];
pub(super) const SOURCE_SIN_PARAM_DEFAULTS: &[ParamDefault] = &[
    param_default_real!("amplitude", SOURCE_SIN_AMPLITUDE_DEFAULT),
    param_default_required!("freqHz"),
    param_default_real!("phase", SOURCE_SIN_PHASE_DEFAULT),
    param_default_real!("offset", SOURCE_SIN_OFFSET_DEFAULT),
    param_default_real!("startTime", SOURCE_SIN_START_TIME_DEFAULT),
];
pub(super) const CALENDAR_TIME_PARAM_DEFAULTS: &[ParamDefault] = &[
    param_default_required!("zerTim"),
    param_default_integer!("yearRef", CALENDAR_YEAR_REF_DEFAULT),
    param_default_real!("offset", CALENDAR_OFFSET_DEFAULT),
];
pub(super) const ROUND_PARAM_DEFAULTS: &[ParamDefault] = &[param_default_required!("n")];
pub(super) const ADD_PARAMETER_PARAM_DEFAULTS: &[ParamDefault] = &[param_default_required!("p")];
pub(super) const MULTIPLY_BY_PARAMETER_PARAM_DEFAULTS: &[ParamDefault] =
    &[param_default_required!("k")];
pub(super) const MULTI_REAL_PARAM_DEFAULTS: &[ParamDefault] =
    &[param_default_integer!("nin", MULTI_NIN_DEFAULT)];
pub(super) const MULTI_SUM_PARAM_DEFAULTS: &[ParamDefault] = &[
    param_default_integer!("nin", MULTI_NIN_DEFAULT),
    param_default_real!("k_<i>", MULTI_SUM_K_DEFAULT),
];
pub(super) const MATRIX_GAIN_PARAM_DEFAULTS: &[ParamDefault] = &[
    param_default_integer!("nout", MATRIX_GAIN_NOUT_DEFAULT),
    param_default_integer!("nin", MATRIX_GAIN_NIN_DEFAULT),
    param_default_derived!("K_<row>_<col>", "1.0 if row == col else 0.0"),
];
pub(super) const MATRIX_MAX_PARAM_DEFAULTS: &[ParamDefault] = &[
    param_default_required!("nRow"),
    param_default_required!("nCol"),
    param_default_boolean!("rowMax", MATRIX_MAX_ROW_DEFAULT),
];
pub(super) const MATRIX_MIN_PARAM_DEFAULTS: &[ParamDefault] = &[
    param_default_required!("nRow"),
    param_default_required!("nCol"),
    param_default_boolean!("rowMin", MATRIX_MIN_ROW_DEFAULT),
];
pub(super) const SORT_PARAM_DEFAULTS: &[ParamDefault] = &[
    param_default_integer!("nin", SORT_NIN_DEFAULT),
    param_default_boolean!("ascending", SORT_ASCENDING_DEFAULT),
];
pub(super) const LIMITER_PARAM_DEFAULTS: &[ParamDefault] = &[
    param_default_required!("uMin"),
    param_default_required!("uMax"),
];
pub(super) const LINE_PARAM_DEFAULTS: &[ParamDefault] = &[
    param_default_boolean!("limitBelow", LINE_LIMIT_BELOW_DEFAULT),
    param_default_boolean!("limitAbove", LINE_LIMIT_ABOVE_DEFAULT),
];
pub(super) const COMPARATOR_PARAM_DEFAULTS: &[ParamDefault] = &[
    param_default_real!("h", COMPARATOR_H_DEFAULT),
    param_default_boolean!("pre_y_start", COMPARATOR_PRE_Y_START_DEFAULT),
];
pub(super) const THRESHOLD_COMPARATOR_PARAM_DEFAULTS: &[ParamDefault] = &[
    param_default_real!("t", COMPARATOR_T_DEFAULT),
    param_default_real!("h", COMPARATOR_H_DEFAULT),
    param_default_boolean!("pre_y_start", COMPARATOR_PRE_Y_START_DEFAULT),
];
pub(super) const HYSTERESIS_PARAM_DEFAULTS: &[ParamDefault] = &[
    param_default_required!("uLow"),
    param_default_required!("uHigh"),
    param_default_boolean!("pre_y_start", HYSTERESIS_PRE_Y_START_DEFAULT),
];
