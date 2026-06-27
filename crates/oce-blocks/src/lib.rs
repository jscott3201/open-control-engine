#![forbid(unsafe_code)]
//! `oce-blocks` — the native CDL elementary-block library and the [`Block`] trait for the
//! Open Control Engine (`03-block-library.md`).
//!
//! Every elementary block (CDL §7.6) implements [`Block`], classified statically as stateless
//! `[A]` ([`BlockKind::Algebraic`]) or stateful `[S]` ([`BlockKind::Stateful`]). Per the **arena
//! model** (`01` §7/§9) a block object is an *immutable* description (class + resolved
//! parameters); all mutable `[S]` state lives in an engine-owned `RunState.words` region, never in
//! the block struct. The split between [`Block::emit_from_state`] (emit from **prior** state) and
//! [`Block::update_state`] (advance state after) is what lets `oce-graph` evaluate a tick in
//! topological order and cut algebraic loops at non-feedthrough stateful blocks. This crate is
//! **Group A**: no store, no database (D-OWNER-1); it carries behavior only — never per-tick state
//! in the struct, never non-computational metadata (CDL §7.17).
//!
//! The arena trait, per-tick context seam, scalar Reals, Logical/Conversions/Integers algebraic
//! blocks, Reals dynamic blocks, PID controllers, logical timing/latch blocks, integer edge/count
//! blocks, and the `Utilities.Assert` diagnostics sink are implemented and registered by canonical
//! CDL class path. The remaining CDL catalog breadth is added family-by-family behind the same
//! trait and registry contracts.

use oce_model::{ParamTable, Value, determinism::canonicalize_real};

mod conversions;
mod discrete;
mod discrete_sampled;
mod dynamics;
mod integers;
mod integers_edge;
mod integers_stage;
mod logical;
mod logical_latch;
mod logical_proof;
mod logical_timing;
mod logical_variable_pulse;
mod pid;
mod reals_arithmetic;
mod reals_comparators;
mod reals_filters;
mod reals_integrator;
mod reals_ramp;
mod reals_sources;
mod registry;
mod source_pulse;
mod utilities;

pub use conversions::{BooleanToInteger, BooleanToReal, IntegerToReal, RealToInteger};
pub use discrete::{TriggeredMax, TriggeredMovingMean, TriggeredSampler, UnitDelay};
pub use discrete_sampled::{FirstOrderHold, Sampler, ZeroOrderHold};
pub use integers::{
    IntegerAbs, IntegerAdd, IntegerAddParameter, IntegerConstant, IntegerEqual, IntegerGreater,
    IntegerGreaterEqual, IntegerGreaterEqualThreshold, IntegerGreaterThreshold, IntegerLess,
    IntegerLessEqual, IntegerLessEqualThreshold, IntegerLessThreshold, IntegerMax, IntegerMin,
    IntegerMultiply, IntegerSubtract, IntegerSwitch,
};
pub use integers_edge::{IntegerChange, OnCounter};
pub use integers_stage::IntegerStage;
pub use logical::{
    And, Edge, LogicalConstant, LogicalSwitch, Nand, Nor, Not, Or, Pre, SampleTrigger, Xor,
};
pub use logical_latch::{FallingEdge, Latch, LogicalChange, Toggle};
pub use logical_proof::Proof;
pub use logical_timing::{Timer, TimerAccumulating, TrueDelay, TrueFalseHold, TrueHoldWithReset};
pub use logical_variable_pulse::LogicalVariablePulse;
pub use pid::{Pid, PidWithReset};
pub use reals_arithmetic::{
    Abs, Add, AddParameter, Average, Constant, Divide, Limiter, Line, Max, Min, Modulo, Multiply,
    MultiplyByParameter, Round, Sqrt, Subtract, Switch,
};
pub use reals_comparators::{Greater, GreaterThreshold, Hysteresis, Less, LessThreshold};
pub use reals_filters::{Derivative, LimitSlewRate, MovingAverage};
pub use reals_integrator::IntegratorWithReset;
pub use reals_ramp::Ramp;
pub use reals_sources::{CivilTime, SourceRamp};
pub use registry::lookup;
pub use source_pulse::{IntegerPulse, LogicalPulse, RealPulse};
pub use utilities::Assert;

/// Wall-clock-free model time in seconds, chosen by the host scheduler (CDL §7.16; `01` §8).
pub type Time = f64;

