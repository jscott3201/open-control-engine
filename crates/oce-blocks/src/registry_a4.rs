//! A4 registry constructors kept out of `registry.rs` so the catalog file stays under the LOC cap.

use oce_model::ParamTable;

use crate::registry::{bool_param, int_param, real_param};
use crate::{
    Block, FallingEdge, IntegerChange, Latch, LogicalChange, OnCounter, Timer, TimerAccumulating,
    Toggle, TrueDelay, TrueFalseHold, TrueHoldWithReset,
};

pub(crate) fn make_falling_edge(p: &ParamTable) -> Box<dyn Block> {
    Box::new(FallingEdge {
        pre_u_start: bool_param(p, "pre_u_start", false),
    })
}

pub(crate) fn make_logical_change(p: &ParamTable) -> Box<dyn Block> {
    Box::new(LogicalChange {
        pre_u_start: bool_param(p, "pre_u_start", false),
    })
}

pub(crate) fn make_latch(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Latch)
}

pub(crate) fn make_toggle(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Toggle)
}

pub(crate) fn make_timer(p: &ParamTable) -> Box<dyn Block> {
    Box::new(Timer {
        t: real_param(p, "t", 0.0),
    })
}

pub(crate) fn make_timer_accumulating(p: &ParamTable) -> Box<dyn Block> {
    Box::new(TimerAccumulating {
        t: real_param(p, "t", 0.0),
    })
}

pub(crate) fn make_true_delay(p: &ParamTable) -> Box<dyn Block> {
    Box::new(TrueDelay {
        delay_time: real_param(p, "delayTime", 0.0),
        delay_on_init: bool_param(p, "delayOnInit", false),
    })
}

pub(crate) fn make_true_false_hold(p: &ParamTable) -> Box<dyn Block> {
    let true_hold_duration = real_param(p, "trueHoldDuration", 0.0);
    Box::new(TrueFalseHold {
        true_hold_duration,
        false_hold_duration: real_param(p, "falseHoldDuration", true_hold_duration),
    })
}

pub(crate) fn make_true_hold_with_reset(p: &ParamTable) -> Box<dyn Block> {
    Box::new(TrueHoldWithReset {
        duration: real_param(p, "duration", 0.0),
    })
}

pub(crate) fn make_integer_on_counter(p: &ParamTable) -> Box<dyn Block> {
    Box::new(OnCounter {
        y_start: int_param(p, "y_start", 0),
    })
}

pub(crate) fn make_integer_change(p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerChange {
        pre_u_start: int_param(p, "pre_u_start", 0),
    })
}
