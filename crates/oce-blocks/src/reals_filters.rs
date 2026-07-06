//! Remaining scalar dynamic `CDL.Reals` blocks.
//!
//! These blocks are stateful and feed through on their current tick inputs (`Derivative` on all
//! of `k`, `T`, `u`; the others on `u`). The tick output is computed from prior state plus the
//! current inputs; `update_state` stores the same next state so no block in this module is a
//! feedback loop cut.

use oce_model::{
    ParamTable, Value,
    determinism::{det_max, det_min},
};

use crate::dynamics::{
    PREV_T_UNSET, first_order_filter_implicit, forward_euler_accumulate, is_first_tick, tick_dt,
};
use crate::{Block, BlockKind, BlockSignature, Ctx, PortKind, emit_real, read_real};

const EPS: f64 = 1e-15;
const MIN_PARAM: f64 = 100.0 * EPS;
const MIN_DELTA: f64 = 1e-5;

const X_WORD: usize = 0;
const PREV_T_WORD: usize = 1;
const TWO_WORD_STATE: usize = 2;

fn positive(x: f64) -> f64 {
    // Deterministic positive-floor clamp. `Derivative` feeds this live INPUT data (its time
    // constant `T` is a RealInput), so a NaN operand must resolve identically on every target:
    // raw `f64::max` lowers to arch intrinsics whose signaling-NaN results differ (aarch64
    // `fmaxnm` returns the quieted NaN; x86_64 returns the non-NaN operand). `det_max` branches
    // on `is_nan` before any intrinsic and is bit-identical to raw max for every non-NaN input
    // here, because the floor is strictly positive so the signed-zero fixup can never engage.
    det_max(x, MIN_PARAM)
}

fn derivative_x_for_start(u: f64, k: f64, t: f64, y_start: f64) -> f64 {
    if k.abs() < EPS {
        u
    } else {
        u - t * y_start / k
    }
}

/// Input indices for [`Derivative`], in the upstream connector declaration order.
const DERIVATIVE_K_INPUT: usize = 0;
const DERIVATIVE_T_INPUT: usize = 1;
const DERIVATIVE_U_INPUT: usize = 2;

/// One-shot warn latch for a clamped [`Derivative`] time-constant input (`0` = not yet warned).
const DERIVATIVE_T_WARNED_WORD: usize = 2;
const DERIVATIVE_STATE_WORDS: usize = 3;

/// `CDL.Reals.Derivative` — first-order filtered derivative:
/// `y = (k/T_nonZero)*(u-x)`, `x' = (u-x)/T_nonZero`, `T_nonZero = max(T, 100*eps)`, discretized
/// with the shared implicit Euler filter.
///
/// Upstream (Buildings `Reals/Derivative.mo`, pin `a131864`) declares the gain `k` and the time
/// constant `T` as **`RealInput` connectors** in declaration order `k, T, u` (input indices
/// `0/1/2`), so both may vary at runtime (the `PIDWithAutotuning` wiring); `y_start` is the only
/// parameter. The state seeds from the upstream initial equation
/// `x = if |k| < eps then u else u - T*y_start/k` (raw `T`, not `T_nonZero`), evaluated on the
/// first tick from the live inputs. `[S]`, feedthrough `y <- {k, T, u}`, not a loop cut.
///
/// Non-finite/out-of-range input policy (the engine has no input-connector range validation;
/// upstream annotates `T(min=100*eps)`, "T > 0 required", which a Modelica tool asserts on).
/// Both equations above run under plain IEEE-754 semantics; the ONLY sanitized value is the
/// `T` floor:
///
/// - **T floor + warn:** `T_nonZero = det_max(T, 100*eps)` floors every `T` that is NaN or
///   below `100*eps = 1e-13` (e.g. zero — the upstream-documented "ideal derivative" wiring —
///   or any negative value; the NaN is dropped before any arch intrinsic, so the floor is
///   bit-identical on every target). The first floored tick warns **once per instance**
///   through `Ctx::warn` during `update_state`. `T = +inf` is never floored and never warns:
///   after a healthy first tick it freezes `x` (`alpha = dt/inf = 0`) and scales `y` by
///   `k/inf` (a signed zero for finite `k`).
/// - **Taint vs. poison:** after the first tick the state update reads only `T` and `u`, so a
///   non-finite `k` taints only that tick's `y` (NaN emits leave canonicalized). The state is
///   poisoned once the seed or filter arithmetic drives `x` non-finite — e.g. a non-finite
///   `u`, or a first-tick seed `x = u - T*y_start/k` that goes non-finite when the
///   `|k| < eps` guard does not short-circuit it (the guard seeds `x = u` without reading
///   `T`, so even a NaN `T` leaves the state recoverable there). A non-finite `x` never
///   returns to finite under the implicit filter, so `y` then emits canonical NaN or `±inf`
///   forever. Poisoning itself carries no diagnostic; a warn accompanies it only when the
///   same tick's `T` independently crossed the floor.
#[derive(Clone, Copy, Debug, Default)]
pub struct Derivative {
    pub(crate) y_start: f64,
}

