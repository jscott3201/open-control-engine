//! The tune-at-rest guard for value-dependent structure: an edit whose value changes what the frozen
//! schedule was derived from is refused (issue #768).
//!
//! `resume` re-instantiates the edited blocks but never recomputes the schedule (R-PARAM-3), so a
//! parameter whose *value* changes a block's dependence on its inputs would leave a schedule built
//! for the old shape. `ParamRule::Structural` covers parameters whose every edit does that (`nin`,
//! `rowMax`); this guard covers a parameter that does it only for some values, such as
//! `PIDWithReset.controllerType`: in the PI and PID modes `trigger` feeds through to `y`, in the P
//! and PD modes it does not (design note `reset-timing.md`, decision 10), and P mode, holding no
//! state, reads it when it fires while PD reads it never.

use oce_blocks::{Block, BlockKind, lookup};
use oce_model::{BlockId, ParamTable, Value};

use crate::engine::Engine;

/// What a block's place in the frozen schedule is derived from: its port counts, which inputs
/// feed through to which outputs (the connector feedthrough DAG), which inputs it reads when it
/// fires (the block-level emit-before order: every input of an `[A]` block, the feeding-through
/// inputs of an `[S]` block), and each input's event role (the loop diagnostic's remedy).
/// `kind()` and `state_len` enter only through the reads-at-emit set: `resume` re-allocates state.
#[derive(PartialEq, Eq)]
struct ScheduleShape {
    inputs: usize,
    outputs: usize,
    feeds_through: Vec<bool>,
    reads_at_emit: Vec<bool>,
    event_roles: Vec<Option<oce_blocks::EventInputRole>>,
}

impl ScheduleShape {
    /// Derive the shape of `block`. `O(inputs × outputs)` calls of `feeds_through`.
    fn of(block: &dyn Block) -> Self {
        let signature = block.resolved_signature();
        let (inputs, outputs) = (signature.inputs.len(), signature.outputs.len());
        let mut feeds_through = Vec::with_capacity(inputs.saturating_mul(outputs));
        let mut reads_at_emit = Vec::with_capacity(inputs);
        let mut event_roles = Vec::with_capacity(inputs);
        let algebraic = block.kind() == BlockKind::Algebraic;
        for input in 0..inputs {
            let mut any = false;
            for output in 0..outputs {
                let feeds = block.feeds_through(input, output);
                any |= feeds;
                feeds_through.push(feeds);
            }
            reads_at_emit.push(algebraic || any);
            event_roles.push(block.event_input_role(input));
        }
        Self {
            inputs,
            outputs,
            feeds_through,
            reads_at_emit,
            event_roles,
        }
    }
}

impl Engine {
    /// Whether staging `value` for parameter `idx`, together with every edit already pending on
    /// the same block, would change the block's [`ScheduleShape`] from the one the frozen schedule
    /// was built for.
    ///
    /// The reference is the block instance the engine runs now. Every accepted edit keeps that
    /// shape (this guard refuses the others), so it is the shape of the instance the schedule was
    /// built from. The candidate is the block as `resume` would instantiate it: the model's
    /// parameters with every pending edit of that block and this value applied, through the same
    /// registry constructor. The constructed budget is not charged here: no parameter `set_param`
    /// accepts sizes a block's buffers (the sizing parameters are `Structural`), and `resume`
    /// still charges the whole model. Cost: one constructor call and `O(inputs × outputs)`
    /// `feeds_through` calls for each of the two instances. Never panics: a constructor accepts any
    /// parameter table and degrades an invalid value to its documented default.
    pub(super) fn edit_changes_schedule_shape(&self, idx: usize, value: &Value) -> bool {
        let edited = &self.params.entries[idx];
        let BlockId(block_index) = edited.block;
        let (Some(instance), Some(current)) = (
            self.model.blocks.get(block_index as usize),
            self.blocks.get(block_index as usize),
        ) else {
            return false;
        };
        let Some(entry) = lookup(&instance.class_iri) else {
            return false;
        };
        let mut params: ParamTable = instance.params.clone();
        for pending in self
            .params
            .entries
            .iter()
            .filter(|e| e.block == edited.block)
        {
            let staged = if core::ptr::eq(pending, edited) {
                value
            } else {
                &pending.value
            };
            if let Some(slot) = params.values.iter_mut().find(|(n, _)| *n == pending.name) {
                slot.1 = staged.clone();
            }
        }
        let candidate = (entry.make)(&params);
        ScheduleShape::of(current.as_ref()) != ScheduleShape::of(candidate.as_ref())
    }
}
