use oce_model::{ParamTable, SimpleController};

use super::{bool_param, controller_type_param, real_param};
use crate::pid::{ControllerConfig, MIN_PARAM};
use crate::{Block, ParamDefault, ParamRule, Pid, PidWithReset, RegistryEntry};

const CONTROLLER_TYPE_DEFAULT: &str = "PI";
const K_DEFAULT: f64 = 1.0;
const TI_DEFAULT: f64 = 0.5;
const TD_DEFAULT: f64 = 0.1;
const R_DEFAULT: f64 = 1.0;
const Y_MAX_DEFAULT: f64 = 1.0;
const Y_MIN_DEFAULT: f64 = 0.0;
const NI_DEFAULT: f64 = 0.9;
const ND_DEFAULT: f64 = 10.0;
const XI_START_DEFAULT: f64 = 0.0;
const YD_START_DEFAULT: f64 = 0.0;
const REVERSE_ACTING_DEFAULT: bool = true;

pub(super) const PID_PARAM_DEFAULTS: &[ParamDefault] = &[
    param_default_enum!("controllerType", CONTROLLER_TYPE_DEFAULT),
    param_default_real!("k", K_DEFAULT),
    param_default_real!("Ti", TI_DEFAULT),
    param_default_real!("Td", TD_DEFAULT),
    param_default_real!("r", R_DEFAULT),
    param_default_real!("yMax", Y_MAX_DEFAULT),
    param_default_real!("yMin", Y_MIN_DEFAULT),
    param_default_real!("Ni", NI_DEFAULT),
    param_default_real!("Nd", ND_DEFAULT),
    param_default_real!("xi_start", XI_START_DEFAULT),
    param_default_real!("yd_start", YD_START_DEFAULT),
    param_default_boolean!("reverseActing", REVERSE_ACTING_DEFAULT),
];
pub(super) const PID_WITH_RESET_PARAM_DEFAULTS: &[ParamDefault] = &[
    param_default_enum!("controllerType", CONTROLLER_TYPE_DEFAULT),
    param_default_real!("k", K_DEFAULT),
    param_default_real!("Ti", TI_DEFAULT),
    param_default_real!("Td", TD_DEFAULT),
    param_default_real!("r", R_DEFAULT),
    param_default_real!("yMax", Y_MAX_DEFAULT),
    param_default_real!("yMin", Y_MIN_DEFAULT),
    param_default_real!("Ni", NI_DEFAULT),
    param_default_real!("Nd", ND_DEFAULT),
    param_default_real!("xi_start", XI_START_DEFAULT),
    param_default_real!("yd_start", YD_START_DEFAULT),
    param_default_boolean!("reverseActing", REVERSE_ACTING_DEFAULT),
    param_default_derived!("y_reset", "xi_start"),
];

pub(super) const ENTRIES: &[RegistryEntry] = &[
    RegistryEntry {
        class_path: "CDL.Reals.PID",
        make: make_pid,
    },
    RegistryEntry {
        class_path: "CDL.Reals.PIDWithReset",
        make: make_pid_with_reset,
    },
];

