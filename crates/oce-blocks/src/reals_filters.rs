//! Scalar first-order dynamic `CDL.Reals` blocks: `Derivative` and `LimitSlewRate`.
//!
//! These blocks are stateful and feed through on their current tick inputs (`Derivative` on all
//! of `k`, `T`, `u`; `LimitSlewRate` on `u`). `CDL.Reals.MovingAverage` lives in
//! `reals_moving_average`. The tick output is computed from prior state plus the
//! current inputs; `update_state` stores the same next state so no block in this module is a
//! feedback loop cut.

use oce_model::{
    ParamTable, Value,
    determinism::{det_max, det_min},
};

use crate::dynamics::{PREV_T_UNSET, first_order_filter_implicit, is_first_tick, tick_dt};
use crate::{Block, BlockKind, BlockSignature, Ctx, PortKind, emit_real, read_real};

const EPS: f64 = 1e-15;
const MIN_PARAM: f64 = 100.0 * EPS;

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
        // first clamped tick warns (once per instance).
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
