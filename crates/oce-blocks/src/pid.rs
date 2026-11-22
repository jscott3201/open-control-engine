//! Limited `CDL.Reals` PID controllers.
//!
//! These blocks use the same tick timing convention as `IntegratorWithReset`: a tick emits from
//! prior controller state (except on a reset frame, below), then `update_state` advances the
//! integrator and derivative filter with `dt = 0` on the first tick and `dt = t(k) - t(k-1)`
//! thereafter. The integrator uses
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
//! `PIDWithReset` resets at the event (upstream `when trigger then reinit(I.y, y_reset - (P + D))`):
//! at a frame `k > 0` where `trigger` is true and was false at frame `k - 1`, the integrator is
//! back-solved from frame `k`'s terms to `xI = y_reset - (yP + yD)`, and frame `k` emits through the
//! normal path, `y = clamp(yP + yD + xI)`, which can differ from `clamp(y_reset)` by an ulp. The
//! frame's update stores that `xI` without an Euler step (the step covers the interval before the
//! event); the derivative filter advances as on any frame. Frame 0 is initialisation: a trigger
//! already true there does not reset, and becomes `pre(trigger)` for frame 1. In the modes with an
//! integrator `trigger` therefore feeds through to `y`, and feedback from `y` into `trigger` is an
//! algebraic loop that a `CDL.Logical.Pre` on the trigger path breaks. This matches the canonical
//! Buildings `PIDWithReset.mo` wiring: `trigger` is the only reset input, while `y_reset` is a
//! parameter that defaults to the resolved `xi_start`.
//!
//! Non-finite input *signals* are deliberately not trapped here: NaN/Inf propagate through the
//! documented recurrence, and runtime non-finite signal diagnostics remain deferred. Non-finite
//! *parameters* are refused engine-wide at load and at `set_param` (issue #362), so the gains,
//! time constants, limits and start values seen here are always finite.

use alloc::string::String;
use oce_model::{ParamTable, SimpleController, Value, determinism::canonicalize_real};

use crate::dynamics::{
    PREV_T_UNSET, first_order_filter_implicit, forward_euler_accumulate,
    is_first_tick as prev_t_is_unset, tick_dt,
};
use crate::{
    Block, BlockKind, BlockSignature, Ctx, EventInputRole, PortKind, emit_real, read_bool,
    read_real,
};

const CDL_EPS: f64 = 1e-15;
/// `PIDWithReset`'s `trigger` input index.
const TRIGGER_PORT: usize = 2;
/// The upstream `min=100*Constants.eps` parameter floor (Buildings `Constants.mo`: `eps=1E-15`),
/// shared with the registry's load-time PID range rules so validation and the runtime
/// defense-in-depth floor agree bit-exactly.
pub(crate) const MIN_PARAM: f64 = 100.0 * CDL_EPS;

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
    pub(crate) y_reset: f64,
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
            y_reset: 0.0,
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
        // Defense-in-depth positive-floor clamp for the effective controller parameters. Unlike
        // `Derivative`'s `positive()` (reals_filters.rs), the operand here is a resolved
        // build-time PARAMETER, never live Real input data: load-time rules floor k/Ti/Td/r/Ni/Nd
        // at this same `MIN_PARAM`, and a value that bypasses validation (direct in-crate
        // construction, unvalidated embedding) is a parsed constant — at worst a quiet NaN,
        // never a computed signaling NaN — so raw `f64::max` cannot hit the sNaN arch-intrinsic
        // divergence and is bit-identical to `det_max` here. The strictly positive floor keeps
        // the signed-zero fixup irrelevant.
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
        // Upstream PID instantiates `CDL.Reals.Limiter lim(final uMax=yMax, final uMin=yMin)`
        // for the output clamp; mirror its comparison chain exactly (see
        // `Limiter::step_algebraic`): a NaN control signal passes through fail-visible instead
        // of being silently absorbed into `y_min`.
        if y_u > self.y_max {
            self.y_max
        } else if y_u < self.y_min {
            self.y_min
        } else {
            y_u
        }
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

