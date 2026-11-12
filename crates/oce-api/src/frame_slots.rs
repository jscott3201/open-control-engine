//! Slot-indexed complete-frame execution: names resolved to slots once, then `(t, &[Value])`
//! frames over the same core HostTick v1 driver as the named path (issue #444; design note
//! `docs/design/replay.md`, decisions 4 and 14).
//!
//! [`Engine::frame_slots`] resolves the executable boundary once into a [`FrameSlots`] map: input
//! slot `i` is row `i` of [`Engine::input_definitions`] (canonical lexical UTF-8 path order, which
//! is also the compiled-schedule blob's input order) and output slot `j` is the `j`-th boundary
//! output of a [`crate::CompletedFrame`] (the same lexical order, and the blob's output order).
//! [`Engine::execute_slots`] then submits one value per input slot and returns a borrowed
//! [`SlotFrame`] whose outputs are read in output-slot order.
//!
//! Both entries call [`oce_graph::frame::FrameDriver`]; this module adds only the slot map, its
//! incarnation check and a borrowed result. The refusal order, the error variants and the
//! atomicity of a refusal are the named path's, so the same frame refuses, or commits the same
//! state, through either entry. Once the map exists, an accepted slot frame allocates nothing
//! beyond what evaluation itself documents and what its warnings need (see
//! [`Engine::execute_slots`]).

use std::cell::{Cell, RefCell};
use std::fmt;
use std::sync::Arc;

use oce_blocks::Diagnostics;
use oce_graph::RunState;
use oce_graph::frame::{FrameDriver, FrameInputs, InputSlot};
use oce_model::{ConnectorId, Value, ValueType};

use crate::{AssertEvent, AssertLevel, Engine, InputDefinition, OcError};

/// The one-time name→slot map of one loaded executable's complete-frame boundary.
///
/// Input slot `i` is [`Self::inputs`]`[i]`, in the canonical lexical UTF-8 path order of
/// [`Engine::input_definitions`]; output slot `j` is [`Self::outputs`]`[j]`, in the lexical order
/// of [`crate::CompletedFrame::outputs`]. Both orders equal the compiled-schedule blob's I/O
/// tables. A host resolves its own names to slots here once (with [`Self::input_slot`] and
/// [`Self::output_slot`]) and then submits frames by slot with [`Engine::execute_slots`]. Both
/// sides are typed before any frame runs: an input slot by its definition's `value_type`, an
/// output slot by [`Self::output_types`].
///
/// The map belongs to one engine-local successful load incarnation, exactly like a
/// [`crate::PreparedInputFrame`]: every successful reload (identical bytes included) and every
/// dirty parameter resume invalidates it, and [`Engine::execute_slots`] then refuses it with
/// [`OcError::StalePreparedFrame`] rather than binding old slots to a new schema. Clean resume and
/// compatible restore keep it valid. It retains every fan-out target privately and exposes no
/// connector index, serialization or public constructor. Building it allocates in proportion to
/// the boundary; reading it allocates nothing. Debug prints counts only.
#[derive(Clone)]
pub struct FrameSlots {
    generation: Arc<()>,
    inputs: Vec<InputDefinition>,
    targets: Vec<Vec<ConnectorId>>,
    outputs: Vec<String>,
    output_types: Vec<ValueType>,
}

impl fmt::Debug for FrameSlots {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FrameSlots")
            .field("inputs", &self.inputs.len())
            .field("outputs", &self.outputs.len())
            .finish_non_exhaustive()
    }
}

impl FrameSlots {
    /// The input definitions in slot order (canonical lexical UTF-8 path order): the exact native
    /// type and inclusive domain each slot's value must satisfy. Does not allocate.
    #[must_use]
    pub fn inputs(&self) -> &[InputDefinition] {
        &self.inputs
    }

    /// The executable root boundary-output identities in output-slot order (lexical UTF-8), the
    /// identities [`crate::CompletedFrame::outputs`] pairs with its values. Does not allocate.
    #[must_use]
    pub fn outputs(&self) -> &[String] {
        &self.outputs
    }

