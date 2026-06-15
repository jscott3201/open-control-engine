#![forbid(unsafe_code)]
//! `oce-graph` — the deterministic dataflow scheduler/executor for the Open Control Engine
//! (`01-execution-model.md`).
//!
//! Two strict phases. **BUILD** (once per load, off the tick): build the direct-feedthrough
//! DAG over connector vertices, hard-reject algebraic loops (CDL §7.16), run an own Kahn
//! topological sort with declaration-order tie-break (FRAME D6), and allocate `[S]` state
//! seeded from parameters. **TICK** (the hot path): evaluate the frozen [`Schedule`] over flat
//! arrays — no graph walks, no hashing, no allocation, no IO, no store. This crate is
//! **Group A**: it has zero dependency on `oce-store`/`oce-store-mem` or any store/database
//! crate (D-OWNER-1; `01` §10).
//!
//! Public API names (normative, `01` §6.2): the frozen schedule is [`Schedule`] (alias
//! [`CompiledSchedule`]); BUILD entry point is [`compile`]; TICK entry point is [`eval_tick`]
//! (alias [`run_tick`]).
//!
//! Status: **M0 scaffold.** The type *shapes* match the spec; bodies are stubs
//! (`unimplemented!()`) and land in M0 (the hand-built-graph exit criteria) and M1.

use std::fmt;

use oce_blocks::Block;
use oce_model::{BlockId, ConnectorId, Model, ModelGraph, Value};

mod build;
mod topo;

pub use build::{FeedthroughDag, build_feedthrough_dag};
pub use topo::topo_sort;

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

/// A human-readable dotted instance/connector path used in BUILD diagnostics (e.g. the members of
/// a [`BuildError::AlgebraicLoop`]). At M0 it is rendered from the block class IRI and the dense
/// block/connector ids (`<class-iri>#b<block>.<dir>#c<conn>`); CXF ingest (M1) will enrich it with
/// the source instance names.
#[derive(Clone, PartialEq, Eq)]
pub struct ConnectorPath(pub String);

impl fmt::Display for ConnectorPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for ConnectorPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Render as the bare quoted path so `{members:?}` on the error reads cleanly.
        write!(f, "{:?}", self.0)
    }
}

/// A BUILD-phase error (typed; never a panic).
#[derive(Clone, Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BuildError {
    /// One or more connection cycles are not broken by a state-holding block. CDL §7.16 forbids
    /// algebraic loops. Carries the dotted connector paths of one concrete cycle (a single
    /// back-edge path from a deterministic DFS over the residual subgraph) for actionable
    /// diagnostics.
    #[error(
        "algebraic loop detected: {} connector(s) form a cycle not broken by a state-holding \
         block. CDL §7.16 forbids algebraic loops. Remedy: insert a delay block (CDL.Logical.Pre, \
         CDL.Discrete.UnitDelay) or an integrator into the loop, since their outputs do not depend \
         directly on the current input. Cycle members: {members:?}",
        .members.len()
    )]
    AlgebraicLoop {
        /// The dotted connector paths participating in one concrete cycle.
        members: Vec<ConnectorPath>,
    },

    /// The connector feedthrough graph is acyclic, but **atomic block firing** (one `emit` call
    /// produces all of a block's outputs, `01` §9) still cannot schedule the model: the block-level
    /// *emit-before* graph has a cycle. This arises when a stateful multi-output block's state-only
    /// (cut) output feeds a path that returns to one of its own feedthrough inputs — there is no
    /// single position at which the block can emit all its outputs consistently. Same remedy as a
    /// connector-level loop. Carries the dotted block paths of one concrete cycle.
    #[error(
        "block-granularity algebraic loop: {} block(s) form a feedthrough cycle that single-pass \
         atomic block firing cannot schedule (a stateful multi-output block whose state-only \
         output feeds back into one of its feedthrough inputs). CDL §7.16 forbids algebraic loops. \
         Remedy: insert a delay block (CDL.Logical.Pre, CDL.Discrete.UnitDelay) or an integrator \
         into the loop. Cycle members: {members:?}",
        .members.len()
    )]
    BlockAlgebraicLoop {
        /// The dotted block paths participating in one concrete block-level feedthrough cycle.
        members: Vec<ConnectorPath>,
    },
}

/// BUILD: compile a flattened, validated [`Model`] into a frozen [`Schedule`] — build the
/// direct-feedthrough DAG ([`build_feedthrough_dag`]), then run the deterministic own Kahn sort
/// with a declaration-order tie-break and algebraic-loop rejection ([`topo_sort`]); `01` §4–§6.
/// Runs once per load, off the tick.
///
/// `blocks[i]` is the instantiated, parameter-resolved block impl for `BlockId(i)` (index equals
/// `BlockId.0`); the feedthrough oracle is queried on these instances, never the bare class.
///
/// State allocation (`01` §7, [`allocate_state`]) is a sibling BUILD step the engine invokes
/// alongside `compile`; it is kept separate so the frozen [`Schedule`] stays a shareable,
/// state-free value.
///
/// # Errors
/// Returns [`BuildError::AlgebraicLoop`] if the direct-feedthrough graph contains a cycle that no
/// loop-breaker block cuts, or [`BuildError::BlockAlgebraicLoop`] if the connector graph is acyclic
/// but atomic block firing still cannot schedule it (`01` §9).
pub fn compile(model: &Model, blocks: &[Box<dyn Block>]) -> Result<Schedule, BuildError> {
    let dag = build_feedthrough_dag(model, blocks);
    topo_sort(&dag, model, blocks)
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

#[cfg(test)]
mod tests;
