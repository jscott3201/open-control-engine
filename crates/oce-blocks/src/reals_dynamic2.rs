//! Remaining scalar dynamic `CDL.Reals` blocks.
//!
//! These blocks are stateful and feed through on their current `u` input. The tick output is
//! computed from prior state plus the current input; `update_state` stores the same next state so
//! no block in this module is a feedback loop cut.

use oce_model::{ParamTable, Value};

use crate::dynamics::{
    PREV_T_UNSET, first_order_filter_implicit, forward_euler_accumulate, is_first_tick, tick_dt,
};
use crate::{Block, BlockKind, BlockSignature, Ctx, PortKind, read_real};

const EPS: f64 = 1e-15;
const MIN_PARAM: f64 = 100.0 * EPS;
const MIN_DELTA: f64 = 1e-5;

const X_WORD: usize = 0;
const PREV_T_WORD: usize = 1;
const TWO_WORD_STATE: usize = 2;

fn positive(x: f64) -> f64 {
    x.max(MIN_PARAM)
}

fn derivative_x_for_start(u: f64, k: f64, t: f64, y_start: f64) -> f64 {
    if k.abs() < EPS {
        u
    } else {
        u - t * y_start / k
    }
}

/// `CDL.Reals.Derivative` — first-order filtered derivative:
/// `y = k*(u-x)/T`, `x'=(u-x)/T`, discretized with the shared implicit Euler filter.
/// `[S]`, feedthrough `y <- {u}`, not a loop cut.
#[derive(Clone, Copy, Debug)]
pub struct Derivative {
    pub(crate) k: f64,
    pub(crate) t: f64,
    pub(crate) y_start: f64,
}

impl Default for Derivative {
    fn default() -> Self {
        Self {
            k: 1.0,
            t: 0.1,
            y_start: 0.0,
        }
    }
}

impl Derivative {
    fn t_eff(self) -> f64 {
        positive(self.t)
    }

    fn output(self, u: f64, region: &[u64]) -> f64 {
        if is_first_tick(region[PREV_T_WORD]) {
            self.y_start
        } else {
            let x = f64::from_bits(region[X_WORD]);
            self.k * (u - x) / self.t_eff()
        }
    }

    fn next_x(self, u: f64, region: &[u64], dt: f64) -> f64 {
        let x = if is_first_tick(region[PREV_T_WORD]) {
            derivative_x_for_start(u, self.k, self.t_eff(), self.y_start)
        } else {
            f64::from_bits(region[X_WORD])
        };
        first_order_filter_implicit(x, u, self.t_eff(), dt)
    }
}

impl Block for Derivative {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.Derivative",
            inputs: &[PortKind::Real],
            outputs: &[PortKind::Real],
            stateful: true,
        };
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Stateful
    }

    fn feeds_through(&self, in_idx: usize, _out_idx: usize) -> bool {
        in_idx == 0
    }

    fn state_len(&self) -> usize {
        TWO_WORD_STATE
    }

    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region[X_WORD] = 0.0f64.to_bits();
        region[PREV_T_WORD] = PREV_T_UNSET;
    }

    fn emit_from_state(
        &self,
        _ctx: &Ctx<'_>,
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        emit(0, Value::Real(self.output(read_real(inputs, 0), region)));
    }

    fn update_state(&self, ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        let u = read_real(inputs, 0);
        let dt = tick_dt(ctx.t(), region[PREV_T_WORD]);
        region[X_WORD] = self.next_x(u, region, dt).to_bits();
        region[PREV_T_WORD] = ctx.t().to_bits();
    }
}

/// `CDL.Reals.LimitSlewRate` — first-order lag toward `u`, with per-tick `dy` clamped to
/// `[fallingSlewRate*dt, raisingSlewRate*dt]`. `[S]`, feedthrough `y <- {u}`, not a loop cut.
#[derive(Clone, Copy, Debug)]
pub struct LimitSlewRate {
    pub(crate) raising_slew_rate: f64,
    pub(crate) falling_slew_rate: f64,
    pub(crate) td: f64,
    pub(crate) enable: bool,
}

impl Default for LimitSlewRate {
    fn default() -> Self {
        let raising_slew_rate = 1.0;
        Self {
            raising_slew_rate,
            falling_slew_rate: -raising_slew_rate,
            td: raising_slew_rate * 10.0,
            enable: true,
        }
    }
}