    /// The exact native value type of each boundary output in output-slot order: entry `j` types
    /// [`Self::outputs`]`[j]`, so the slice always has the length of `outputs()` and is empty for
    /// an executable with no boundary output.
    ///
    /// Each entry is the declared type of the connector that drives the output (for a
    /// pass-through declared output, the type of the boundary input it copies), fixed at load and
    /// known before the first frame. It is the type alone, as [`InputDefinition::value_type`] is on
    /// the input side: no unit, bound or other attribute, and no projection-carrier coercion. An
    /// Enum output is [`ValueType::Enum`] with its exact class identity, never `Integer`.
    ///
    /// Invariant: for every frame accepted in the load incarnation this map was built from,
    /// through either entry, the `j`-th output value has exactly this type; `frame.output(j)`
    /// satisfies `value.value_type() == slots.output_types()[j]`, and so does the `j`-th entry of
    /// [`crate::CompletedFrame::outputs`]. The types belong to that incarnation, like the paths.
    /// Once a successful reload or a dirty parameter resume supersedes the map it keeps returning
    /// the types it was built with, which say nothing about the executable now loaded, and
    /// [`Engine::execute_slots`] refuses it with [`OcError::StalePreparedFrame`]; compare
    /// boundaries only between maps built from the loads being compared. Does not allocate and
    /// does not panic.
    #[must_use]
    pub fn output_types(&self) -> &[ValueType] {
        &self.output_types
    }

    /// The input slot of canonical input `path`, or `None` when `path` is not a boundary input
    /// of this executable (outputs, internal driven inputs and unknown names alike). Exact byte
    /// comparison: no alias expansion. Logarithmic in the input count; does not allocate.
    #[must_use]
    pub fn input_slot(&self, path: &str) -> Option<usize> {
        self.inputs
            .binary_search_by(|definition| definition.path.as_str().cmp(path))
            .ok()
    }

    /// The output slot of boundary-output identity `path`, or `None` when it is not one. Exact
    /// byte comparison: read-only output aliases that are not themselves boundary identities do
    /// not resolve. Logarithmic in the output count; does not allocate.
    #[must_use]
    pub fn output_slot(&self, path: &str) -> Option<usize> {
        self.outputs
            .binary_search_by(|output| output.as_str().cmp(path))
            .ok()
    }
}

/// One accepted slot-indexed HostTick v1 transition, borrowed from the engine that committed it.
///
/// Outputs are the same values, with the same exact bits, as the [`crate::CompletedFrame`] the
/// named path would return for the same frame, read in output-slot order
/// ([`FrameSlots::outputs`]). Warnings are the same records in evaluator emission order. Unlike a
/// `CompletedFrame` nothing is owned: the view borrows the engine, so the next frame, reload or
/// restore cannot run while it is alive, and a host that keeps values copies them out. It is not
/// a durable receipt or replay record. Accessors do not allocate or panic.
pub struct SlotFrame<'a> {
    time: f64,
    sequence: u64,
    state: &'a RunState,
    outputs: &'a [(String, ConnectorId)],
    diagnostics: &'a [AssertEvent],
}

impl fmt::Debug for SlotFrame<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SlotFrame")
            .field("time", &self.time)
            .field("sequence", &self.sequence)
            .field("outputs", &self.outputs().collect::<Vec<_>>())
            .field("diagnostics", &self.diagnostics)
            .finish()
    }
}

impl<'a> SlotFrame<'a> {
    /// Model time in seconds, with the submitted finite binary64 bits (signed zero included).
    #[must_use]
    pub fn time(&self) -> f64 {
        self.time
    }

    /// Accepted-frame position within one [`Engine`] lifetime: the same counter the named path
    /// advances, starting at one, never wrapping, advanced only by accepted frames.
    #[must_use]
    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    /// The number of output slots, [`FrameSlots::outputs`]`.len()`.
    #[must_use]
    pub fn output_count(&self) -> usize {
        self.outputs.len()
    }

