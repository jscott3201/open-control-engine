use oce_model::{ParamTable, SimpleController};

use super::{bool_param, controller_type_param, real_param};
use crate::pid::ControllerConfig;
use crate::{Block, Pid, PidWithReset, RegistryEntry};

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

fn pid_config(p: &ParamTable) -> ControllerConfig {
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
        xi_start: real_param(p, "xi_start", 0.0),
        yd_start: real_param(p, "yd_start", 0.0),
        reverse_acting: bool_param(p, "reverseActing", true),
    }
}

fn make_pid(p: &ParamTable) -> Box<dyn Block> {
    Box::new(Pid {
        config: pid_config(p),
    })
}

fn make_pid_with_reset(p: &ParamTable) -> Box<dyn Block> {
    Box::new(PidWithReset {
        config: pid_config(p),
    })
}
