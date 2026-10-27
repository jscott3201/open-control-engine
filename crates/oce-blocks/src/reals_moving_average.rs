//! `CDL.Reals.MovingAverage` — the sliding-window mean and its parameter-sized checkpoint ring.
//!
//! Upstream (`Buildings/Controls/OBC/CDL/Reals/MovingAverage.mo`) integrates `mu = ∫u`, delays it
//! by `delta` and emits `y = (mu - delay(mu, delta))/delta` once `time >= tStart + delta`, or
//! `(mu - delay(mu, delta))/(time - tStart + 1e-3)` during warm-up. The engine discretizes `mu`
//! with the shared forward-Euler accumulator on the current tick input, so between two ticks the
//! reconstructed input is the later tick's `u` held over the gap, and `delay(mu, delta)` is the
//! linear interpolation of `mu` between the two retained checkpoints that bracket `t - delta`.
//!
//! The ring that holds those checkpoints is sized from `delta` at build time (see
//! [`MovingAverage`]), so the window always spans the full `delta`; the ring never evicts an
//! in-window checkpoint.

use oce_model::{ParamTable, Value};

use crate::dynamics::{PREV_T_UNSET, forward_euler_accumulate, is_first_tick, tick_dt};
use crate::{Block, BlockKind, BlockSignature, Ctx, PortKind, emit_real, read_real};

/// Upstream `delta(min=1E-5)`; also the internal positive floor for an unvalidated `delta`.
const MIN_DELTA: f64 = 1e-5;

const MU_WORD: usize = 0;
const T_START_WORD: usize = 1;
const PREV_T_WORD: usize = 2;
const HEAD_WORD: usize = 3;
const LEN_WORD: usize = 4;
const COALESCED_WORD: usize = 5;
const META_WORDS: usize = 6;

/// Fewest history slots any window gets, so short windows keep a fine resolution.
const MIN_SLOTS: usize = 2048;

/// History slots per second of window: two, so the resolution stays at or below 0.5 s.
const SLOTS_PER_SECOND: f64 = 2.0;

/// Most history slots any window gets (2 MiB of checkpoints). Windows up to 65536 s keep a
/// resolution of at most 0.5 s; longer windows share these slots, so their resolution is
/// `delta/MAX_SLOTS`.
const MAX_SLOTS: usize = 131_072;

/// Ring capacity beyond `slots`. The coalescing rule bounds the retained checkpoints by
/// `slots + 3` (see [`MovingAverage`]); one more slot absorbs floating-point rounding.
const SLACK_SLOTS: usize = 4;

const COALESCED_WARNING: &str = "MovingAverage: ticks arrived closer than the window history resolution; \
     checkpoints coalesced (window still spans delta)";

/// Resolved ring geometry for one `delta`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RingGeometry {
    /// Number of checkpoint slots (`slots + SLACK_SLOTS`).
    pub(crate) capacity: usize,
    /// History resolution `r` in seconds: ticks at least `r` apart are all retained.
    pub(crate) resolution: f64,
}

impl RingGeometry {
    /// Word count of the whole state region.
    pub(crate) fn state_words(self) -> usize {
        META_WORDS + 2 * self.capacity
    }

    fn time_idx(self, slot: usize) -> usize {
        META_WORDS + slot
    }

    fn mu_idx(self, slot: usize) -> usize {
        META_WORDS + self.capacity + slot
    }

    fn physical(self, region: &[u64], logical: usize) -> usize {
        (region[HEAD_WORD] as usize + logical) % self.capacity
    }

    fn point(self, region: &[u64], logical: usize) -> (f64, f64) {
        let slot = self.physical(region, logical);
        (
            f64::from_bits(region[self.time_idx(slot)]),
            f64::from_bits(region[self.mu_idx(slot)]),
        )
    }

    fn write(self, region: &mut [u64], logical: usize, t: f64, mu: f64) {
        let slot = self.physical(region, logical);
        region[self.time_idx(slot)] = t.to_bits();
        region[self.mu_idx(slot)] = mu.to_bits();
    }

