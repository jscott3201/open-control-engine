//! Existing aligned-suite fixtures, shared with the strict-bit inventory audit.

use super::{B, BlockCase, I, Param, ParamValue, Port, R, case};

const U: &[Port] = &[Port { name: "u", kind: R }];
const U1_U2: &[Port] = &[
    Port {
        name: "u1",
        kind: R,
    },
    Port {
        name: "u2",
        kind: R,
    },
];
const REAL_Y: &[Port] = &[Port { name: "y", kind: R }];

pub(crate) const REALS: &[BlockCase] = &[
    case("reals_sin", "CDL.Reals.Sin", "Sin", U, &[], REAL_Y),
    case("reals_cos", "CDL.Reals.Cos", "Cos", U, &[], REAL_Y),
    case("reals_tan", "CDL.Reals.Tan", "Tan", U, &[], REAL_Y),
    case("reals_asin", "CDL.Reals.Asin", "Asin", U, &[], REAL_Y),
    case("reals_acos", "CDL.Reals.Acos", "Acos", U, &[], REAL_Y),
    case("reals_atan", "CDL.Reals.Atan", "Atan", U, &[], REAL_Y),
    case(
        "reals_atan2",
        "CDL.Reals.Atan2",
        "Atan2",
        U1_U2,
        &[],
        REAL_Y,
    ),
    case("reals_exp", "CDL.Reals.Exp", "Exp", U, &[], REAL_Y),
    case("reals_log", "CDL.Reals.Log", "Log", U, &[], REAL_Y),
    case("reals_log10", "CDL.Reals.Log10", "Log10", U, &[], REAL_Y),
];

const SOURCE_SIN_PARAMS: &[Param] = &[
    Param {
        name: "amplitude",
        value: ParamValue::Real("2.0"),
    },
    Param {
        name: "freqHz",
        value: ParamValue::Real("0.25"),
    },
    Param {
        name: "phase",
        value: ParamValue::Real("1.5707963267948966"),
    },
    Param {
        name: "offset",
        value: ParamValue::Real("0.5"),
    },
    Param {
        name: "startTime",
        value: ParamValue::Real("1.0"),
    },
];
pub(crate) const SOURCE_SIN: &[BlockCase] = &[case(
    "reals_source_sin",
    "CDL.Reals.Sources.Sin",
    "Sources/Sin",
    &[],
    SOURCE_SIN_PARAMS,
    REAL_Y,
)];

const CALENDAR_OUTPUTS: &[Port] = &[
    Port {
        name: "year",
        kind: I,
    },
    Port {
        name: "month",
        kind: I,
    },
    Port {
        name: "day",
        kind: I,
    },
    Port {
        name: "hour",
        kind: I,
    },
    Port {
        name: "minute",
        kind: R,
    },
    Port {
        name: "weekDay",
        kind: I,
    },
];
const CALENDAR_PARAMS: &[Param] = &[
    Param {
        name: "zerTim",
        value: ParamValue::Integer("11"),
    },
    Param {
        name: "yearRef",
        value: ParamValue::Integer("2016"),
    },
    Param {
        name: "offset",
        value: ParamValue::Real("0.0"),
    },
];
const CALENDAR_ANOMALY_PARAMS: &[Param] = &[
    Param {
        name: "zerTim",
        value: ParamValue::Integer("18"),
    },
    Param {
        name: "yearRef",
        value: ParamValue::Integer("2016"),
    },
    Param {
        name: "offset",
        value: ParamValue::Real("0.0"),
    },
];
pub(crate) const CALENDAR: &[BlockCase] = &[
    case(
        "reals_source_calendar_time",
        "CDL.Reals.Sources.CalendarTime",
        "Sources/CalendarTime",
        &[],
        CALENDAR_PARAMS,
        CALENDAR_OUTPUTS,
    ),
    case(
        "reals_source_calendar_time_source_year_anomaly",
        "CDL.Reals.Sources.CalendarTime",
        "Sources/CalendarTime/source_year_anomaly",
        &[],
        CALENDAR_ANOMALY_PARAMS,
        CALENDAR_OUTPUTS,
    ),
];