/// The two execution classes — the whole ballgame (`03` §1.1).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum BlockKind {
    /// Stateless `[A]`: `y = f(p, t, u)`. Empty state.
    Algebraic,
    /// Stateful `[S]`: `y = f(p, t, u, x)`; owns per-instance state across ticks.
    Stateful,
}

/// The kind of value a port carries on the hot path (Real/Integer/Boolean only; §7.8 admits no
/// String connector, and enumerations are parameters not signals).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum PortKind {
    /// A `Real` (`f64`) port.
    Real,
    /// An `Integer` (`i64`) port.
    Integer,
    /// A `Boolean` (`bool`) port.
    Boolean,
}

/// Static interface descriptor for an elementary block class. Built once at load time and
/// consumed by `oce-graph` to size buffers and build the DAG. Carries no non-computational
/// metadata (§7.17).
#[derive(Clone, Copy, Debug)]
pub struct BlockSignature {
    /// Canonical class path, e.g. `"CDL.Reals.PID"`.
    pub class_path: &'static str,
    /// Input port kinds in declaration order (index = port index).
    pub inputs: &'static [PortKind],
    /// Output port kinds in declaration order (index = port index).
    pub outputs: &'static [PortKind],
    /// Static class-level state hint. For parameter-dependent classes, use [`Block::kind`] on the
    /// resolved instance for the authoritative `[A]`/`[S]` allocation decision.
    pub stateful: bool,
}

/// A diagnostics sink for `Utilities.Assert` and unit warnings — injected by the scheduler,
/// never a global, to keep the crate store-free and side-effect-explicit (`03` §2.4).
pub trait Diagnostics {
    /// Emit a warning attributed to `source` at model time `t`.
    fn warn(&self, source: &str, message: &str, t: Time);
}

/// Per-tick block execution context. The scheduler owns the diagnostics sink and supplies the
/// model time; blocks read time through [`Ctx::t`] and report warnings through [`Ctx::warn`].
pub struct Ctx<'a> {
    t: Time,
    diag: &'a dyn Diagnostics,
}

impl<'a> Ctx<'a> {
    /// Build a per-tick context from scheduler-owned time and diagnostics sink.
    #[must_use]
    pub fn new(t: Time, diag: &'a dyn Diagnostics) -> Self {
        Self { t, diag }
    }

    /// Current model time in seconds.
    #[must_use]
    pub fn t(&self) -> Time {
        self.t
    }

    /// Emit a warning at this context's model time.
    pub fn warn(&self, source: &str, message: &str) {
        self.diag.warn(source, message, self.t);
    }
}

/// Zero-size diagnostics sink used where assertions are intentionally dropped.
pub struct NoopDiagnostics;

impl Diagnostics for NoopDiagnostics {
    fn warn(&self, _source: &str, _message: &str, _t: Time) {}
}

/// The core elementary-block contract (CDL §7.6) — the **arena model** (`01` §7/§9, the binding
/// FRAME Block-trait resolution). A block object is an *immutable* description (class + parameters
/// resolved at construction); all mutable per-instance `[S]` state lives in an engine-owned flat
/// region (`RunState.words`), never in the block struct. This keeps the frozen schedule shareable
/// and the tick zero-allocation (`01` §7 req 4, §9 req 4), so every method takes `&self`.
///
/// `[A]` blocks override [`Block::step_algebraic`]. `[S]` blocks set [`Block::state_len`], seed via
/// [`Block::init_state`], and override [`Block::emit_from_state`] + [`Block::update_state`].
///
/// The trait is **`Send + Sync`** (R-API-PY-2 / `08` R-ENG-1): a block holds only resolved
/// parameters — plain owned `f64`/`bool` data, no `Rc`/`RefCell`/raw pointers (per-instance mutable
/// `[S]` state lives in the engine's `RunState`, never here) — so `Box<dyn Block>` is `Send + Sync`
/// and the frozen schedule is shareable across host threads as `Arc<Engine<S>>`. The bound is a
/// zero-cost marker; it is what lets `oce-api`'s `_assert_send_sync` compile.
pub trait Block: Send + Sync {
    /// Class-level interface descriptor. Drives buffer sizing and the DAG.
    fn signature(&self) -> &'static BlockSignature;

