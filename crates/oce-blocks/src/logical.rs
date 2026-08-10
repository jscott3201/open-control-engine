//! `CDL.Logical` blocks (`03` §4.3): combinational Boolean algebra `[A]`, vector reductions,
//! the canonical loop-breaker `Pre` `[S]`, the rising-edge detector `Edge` `[S]`, and the
//! parameter-clocked Boolean source `Sources.SampleTrigger` `[S]` — the two discrete primitives of
//! `01` §11.

use std::borrow::Cow;

use oce_model::{ParamTable, Value};

use crate::{
    Block, BlockKind, BlockSignature, Ctx, PortKind, PortShape, ResolvedBlockSignature, Time,
    read_bool,
};

/// `CDL.Logical.Sources.Constant` — `y = k`. Stateless `[A]` source, no feedthrough edges.
#[derive(Clone, Copy, Debug, Default)]
pub struct LogicalConstant {
    pub(crate) k: bool,
}

impl Block for LogicalConstant {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Logical.Sources.Constant",
            inputs: &[],
            outputs: &[PortKind::Boolean],
            stateful: false,
        };
        &SIG
    }
    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }
    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        false
    }
    fn step_algebraic(
        &self,
        _ctx: &Ctx<'_>,
        _inputs: &[Value],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        emit(0, Value::Boolean(self.k));
    }
}

/// `CDL.Logical.And` — `y = u1 ∧ u2` (`03` §4.3).
#[derive(Clone, Copy, Debug, Default)]
pub struct And;

impl Block for And {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Logical.And",
            inputs: &[PortKind::Boolean, PortKind::Boolean],
            outputs: &[PortKind::Boolean],
            stateful: false,
        };
        &SIG
    }
    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }
    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        true
    }
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit(
            0,
            Value::Boolean(read_bool(inputs, 0) && read_bool(inputs, 1)),
        );
    }
}

/// `CDL.Logical.Or` — `y = u1 ∨ u2` (`03` §4.3). Stateless `[A]`, full feedthrough.
#[derive(Clone, Copy, Debug, Default)]
pub struct Or;

impl Block for Or {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Logical.Or",
            inputs: &[PortKind::Boolean, PortKind::Boolean],
            outputs: &[PortKind::Boolean],
            stateful: false,
        };
        &SIG
    }
    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }
    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        true
    }
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit(
            0,
            Value::Boolean(read_bool(inputs, 0) || read_bool(inputs, 1)),
        );
    }
}

/// `CDL.Logical.MultiAnd` — `y = u[1] ∧ ... ∧ u[nin]`; empty input yields `false`.
#[derive(Clone, Debug, Default)]
pub struct MultiAnd {
    pub(crate) inputs: Vec<PortKind>,
}

impl MultiAnd {
    pub(crate) fn new(nin: usize) -> Self {
        Self {
            inputs: PortShape::new(PortKind::Boolean, nin).to_kinds(),
        }
    }
}

impl Block for MultiAnd {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Logical.MultiAnd",
            inputs: &[],
            outputs: &[PortKind::Boolean],
            stateful: false,
        };
        &SIG
    }
    fn resolved_signature(&self) -> ResolvedBlockSignature<'_> {
        static OUTPUTS: [PortKind; 1] = [PortKind::Boolean];
        ResolvedBlockSignature {
            class_path: self.signature().class_path,
            inputs: Cow::Borrowed(self.inputs.as_slice()),
            outputs: Cow::Borrowed(&OUTPUTS),
        }
    }
    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }
    fn feeds_through(&self, in_idx: usize, out_idx: usize) -> bool {
        out_idx == 0 && in_idx < self.inputs.len()
    }
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        let y = !self.inputs.is_empty() && (0..self.inputs.len()).all(|i| read_bool(inputs, i));
        emit(0, Value::Boolean(y));
    }
}

/// `CDL.Logical.MultiOr` — `y = u[1] ∨ ... ∨ u[nin]`; empty input yields `false`.
#[derive(Clone, Debug, Default)]
pub struct MultiOr {
    pub(crate) inputs: Vec<PortKind>,
}

impl MultiOr {
    pub(crate) fn new(nin: usize) -> Self {
        Self {
            inputs: PortShape::new(PortKind::Boolean, nin).to_kinds(),
        }
    }
}