    /// Drop the oldest checkpoint while the next one is still at or before `cutoff`, so exactly
    /// one retained checkpoint sits at or before the window start.
    fn prune(self, region: &mut [u64], cutoff: f64) {
        while len(region) > 1 && self.point(region, 1).0 <= cutoff {
            region[HEAD_WORD] = ((region[HEAD_WORD] as usize + 1) % self.capacity) as u64;
            region[LEN_WORD] -= 1;
        }
    }

    /// Append `(t, mu)`, or coalesce it into the newest checkpoint when it lands within `2r` of
    /// the checkpoint before that (or, defensively, when the ring is full). Coalescing never
    /// touches logical slot 0, so the window start stays covered.
    fn store(self, region: &mut [u64], t: f64, mu: f64, ctx: &Ctx<'_>) {
        let count = len(region);
        if count > 0 && region[self.time_idx(self.physical(region, count - 1))] == t.to_bits() {
            self.write(region, count - 1, t, mu);
            return;
        }
        let too_close = count >= 2 && t - self.point(region, count - 2).0 < 2.0 * self.resolution;
        if too_close || count >= self.capacity {
            if region[COALESCED_WORD] == 0 {
                ctx.warn("CDL.Reals.MovingAverage", COALESCED_WARNING);
                region[COALESCED_WORD] = 1;
            }
            self.write(region, count - 1, t, mu);
            return;
        }
        self.write(region, count, t, mu);
        region[LEN_WORD] = (count + 1) as u64;
    }

    /// `mu` at `target` by linear interpolation over the retained checkpoints, closed by the
    /// current `(t_now, mu_now)` point.
    fn mu_at(self, region: &[u64], target: f64, t_now: f64, mu_now: f64) -> f64 {
        let count = len(region);
        if count == 0 {
            return mu_now;
        }
        let first = self.point(region, 0);
        if target <= first.0 {
            return first.1;
        }
        let mut prev = first;
        for logical in 1..count {
            let next = self.point(region, logical);
            if target <= next.0 {
                return interpolate(prev, next, target);
            }
            prev = next;
        }
        if target <= t_now {
            interpolate(prev, (t_now, mu_now), target)
        } else {
            mu_now
        }
    }
}

fn len(region: &[u64]) -> usize {
    region[LEN_WORD] as usize
}

fn interpolate(prev: (f64, f64), next: (f64, f64), target: f64) -> f64 {
    let den = next.0 - prev.0;
    if den == 0.0 {
        next.1
    } else {
        prev.1 + (next.1 - prev.1) * ((target - prev.0) / den)
    }
}

