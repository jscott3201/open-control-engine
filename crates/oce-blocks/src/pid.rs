//! Limited `CDL.Reals` PID controllers.
//!
//! These blocks use the same tick timing convention as `IntegratorWithReset`: a tick emits from
//! prior controller state, then `update_state` advances the integrator and derivative filter with
//! `dt = 0` on the first tick and `dt = t(k) - t(k-1)` thereafter. The integrator uses
//! forward-Euler accumulation; the first-order derivative filter uses implicit Euler for stable
//! decay at large `dt/(Td/Nd)`. The limited PID recurrence is the Buildings parallel form:
//!
//! `e = revAct * (u_s - u_m) / r`
//!
//! `yP = k * e`
//!
//! `yD = (k*Td)/(Td/Nd) * (e - xD)` when D is enabled
//!
//! `y_u = yP + yD + xI`, `y = clamp(y_u, yMin, yMax)`
//!
//! `antWinGai = (y_u - y) / (k*Ni)`, `errI2 = e - antWinGai`
//!
//! `xI(k+1) = xI(k) + (k/Ti) * errI2 * dt`
//!
//! `xD(k+1) = (xD(k) + (dt/(Td/Nd))*e) / (1 + dt/(Td/Nd))`
//!
//! `PIDWithReset` applies reset during `update_state`: on a rising trigger, the integrator state is
//! back-solved to `y_reset_in - (yP + yD)`, so the reset target is visible at the next emit when the
//! current P/D path is unchanged; with derivative enabled, the next emit also reflects the updated
//! derivative filter state. This matches the arena loop-cut timing used by reset/integrator blocks;
//! `trigger` and `y_reset_in` do not feed through.
//!
//! Non-finite inputs are deliberately not trapped here yet: NaN/Inf propagate through the
//! documented recurrence and join the deferred centralized non-finite validation/diagnostic seam.

use oce_model::{
    ParamTable, SimpleController, Value,
    determinism::{canonicalize_real, det_max, det_min},
};

use crate::dynamics::{
    PREV_T_UNSET, first_order_filter_implicit, forward_euler_accumulate,
    is_first_tick as prev_t_is_unset, tick_dt,
};
use crate::{Block, BlockKind, BlockSignature, Ctx, PortKind, emit_real, read_bool, read_real};

const CDL_EPS: f64 = 1e-15;
const MIN_PARAM: f64 = 100.0 * CDL_EPS;

#[derive(Clone, Copy, Debug)]
pub(crate) struct ControllerConfig {
    pub(crate) controller_type: SimpleController,
    pub(crate) k: f64,
    pub(crate) ti: f64,
    pub(crate) td: f64,
    pub(crate) r: f64,
    pub(crate) y_max: f64,
    pub(crate) y_min: f64,
    pub(crate) ni: f64,
    pub(crate) nd: f64,
    pub(crate) xi_start: f64,
    pub(crate) yd_start: f64,
    pub(crate) reverse_acting: bool,
}

impl Default for ControllerConfig {
    fn default() -> Self {
        Self {
            controller_type: SimpleController::Pi,
            k: 1.0,
            ti: 0.5,
            td: 0.1,
            r: 1.0,
            y_max: 1.0,
            y_min: 0.0,
            ni: 0.9,
            nd: 10.0,
            xi_start: 0.0,
            yd_start: 0.0,
            reverse_acting: true,
        }
    }
}

impl ControllerConfig {
    fn with_i(self) -> bool {
        matches!(
            self.controller_type,
            SimpleController::Pi | SimpleController::Pid
        )
    }

    fn with_d(self) -> bool {
        matches!(
            self.controller_type,
            SimpleController::Pd | SimpleController::Pid
        )
    }

    fn guarded_positive(x: f64) -> f64 {
        // Internal positive-floor clamp: the effective parameter is always > 0 and is not a
        // signed-zero/NaN-bearing Real output, so raw max is deterministic enough here.
        x.max(MIN_PARAM)
    }

    fn k_eff(self) -> f64 {
        Self::guarded_positive(self.k)
    }

    fn ti_eff(self) -> f64 {
        Self::guarded_positive(self.ti)
    }

    fn td_eff(self) -> f64 {
        Self::guarded_positive(self.td)
    }

    fn r_eff(self) -> f64 {
        Self::guarded_positive(self.r)
    }

    fn ni_eff(self) -> f64 {
        Self::guarded_positive(self.ni)
    }

    fn nd_eff(self) -> f64 {
        Self::guarded_positive(self.nd)
    }