impl Block for MultiOr {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Logical.MultiOr",
            inputs: &[],
            outputs: &[PortKind::Boolean],
            stateful: false,
        };
        &SIG
    }
    fn resolved_signature(&self) -> ResolvedBlockSignature<'_> {
        static OUTPUTS: [PortKind; 1] = [PortKind::Boolean];
        ResolvedBlockSignature {
            class_path: self.signature().class_path,
            inputs: Cow::Borrowed(self.inputs.as_slice()),
            outputs: Cow::Borrowed(&OUTPUTS),
        }
    }
    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }
    fn feeds_through(&self, in_idx: usize, out_idx: usize) -> bool {
        out_idx == 0 && in_idx < self.inputs.len()
    }
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        let y = (0..self.inputs.len()).any(|i| read_bool(inputs, i));
        emit(0, Value::Boolean(y));
    }
}

/// `CDL.Logical.Not` — `y = ¬u` (`03` §4.3).
#[derive(Clone, Copy, Debug, Default)]
pub struct Not;

impl Block for Not {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Logical.Not",
            inputs: &[PortKind::Boolean],
            outputs: &[PortKind::Boolean],
            stateful: false,
        };
        &SIG
    }
    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }
    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        true
    }
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit(0, Value::Boolean(!read_bool(inputs, 0)));
    }
}

/// `CDL.Logical.Nand` — `y = ¬(u1 ∧ u2)` (`03` §4.3). Stateless `[A]`, full feedthrough.
#[derive(Clone, Copy, Debug, Default)]
pub struct Nand;

impl Block for Nand {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Logical.Nand",
            inputs: &[PortKind::Boolean, PortKind::Boolean],
            outputs: &[PortKind::Boolean],
            stateful: false,
        };
        &SIG
    }
    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }
    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        true
    }
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit(
            0,
            Value::Boolean(!(read_bool(inputs, 0) && read_bool(inputs, 1))),
        );
    }
}

/// `CDL.Logical.Nor` — `y = ¬(u1 ∨ u2)` (`03` §4.3). Stateless `[A]`, full feedthrough.
#[derive(Clone, Copy, Debug, Default)]
pub struct Nor;

impl Block for Nor {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Logical.Nor",
            inputs: &[PortKind::Boolean, PortKind::Boolean],
            outputs: &[PortKind::Boolean],
            stateful: false,
        };
        &SIG
    }
    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }
    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        true
    }
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit(
            0,
            Value::Boolean(!(read_bool(inputs, 0) || read_bool(inputs, 1))),
        );
    }
}

/// `CDL.Logical.Xor` — `y = u1 xor u2` (`03` §4.3). Stateless `[A]`, full feedthrough.
#[derive(Clone, Copy, Debug, Default)]
pub struct Xor;

impl Block for Xor {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Logical.Xor",
            inputs: &[PortKind::Boolean, PortKind::Boolean],
            outputs: &[PortKind::Boolean],
            stateful: false,
        };
        &SIG
    }
    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }
    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        true
    }
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit(
            0,
            Value::Boolean(read_bool(inputs, 0) ^ read_bool(inputs, 1)),
        );
    }
}

/// `CDL.Logical.Switch` — `y = if u2 then u1 else u3`; `u2` is the selector (`03` §4.3).
/// Stateless `[A]`, full feedthrough.
#[derive(Clone, Copy, Debug, Default)]
pub struct LogicalSwitch;

impl Block for LogicalSwitch {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Logical.Switch",
            inputs: &[PortKind::Boolean, PortKind::Boolean, PortKind::Boolean],
            outputs: &[PortKind::Boolean],
            stateful: false,
        };
        &SIG
    }
    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }
    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        true
    }
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        let y = if read_bool(inputs, 1) {
            read_bool(inputs, 0)
        } else {
            read_bool(inputs, 2)
        };
        emit(0, Value::Boolean(y));
    }
}

/// `CDL.Logical.Pre` — `y = pre(u)`, the canonical one-evaluation Boolean delay and **the** CDL
/// algebraic-loop breaker (`03` §4.3): stateful, with `feeds_through == false` so it cuts the
/// direct-feedthrough DAG. Per the arena model the one-tick Boolean memory lives in one `[S]` state
/// word (`0`/`1`), not in the struct; it is seeded from the `pre` parameter (CDL has no `start`).
#[derive(Clone, Copy, Debug, Default)]
pub struct Pre {
    pub(crate) y_start: bool,
}