/// Upstream `PID.mo`/`PIDWithReset.mo` (pin `a131864`) annotate `min=100*Constants.eps` on
/// exactly {k, Ti, Td, r, Ni, Nd} — an INCLUSIVE Modelica lower bound, hence
/// `RealGreaterOrEqual` at the shared [`MIN_PARAM`] floor (the same constant the runtime
/// defense-in-depth clamp uses). Upstream constrains the yMin/yMax pair to `yMin < yMax` two ways:
/// directly, via `cheYMinMax(final k=yMin < yMax)` wired into `assMesYMinMax` ("LimPID: Limits
/// must be yMin < yMax"), and transitively, through the instantiated
/// `Limiter lim(final uMax=yMax, final uMin=yMin)` and its `assert(uMin < uMax)`. The engine mirrors
/// its own Limiter precedent (error on inversion, warning on equality). yMin/yMax themselves have
/// upstream defaults (`yMax=1`, `yMin=0`) so they stay optional; xi_start/yd_start/y_reset are
/// unconstrained upstream and stay unruled.
pub(super) const PID_PARAM_RULES: &[ParamRule] = &[
    ParamRule::RealGreaterOrEqual {
        name: "k",
        min: MIN_PARAM,
    },
    ParamRule::RealGreaterOrEqual {
        name: "Ti",
        min: MIN_PARAM,
    },
    ParamRule::RealGreaterOrEqual {
        name: "Td",
        min: MIN_PARAM,
    },
    ParamRule::RealGreaterOrEqual {
        name: "r",
        min: MIN_PARAM,
    },
    ParamRule::RealGreaterOrEqual {
        name: "Ni",
        min: MIN_PARAM,
    },
    ParamRule::RealGreaterOrEqual {
        name: "Nd",
        min: MIN_PARAM,
    },
    ParamRule::RealLessOrEqual {
        lower: "yMin",
        upper: "yMax",
    },
    ParamRule::RealEqualWarning {
        left: "yMin",
        right: "yMax",
    },
];

/// Identical annotation set upstream (`PIDWithReset.mo` is bit-for-bit the same on these
/// params); `y_reset` is unconstrained upstream and stays unruled.
pub(super) const PID_WITH_RESET_PARAM_RULES: &[ParamRule] = &[
    ParamRule::RealGreaterOrEqual {
        name: "k",
        min: MIN_PARAM,
    },
    ParamRule::RealGreaterOrEqual {
        name: "Ti",
        min: MIN_PARAM,
    },
    ParamRule::RealGreaterOrEqual {
        name: "Td",
        min: MIN_PARAM,
    },
    ParamRule::RealGreaterOrEqual {
        name: "r",
        min: MIN_PARAM,
    },
    ParamRule::RealGreaterOrEqual {
        name: "Ni",
        min: MIN_PARAM,
    },
    ParamRule::RealGreaterOrEqual {
        name: "Nd",
        min: MIN_PARAM,
    },
    ParamRule::RealLessOrEqual {
        lower: "yMin",
        upper: "yMax",
    },
    ParamRule::RealEqualWarning {
        left: "yMin",
        right: "yMax",
    },
];

fn pid_config(p: &ParamTable) -> ControllerConfig {
    let xi_start = real_param(p, "xi_start", XI_START_DEFAULT);
    ControllerConfig {
        controller_type: controller_type_param(
            p,
            "controllerType",
            simple_controller_member(CONTROLLER_TYPE_DEFAULT),
        ),
        k: real_param(p, "k", K_DEFAULT),
        ti: real_param(p, "Ti", TI_DEFAULT),
        td: real_param(p, "Td", TD_DEFAULT),
        r: real_param(p, "r", R_DEFAULT),
        y_max: real_param(p, "yMax", Y_MAX_DEFAULT),
        y_min: real_param(p, "yMin", Y_MIN_DEFAULT),
        ni: real_param(p, "Ni", NI_DEFAULT),
        nd: real_param(p, "Nd", ND_DEFAULT),
        xi_start,
        yd_start: real_param(p, "yd_start", YD_START_DEFAULT),
        y_reset: 0.0,
        reverse_acting: bool_param(p, "reverseActing", REVERSE_ACTING_DEFAULT),
    }
}

fn simple_controller_member(member: &str) -> SimpleController {
    match member {
        "P" => SimpleController::P,
        "PI" => SimpleController::Pi,
        "PD" => SimpleController::Pd,
        "PID" => SimpleController::Pid,
        _ => unreachable!("registry-owned controller defaults are valid CDL member tokens"),
    }
}

fn make_pid(p: &ParamTable) -> Box<dyn Block> {
    Box::new(Pid {
        config: pid_config(p),
    })
}

fn make_pid_with_reset(p: &ParamTable) -> Box<dyn Block> {
    let mut config = pid_config(p);
    config.y_reset = real_param(p, "y_reset", config.xi_start);
    Box::new(PidWithReset { config })
}