    /// The value of output `slot`, or `None` when `slot` is not below [`Self::output_count`].
    /// Native type, declared units and exact Real bits.
    #[must_use]
    pub fn output(&self, slot: usize) -> Option<&'a Value> {
        let state = self.state;
        self.outputs
            .get(slot)
            .map(|(_, id)| oce_graph::frame::output_value(state, *id))
    }

    /// Every output value in output-slot order.
    pub fn outputs(&self) -> impl ExactSizeIterator<Item = &'a Value> + 'a {
        let state = self.state;
        self.outputs
            .iter()
            .map(move |(_, id)| oce_graph::frame::output_value(state, *id))
    }

    /// The transition's warnings in evaluator emission order, without sorting or deduplication.
    /// Warnings neither refuse execution nor escalate to an error or equipment interlock.
    #[must_use]
    pub fn diagnostics(&self) -> &'a [AssertEvent] {
        self.diagnostics
    }
}

/// A slot frame's values beside the map's targets and the engine's definitions, as the core
/// driver reads them.
struct SlotInputs<'a> {
    definitions: &'a [InputDefinition],
    targets: &'a [Vec<ConnectorId>],
    values: &'a [Value],
}

impl FrameInputs for SlotInputs<'_> {
    fn slot_count(&self) -> usize {
        self.definitions.len()
    }

    fn value_count(&self) -> usize {
        self.values.len()
    }

    fn slot(&self, slot: usize) -> InputSlot<'_> {
        let definition = &self.definitions[slot];
        InputSlot {
            targets: &self.targets[slot],
            min: definition.min.as_ref(),
            max: definition.max.as_ref(),
        }
    }

    fn value(&self, slot: usize) -> &Value {
        &self.values[slot]
    }
}

/// A warning sink that overwrites retained records in place, so a frame whose warnings fit what
/// earlier frames left behind allocates nothing.
struct ReusingCollector<'a> {
    events: RefCell<&'a mut Vec<AssertEvent>>,
    len: Cell<usize>,
}

impl Diagnostics for ReusingCollector<'_> {
    fn warn(&self, source: &str, message: &str, t: f64) {
        let mut events = self.events.borrow_mut();
        let len = self.len.get();
        if let Some(event) = events.get_mut(len) {
            event.block.clear();
            event.block.push_str(source);
            event.message.clear();
            event.message.push_str(message);
            event.t = t;
            event.level = AssertLevel::Warning;
        } else {
            events.push(AssertEvent {
                block: source.to_owned(),
                message: message.to_owned(),
                t,
                level: AssertLevel::Warning,
            });
        }
        self.len.set(len + 1);
    }
}

impl Engine {
    /// Resolve the loaded executable's complete-frame boundary to slots, once.
    ///
    /// Input slots follow [`Self::input_definitions`] and output slots the lexical boundary-output
    /// order of [`crate::CompletedFrame::outputs`]; see [`FrameSlots`] for the invariants. No
    /// engine mutation occurs. Allocates the owned map in proportion to the boundary (inputs, their
    /// fan-out targets, outputs and output types); call it after each successful load or dirty
    /// resume, not per frame. A clean resume, a compatible restore and a failed load keep an
    /// existing map valid.
    ///
    /// # Errors
    /// `State(NoLoadedModel)` before a successful load, or `State(PendingParameterEdits)` while
    /// edits await resume. Does not panic.
    pub fn frame_slots(&self) -> Result<FrameSlots, OcError> {
        self.frame_preconditions()?;
        let definitions = &self.io.frame_definitions;
        let targets = definitions
            .iter()
            .map(|definition| {
                self.io
                    .input_binding(&definition.path)
                    .map_or_else(Vec::new, |binding| binding.targets.clone())
            })
            .collect();
        Ok(FrameSlots {
            generation: Arc::clone(&self.frame_generation),
            inputs: definitions.clone(),
            targets,
            outputs: self
                .io
                .frame_outputs
                .iter()
                .map(|(path, _)| path.clone())
                .collect(),
            // The declared type of each output's driving connector. Load admits a boundary output
            // only with an in-range source (the same guarantee `SlotFrame::output` reads state
            // under), so the index is in bounds for every loaded executable.
            output_types: self
                .io
                .frame_outputs
                .iter()
                .map(|(_, source)| self.model.connectors[source.0 as usize].value_type)
                .collect(),
        })
    }