impl Block for Pre {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Logical.Pre",
            inputs: &[PortKind::Boolean],
            outputs: &[PortKind::Boolean],
            stateful: true,
        };
        &SIG
    }
    fn kind(&self) -> BlockKind {
        BlockKind::Stateful
    }
    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        false // THE loop cut: output is the prior input, not the current one
    }
    fn state_len(&self) -> usize {
        1 // one word holds the one-tick-delayed boolean
    }
    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region[0] = u64::from(self.y_start);
    }
    fn emit_from_state(
        &self,
        _ctx: &Ctx<'_>,
        _inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        emit(0, Value::Boolean(region[0] != 0)); // the prior input, held since last tick
    }
    fn update_state(&self, _ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        region[0] = u64::from(read_bool(inputs, 0)); // latch the current input for next tick
    }
}

/// `CDL.Logical.Edge` — `y = u ∧ ¬pre(u)`, true exactly on a **rising edge** (`false → true`) of the
/// Boolean input (`03` §4.3; the rising-edge primitive of `01` §11.2). Stateful `[S]` — it owns the
/// one-tick-prior input bit — but it **feeds through** on the current `u`: the edge is a function of
/// the *current* input versus the prior `prev`, so unlike `Pre`/`UnitDelay` it is **NOT** a loop cut
/// (`feeds_through == true`, `01` §11.2 req 3). The prior bit lives in one `[S]` state word, seeded
/// from `pre_u_start` (CDL has no `start`; default `false` per the reference's `pre = false`
/// convention, so a `u` already `true` on tick 0 emits an edge on tick 0).
#[derive(Clone, Copy, Debug, Default)]
pub struct Edge {
    pub(crate) pre_u_start: bool,
}

impl Block for Edge {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Logical.Edge",
            inputs: &[PortKind::Boolean],
            outputs: &[PortKind::Boolean],
            stateful: true,
        };
        &SIG
    }
    fn kind(&self) -> BlockKind {
        BlockKind::Stateful
    }
    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        true // the edge is a function of the CURRENT u vs the prior bit — feeds through, NOT a cut
    }
    fn state_len(&self) -> usize {
        1 // one word holds the one-tick-prior input bit
    }
    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region[0] = u64::from(self.pre_u_start);
    }
    fn emit_from_state(
        &self,
        _ctx: &Ctx<'_>,
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        let prev = region[0] != 0;
        let u = read_bool(inputs, 0);
        emit(0, Value::Boolean(u && !prev)); // rising edge: current true, prior false
    }
    fn update_state(&self, _ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        region[0] = u64::from(read_bool(inputs, 0)); // latch current input as next tick's prev
    }
}

/// Relative epsilon for [`SampleTrigger`]'s boundary test (`01` §11.1 req 3 — a *fixed, documented*
/// epsilon that is part of the discrete-block conformance contract). It is added to the
/// **period-normalized** sample position, so it is inherently relative to `period`: a tick that lands
/// a hair below an exact sample instant `phase + k·period` (floating-point rounding) still counts as
/// having reached boundary `k`, so the trigger never silently skips a sample.
///
/// The rounding is **symmetric**: because the epsilon is folded into the period-normalized quotient
/// (a deliberate departure from §11.1's *illustrative* pseudocode, which uses an absolute
/// `t + EPS < start` pre-gate plus a bare floor — both satisfy req 3), a tick up to `1e-9 · period`
/// seconds *before* an exact instant also rounds up to boundary `k`. So the trigger can fire at most
/// `1e-9 · period` s **early**, never late (≈1 ns at `period = 1 s`; ≈3.6 µs at `period = 1 h`). This
/// is bounded, well inside the conformance event-timing band (`07` §8 `atolx`), and never exercised
/// on the `EventAligned` grid, which lands a tick exactly on every instant (`01` §11.1 req 2).
const SAMPLE_INDEX_EPS: f64 = 1e-9;