    /// Stateless `[A]` vs stateful `[S]` (CDL §7.2/§7.6). Queried on the parameter-resolved
    /// instance, never the bare class (`01` §4.2 design note).
    fn kind(&self) -> BlockKind;

    /// Direct-feedthrough oracle: returns `true` iff output `out_idx` algebraically (zero-time,
    /// same-tick) depends on input `in_idx`. This is what cuts the DAG for loop-breakers
    /// (`01` §4.2; `03` §3). Loop-breaker `[S]` blocks return `false` for the state-bearing path.
    fn feeds_through(&self, in_idx: usize, out_idx: usize) -> bool;

    /// Word count of this block's `[S]` state region within `RunState.words` (0 for `[A]`). Fixed
    /// at BUILD; the tick never resizes it (`01` §7 req 1).
    fn state_len(&self) -> usize {
        0
    }

    /// Seed the `[S]` state `region` from resolved parameters (CDL has no `start` attribute; the
    /// initial state seeds from a parameter — §7.4, R-TRAIT-4 / `01` §7 req 2). `[A]` blocks: no-op.
    fn init_state(&self, _region: &mut [u64], _params: &ParamTable) {}

    /// `[A]` output: `y = f(p, t, u)`. Emit each output by port index via `emit` (R-TRAIT-1).
    /// `[S]` blocks leave this as the default no-op — their output is [`Block::emit_from_state`].
    fn step_algebraic(
        &self,
        _ctx: &Ctx<'_>,
        _inputs: &[Value],
        _emit: &mut dyn FnMut(usize, Value),
    ) {
    }

    /// `[S]` output pass: emit outputs from **prior** state — `region` is read-only here — before
    /// any state update this tick (`01` §9 req 2, R-TRAIT-1). `[A]` blocks: default no-op.
    fn emit_from_state(
        &self,
        _ctx: &Ctx<'_>,
        _inputs: &[Value],
        _region: &[u64],
        _emit: &mut dyn FnMut(usize, Value),
    ) {
    }

    /// `[S]` state pass: advance `x(t) → x'(t)` into the mutable `region`, exactly once per tick,
    /// after [`Block::emit_from_state`] (`01` §9 req 2, R-TRAIT-2). `[A]` blocks: default no-op.
    fn update_state(&self, _ctx: &Ctx<'_>, _inputs: &[Value], _region: &mut [u64]) {}
}

/// A class-level parameter validation rule published by the block registry.
///
/// Rules are static metadata for resolved block parameters: `oce-validate` uses them at load time,
/// and `oce-api` mirrors the single-parameter bounds into tune-at-rest metadata. They intentionally
/// describe only CDL semantics, not storage or host UI policy.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParamRule {
    /// The named parameter must appear in the resolved [`ParamTable`].
    Required {
        /// Parameter name as it appears in CDL / the resolved model.
        name: &'static str,
    },
    /// The named `Real` parameter must be strictly greater than `min`.
    RealGreaterThan {
        /// Parameter name as it appears in CDL / the resolved model.
        name: &'static str,
        /// Exclusive lower bound.
        min: f64,
    },
    /// The named `Integer` parameter must be greater than or equal to `min`.
    IntegerGreaterOrEqual {
        /// Parameter name as it appears in CDL / the resolved model.
        name: &'static str,
        /// Inclusive lower bound.
        min: i64,
    },
    /// The named `Real` parameter must be greater than or equal to `min`.
    RealGreaterOrEqual {
        /// Parameter name as it appears in CDL / the resolved model.
        name: &'static str,
        /// Inclusive lower bound.
        min: f64,
    },
    /// The named `Real` parameter must be less than or equal to `max`.
    RealLessOrEqualConstant {
        /// Parameter name as it appears in CDL / the resolved model.
        name: &'static str,
        /// Inclusive upper bound.
        max: f64,
    },
    /// The named `Real` parameter, multiplied by an `Integer` parameter, must be in range.
    RealTimesIntegerInclusiveRange {
        /// Real-valued parameter name.
        real: &'static str,
        /// Integer-valued scale parameter name.
        integer: &'static str,
        /// Inclusive lower bound on `real * integer`.
        min: f64,
        /// Inclusive upper bound on `real * integer`.
        max: f64,
    },
    /// The two named `Real` parameters must satisfy `lower <= upper`.
    RealLessOrEqual {
        /// Lower-bound parameter name.
        lower: &'static str,
        /// Upper-bound parameter name.
        upper: &'static str,
    },
    /// The two named `Real` parameters should satisfy `lower <= upper`; violations warn only.
    RealLessOrEqualWarning {
        /// Lower-bound parameter name.
        lower: &'static str,
        /// Upper-bound parameter name.
        upper: &'static str,
    },
    /// The left `Real` parameter should be at least `factor * right`; violations warn only.
    RealGreaterOrEqualScaledWarning {
        /// Left-hand parameter name.
        left: &'static str,
        /// Right-hand parameter name multiplied by `factor`.
        right: &'static str,
        /// Positive scale factor applied to `right`.
        factor: f64,
    },
    /// Equal `Real` parameter values are permitted but should produce a warning.
    RealEqualWarning {
        /// Left-hand parameter name.
        left: &'static str,
        /// Right-hand parameter name.
        right: &'static str,
    },
}

