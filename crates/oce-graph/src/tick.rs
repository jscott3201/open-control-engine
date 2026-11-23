//! TICK evaluation (`01` §7 state allocation + §9 the eval loop): the hot path. IO-free,
//! hashing-free, store-free, selene-free. The loop itself allocates nothing — schedule, state and
//! gather scratch are all preallocated at BUILD — but it calls into `Block::step_algebraic`,
//! which is not required to be allocation-free: `CDL.Reals.Sort` uses stack buffers through width
//! 64 and two `Vec`s above that width. `Reals.Log`/`Reals.Log10` warning messages allocate nothing
//! block-side, though a diagnostics sink may allocate while recording them. Do not read "the tick
//! never allocates" as a whole-engine guarantee.
//!
//! **HostTick v1.** Every `eval_tick` call is one state transition, even if a facade caller repeats
//! the model timestamp. The evaluator does not perform Modelica same-time event iteration or
//! convergence checks. `CDL.Logical.Pre` is therefore one-transition memory in this profile.
//!
//! **Two passes per tick (binding correction to the §9 sketch).** Pass 1 *emits*: every block
//! produces all its outputs atomically (`[A]` via `step_algebraic`, `[S]` via `emit_from_state`
//! from **prior** state). Pass 2 *updates* `[S]` state via `update_state`. The update pass is
//! **separate** — run after every emit — so a loop-breaker's `update_state` reads its feedback
//! input at the **current** tick's value. (An inline per-block "emit then update" would make a
//! loop-breaker scheduled before its input's producer latch the *previous* tick's input, yielding a
//! two-tick delay instead of one for the canonical `Pre`/`UnitDelay` feedback loop.)

use alloc::{boxed::Box, vec, vec::Vec};
use oce_blocks::{Block, BlockKind, Ctx, NoopDiagnostics};
use oce_model::{BlockId, BlockInstance, Model, Value};

use crate::topo::block_path;
use crate::{BuildError, MAX_STATE_WORDS, RunState, Schedule, ShadowSource, StateSlot};

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
    /// Whether this is the initial frame: the first since every state region was seeded (the
    /// frame driver's previous accepted time is unset). Blocks read it through
    /// [`Ctx::is_initial`]; issue #800, decision 3.
    pub initial: bool,
}