/// `CDL.Logical.Sources.SampleTrigger` — a parameter-clocked Boolean pulse that is `true` on exactly
/// the tick that first reaches each periodic sample instant `phase + k·period` (`k ≥ 0`), where
/// `phase = mod(shift, period)` is the floored Modelica modulo in `[0, period)` at CDL start time
/// `0`. This matches Buildings `SampleTrigger.mo`'s `t0 = mod(shift, period)` anchoring. It is the
/// trigger generator that drives the `Discrete.*` / triggered families. A **source** (no inputs), so
/// `feeds_through` is vacuously `false` (the `Constant` convention). Stateful `[S]`: the last sample
/// index already fired lives in one `[S]` state word as an `i64` (seeded to `-1` = "no sample yet").
///
/// Robust to **arbitrary host cadence** (`01` §11.1 req 1/2): coarser than `period` (fires on the
/// tick that first reaches/passes a boundary), equal, or finer (fires only on the crossing tick). If
/// one coarse tick spans several boundaries it fires **once** and snaps `last_k` to the current `k`
/// (samples the current instant — no sub-tick replay; a documented host-cadence behavior OUT of the
/// conformance guarantee, `01` §11.1 req 2 / `07` §13, since the `EventAligned` driver pins a tick on
/// every instant).
#[derive(Clone, Copy, Debug)]
pub struct SampleTrigger {
    pub(crate) period: f64,
    pub(crate) shift: f64,
}

impl Default for SampleTrigger {
    fn default() -> Self {
        // `period`/`shift` are author-supplied via CXF; `period > 0` is REQUIRED by CDL (a dedicated
        // oce-validate rule enforcing it is pending — see `sample_index` for the safe degradation
        // until then). These defaults only make a param-less construction well-defined (a real model
        // always carries the resolved values, so this is never a silent-wrong-value path).
        Self {
            period: 1.0,
            shift: 0.0,
        }
    }
}

impl SampleTrigger {
    /// Current sample index `k = floor((t − phase)/period + ε)`, where
    /// `phase = mod(shift, period)` is the floored Modelica modulo in `[0, period)` for a positive
    /// period, or a value `< 0` before the first instant. A pure function of `t_now` and the block's
    /// parameters (`01` §11.1 req 1) — it never panics, including on a non-positive or NaN `period`:
    /// `period` is **input-derived**, so the degraded path stays panic-free in *all* builds (no
    /// `debug_assert`), and the `f64 → i64` cast saturates rather than wrapping at extreme horizons.
    fn sample_index(&self, t_now: Time) -> i64 {
        // The `> 0.0` test (not a negated `<=`) is deliberate: the else-branch also catches a NaN
        // `period`, since `NaN > 0.0` is false. `f64 → i64` saturates (no UB) at extreme horizons.
        if self.period > 0.0 {
            let phase = self.shift - (self.shift / self.period).floor() * self.period;
            ((t_now - phase) / self.period + SAMPLE_INDEX_EPS).floor() as i64
        } else {
            // `period > 0` is REQUIRED by CDL but is NOT yet enforced by oce-validate here; until
            // that rule lands, a non-positive or NaN `period` degrades safely to "one sample
            // at/after `shift`, then never" — deterministic, never dividing by zero, and panic-free
            // in every build.
            if t_now >= self.shift { 0 } else { -1 }
        }
    }
}

impl Block for SampleTrigger {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Logical.Sources.SampleTrigger",
            inputs: &[],
            outputs: &[PortKind::Boolean],
            stateful: true,
        };
        &SIG
    }
    fn kind(&self) -> BlockKind {
        BlockKind::Stateful
    }
    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        false // a source: no inputs, nothing to feed through (the `Constant` convention)
    }
    fn state_len(&self) -> usize {
        1 // one word holds the last fired sample index (i64 reinterpreted into the u64 word)
    }
    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region[0] = (-1i64).cast_unsigned(); // -1 == "no sample fired yet"
    }
    fn validate_state(&self, region: &[u64], state_t: Time, _prev_t: Time) -> Result<(), String> {
        crate::state_contract::validate_sample_trigger(region, state_t, self.period, self.shift)
    }
    fn time_is_representable(&self, t_now: Time, _region: &[u64]) -> bool {
        crate::state_contract::sample_trigger_time_representable(t_now, self.period, self.shift)
    }
    fn simulation_time_is_representable(&self, first: Time, last: Time, _region: &[u64]) -> bool {
        crate::state_contract::sample_trigger_horizon_representable(
            first,
            last,
            self.period,
            self.shift,
        )
    }
    fn emit_from_state(
        &self,
        ctx: &Ctx<'_>,
        _inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        let last_k = region[0].cast_signed();
        emit(0, Value::Boolean(self.sample_index(ctx.t()) > last_k)); // true iff a new boundary reached
    }
    fn update_state(&self, ctx: &Ctx<'_>, _inputs: &[Value], region: &mut [u64]) {
        let last_k = region[0].cast_signed();
        let k = self.sample_index(ctx.t());
        if k > last_k {
            region[0] = k.cast_unsigned(); // snap to the latest crossed boundary (fire-once)
        }
    }
}
