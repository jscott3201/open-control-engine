//! Per-class dispatch for authored parameter defaults.

use crate::{DefaultLiteral as L, DefaultSource as S, ParamDefault};

macro_rules! p {
    ($name:literal, real $value:expr) => {
        ParamDefault {
            name: $name,
            default: S::Literal(L::Real($value)),
        }
    };
    ($name:literal, int $value:expr) => {
        ParamDefault {
            name: $name,
            default: S::Literal(L::Integer($value)),
        }
    };
    ($name:literal, bool $value:expr) => {
        ParamDefault {
            name: $name,
            default: S::Literal(L::Boolean($value)),
        }
    };
    ($name:literal, str $value:expr) => {
        ParamDefault {
            name: $name,
            default: S::Literal(L::Str($value)),
        }
    };
    ($name:literal, enum $value:expr) => {
        ParamDefault {
            name: $name,
            default: S::Literal(L::EnumMember($value)),
        }
    };
    ($name:literal, derived $value:expr) => {
        ParamDefault {
            name: $name,
            default: S::Derived { formula: $value },
        }
    };
    ($name:literal, required) => {
        ParamDefault {
            name: $name,
            default: S::Required,
        }
    };
}

pub(crate) fn param_defaults(class_path: &str) -> &'static [ParamDefault] {
    match class_path {
        "CDL.Conversions.BooleanToInteger" => {
            &[p!("integerTrue", int 1), p!("integerFalse", int 0)]
        }
        "CDL.Conversions.BooleanToReal" => &[p!("realTrue", real 1.0), p!("realFalse", real 0.0)],
        "CDL.Discrete.FirstOrderHold" | "CDL.Discrete.Sampler" | "CDL.Discrete.ZeroOrderHold" => {
            &[p!("samplePeriod", required)]
        }
        "CDL.Discrete.TriggeredMovingMean" => &[p!("n", required)],
        "CDL.Discrete.TriggeredSampler" => &[p!("y_start", real 0.0)],
        "CDL.Discrete.UnitDelay" => &[p!("samplePeriod", required), p!("y_start", real 0.0)],
        "CDL.Integers.Sources.Constant" => &[p!("k", required)],
        "CDL.Integers.Sources.Pulse" => &[
            p!("amplitude", int 1),
            p!("width", real 0.5),
            p!("period", required),
            p!("shift", real 0.0),
            p!("offset", int 0),
        ],
        "CDL.Integers.Sources.TimeTable" => &[p!("period", required)],
        "CDL.Integers.AddParameter" => &[p!("p", required)],
        "CDL.Integers.MultiSum" => &[p!("nin", int 0), p!("k_<i>", int 1)],
        "CDL.Integers.GreaterThreshold"
        | "CDL.Integers.GreaterEqualThreshold"
        | "CDL.Integers.LessThreshold"
        | "CDL.Integers.LessEqualThreshold" => &[p!("t", int 0)],
        "CDL.Integers.OnCounter" => &[p!("y_start", int 0)],
        "CDL.Integers.Change" => &[p!("pre_u_start", int 0)],
        "CDL.Integers.Stage" => &[
            p!("n", required),
            p!("holdDuration", required),
            p!("h", derived "0.02 / n"),
            p!("pre_y_start", int 0),
        ],
        "CDL.Logical.Sources.Constant" => &[p!("k", required)],
        "CDL.Logical.Sources.Pulse" => &[
            p!("width", real 0.5),
            p!("period", required),
            p!("shift", real 0.0),
        ],
        "CDL.Logical.Sources.TimeTable" => &[p!("period", required)],
        "CDL.Logical.MultiAnd" | "CDL.Logical.MultiOr" => &[p!("nin", int 0)],
        "CDL.Logical.Pre" | "CDL.Logical.Edge" => &[p!("pre_u_start", bool false)],
        "CDL.Logical.Sources.SampleTrigger" => &[p!("period", required), p!("shift", real 0.0)],
        "CDL.Logical.Proof" => &[p!("debounce", required), p!("feedbackDelay", required)],
        "CDL.Logical.VariablePulse" => &[
            p!("period", required),
            p!("deltaU", real 0.01),
            p!("minTruFalHol", derived "0.01 * period"),
        ],
        "CDL.Logical.FallingEdge" | "CDL.Logical.Change" => &[p!("pre_u_start", bool false)],
        "CDL.Logical.Timer" | "CDL.Logical.TimerAccumulating" => &[p!("t", real 0.0)],
        "CDL.Logical.TrueDelay" => &[p!("delayTime", required), p!("delayOnInit", bool false)],
        "CDL.Logical.TrueFalseHold" => &[
            p!("trueHoldDuration", required),
            p!("falseHoldDuration", derived "trueHoldDuration"),
        ],
        "CDL.Logical.TrueHoldWithReset" => &[p!("duration", real 0.0)],
        "CDL.Reals.PID" => pid_defaults(false),
        "CDL.Reals.PIDWithReset" => pid_defaults(true),
        "CDL.Psychrometrics.SpecificEnthalpy_TDryBulPhi" => &[p!("pAtm", real 101_325.0)],
        "CDL.Reals.Sources.Constant" => &[p!("k", required)],
        "CDL.Reals.Sources.Pulse" => &[
            p!("amplitude", real 1.0),
            p!("width", real 0.5),
            p!("period", required),
            p!("shift", real 0.0),
            p!("offset", real 0.0),
        ],
        "CDL.Reals.Sources.Ramp" => &[
            p!("height", real 1.0),
            p!("duration", required),
            p!("offset", real 0.0),
            p!("startTime", real 0.0),
        ],
        "CDL.Reals.Sources.Sin" => &[
            p!("amplitude", real 1.0),
            p!("freqHz", required),
            p!("phase", real 0.0),
            p!("offset", real 0.0),
            p!("startTime", real 0.0),
        ],
        "CDL.Reals.Sources.CalendarTime" => &[
            p!("zerTim", required),
            p!("yearRef", int 2016),
            p!("offset", real 0.0),
        ],
        "CDL.Reals.Round" => &[p!("n", required)],
        "CDL.Reals.AddParameter" => &[p!("p", required)],
        "CDL.Reals.MultiplyByParameter" => &[p!("k", required)],
        "CDL.Reals.MultiMax" | "CDL.Reals.MultiMin" => &[p!("nin", int 0)],
        "CDL.Reals.MultiSum" => &[p!("nin", int 0), p!("k_<i>", real 1.0)],
        "CDL.Reals.MatrixGain" => &[
            p!("nout", int 2),
            p!("nin", int 2),
            p!("K_<row>_<col>", derived "1.0 if row == col else 0.0"),
        ],
        "CDL.Reals.MatrixMax" => &[
            p!("nRow", required),
            p!("nCol", required),
            p!("rowMax", bool true),
        ],
        "CDL.Reals.MatrixMin" => &[
            p!("nRow", required),
            p!("nCol", required),
            p!("rowMin", bool true),
        ],
        "CDL.Reals.Sort" => &[p!("nin", int 0), p!("ascending", bool true)],
        "CDL.Reals.Limiter" => &[p!("uMin", required), p!("uMax", required)],
        "CDL.Reals.Line" => &[p!("limitBelow", bool true), p!("limitAbove", bool true)],
        "CDL.Reals.Greater" | "CDL.Reals.Less" => {
            &[p!("h", real 0.0), p!("pre_y_start", bool false)]
        }
        "CDL.Reals.GreaterThreshold" | "CDL.Reals.LessThreshold" => &[
            p!("t", real 0.0),
            p!("h", real 0.0),
            p!("pre_y_start", bool false),
        ],
        "CDL.Reals.Hysteresis" => &[
            p!("uLow", required),
            p!("uHigh", required),
            p!("pre_y_start", bool false),
        ],
        "CDL.Reals.Derivative" => &[p!("y_start", real 0.0)],
        "CDL.Reals.LimitSlewRate" => &[
            p!("raisingSlewRate", required),
            p!("fallingSlewRate", derived "-raisingSlewRate"),
            p!("Td", derived "raisingSlewRate * 10.0"),
            p!("enable", bool true),
        ],
        "CDL.Reals.MovingAverage" => &[p!("delta", required)],
        "CDL.Reals.IntegratorWithReset" => &[p!("k", real 1.0), p!("y_start", real 0.0)],
        "CDL.Reals.Ramp" => &[
            p!("raisingSlewRate", required),
            p!("fallingSlewRate", derived "-raisingSlewRate"),
            p!("Td", derived "raisingSlewRate * 0.001"),
        ],
        "CDL.Utilities.Assert" => &[p!("message", required)],
        "CDL.Utilities.SunRiseSet" => &[
            p!("lat", required),
            p!("lon", required),
            p!("timZon", required),
        ],
        path if path.starts_with("CDL.Routing.") => routing_defaults(path),
        _ => &[],
    }
}