impl LimitSlewRate {
    fn rising(self) -> f64 {
        positive(self.raising_slew_rate)
    }

    fn falling(self) -> f64 {
        if self.falling_slew_rate < 0.0 {
            self.falling_slew_rate
        } else {
            -self.rising()
        }
    }

    fn td_eff(self) -> f64 {
        positive(self.td)
    }

    fn next_y(self, u: f64, region: &[u64], dt: f64) -> f64 {
        if !self.enable || is_first_tick(region[PREV_T_WORD]) {
            return u;
        }
        let y = f64::from_bits(region[X_WORD]);
        let filtered = first_order_filter_implicit(y, u, self.td_eff(), dt);
        let dy = (filtered - y).clamp(self.falling() * dt, self.rising() * dt);
        y + dy
    }
}

impl Block for LimitSlewRate {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.LimitSlewRate",
            inputs: &[PortKind::Real],
            outputs: &[PortKind::Real],
            stateful: true,
        };
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Stateful
    }

    fn feeds_through(&self, in_idx: usize, _out_idx: usize) -> bool {
        in_idx == 0
    }

    fn state_len(&self) -> usize {
        TWO_WORD_STATE
    }

    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region[X_WORD] = 0.0f64.to_bits();
        region[PREV_T_WORD] = PREV_T_UNSET;
    }

    fn emit_from_state(
        &self,
        ctx: &Ctx<'_>,
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        let dt = tick_dt(ctx.t(), region[PREV_T_WORD]);
        emit(
            0,
            Value::Real(self.next_y(read_real(inputs, 0), region, dt)),
        );
    }

    fn update_state(&self, ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        let u = read_real(inputs, 0);
        let dt = tick_dt(ctx.t(), region[PREV_T_WORD]);
        region[X_WORD] = self.next_y(u, region, dt).to_bits();
        region[PREV_T_WORD] = ctx.t().to_bits();
    }
}

const MA_MU_WORD: usize = 0;
const MA_T_START_WORD: usize = 1;
const MA_PREV_T_WORD: usize = 2;
const MA_HEAD_WORD: usize = 3;
const MA_LEN_WORD: usize = 4;
const MA_META_WORDS: usize = 5;
const MA_CAPACITY: usize = 64;
const MA_TIME_BASE: usize = MA_META_WORDS;
const MA_MU_BASE: usize = MA_TIME_BASE + MA_CAPACITY;
const MA_STATE_WORDS: usize = MA_META_WORDS + 2 * MA_CAPACITY;

fn ma_time_idx(slot: usize) -> usize {
    MA_TIME_BASE + slot
}

fn ma_mu_idx(slot: usize) -> usize {
    MA_MU_BASE + slot
}

fn ma_physical(region: &[u64], logical: usize) -> usize {
    ((region[MA_HEAD_WORD] as usize) + logical) % MA_CAPACITY
}

fn ma_len(region: &[u64]) -> usize {
    region[MA_LEN_WORD] as usize
}

fn ma_point(region: &[u64], logical: usize) -> (f64, f64) {
    let slot = ma_physical(region, logical);
    (
        f64::from_bits(region[ma_time_idx(slot)]),
        f64::from_bits(region[ma_mu_idx(slot)]),
    )
}

fn ma_last_time_bits(region: &[u64]) -> Option<u64> {
    let len = ma_len(region);
    (len > 0).then(|| {
        let slot = ma_physical(region, len - 1);
        region[ma_time_idx(slot)]
    })
}

fn ma_prune(region: &mut [u64], cutoff: f64) {
    while ma_len(region) > 1 {
        let (next_time, _) = ma_point(region, 1);
        if next_time <= cutoff {
            region[MA_HEAD_WORD] = ((region[MA_HEAD_WORD] as usize + 1) % MA_CAPACITY) as u64;
            region[MA_LEN_WORD] -= 1;
        } else {
            break;
        }
    }
}

fn ma_store(region: &mut [u64], t: f64, mu: f64, ctx: &Ctx<'_>) {
    if ma_last_time_bits(region).is_some_and(|bits| bits == t.to_bits()) {
        let slot = ma_physical(region, ma_len(region) - 1);
        region[ma_mu_idx(slot)] = mu.to_bits();
        return;
    }

    let mut len = ma_len(region);
    if len == MA_CAPACITY {
        ctx.warn(
            "CDL.Reals.MovingAverage",
            "MovingAverage: checkpoint ring capacity exceeded; oldest in-window sample dropped",
        );
        region[MA_HEAD_WORD] = ((region[MA_HEAD_WORD] as usize + 1) % MA_CAPACITY) as u64;
        len -= 1;
        region[MA_LEN_WORD] = len as u64;
    }
    let slot = ma_physical(region, len);
    region[ma_time_idx(slot)] = t.to_bits();
    region[ma_mu_idx(slot)] = mu.to_bits();
    region[MA_LEN_WORD] = (len + 1) as u64;
}