/// A `&'static` registry entry mapping a class path to its block constructor (R-IMPL-2).
pub struct RegistryEntry {
    /// Canonical (or accepted-alias) class path.
    pub class_path: &'static str,
    /// Constructor from a resolved parameter table.
    pub make: fn(&ParamTable) -> Box<dyn Block>,
}

impl RegistryEntry {
    /// Class-level parameter rules for this registered block.
    #[must_use]
    pub fn param_rules(&self) -> &'static [ParamRule] {
        registry::param_rules(self.class_path)
    }
}

// ---- shared hot-path value readers (used by block impls) ------------------------------------
//
// Inputs are gathered from typed connector values that `oce-validate` has already type-checked
// (CDL forbids implicit coercion, §7.10/A.9), so a wrong variant here is a build/validation bug,
// not a runtime condition. The readers never panic on the tick: they `debug_assert` the type and
// fall back to the type's zero in release so a mis-wired model degrades rather than aborting.

/// Read input `i` as a `Real`, defaulting to `0.0` on a (validation-prevented) type mismatch.
#[must_use]
pub(crate) fn read_real(inputs: &[Value], i: usize) -> f64 {
    match inputs.get(i) {
        Some(Value::Real(x)) => *x,
        other => {
            debug_assert!(false, "expected Real input at {i}, found {other:?}");
            0.0
        }
    }
}

/// Emit a `Real` output after canonicalizing target-dependent NaN encodings.
pub(crate) fn emit_real(port: usize, y: f64, emit: &mut dyn FnMut(usize, Value)) {
    emit(port, Value::Real(canonicalize_real(y)));
}

/// Read input `i` as a `Boolean`, defaulting to `false` on a (validation-prevented) type mismatch.
#[must_use]
pub(crate) fn read_bool(inputs: &[Value], i: usize) -> bool {
    match inputs.get(i) {
        Some(Value::Boolean(b)) => *b,
        other => {
            debug_assert!(false, "expected Boolean input at {i}, found {other:?}");
            false
        }
    }
}

/// Read input `i` as an `Integer`, defaulting to `0` on a (validation-prevented) type mismatch.
#[must_use]
pub(crate) fn read_int(inputs: &[Value], i: usize) -> i64 {
    match inputs.get(i) {
        Some(Value::Integer(n)) => *n,
        other => {
            debug_assert!(false, "expected Integer input at {i}, found {other:?}");
            0
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod param_rules_tests;

#[cfg(test)]
mod reals_arithmetic_tests;

#[cfg(test)]
mod reals_comparators_tests;

#[cfg(test)]
mod logical_tests;

#[cfg(test)]
mod conversions_tests;

#[cfg(test)]
mod integers_tests;

#[cfg(test)]
mod logical_latch_tests;

#[cfg(test)]
mod logical_proof_tests;

#[cfg(test)]
mod logical_timing_tests;

#[cfg(test)]
mod logical_variable_pulse_tests;

#[cfg(test)]
mod integers_edge_tests;

#[cfg(test)]
mod integers_stage_tests;

#[cfg(test)]
mod pid_tests;

#[cfg(test)]
mod reals_integrator_tests;

#[cfg(test)]
mod reals_filters_tests;

#[cfg(test)]
mod reals_ramp_tests;

#[cfg(test)]
mod reals_sources_tests;

#[cfg(test)]
mod source_pulse_tests;

#[cfg(test)]
mod discrete_tests;

#[cfg(test)]
mod utilities_tests;
