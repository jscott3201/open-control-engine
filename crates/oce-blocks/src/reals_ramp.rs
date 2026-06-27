//! Stateful base `CDL.Reals.Ramp` rate limiter.
//!
//! This is the base Reals block, not `CDL.Reals.Sources.Ramp`. It follows the same discrete
//! approximation convention as `LimitSlewRate`: an implicit one-pole step toward `u`, then a
//! per-tick slew clamp. The `active` Boolean input selects limited output vs same-tick pass-through
//! and reinitializes the internal limited state on a rising edge.

use oce_model::{
    ParamTable, Value,
    determinism::{canonicalize_real, det_max, det_min},
};

use crate::dynamics::{PREV_T_UNSET, first_order_filter_implicit, is_first_tick, tick_dt};
use crate::{Block, BlockKind, BlockSignature, Ctx, PortKind, emit_real, read_bool, read_real};

const CDL_EPS: f64 = 1e-15;
const CDL_SMALL: f64 = 1e-37;

const Y_INTERNAL_WORD: usize = 0;
const PREV_ACTIVE_WORD: usize = 1;
const PREV_T_WORD: usize = 2;
const STATE_LEN: usize = 3;

fn bool_word(value: bool) -> u64 {
    u64::from(value)
}

fn word_bool(word: u64) -> bool {
    word != 0
}

/// `CDL.Reals.Ramp` — rate-limit `u` while `active`, otherwise pass `u` through.
///
/// Parameters are `raisingSlewRate` in units of value/second, `fallingSlewRate`, and derivative
/// time constant `Td` in seconds. Source parameter bounds use `CDL.Constants.small = 1e-37` and
/// `CDL.Constants.eps = 1e-15`; validation rejects out-of-range resolved parameters, while direct
/// construction floors to those source constants to avoid division by zero. The block has current
/// input feedthrough on both `u` and `active`.
#[derive(Clone, Copy, Debug)]
pub struct Ramp {
    /// Maximum rising slew rate.
    pub(crate) raising_slew_rate: f64,
    /// Maximum falling slew rate.
    pub(crate) falling_slew_rate: f64,
    /// Derivative time constant in seconds.
    pub(crate) td: f64,
}

impl Default for Ramp {
    fn default() -> Self {
        let raising_slew_rate = 1.0;
        Self {
            raising_slew_rate,
            falling_slew_rate: -raising_slew_rate,
            td: raising_slew_rate * 0.001,
        }
    }
}

impl Ramp {
    fn rising(self) -> f64 {
        if self.raising_slew_rate >= CDL_SMALL {
            self.raising_slew_rate
        } else {
            CDL_SMALL
        }
    }

    fn falling(self) -> f64 {
        if self.falling_slew_rate <= -CDL_SMALL {
            self.falling_slew_rate
        } else {
            -self.rising()
        }
    }

    fn td_eff(self) -> f64 {
        if self.td >= CDL_EPS { self.td } else { CDL_EPS }
    }

    fn next_internal(self, u: f64, active: bool, region: &[u64], dt: f64) -> f64 {
        debug_assert!(dt >= 0.0, "model time must be monotonic");
        let first = is_first_tick(region[PREV_T_WORD]);
        let active_rising = active && (first || !word_bool(region[PREV_ACTIVE_WORD]));
        if first || active_rising {
            return u;
        }

        let y_internal = f64::from_bits(region[Y_INTERNAL_WORD]);
        if !active {
            return y_internal;
        }

        let filtered = first_order_filter_implicit(y_internal, u, self.td_eff(), dt);
        let dy = det_min(
            det_max(filtered - y_internal, self.falling() * dt),
            self.rising() * dt,
        );
        y_internal + dy
    }

    fn output(self, u: f64, active: bool, region: &[u64], dt: f64) -> f64 {
        if active {
            self.next_internal(u, active, region, dt)
        } else {
            u
        }
    }
}

impl Block for Ramp {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.Ramp",
            inputs: &[PortKind::Real, PortKind::Boolean],
            outputs: &[PortKind::Real],
            stateful: true,
        };
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Stateful
    }

    fn feeds_through(&self, in_idx: usize, out_idx: usize) -> bool {
        out_idx == 0 && (in_idx == 0 || in_idx == 1)
    }

    fn state_len(&self) -> usize {
        STATE_LEN
    }

    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region[Y_INTERNAL_WORD] = 0.0f64.to_bits();
        region[PREV_ACTIVE_WORD] = 0;
        region[PREV_T_WORD] = PREV_T_UNSET;
    }

    fn emit_from_state(
        &self,
        ctx: &Ctx<'_>,
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        let u = read_real(inputs, 0);
        let active = read_bool(inputs, 1);
        let dt = tick_dt(ctx.t(), region[PREV_T_WORD]);
        emit_real(0, self.output(u, active, region, dt), emit);
    }

    fn update_state(&self, ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        let u = read_real(inputs, 0);
        let active = read_bool(inputs, 1);
        let dt = tick_dt(ctx.t(), region[PREV_T_WORD]);
        region[Y_INTERNAL_WORD] =
            canonicalize_real(self.next_internal(u, active, region, dt)).to_bits();
        region[PREV_ACTIVE_WORD] = bool_word(active);
        region[PREV_T_WORD] = ctx.t().to_bits();
    }
}
