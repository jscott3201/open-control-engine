use oce_model::{ParamTable, SimpleController};

use super::{bool_param, controller_type_param, real_param};
use crate::pid::{ControllerConfig, MIN_PARAM};
use crate::{Block, ParamRule, Pid, PidWithReset, RegistryEntry};

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
/// defense-in-depth clamp uses). The yMin/yMax pair is constrained upstream only transitively,
/// through the instantiated `Limiter lim(final uMax=yMax, final uMin=yMin)` and its
/// `assert(uMin < uMax)`; the engine mirrors its own Limiter precedent (error on inversion,
/// warning on equality). xi_start/yd_start/y_reset are unconstrained upstream and stay unruled.
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
    let xi_start = real_param(p, "xi_start", 0.0);
    ControllerConfig {
        controller_type: controller_type_param(p, "controllerType", SimpleController::Pi),
        k: real_param(p, "k", 1.0),
        ti: real_param(p, "Ti", 0.5),
        td: real_param(p, "Td", 0.1),
        r: real_param(p, "r", 1.0),
        y_max: real_param(p, "yMax", 1.0),
        y_min: real_param(p, "yMin", 0.0),
        ni: real_param(p, "Ni", 0.9),
        nd: real_param(p, "Nd", 10.0),
        xi_start,
        yd_start: real_param(p, "yd_start", 0.0),
        y_reset: 0.0,
        reverse_acting: bool_param(p, "reverseActing", true),
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