    /// Commit exactly one HostTick v1 transition at `time` model seconds from one value per input
    /// slot of `slots`, and borrow its outputs in output-slot order.
    ///
    /// The slot-indexed twin of [`Self::prepare_frame`] followed by [`Self::execute_frame`]: the
    /// same core driver ([`oce_graph::frame::FrameDriver`]) checks and commits it, so a frame whose
    /// `values[i]` is the named path's value for `slots.inputs()[i].path` refuses with the same
    /// error, or commits the same state, outputs, warnings and sequence. `values` is borrowed for
    /// the call only; nothing is coerced, defaulted or held from an earlier frame. Equal finite
    /// time advances again.
    ///
    /// Once `slots` exists an accepted frame allocates nothing itself: values are cloned into
    /// their fan-out targets (a `Value::String` shares its storage) and the result borrows the
    /// engine. The evaluator's documented exceptions remain (a `CDL.Reals.Sort` wider than 64
    /// inputs), and warning records are reused in place, so only a frame that emits more warnings,
    /// or longer source or message text, than any earlier slot frame of this engine allocates.
    ///
    /// ```
    /// use oce_api::{Engine, OcError, Value};
    /// fn run(engine: &mut Engine, frames: &[(f64, Vec<Value>)]) -> Result<Vec<Value>, OcError> {
    ///     let slots = engine.frame_slots()?; // once per load
    ///     let mut last = Vec::new();
    ///     for (time, values) in frames {
    ///         let frame = engine.execute_slots(&slots, *time, values)?;
    ///         last = frame.outputs().cloned().collect();
    ///     }
    ///     Ok(last)
    /// }
    /// ```
    ///
    /// # Errors
    /// First cause: `State(NoLoadedModel)`, `State(PendingParameterEdits)`, `StalePreparedFrame`
    /// (`slots` belongs to another engine or a superseded incarnation), `NonFiniteTime`,
    /// `TimeOutOfRange`, `TimeRegression`, `NonFiniteTimeStep`, `ModelTimeUnrepresentable`;
    /// then the value count: fewer values than slots is `FrameMissingInput` naming the first
    /// absent slot's path (what the named path returns for the same omission), more is
    /// `FrameExtraInputs`, refused before any value is read; then, per slot in slot order,
    /// `InputType`, `InputNonFinite` (only under [`crate::NonFiniteInputPolicy::Refuse`]) and
    /// `InputDomain`, each naming the slot's canonical path; finally `FrameSequenceExhausted`.
    /// Every returned error precedes mutation: run state, clock, sequence, restore readiness and
    /// latest-state views are unchanged. Panic, allocation failure and process death are
    /// excluded; nothing here panics on caller input.
    pub fn execute_slots(
        &mut self,
        slots: &FrameSlots,
        time: f64,
        values: &[Value],
    ) -> Result<SlotFrame<'_>, OcError> {
        self.frame_preconditions()?;
        if !Arc::ptr_eq(&self.frame_generation, &slots.generation) {
            return Err(OcError::StalePreparedFrame);
        }
        let inputs = SlotInputs {
            definitions: &self.io.frame_definitions,
            targets: &slots.targets,
            values,
        };
        let collector = ReusingCollector {
            events: RefCell::new(&mut self.slot_warnings),
            len: Cell::new(0),
        };
        let accepted = FrameDriver {
            model: &self.model,
            schedule: &self.schedule,
            blocks: &self.blocks,
            state: &mut self.state,
            prev_t: &mut self.prev_t,
            sequence: &mut self.accepted_frame_sequence,
        }
        .execute(time, &inputs, self.non_finite_input_policy, &collector);
        let warnings = collector.len.get();
        let sequence = accepted.map_err(|refusal| self.frame_refusal(refusal))?;
        // An accepted frame is the first mutation boundary: it closes startup restore.
        self.durable_restore_ready = false;
        #[cfg(test)]
        crate::frame::count_transition();
        Ok(SlotFrame {
            time,
            sequence,
            state: &self.state,
            outputs: &self.io.frame_outputs,
            diagnostics: &self.slot_warnings[..warnings],
        })
    }
}

#[cfg(test)]
#[path = "frame_slots_tests.rs"]
mod tests;