    fn error(self, inputs: &[Value]) -> f64 {
        let rev_act = if self.reverse_acting { 1.0 } else { -1.0 };
        rev_act * (read_real(inputs, 0) - read_real(inputs, 1)) / self.r_eff()
    }

    fn limit(self, y_u: f64) -> f64 {
        det_min(det_max(y_u, self.y_min), self.y_max)
    }

    fn derivative_gain(self) -> f64 {
        self.k_eff() * self.td_eff()
    }

    fn derivative_time(self) -> f64 {
        self.td_eff() / self.nd_eff()
    }
}

#[derive(Clone, Copy, Debug)]
struct StateLayout {
    i: Option<usize>,
    prev_t: Option<usize>,
    d: Option<usize>,
    prev_trigger: Option<usize>,
    len: usize,
}

fn layout(config: ControllerConfig, with_reset: bool) -> StateLayout {
    let mut next = 0usize;
    let i = config.with_i().then(|| {
        let slot = next;
        next += 1;
        slot
    });
    let prev_t = (config.with_i() || config.with_d()).then(|| {
        let slot = next;
        next += 1;
        slot
    });
    let d = config.with_d().then(|| {
        let slot = next;
        next += 1;
        slot
    });
    let prev_trigger = (with_reset && config.with_i()).then(|| {
        let slot = next;
        next += 1;
        slot
    });
    StateLayout {
        i,
        prev_t,
        d,
        prev_trigger,
        len: next,
    }
}

fn kind_for(config: ControllerConfig, with_reset: bool) -> BlockKind {
    if layout(config, with_reset).len == 0 {
        BlockKind::Algebraic
    } else {
        BlockKind::Stateful
    }
}

fn is_first_tick(region: &[u64], state: StateLayout) -> bool {
    state
        .prev_t
        .is_some_and(|prev_t| prev_t_is_unset(region[prev_t]))
}

fn dt(ctx: &Ctx<'_>, region: &[u64], state: StateLayout) -> f64 {
    match state.prev_t {
        Some(prev_t) => tick_dt(ctx.t(), region[prev_t]),
        _ => 0.0,
    }
}

fn derivative_x_for_start(e: f64, config: ControllerConfig) -> f64 {
    let gain = config.derivative_gain();
    if gain.abs() < CDL_EPS {
        e
    } else {
        e - config.derivative_time() * config.yd_start / gain
    }
}

fn derivative_output(e: f64, config: ControllerConfig, region: &[u64], state: StateLayout) -> f64 {
    let Some(d) = state.d else {
        return 0.0;
    };
    if is_first_tick(region, state) {
        config.yd_start
    } else {
        let x = f64::from_bits(region[d]);
        (config.derivative_gain() / config.derivative_time()) * (e - x)
    }
}

fn current_terms(
    inputs: &[Value],
    config: ControllerConfig,
    region: Option<&[u64]>,
    state: StateLayout,
) -> (f64, f64, f64, f64, f64, f64) {
    let e = config.error(inputs);
    let y_p = config.k_eff() * e;
    let y_d = region
        .filter(|_| config.with_d())
        .map_or(0.0, |r| derivative_output(e, config, r, state));
    let y_i = match (state.i, region) {
        (Some(i), Some(r)) => f64::from_bits(r[i]),
        _ => 0.0,
    };
    let y_pd = y_p + y_d;
    let y_u = y_pd + y_i;
    let y = config.limit(y_u);
    (e, y_p, y_d, y_i, y_u, y)
}

fn emit_pid(
    inputs: &[Value],
    config: ControllerConfig,
    region: Option<&[u64]>,
    state: StateLayout,
    emit: &mut dyn FnMut(usize, Value),
) {
    let (_, _, _, _, _, y) = current_terms(inputs, config, region, state);
    emit_real(0, y, emit);
}

