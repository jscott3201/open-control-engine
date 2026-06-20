use oce_model::ParamTable;

use super::{bool_param, real_param};
use crate::{
    Block, FallingEdge, Latch, LogicalChange, RegistryEntry, Timer, TimerAccumulating, Toggle,
    TrueDelay, TrueFalseHold, TrueHoldWithReset,
};

pub(super) const ENTRIES: &[RegistryEntry] = &[
    RegistryEntry {
        class_path: "CDL.Logical.FallingEdge",
        make: make_falling_edge,
    },
    RegistryEntry {
        class_path: "CDL.Logical.Change",
        make: make_logical_change,
    },
    RegistryEntry {
        class_path: "CDL.Logical.Latch",
        make: make_latch,
    },
    RegistryEntry {
        class_path: "CDL.Logical.Toggle",
        make: make_toggle,
    },
    RegistryEntry {
        class_path: "CDL.Logical.Timer",
        make: make_timer,
    },
    RegistryEntry {
        class_path: "CDL.Logical.TimerAccumulating",
        make: make_timer_accumulating,
    },
    RegistryEntry {
        class_path: "CDL.Logical.TrueDelay",
        make: make_true_delay,
    },
    RegistryEntry {
        class_path: "CDL.Logical.TrueFalseHold",
        make: make_true_false_hold,
    },
    RegistryEntry {
        class_path: "CDL.Logical.TrueHoldWithReset",
        make: make_true_hold_with_reset,
    },
];

fn make_falling_edge(p: &ParamTable) -> Box<dyn Block> {
    Box::new(FallingEdge {
        pre_u_start: bool_param(p, "pre_u_start", false),
    })
}

fn make_logical_change(p: &ParamTable) -> Box<dyn Block> {
    Box::new(LogicalChange {
        pre_u_start: bool_param(p, "pre_u_start", false),
    })
}

fn make_latch(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Latch)
}

fn make_toggle(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Toggle)
}

fn make_timer(p: &ParamTable) -> Box<dyn Block> {
    Box::new(Timer {
        t: real_param(p, "t", 0.0),
    })
}

fn make_timer_accumulating(p: &ParamTable) -> Box<dyn Block> {
    Box::new(TimerAccumulating {
        t: real_param(p, "t", 0.0),
    })
}

fn make_true_delay(p: &ParamTable) -> Box<dyn Block> {
    Box::new(TrueDelay {
        delay_time: real_param(p, "delayTime", 0.0),
        delay_on_init: bool_param(p, "delayOnInit", false),
    })
}

fn make_true_false_hold(p: &ParamTable) -> Box<dyn Block> {
    let true_hold_duration = real_param(p, "trueHoldDuration", 0.0);
    Box::new(TrueFalseHold {
        true_hold_duration,
        false_hold_duration: real_param(p, "falseHoldDuration", true_hold_duration),
    })
}

fn make_true_hold_with_reset(p: &ParamTable) -> Box<dyn Block> {
    Box::new(TrueHoldWithReset {
        duration: real_param(p, "duration", 0.0),
    })
}