impl Derivative {
    fn x_now(self, inputs: &[Value], region: &[u64]) -> f64 {
        if is_first_tick(region[PREV_T_WORD]) {
            derivative_x_for_start(
                read_real(inputs, DERIVATIVE_U_INPUT),
                read_real(inputs, DERIVATIVE_K_INPUT),
                read_real(inputs, DERIVATIVE_T_INPUT),
                self.y_start,
            )
        } else {
            f64::from_bits(region[X_WORD])
        }
    }

    fn output(self, inputs: &[Value], region: &[u64]) -> f64 {
        let k = read_real(inputs, DERIVATIVE_K_INPUT);
        let t_non_zero = positive(read_real(inputs, DERIVATIVE_T_INPUT));
        let u = read_real(inputs, DERIVATIVE_U_INPUT);
        // Mirrors the upstream association exactly: `y = (k/T_nonZero)*(u-x)`.
        (k / t_non_zero) * (u - self.x_now(inputs, region))
    }
}

impl Block for Derivative {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.Derivative",
            inputs: &[PortKind::Real, PortKind::Real, PortKind::Real],
            outputs: &[PortKind::Real],
            stateful: true,
        };
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Stateful
    }

    fn feeds_through(&self, in_idx: usize, out_idx: usize) -> bool {
        in_idx < 3 && out_idx == 0
    }

    fn state_len(&self) -> usize {
        DERIVATIVE_STATE_WORDS
    }

    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region[X_WORD] = 0.0f64.to_bits();
        region[PREV_T_WORD] = PREV_T_UNSET;
        region[DERIVATIVE_T_WARNED_WORD] = 0;
    }

    fn emit_from_state(
        &self,
        _ctx: &Ctx<'_>,
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        emit_real(0, self.output(inputs, region), emit);
    }

    fn update_state(&self, ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        let u = read_real(inputs, DERIVATIVE_U_INPUT);
        let t_raw = read_real(inputs, DERIVATIVE_T_INPUT);
        // Diagnostic parity with the upstream `T(min=100*eps)` annotation ("T > 0 required"):
        // the clamp below silently turns a faulted T wire into a huge-but-finite gain, so the
        // first clamped tick warns (once per instance, like the MovingAverage ring overflow).
        if (t_raw.is_nan() || t_raw < MIN_PARAM) && region[DERIVATIVE_T_WARNED_WORD] == 0 {
            ctx.warn(
                "CDL.Reals.Derivative",
                "Derivative: time-constant input T is NaN or below the 100*eps floor; clamping T_nonZero to 1e-13",
            );
            region[DERIVATIVE_T_WARNED_WORD] = 1;
        }
        let t_non_zero = positive(t_raw);
        let dt = tick_dt(ctx.t(), region[PREV_T_WORD]);
        let x = self.x_now(inputs, region);
        region[X_WORD] = first_order_filter_implicit(x, u, t_non_zero, dt).to_bits();
        region[PREV_T_WORD] = ctx.t().to_bits();
    }
}

/// `CDL.Reals.LimitSlewRate` — first-order lag toward `u`, with per-tick `dy` clamped to
/// `[fallingSlewRate*dt, raisingSlewRate*dt]`. `[S]`, feedthrough `y <- {u}`, not a loop cut.
/// This uses the shared-helper discretization: implicit lag first, then clamp the post-filter
/// per-tick increment, rather than a literal continuous-ODE `der(y)=clamp((u-y)/Td, ...)` solve.
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
        debug_assert!(dt >= 0.0, "model time must be monotonic");
        if !self.enable || is_first_tick(region[PREV_T_WORD]) {
            return u;
        }
        let y = f64::from_bits(region[X_WORD]);
        let filtered = first_order_filter_implicit(y, u, self.td_eff(), dt);
        let dy = det_min(
            det_max(filtered - y, self.falling() * dt),
            self.rising() * dt,
        );
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
        emit_real(0, self.next_y(read_real(inputs, 0), region, dt), emit);
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
const MA_WARNED_WORD: usize = 5;
const MA_META_WORDS: usize = 6;
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

fn ma_oldest_time(region: &[u64]) -> Option<f64> {
    (ma_len(region) > 0).then(|| ma_point(region, 0).0)
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
        if region[MA_WARNED_WORD] == 0 {
            ctx.warn(
                "CDL.Reals.MovingAverage",
                "MovingAverage: checkpoint ring capacity exceeded; oldest in-window sample dropped",
            );
            region[MA_WARNED_WORD] = 1;
        }
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
/// the current window, the block warns once per instance and drops the oldest retained checkpoint.
/// `[S]`, feedthrough `y <- {u}`, not a loop cut.
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
        // Internal positive-floor clamp: the effective window is always > 0 before any division.
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
            let retained_lo = ma_oldest_time(region).unwrap_or(t_start);
            let t_lo = det_max(det_max(t - delta, retained_lo), t_start);
            // `MIN_DELTA` is only a division-by-zero guard for a collapsed retained span; legal
            // non-overflow windows divide by their true `delta`, even when `delta < 1e-3`.
            // The positive floor is internal and cannot surface a signed-zero Real output.
            (t - t_lo).max(MIN_DELTA)
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
        region[MA_WARNED_WORD] = 0;
    }

    fn emit_from_state(
        &self,
        ctx: &Ctx<'_>,
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        emit_real(0, self.output(ctx, inputs, region), emit);
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