/// `CDL.Reals.MovingAverage` — sliding-window mean over exactly `delta` seconds.
///
/// **Discretization.** `mu` advances by forward Euler on the current input,
/// `mu_k = mu_{k-1} + u_k*(t_k - t_{k-1})`, and `delay(mu, delta)` is `mu` linearly interpolated at
/// `t - delta` between retained checkpoints. Once `t >= tStart + delta` the output is
/// `(mu(t) - mu(t - delta))/delta` — the mean over `[t - delta, t]` of the input held backward
/// over each tick gap — and before that it is the upstream warm-up quotient
/// `(mu(t) - mu(tStart))/(t - tStart + 1e-3)`. The divisor is always `delta` in the full-window
/// regime: the oldest retained checkpoint is never later than the window start.
///
/// **Ring sizing.** The checkpoint ring is sized at build time from `delta`:
/// `slots = clamp(2*ceil(delta), 2048, 131072)`, history resolution `r = delta/slots`
/// (so `r <= delta/2048`, and `r <= 0.5 s` for every `delta <= 65536 s`), and capacity
/// `slots + 4`. The state region is `6 + 2*capacity` words: `mu`, `tStart`, previous tick time,
/// ring head, ring length, the coalesce warn latch, then the checkpoint times and the checkpoint
/// `mu` values. That is 4110 words (about 33 KB) for every `delta <= 1024 s` — including the G36
/// `delta = 300` — 14414 words for `delta = 3600`, and at most 262158 words (about 2.1 MB) for
/// `delta >= 65536 s`. A window longer than 65536 s keeps the full `delta` span at the coarser
/// resolution `delta/131072`; it is not shortened.
///
/// **Snapshot size.** These words are part of every engine state snapshot, and a snapshot is
/// capped at 64 MiB (`oce-api`), so a model holding a few dozen maximum-size windows still runs
/// but has `state_snapshot` refuse it with `SnapshotTooLarge`. Budget long windows accordingly;
/// nothing at load checks the total.
///
/// **Exactness and coalescing.** A new tick coalesces into the newest checkpoint only when it is
/// less than `2r` after the checkpoint before that. The output is therefore bit-identical to an
/// unbounded history exactly as long as every pair of consecutive tick gaps sums to at least `2r`
/// (`t_k - t_{k-2} >= 2r`). For a nominal 1 s host with `delta <= 65536 s`, `2r <= 1 s`, so no
/// tick ever coalesces while each tick time stays within ±0.5 s of its nominal slot. Denser
/// ticks are coalesced so that any two retained checkpoints two apart are at least `2r` apart,
/// which bounds the ring at `slots + 3` entries. The window still spans `delta`; only the
/// interpolation inside a coalesced gap (shorter than `2r`) is coarser, which moves `y` by at most
/// `r*range/(2*delta)` for an input spanning `range` over that gap. The first coalesce warns once
/// per instance through `Ctx::warn` during `update_state`. Nothing is ever evicted from inside
/// the window.
///
/// `[S]`, feedthrough `y <- {u}`, not a loop cut. Never panics; the tick path never allocates.
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
        // Internal positive floor for an unvalidated direct construction (validated loads carry
        // `delta > 0`). `f64::max` drops a NaN `delta` to the floor.
        self.delta.max(MIN_DELTA)
    }

    /// Ring geometry resolved from `delta`.
    pub(crate) fn geometry(self) -> RingGeometry {
        let delta = self.delta_eff();
        let whole = SLOTS_PER_SECOND * delta.ceil();
        let slots = if whole >= MAX_SLOTS as f64 {
            MAX_SLOTS
        } else if whole <= MIN_SLOTS as f64 {
            MIN_SLOTS
        } else {
            whole as usize
        };
        RingGeometry {
            capacity: slots + SLACK_SLOTS,
            resolution: delta / slots as f64,
        }
    }

    fn mu_now(self, ctx: &Ctx<'_>, inputs: &[Value], region: &[u64]) -> f64 {
        let mu = f64::from_bits(region[MU_WORD]);
        let dt = tick_dt(ctx.t(), region[PREV_T_WORD]);
        forward_euler_accumulate(mu, read_real(inputs, 0), dt)
    }

    fn output(self, ctx: &Ctx<'_>, inputs: &[Value], region: &[u64]) -> f64 {
        let t = ctx.t();
        let t_start = if is_first_tick(region[PREV_T_WORD]) {
            t
        } else {
            f64::from_bits(region[T_START_WORD])
        };
        let mu_now = self.mu_now(ctx, inputs, region);
        let delta = self.delta_eff();
        let mu_del = self.geometry().mu_at(region, t - delta, t, mu_now);
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
        self.geometry().state_words()
    }

    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region.fill(0);
        region[MU_WORD] = 0.0f64.to_bits();
        region[T_START_WORD] = 0.0f64.to_bits();
        region[PREV_T_WORD] = PREV_T_UNSET;
    }

    fn validate_state(&self, region: &[u64], state_t: f64, prev_t: f64) -> Result<(), String> {
        let geometry = self.geometry();
        crate::state_contract::validate_moving_average(
            region,
            state_t,
            prev_t,
            geometry.capacity,
            self.delta_eff(),
        )
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
        let mu_now = self.mu_now(ctx, inputs, region);
        if is_first_tick(region[PREV_T_WORD]) {
            region[T_START_WORD] = t.to_bits();
        }
        let geometry = self.geometry();
        geometry.prune(region, t - self.delta_eff());
        geometry.store(region, t, mu_now, ctx);
        region[MU_WORD] = mu_now.to_bits();
        region[PREV_T_WORD] = t.to_bits();
    }
}
