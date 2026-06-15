#![forbid(unsafe_code)]
//! `oce-graph` — the deterministic dataflow scheduler/executor for the Open Control Engine
//! (`01-execution-model.md`).
//!
//! Two strict phases. **BUILD** (once per load, off the tick): build the direct-feedthrough
//! DAG over connector vertices, hard-reject algebraic loops (CDL §7.16), run an own Kahn
//! topological sort with declaration-order tie-break (FRAME D6), and allocate `[S]` state
//! seeded from parameters. **TICK** (the hot path): evaluate the frozen [`Schedule`] over flat
//! arrays — no graph walks, no hashing, no allocation, no IO, no store. This crate is
//! **Group A**: it has zero dependency on `oce-store`/`oce-store-mem`/`oce-store-selene` or any
//! selene crate (D-OWNER-1; `01` §10) — enforced by the CI seam gate.
//!
//! Public API names (normative, `01` §6.2): the frozen schedule is [`Schedule`] (alias
//! [`CompiledSchedule`]); BUILD entry point is [`compile`]; TICK entry point is [`eval_tick`]
//! (alias [`run_tick`]).
//!
//! Status: **M0 scaffold.** The type *shapes* match the spec; bodies are stubs
//! (`unimplemented!()`) and land in M0 (the hand-built-graph exit criteria) and M1.

use oce_model::{BlockId, ConnectorId, Model, ModelGraph, Value};

/// The frozen per-tick evaluation order, computed once in BUILD and reused every tick (`01` §6.2).
#[derive(Clone, Debug, Default)]
pub struct Schedule {
    /// Blocks in topological evaluation order (block granularity; the hot loop iterates this).
    pub order: Vec<BlockId>,
    /// The underlying connector-level toposort, retained for trace tooling/debugging.
    pub connector_order: Vec<ConnectorId>,
}

/// Public alias for [`Schedule`] used by `06`/`08` (`01` §6.2: the same type).
pub type CompiledSchedule = Schedule;

/// A `[S]` block's fixed-size state region within [`RunState::words`] (`01` §7).
#[derive(Clone, Copy, Debug)]
pub struct StateSlot {
    /// The owning block instance.
    pub block: BlockId,
    /// Start offset within `RunState::words`.
    pub offset: usize,
    /// Length of the region in words.
    pub len: usize,
}

/// All mutable execution state — one per running model instance, owned by the engine handle.
/// The **sole** mutable structure on the tick (`01` §7).
#[derive(Clone, Debug, Default)]
pub struct RunState {
    /// Flat connector-value array, indexed by `ConnectorId.0`. These are the values that persist
    /// between ticks (CDL §7.16 principle 1); never cleared between ticks.
    pub values: Vec<Value>,
    /// Flat per-`[S]`-block state words (reinterpreted per block: `f64::to_bits`, bools, counters).
    pub words: Vec<u64>,
    /// Slot directory (immutable after BUILD).
    pub slots: Vec<StateSlot>,
    /// Current model time `t` (seconds); advanced by the host, read by elementary blocks (`01` §8).
    pub t: f64,
}

/// A BUILD-phase error (typed; never a panic).
#[derive(Clone, Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BuildError {
    /// One or more connection cycles are not broken by a state-holding block. CDL §7.16 forbids
    /// algebraic loops; the canonical remedy is to insert a `Pre`/`UnitDelay`/integrator into the
    /// loop. Carries the dotted instance/connector paths of the cycle members for diagnostics.
    #[error(
        "algebraic loop detected: {} connector(s) form a cycle not broken by a state-holding \
         block (Pre/UnitDelay/integrator). CDL §7.16 forbids algebraic loops.",
        .members.len()
    )]
    AlgebraicLoop {
        /// Human-readable dotted connector paths participating in the cycle.
        members: Vec<String>,
    },
}

/// BUILD: compile a flattened, validated [`ModelGraph`] into a frozen [`Schedule`] (own Kahn
/// sort, declaration-order tie-break, algebraic-loop rejection; `01` §4–§6). Runs once per load,
/// off the tick.
///
/// # Errors
/// Returns [`BuildError::AlgebraicLoop`] if the direct-feedthrough graph contains a cycle that no
/// loop-breaker block cuts.
pub fn compile(_model: &ModelGraph) -> Result<Schedule, BuildError> {
    unimplemented!("oce-graph::compile — M0 scaffold (DAG + Kahn sort land with the M0 graph)")
}

/// Allocate the mutable [`RunState`] for a model: one fixed-size state slot per `[S]` block,
/// seeded from parameters (CDL has no `start` attribute; `01` §7). Connector values are seeded so
/// constant/source outputs hold their value before the first tick.
#[must_use]
pub fn allocate_state(_model: &ModelGraph) -> RunState {
    unimplemented!("oce-graph::allocate_state — M0 scaffold")
}

/// TICK: evaluate exactly one tick at absolute time `t_now` (seconds, monotonic non-decreasing).
/// External/host-driven inputs must already be staged into `state.values`. `[S]` blocks emit
/// output from prior state then update state; `[A]` blocks compute `y = f(p, t, u)`. The hot path
/// is allocation/IO/hashing/store-free (`01` §9).
///
/// `model` is the scheduler-facing [`Model`] view of the same in-memory artifact `schedule` was
/// compiled from.
pub fn eval_tick(state: &mut RunState, _schedule: &Schedule, _model: &Model, t_now: f64) {
    state.t = t_now;
    unimplemented!("oce-graph::eval_tick — M0 scaffold (eval loop lands with the M0 graph)")
}

/// Public alias for [`eval_tick`] used by `06`/`08` (`01` §6.2: the same function).
pub use eval_tick as run_tick;