const PID: &[ParamDefault] = &[
    p!("controllerType", enum "PI"),
    p!("k", real 1.0),
    p!("Ti", real 0.5),
    p!("Td", real 0.1),
    p!("r", real 1.0),
    p!("yMax", real 1.0),
    p!("yMin", real 0.0),
    p!("Ni", real 0.9),
    p!("Nd", real 10.0),
    p!("xi_start", real 0.0),
    p!("yd_start", real 0.0),
    p!("reverseActing", bool true),
];
const PID_WITH_RESET: &[ParamDefault] = &[
    p!("controllerType", enum "PI"),
    p!("k", real 1.0),
    p!("Ti", real 0.5),
    p!("Td", real 0.1),
    p!("r", real 1.0),
    p!("yMax", real 1.0),
    p!("yMin", real 0.0),
    p!("Ni", real 0.9),
    p!("Nd", real 10.0),
    p!("xi_start", real 0.0),
    p!("yd_start", real 0.0),
    p!("reverseActing", bool true),
    p!("y_reset", derived "xi_start"),
];

const fn pid_defaults(with_reset: bool) -> &'static [ParamDefault] {
    if with_reset { PID_WITH_RESET } else { PID }
}

fn routing_defaults(path: &str) -> &'static [ParamDefault] {
    if path.ends_with("ExtractSignal") {
        &[
            p!("nin", int 1),
            p!("nout", int 1),
            p!("extract_<i>", derived "i"),
        ]
    } else if path.ends_with("Extractor") {
        &[p!("nin", int 1)]
    } else if path.ends_with("ScalarReplicator") {
        &[p!("nout", int 1)]
    } else if path.ends_with("VectorFilter") {
        &[
            p!("nin", required),
            p!("nout", required),
            p!("msk_<i>", bool true),
        ]
    } else if path.ends_with("VectorReplicator") {
        &[p!("nin", int 1), p!("nout", int 1)]
    } else {
        &[]
    }
}