fn update_pid_state(
    ctx: &Ctx<'_>,
    inputs: &[Value],
    config: ControllerConfig,
    with_reset: bool,
    region: &mut [u64],
) {
    let state = layout(config, with_reset);
    let first_tick = is_first_tick(region, state);
    let step_dt = dt(ctx, region, state);
    let (e, y_p, y_d, y_i, y_u, y) = current_terms(inputs, config, Some(region), state);

    if let Some(i) = state.i {
        let rising_reset = state
            .prev_trigger
            .is_some_and(|prev_trigger| read_bool(inputs, 2) && region[prev_trigger] == 0);
        let next_i = if rising_reset {
            let reset_target = read_real(inputs, 3);
            reset_target - (y_p + y_d)
        } else {
            let ant_win_gai = (y_u - y) / (config.k_eff() * config.ni_eff());
            let err_i2 = e - ant_win_gai;
            forward_euler_accumulate(y_i, (config.k_eff() / config.ti_eff()) * err_i2, step_dt)
        };
        region[i] = canonicalize_real(next_i).to_bits();
    }

    if let Some(d) = state.d {
        let x = if first_tick {
            derivative_x_for_start(e, config)
        } else {
            f64::from_bits(region[d])
        };
        let next_d = first_order_filter_implicit(x, e, config.derivative_time(), step_dt);
        region[d] = canonicalize_real(next_d).to_bits();
    }

    if let Some(prev_t) = state.prev_t {
        region[prev_t] = ctx.t().to_bits();
    }
    if let Some(prev_trigger) = state.prev_trigger {
        region[prev_trigger] = u64::from(read_bool(inputs, 2));
    }
}

/// `CDL.Reals.PID` — limited P/PI/PD/PID controller with Buildings-style back-calculation
/// anti-windup. Param-dependent `[A]`/`[S]`: P mode has no state; PI/PD/PID allocate only the
/// active term state. Feedthrough is always `y <- {u_s, u_m}`; PID is never a loop cut.
#[derive(Clone, Copy, Debug, Default)]
pub struct Pid {
    pub(crate) config: ControllerConfig,
}

impl Block for Pid {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.PID",
            inputs: &[PortKind::Real, PortKind::Real],
            outputs: &[PortKind::Real],
            stateful: true,
        };
        &SIG
    }

    fn kind(&self) -> BlockKind {
        kind_for(self.config, false)
    }

    fn feeds_through(&self, in_idx: usize, _out_idx: usize) -> bool {
        in_idx < 2
    }

    fn state_len(&self) -> usize {
        layout(self.config, false).len
    }

    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        let state = layout(self.config, false);
        if let Some(i) = state.i {
            region[i] = self.config.xi_start.to_bits();
        }
        if let Some(prev_t) = state.prev_t {
            region[prev_t] = PREV_T_UNSET;
        }
        if let Some(d) = state.d {
            region[d] = 0.0f64.to_bits();
        }
    }

    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit_pid(inputs, self.config, None, layout(self.config, false), emit);
    }

    fn emit_from_state(
        &self,
        _ctx: &Ctx<'_>,
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        emit_pid(
            inputs,
            self.config,
            Some(region),
            layout(self.config, false),
            emit,
        );
    }

    fn update_state(&self, ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        update_pid_state(ctx, inputs, self.config, false, region);
    }
}

/// `CDL.Reals.PIDWithReset` — limited PID plus a rising-edge integrator reset. The data path
/// remains feedthrough on `u_s,u_m`; `trigger` and `y_reset_in` are state-routed and never
/// feed through. In modes without integral action, reset is dormant and no trigger state is
/// allocated.
#[derive(Clone, Copy, Debug, Default)]
pub struct PidWithReset {
    pub(crate) config: ControllerConfig,
}

impl Block for PidWithReset {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.PIDWithReset",
            inputs: &[
                PortKind::Real,
                PortKind::Real,
                PortKind::Boolean,
                PortKind::Real,
            ],
            outputs: &[PortKind::Real],
            stateful: true,
        };
        &SIG
    }

    fn kind(&self) -> BlockKind {
        kind_for(self.config, true)
    }

    fn feeds_through(&self, in_idx: usize, _out_idx: usize) -> bool {
        in_idx < 2
    }

    fn state_len(&self) -> usize {
        layout(self.config, true).len
    }

    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        let state = layout(self.config, true);
        if let Some(i) = state.i {
            region[i] = self.config.xi_start.to_bits();
        }
        if let Some(prev_t) = state.prev_t {
            region[prev_t] = PREV_T_UNSET;
        }
        if let Some(d) = state.d {
            region[d] = 0.0f64.to_bits();
        }
        if let Some(prev_trigger) = state.prev_trigger {
            region[prev_trigger] = 0;
        }
    }

    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit_pid(inputs, self.config, None, layout(self.config, true), emit);
    }

    fn emit_from_state(
        &self,
        _ctx: &Ctx<'_>,
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        emit_pid(
            inputs,
            self.config,
            Some(region),
            layout(self.config, true),
            emit,
        );
    }

    fn update_state(&self, ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        update_pid_state(ctx, inputs, self.config, true, region);
    }
}
