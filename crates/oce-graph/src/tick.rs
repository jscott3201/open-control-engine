//! TICK evaluation (`01` §7 state allocation + §9 the eval loop): the hot path. Allocation-free,
//! IO-free, hashing-free, store-free, selene-free.
//!
//! **Two passes per tick (binding correction to the §9 sketch).** Pass 1 *emits*: every block
//! produces all its outputs atomically (`[A]` via `step_algebraic`, `[S]` via `emit_from_state`
//! from **prior** state). Pass 2 *updates* `[S]` state via `update_state`. The update pass is
//! **separate** — run after every emit — so a loop-breaker's `update_state` reads its feedback
//! input at the **current** tick's value. (An inline per-block "emit then update" would make a
//! loop-breaker scheduled before its input's producer latch the *previous* tick's input, yielding a
//! two-tick delay instead of one for the canonical `Pre`/`UnitDelay` feedback loop.)

use oce_blocks::{Block, BlockKind, Ctx, NoopDiagnostics};
use oce_model::{BlockInstance, Model, Value};

use crate::{RunState, Schedule, StateSlot};

/// The per-tick evaluation bundle (`01` §9): the frozen artifacts (`model`, `schedule`, immutable
/// block impls) plus the sole mutable [`RunState`]. The block impls are `&` (not `&mut`): the arena
/// model keeps every `Block` method `&self`, with all mutable `[S]` state in `RunState.words`.
pub struct EvalContext<'a> {
    /// The flattened model the schedule was compiled from.
    pub model: &'a Model,
    /// The frozen schedule (`order`/`connector_order`/`driver_of`).
    pub schedule: &'a Schedule,
    /// Instantiated, parameter-resolved block impls, indexed by `BlockId.0`.
    pub blocks: &'a [Box<dyn Block>],
    /// Scheduler-owned diagnostics sink borrowed into block contexts.
    pub diagnostics: &'a dyn oce_blocks::Diagnostics,
    /// The sole mutable per-tick structure.
    pub state: &'a mut RunState,
}

/// BUILD step 4 (`01` §7): allocate the mutable [`RunState`] for a model. Lays out one fixed-size
/// state region per `[S]` block in a flat word buffer, seeds it from the block's parameters via
/// [`Block::init_state`] (CDL has no `start` attribute), seeds connector values (constant/source
/// outputs hold their value before tick 0; all others the type zero), and preallocates the
/// input-gather scratch so the tick never allocates.
#[must_use]
pub fn allocate_state(model: &Model, blocks: &[Box<dyn Block>]) -> RunState {
    let nb = model.blocks.len();
    let mut words: Vec<u64> = Vec::new();
    let mut slots: Vec<StateSlot> = Vec::new();
    let mut slot_of: Vec<usize> = vec![usize::MAX; nb];
    let mut max_arity = 0usize;

    for blk in &model.blocks {
        max_arity = max_arity.max(blk.inputs.len());
        let blk_impl = blocks[blk.id.0 as usize].as_ref();
        if blk_impl.kind() == BlockKind::Stateful {
            let len = blk_impl.state_len();
            let offset = words.len();
            words.resize(offset + len, 0);
            // Seed from parameters — NOT a `start` attribute (none exists in CDL; §7 req 2).
            blk_impl.init_state(&mut words[offset..offset + len], &blk.params);
            slot_of[blk.id.0 as usize] = slots.len();
            slots.push(StateSlot {
                block: blk.id,
                offset,
                len,
            });
        }
    }

    let values = init_values(model, blocks);
    let scratch = vec![Value::Real(0.0); max_arity];
    RunState {
        values,
        words,
        slots,
        slot_of,
        scratch,
        t: 0.0,
    }
}