const T_AND_PHI: &[Port] = &[
    Port {
        name: "TDryBul",
        kind: R,
    },
    Port {
        name: "phi",
        kind: R,
    },
];
const T_DEW: &[Port] = &[Port {
    name: "TDewPoi",
    kind: R,
}];
const ENTHALPY: &[Port] = &[Port { name: "h", kind: R }];
const T_WET: &[Port] = &[Port {
    name: "TWetBul",
    kind: R,
}];
const CUSTOM_P_ATM: &[Param] = &[Param {
    name: "pAtm",
    value: ParamValue::Real("90000.0"),
}];
pub(crate) const PSYCHROMETRICS: &[BlockCase] = &[
    case(
        "psychrometrics_dew_point_t_dry_bul_phi",
        "CDL.Psychrometrics.DewPoint_TDryBulPhi",
        "DewPoint_TDryBulPhi",
        T_AND_PHI,
        &[],
        T_DEW,
    ),
    case(
        "psychrometrics_specific_enthalpy_t_dry_bul_phi",
        "CDL.Psychrometrics.SpecificEnthalpy_TDryBulPhi",
        "SpecificEnthalpy_TDryBulPhi",
        T_AND_PHI,
        &[],
        ENTHALPY,
    ),
    case(
        "psychrometrics_specific_enthalpy_custom_p_atm_pressure_boundaries",
        "CDL.Psychrometrics.SpecificEnthalpy_TDryBulPhi",
        "SpecificEnthalpy_TDryBulPhi/custom_p_atm_pressure_boundaries",
        T_AND_PHI,
        CUSTOM_P_ATM,
        ENTHALPY,
    ),
    case(
        "psychrometrics_wet_bulb_t_dry_bul_phi",
        "CDL.Psychrometrics.WetBulb_TDryBulPhi",
        "WetBulb_TDryBulPhi",
        T_AND_PHI,
        &[],
        T_WET,
    ),
];

const SUN_OUTPUTS: &[Port] = &[
    Port {
        name: "nextSunRise",
        kind: R,
    },
    Port {
        name: "nextSunSet",
        kind: R,
    },
    Port {
        name: "sunUp",
        kind: B,
    },
];
const DEFAULT_PARAMS: &[Param] = &[
    Param {
        name: "lat",
        value: ParamValue::Real("0.0"),
    },
    Param {
        name: "lon",
        value: ParamValue::Real("0.0"),
    },
    Param {
        name: "timZon",
        value: ParamValue::Real("0.0"),
    },
];
const SAN_FRANCISCO_PARAMS: &[Param] = &[
    Param {
        name: "lat",
        value: ParamValue::Real("0.6457718232379"),
    },
    Param {
        name: "lon",
        value: ParamValue::Real("-2.1293016874331"),
    },
    Param {
        name: "timZon",
        value: ParamValue::Real("-28800.0"),
    },
];
const ARCTIC_PARAMS: &[Param] = &[
    Param {
        name: "lat",
        value: ParamValue::Real("1.2566370614359"),
    },
    Param {
        name: "lon",
        value: ParamValue::Real("-1.2566370614359"),
    },
    Param {
        name: "timZon",
        value: ParamValue::Real("-18000.0"),
    },
];
pub(crate) const SUN: &[BlockCase] = &[
    case(
        "utilities_sun_rise_set_default_equator",
        "CDL.Utilities.SunRiseSet",
        "SunRiseSet",
        &[],
        DEFAULT_PARAMS,
        SUN_OUTPUTS,
    ),
    case(
        "utilities_sun_rise_set_san_francisco",
        "CDL.Utilities.SunRiseSet",
        "SunRiseSet/san_francisco_validation",
        &[],
        SAN_FRANCISCO_PARAMS,
        SUN_OUTPUTS,
    ),
    case(
        "utilities_sun_rise_set_arctic_polar_day",
        "CDL.Utilities.SunRiseSet",
        "SunRiseSet/arctic_polar_day",
        &[],
        ARCTIC_PARAMS,
        SUN_OUTPUTS,
    ),
];