/// BUILD step 4 (`01` §7): allocate the mutable [`RunState`] for a model. Lays out one fixed-size
/// state region per `[S]` block in a flat word buffer, seeds it from the block's parameters via
/// [`Block::init_state`] (CDL has no `start` attribute), seeds connector values (constant/source
/// outputs hold their value before tick 0; all others the type zero), and preallocates the
/// input-gather scratch so the eval loop itself never allocates. Individual blocks may still
/// allocate inside `step_algebraic` — see the module header.
///
/// The state buffer is sized before anything is written: region lengths are summed with checked
/// arithmetic against [`MAX_STATE_WORDS`] and reserved with `try_reserve_exact`, so no
/// input-derived [`Block::state_len`] can panic on capacity overflow or abort on allocation
/// failure.
///
/// # Errors
/// [`BuildError::StateTooLarge`] when the summed `[S]` regions exceed [`MAX_STATE_WORDS`] (or the
/// allocator refuses the in-budget buffer). Nothing is seeded on that path.
pub fn allocate_state(model: &Model, blocks: &[Box<dyn Block>]) -> Result<RunState, BuildError> {
    let nb = model.blocks.len();
    let total = state_words_needed(model, blocks)?;
    let mut words: Vec<u64> = Vec::new();
    if words.try_reserve_exact(total).is_err() {
        let last = model
            .blocks
            .iter()
            .rev()
            .find(|blk| blocks[blk.id.0 as usize].kind() == BlockKind::Stateful)
            .map_or(BlockId(0), |blk| blk.id);
        return Err(BuildError::StateTooLarge {
            block: block_path(model, last),
            requested: total as u64,
            limit: MAX_STATE_WORDS,
        });
    }
    let mut slots: Vec<StateSlot> = Vec::new();
    let mut slot_of: Vec<usize> = vec![usize::MAX; nb];
    let mut max_arity = 0usize;

    for blk in &model.blocks {
        let blk_impl = blocks[blk.id.0 as usize].as_ref();
        // Declared inputs plus the `w`, `s` pair the event plan may append per settled input.
        let settled = blk_impl.settled_event_inputs().len();
        max_arity = max_arity.max(blk.inputs.len().saturating_add(settled.saturating_mul(2)));
        if blk_impl.kind() == BlockKind::Stateful {
            let len = blk_impl.state_len();
            let offset = words.len();
            // In budget and reserved above: `offset + len <= total <= MAX_STATE_WORDS`.
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
    // The settled-value shadow buffers (issue #800), only for a model that can need them.
    let (shadow_w, shadow_s, shadow_i, shadow_scratch) = if crate::needs_plan(blocks) {
        (
            vec![false; model.connectors.len()],
            vec![false; model.connectors.len()],
            vec![false; model.connectors.len()],
            vec![Value::Real(0.0); max_arity],
        )
    } else {
        (Vec::new(), Vec::new(), Vec::new(), Vec::new())
    };
    Ok(RunState {
        values,
        words,
        slots,
        slot_of,
        scratch,
        shadow_w,
        shadow_s,
        shadow_i,
        shadow_scratch,
        t: 0.0,
    })
}

/// Sum every `[S]` region length in declaration order, refusing at the first block whose region
/// overflows `usize` or crosses [`MAX_STATE_WORDS`]. Pure; allocates nothing.
fn state_words_needed(model: &Model, blocks: &[Box<dyn Block>]) -> Result<usize, BuildError> {
    let mut total = 0usize;
    for blk in &model.blocks {
        let blk_impl = blocks[blk.id.0 as usize].as_ref();
        if blk_impl.kind() != BlockKind::Stateful {
            continue;
        }
        let len = blk_impl.state_len();
        match total.checked_add(len) {
            Some(next) if next <= MAX_STATE_WORDS => total = next,
            over => {
                let requested = over.map_or(u64::MAX, |n| u64::try_from(n).unwrap_or(u64::MAX));
                return Err(BuildError::StateTooLarge {
                    block: block_path(model, blk.id),
                    requested,
                    limit: MAX_STATE_WORDS,
                });
            }
        }
    }
    Ok(total)
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

/// The shadow arrays of the settled-value rule: `w`, `s`, and `i` at the initial frame only.
#[derive(Clone, Copy)]
struct ShadowView<'a> {
    w: &'a [bool],
    s: &'a [bool],
    i: Option<&'a [bool]>,
}

/// Gather a block's inputs into `scratch` via the alias map: `scratch[i]` is the current value of
/// the output driving input `i` (or the input's own seeded slot if undriven). When the event plan
/// gives the block settled inputs, their `w`, `s` pairs follow the declared inputs, and at the
/// initial frame each settled declared input reads its cone at initialisation `i` in place of its
/// frame value (`settled` lists the block's settled ports, in the plan's order). Returns the
/// number of values gathered.
fn gather(
    blk: &BlockInstance,
    settled: &[usize],
    schedule: &Schedule,
    values: &[Value],
    shadow: ShadowView<'_>,
    scratch: &mut [Value],
) -> usize {
    for (i, &input) in blk.inputs.iter().enumerate() {
        scratch[i] = values[schedule.driver_of[input.0 as usize].0 as usize].clone();
    }
    let mut n = blk.inputs.len();
    if let Some(hidden) = schedule.events.hidden(blk.id.0 as usize) {
        for (slot, source) in hidden.iter().enumerate() {
            let (w, s, i) = shadow_values(*source, values, shadow);
            scratch[n] = w;
            scratch[n + 1] = s;
            n += 2;
            if shadow.i.is_some()
                && let Some(&port) = settled.get(slot)
                && port < blk.inputs.len()
            {
                scratch[port] = i;
            }
        }
    }
    n
}

/// The `(w, s, i)` values of one shadow source at this tick (`i` is meaningful at the initial
/// frame only; at a later frame it is the frame value or `false`, and nothing reads it).
fn shadow_values(
    source: ShadowSource,
    values: &[Value],
    shadow: ShadowView<'_>,
) -> (Value, Value, Value) {
    match source {
        ShadowSource::Level(c) => {
            let v = values[c.0 as usize].clone();
            (v.clone(), v.clone(), v)
        }
        ShadowSource::Pulse(c) => (
            Value::Boolean(false),
            Value::Boolean(false),
            values[c.0 as usize].clone(),
        ),
        ShadowSource::Sample(c) => (
            Value::Boolean(false),
            values[c.0 as usize].clone(),
            Value::Boolean(false),
        ),
        ShadowSource::Node(c) => (
            Value::Boolean(shadow.w[c.0 as usize]),
            Value::Boolean(shadow.s[c.0 as usize]),
            Value::Boolean(shadow.i.is_some_and(|i| i[c.0 as usize])),
        ),
    }
}

/// The shadow arrays of the settled-value rule and the scratch their evaluation gathers into.
struct Shadow<'a> {
    w: &'a mut [bool],
    s: &'a mut [bool],
    i: &'a mut [bool],
    scratch: &'a mut [Value],
}

/// Evaluate a marked logic block of the event plan twice more, on its inputs' `w` and `s`
/// values, into the shadow arrays at its output connectors, and at the initial frame a third
/// time on its inputs' `i` values. The diagnostics of these evaluations are dropped: the block's
/// own evaluation already reported them.
fn shadow_step(
    blk: &BlockInstance,
    blk_impl: &dyn Block,
    sources: &[ShadowSource],
    values: &[Value],
    shadow: &mut Shadow<'_>,
    initial: bool,
    t_now: f64,
) {
    let noop = NoopDiagnostics;
    let cx = Ctx::new(t_now, &noop);
    let passes = if initial { 3 } else { 2 };
    for pass in 0..passes {
        for (k, source) in sources.iter().enumerate() {
            let view = ShadowView {
                w: shadow.w,
                s: shadow.s,
                i: Some(&*shadow.i),
            };
            let (w, s, i) = shadow_values(*source, values, view);
            shadow.scratch[k] = match pass {
                0 => w,
                1 => s,
                _ => i,
            };
        }
        let target: &mut [bool] = match pass {
            0 => shadow.w,
            1 => shadow.s,
            _ => shadow.i,
        };
        blk_impl.step_algebraic(&cx, &shadow.scratch[..sources.len()], &mut |o, v| {
            if let (Some(out), Value::Boolean(b)) = (blk.outputs.get(o), v) {
                target[out.0 as usize] = b;
            }
        });
    }
}

/// Evaluate exactly one HostTick v1 transition at absolute model time `t_now` in seconds.
/// Host-driven external inputs must already be staged into `state.values`. Calling this function
/// again with the same `t_now` performs another transition; it is not an internal event iteration.
/// The hot path is allocation/IO/hashing/store-free (`01` §9). See the module docs for the two-pass
/// (emit-then-separately-update) ordering and why it is required. This function does not panic when
/// BUILD's validated model, schedule, block, and state-layout invariants hold.
pub fn eval_tick(ctx: &mut EvalContext, t_now: f64) {
    let model = ctx.model;
    let schedule = ctx.schedule;
    let blocks = ctx.blocks;
    let diag = ctx.diagnostics;
    let initial = ctx.initial;
    let RunState {
        values,
        words,
        slots,
        slot_of,
        scratch,
        shadow_w,
        shadow_s,
        shadow_i,
        shadow_scratch,
        t,
    } = &mut *ctx.state;
    *t = t_now;
    let cx = Ctx::new(t_now, diag).with_initial(initial);
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
        let blk_impl = blocks[bid.0 as usize].as_ref();
        let view = ShadowView {
            w: shadow_w,
            s: shadow_s,
            i: initial.then_some(&shadow_i[..]),
        };
        let n = gather(
            blk,
            blk_impl.settled_event_inputs(),
            schedule,
            values,
            view,
            scratch,
        );
        let inputs = &scratch[..n];
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
        // The event plan's shadow pass: a marked logic block's `w` and `s`, right after it emits
        // (its inputs' producers come earlier in the order).
        if let Some(sources) = schedule.events.step(bid.0 as usize) {
            let mut shadow = Shadow {
                w: shadow_w,
                s: shadow_s,
                i: shadow_i,
                scratch: shadow_scratch,
            };
            shadow_step(blk, blk_impl, sources, values, &mut shadow, initial, t_now);
        }
    }

    // PASS 2 — update [S] state, after every emit, so updates read current-tick inputs.
    for &bid in &schedule.order {
        let blk_impl = blocks[bid.0 as usize].as_ref();
        if blk_impl.kind() != BlockKind::Stateful {
            continue;
        }
        let blk = &model.blocks[bid.0 as usize];
        let view = ShadowView {
            w: shadow_w,
            s: shadow_s,
            i: initial.then_some(&shadow_i[..]),
        };
        let n = gather(
            blk,
            blk_impl.settled_event_inputs(),
            schedule,
            values,
            view,
            scratch,
        );
        let inputs = &scratch[..n];
        let slot = slots[slot_of[bid.0 as usize]];
        let region = &mut words[slot.offset..slot.offset + slot.len];
        blk_impl.update_state(&cx, inputs, region);
    }
}

/// Public alias for [`eval_tick`] used by `06`/`08` (`01` §6.2: the same function).
pub use eval_tick as run_tick;