/// Seed the connector value array (`01` §7 req 3): every connector starts at its type-appropriate
/// zero, then no-input `[A]` sources (e.g. `Constant`) are fired once so their outputs hold their
/// value before tick 0 — a downstream block reading them through the alias map sees the right value
/// pre-tick.
fn init_values(model: &Model, blocks: &[Box<dyn Block>]) -> Vec<Value> {
    let mut values: Vec<Value> = model
        .connectors
        .iter()
        .map(|c| c.value_type.zero_value())
        .collect();

    let noop = NoopDiagnostics;
    let cx = Ctx::new(0.0, &noop);
    for blk in &model.blocks {
        let blk_impl = blocks[blk.id.0 as usize].as_ref();
        if blk_impl.kind() == BlockKind::Algebraic && blk.inputs.is_empty() {
            blk_impl.step_algebraic(&cx, &[], &mut |o, v| {
                debug_assert!(
                    o < blk.outputs.len(),
                    "source emitted out-of-range output {o}"
                );
                values[blk.outputs[o].0 as usize] = v;
            });
        }
    }
    values
}

/// Gather a block's inputs into `scratch[..arity]` via the alias map: `scratch[i]` is the current
/// value of the output driving input `i` (or the input's own seeded slot if undriven).
fn gather(blk: &BlockInstance, schedule: &Schedule, values: &[Value], scratch: &mut [Value]) {
    for (i, &input) in blk.inputs.iter().enumerate() {
        scratch[i] = values[schedule.driver_of[input.0 as usize].0 as usize].clone();
    }
}

/// TICK: evaluate exactly one tick at absolute model time `t_now` (seconds, monotonic
/// non-decreasing). Host-driven external inputs must already be staged into `state.values`. The hot
/// path is allocation/IO/hashing/store-free (`01` §9). See the module docs for the two-pass
/// (emit-then-separately-update) ordering and why it is required.
pub fn eval_tick(ctx: &mut EvalContext, t_now: f64) {
    let model = ctx.model;
    let schedule = ctx.schedule;
    let blocks = ctx.blocks;
    let diag = ctx.diagnostics;
    let RunState {
        values,
        words,
        slots,
        slot_of,
        scratch,
        t,
    } = &mut *ctx.state;
    *t = t_now;
    let cx = Ctx::new(t_now, diag);
    // The schedule must be tick-ready: `compile` fills `driver_of` (a bare `topo_sort` result does
    // not). Caught in debug so a direct `topo_sort` → `eval_tick` misuse fails loudly, not via an
    // out-of-range gather.
    debug_assert_eq!(
        schedule.driver_of.len(),
        values.len(),
        "Schedule.driver_of not populated — build the schedule with compile(), not topo_sort()"
    );

    // PASS 1 — emit (atomic block firing: all of a block's outputs are produced in one call).
    for &bid in &schedule.order {
        let blk = &model.blocks[bid.0 as usize];
        gather(blk, schedule, values, scratch);
        let inputs = &scratch[..blk.inputs.len()];
        let blk_impl = blocks[bid.0 as usize].as_ref();
        match blk_impl.kind() {
            BlockKind::Algebraic => {
                blk_impl.step_algebraic(&cx, inputs, &mut |o, v| {
                    debug_assert!(
                        o < blk.outputs.len(),
                        "block emitted out-of-range output {o}"
                    );
                    values[blk.outputs[o].0 as usize] = v;
                });
            }
            BlockKind::Stateful => {
                let slot = slots[slot_of[bid.0 as usize]];
                let region = &words[slot.offset..slot.offset + slot.len];
                blk_impl.emit_from_state(&cx, inputs, region, &mut |o, v| {
                    debug_assert!(
                        o < blk.outputs.len(),
                        "block emitted out-of-range output {o}"
                    );
                    values[blk.outputs[o].0 as usize] = v;
                });
            }
        }
    }

    // PASS 2 — update [S] state, after every emit, so updates read current-tick inputs.
    for &bid in &schedule.order {
        let blk_impl = blocks[bid.0 as usize].as_ref();
        if blk_impl.kind() != BlockKind::Stateful {
            continue;
        }
        let blk = &model.blocks[bid.0 as usize];
        gather(blk, schedule, values, scratch);
        let inputs = &scratch[..blk.inputs.len()];
        let slot = slots[slot_of[bid.0 as usize]];
        let region = &mut words[slot.offset..slot.offset + slot.len];
        blk_impl.update_state(&cx, inputs, region);
    }
}

/// Public alias for [`eval_tick`] used by `06`/`08` (`01` §6.2: the same function).
pub use eval_tick as run_tick;