/// Whether this frame is a reset event of `PIDWithReset`: the layout has a trigger word (a mode
/// with an integrator), this is not the first frame, and `trigger` is true now and was false at the
/// previous frame. Frame 0 never is: initialisation leaves the upstream `when` inactive.
fn reset_event(inputs: &[Value], region: &[u64], state: StateLayout) -> bool {
    state.prev_trigger.is_some_and(|prev_trigger| {
        !is_first_tick(region, state)
            && read_bool(inputs, TRIGGER_PORT)
            && region[prev_trigger] == 0
    })
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
) -> (f64, f64, f64, f64) {
    let e = config.error(inputs);
    let y_p = config.k_eff() * e;
    let y_d = region
        .filter(|_| config.with_d())
        .map_or(0.0, |r| derivative_output(e, config, r, state));
    let y_pd = y_p + y_d;
    let y_i = match (state.i, region) {
        // The reset frame back-solves the integrator from this frame's `P + D` and emits through
        // the normal path, `P + D + (y_reset - (P + D))` (`addPID` in `PIDWithReset.mo`).
        (Some(_), Some(r)) if reset_event(inputs, r, state) => {
            canonicalize_real(config.y_reset - y_pd)
        }
        (Some(i), Some(r)) => f64::from_bits(r[i]),
        _ => 0.0,
    };
    let y_u = y_pd + y_i;
    let y = config.limit(y_u);
    (e, y_i, y_u, y)
}

fn emit_pid(
    inputs: &[Value],
    config: ControllerConfig,
    region: Option<&[u64]>,
    state: StateLayout,
    emit: &mut dyn FnMut(usize, Value),
) {
    let (_, _, _, y) = current_terms(inputs, config, region, state);
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
    let (e, y_i, y_u, y) = current_terms(inputs, config, Some(region), state);

    if let Some(i) = state.i {
        // On the reset frame `current_terms` already holds the reset integrator in `y_i`; the
        // frame's Euler step covers the interval before the event and is discarded.
        let next_i = if reset_event(inputs, region, state) {
            y_i
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
        region[prev_trigger] = u64::from(read_bool(inputs, TRIGGER_PORT));
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

    fn validate_state(&self, region: &[u64], _state_t: f64, prev_t: f64) -> Result<(), String> {
        let state = layout(self.config, false);
        crate::state_contract::validate_pid(region, prev_t, state.prev_t, None)
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

/// `CDL.Reals.PIDWithReset` — limited PID plus a rising-edge integrator reset. The data path feeds
/// through on `u_s,u_m`. In the PI and PID modes `trigger` feeds through as well: the frame where it
/// rises (after frame 0) emits the reset controller, with the integrator back-solved to
/// `y_reset - (yP + yD)` from that frame's terms. The reset target is the build-time `y_reset`
/// parameter, which follows `PIDWithReset.mo` and defaults to the resolved `xi_start`. In modes
/// without integral action, reset is dormant, no trigger state is allocated, and `trigger` does not
/// feed through (P mode still reads it when it fires, so feedback into it from `y` is a
/// block-granularity loop).
#[derive(Clone, Copy, Debug, Default)]
pub struct PidWithReset {
    pub(crate) config: ControllerConfig,
}

impl Block for PidWithReset {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.PIDWithReset",
            inputs: &[PortKind::Real, PortKind::Real, PortKind::Boolean],
            outputs: &[PortKind::Real],
            stateful: true,
        };
        &SIG
    }

    fn kind(&self) -> BlockKind {
        kind_for(self.config, true)
    }

    fn feeds_through(&self, in_idx: usize, _out_idx: usize) -> bool {
        in_idx < 2 || (in_idx == TRIGGER_PORT && self.config.with_i())
    }

    fn event_input_role(&self, in_idx: usize) -> Option<EventInputRole> {
        (in_idx == TRIGGER_PORT && self.config.with_i()).then_some(EventInputRole::Condition)
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

    fn validate_state(&self, region: &[u64], _state_t: f64, prev_t: f64) -> Result<(), String> {
        let state = layout(self.config, true);
        crate::state_contract::validate_pid(region, prev_t, state.prev_t, state.prev_trigger)
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