fn ma_mu_at(region: &[u64], target: f64, t_now: f64, mu_now: f64) -> f64 {
    let len = ma_len(region);
    if len == 0 {
        return mu_now;
    }
    let (first_t, first_mu) = ma_point(region, 0);
    if target <= first_t {
        return first_mu;
    }

    let mut prev = (first_t, first_mu);
    for i in 1..len {
        let next = ma_point(region, i);
        if target <= next.0 {
            let den = next.0 - prev.0;
            return if den == 0.0 {
                next.1
            } else {
                prev.1 + (next.1 - prev.1) * ((target - prev.0) / den)
            };
        }
        prev = next;
    }

    if target <= t_now {
        let den = t_now - prev.0;
        if den == 0.0 {
            mu_now
        } else {
            prev.1 + (mu_now - prev.1) * ((target - prev.0) / den)
        }
    } else {
        mu_now
    }
}

/// `CDL.Reals.MovingAverage` — sliding-window mean over `delta`.
///
/// The variable-step `delay(mu, delta)` history is represented by a fixed 64-checkpoint ring in
/// the state region. The tick path never allocates; when more than 64 checkpoints are needed in
/// the current window, the block warns and drops the oldest retained checkpoint. `[S]`,
/// feedthrough `y <- {u}`, not a loop cut.
#[derive(Clone, Copy, Debug)]
pub struct MovingAverage {
    pub(crate) delta: f64,
}

impl Default for MovingAverage {
    fn default() -> Self {
        Self { delta: 1.0 }
    }
}

impl MovingAverage {
    fn delta_eff(self) -> f64 {
        self.delta.max(MIN_DELTA)
    }

    fn mu_now(self, ctx: &Ctx<'_>, inputs: &[Value], region: &[u64]) -> f64 {
        let mu = f64::from_bits(region[MA_MU_WORD]);
        let dt = tick_dt(ctx.t(), region[MA_PREV_T_WORD]);
        forward_euler_accumulate(mu, read_real(inputs, 0), dt)
    }

    fn output(self, ctx: &Ctx<'_>, inputs: &[Value], region: &[u64]) -> f64 {
        let t = ctx.t();
        let first = is_first_tick(region[MA_PREV_T_WORD]);
        let t_start = if first {
            t
        } else {
            f64::from_bits(region[MA_T_START_WORD])
        };
        let mu_now = self.mu_now(ctx, inputs, region);
        let delta = self.delta_eff();
        let mu_del = ma_mu_at(region, t - delta, t, mu_now);
        let denom = if t >= t_start + delta {
            delta
        } else {
            t - t_start + 1e-3
        };
        (mu_now - mu_del) / denom
    }
}

impl Block for MovingAverage {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.MovingAverage",
            inputs: &[PortKind::Real],
            outputs: &[PortKind::Real],
            stateful: true,
        };
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Stateful
    }

    fn feeds_through(&self, in_idx: usize, _out_idx: usize) -> bool {
        in_idx == 0
    }

    fn state_len(&self) -> usize {
        MA_STATE_WORDS
    }

    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region.fill(0);
        region[MA_MU_WORD] = 0.0f64.to_bits();
        region[MA_T_START_WORD] = 0.0f64.to_bits();
        region[MA_PREV_T_WORD] = PREV_T_UNSET;
    }

    fn emit_from_state(
        &self,
        ctx: &Ctx<'_>,
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        emit(0, Value::Real(self.output(ctx, inputs, region)));
    }

    fn update_state(&self, ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        let t = ctx.t();
        let first = is_first_tick(region[MA_PREV_T_WORD]);
        let mu_now = self.mu_now(ctx, inputs, region);
        if first {
            region[MA_T_START_WORD] = t.to_bits();
        }
        ma_prune(region, t - self.delta_eff());
        ma_store(region, t, mu_now, ctx);
        region[MA_MU_WORD] = mu_now.to_bits();
        region[MA_PREV_T_WORD] = t.to_bits();
    }
}
