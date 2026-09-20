//! Complete input preparation and one in-place HostTick transition with an owned outcome.

use std::fmt;
use std::sync::Arc;

use oce_model::{ConnectorId, Value};
use oce_store::Store;

use crate::{AssertEvent, Engine, EngineStateError, OcError};

/// One fully resolved complete input submission, owned independently of its caller's buffers.
///
/// Preparation does not execute or stage inputs. This opaque artifact retains every fan-out
/// target and the exact values, including Real bits. It belongs to one engine-local successful
/// load incarnation. Every successful reload and dirty parameter resume invalidates it; clean
/// resume and compatible state restore alone do not. Later execution must recheck compatibility
/// and the time guard. [`Engine::execute_frame`] consumes it, including on refusal.
///
/// Allocations are proportional to boundary inputs and targets. Strings use `Value`'s shared
/// immutable ownership. There is deliberately no serialization, public constructor, resolved
/// handle access, or durable/cross-engine identity. Debug prints counts and model seconds only.
///
/// ```compile_fail
/// fn persist(frame: &oce_api::PreparedInputFrame) {
///     let _ = serde_json::to_vec(frame); // No Serialize implementation.
/// }
/// ```
pub struct PreparedInputFrame {
    generation: Arc<()>,
    time: f64,
    inputs: Vec<(Vec<ConnectorId>, Value)>,
}

/// An independently owned, immutable result of one accepted complete-frame HostTick transition.
///
/// Contains only executable root boundary outputs, in lexical UTF-8 identity order, with exact
/// native [`Value`] types/bits, plus warnings in evaluator emission order. Internal driver aliases
/// are not extra outputs; distinct declared outputs sharing one driver remain distinct. Undriven
/// declarations absent from the executable are not fabricated. An empty output set is valid.
///
/// Retaining or cloning this result does not retain mutable engine state. Later frames,
/// restore, reload and parameter resume cannot change it. A private load/rebuild fence
/// binds it to its executable/IO/build/profile context; it exposes no portable identity or authority.
/// This is computation evidence, not a persistence, replay, freshness or equipment-delivery receipt.
/// No serialization or public constructor is provided. Accessors do not allocate or panic.
///
/// ```compile_fail
/// fn persist(frame: &oce_api::CompletedFrame) {
///     let _ = serde_json::to_vec(frame); // Deliberately not a wire record.
/// }
/// ```
#[derive(Clone)]
pub struct CompletedFrame {
    // Retain the allocation, but never expose even its Debug representation as an identity.
    _generation: Arc<()>,
    time: f64,
    sequence: u64,
    outputs: Vec<(String, Value)>,
    diagnostics: Vec<AssertEvent>,
}

impl fmt::Debug for CompletedFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CompletedFrame")
            .field("time", &self.time)
            .field("sequence", &self.sequence)
            .field("outputs", &self.outputs)
            .field("diagnostics", &self.diagnostics)
            .finish_non_exhaustive()
    }
}

impl CompletedFrame {
    /// Model time in seconds, preserving the supplied finite binary64 bits (including signed zero).
    #[must_use]
    pub fn time(&self) -> f64 {
        self.time
    }

    /// Accepted-frame position within one [`Engine`] lifetime, starting at one and never wrapping.
    ///
    /// Only successful complete-frame execution increments it. Refusals do not;
    /// reload, clean/dirty resume and restore never reset or rewind it. It is not a persisted
    /// replay position, deployment generation, lease, authentication or cross-engine identity.
    #[must_use]
    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    /// All completed executable boundary `(identity, value)` pairs, sorted lexically by identity.
    /// Values retain their native type, units as declared by the executable, and exact Real bits.
    #[must_use]
    pub fn outputs(&self) -> &[(String, Value)] {
        &self.outputs
    }

    /// Owned warning records borrowed in deterministic evaluator emission order, without sorting
    /// or deduplication. Sources keep producer semantics (currently class-level, not instances).
    /// Warnings neither refuse execution nor escalate to an error or equipment interlock.
    #[must_use]
    pub fn diagnostics(&self) -> &[AssertEvent] {
        &self.diagnostics
    }
}

#[cfg(test)]
#[path = "frame_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "frame_commit_tests.rs"]
mod commit_tests;

impl fmt::Debug for PreparedInputFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedInputFrame")
            .field("time", &self.time)
            .field("inputs", &self.inputs.len())
            .field(
                "targets",
                &self.inputs.iter().map(|(ids, _)| ids.len()).sum::<usize>(),
            )
            .finish_non_exhaustive()
    }
}

impl<S: Store> Engine<S> {
    /// Consume a prepared submission and commit exactly one HostTick v1 transition in place.
    ///
    /// Rechecks readiness, incarnation and time, then sequence capacity, before any mutation.
    /// Stages all resolved fan-out targets, closes durable-restore readiness, evaluates once,
    /// refreshes latest-state inspection and returns an independent [`CompletedFrame`]. Equal
    /// finite time advances again; there is no event iteration. No Store method or host callback
    /// is invoked. This is the only public state-advancing execution surface; cadence and
    /// persistence orchestration are host responsibilities, not alternative execution profiles.
    ///
    /// Allocation/copy costs beyond normal evaluation scale with boundary outputs and emitted
    /// warnings; staging scales with prepared fan-out. There is no run-state clone or rollback.
    ///
    /// ```
    /// use oce_api::{CompletedFrame, Engine, OcError, Value};
    /// fn advance(engine: &mut Engine, time: f64, observations: &[(&str, Value)])
    ///     -> Result<CompletedFrame, OcError>
    /// {
    ///     let prepared = engine.prepare_frame(time, observations)?;
    ///     engine.execute_frame(prepared)
    /// }
    /// ```
    ///
    /// ```compile_fail
    /// fn twice(engine: &mut oce_api::Engine, frame: oce_api::PreparedInputFrame) {
    ///     let _ = engine.execute_frame(frame);
    ///     let _ = engine.execute_frame(frame); // A submission cannot be reused.
    /// }
    /// ```
    ///
    /// # Errors
    /// First cause: `State(NoLoadedModel)`, `State(PendingParameterEdits)`, `StalePreparedFrame`,
    /// `NonFiniteTime`, `TimeRegression`, `ModelTimeUnrepresentable`, `FrameSequenceExhausted`.
    /// Every returned error precedes mutation: the entire execution image, restore readiness,
    /// sequence and retained results remain unchanged. The moved plan is consumed even on refusal.
    /// No ordinary recoverable operation remains after preflight under validated BUILD invariants.
    /// Panic, process death, allocation failure, cancellation and concurrency outside the existing
    /// exclusive-borrow guarantee are excluded. No Store/persistence/actuator atomicity is promised.
    pub fn execute_frame(&mut self, frame: PreparedInputFrame) -> Result<CompletedFrame, OcError> {
        self.check_prepared_frame(&frame)?;
        let sequence = self
            .accepted_frame_sequence
            .checked_add(1)
            .ok_or(OcError::FrameSequenceExhausted)?;

        // COMMIT: only infallible operations remain. No Store calls occur.
        for (targets, value) in &frame.inputs {
            for target in targets {
                self.state.values[target.0 as usize] = value.clone();
            }
        }
        let collector = crate::observations::AssertCollector::default();
        self.transition_host_tick(frame.time, &collector);
        self.accepted_frame_sequence = sequence;
        Ok(CompletedFrame {
            _generation: frame.generation,
            time: frame.time,
            sequence,
            outputs: self
                .io
                .frame_outputs
                .iter()
                .map(|(path, id)| (path.clone(), self.state.values[id.0 as usize].clone()))
                .collect(),
            diagnostics: collector.events.into_inner(),
        })
    }

    /// Own a canonical-path-ordered snapshot of all executable boundary input definitions.
    ///
    /// Each input is required exactly once. Internal driven inputs and output aliases are not
    /// determinants. Paths are expanded authored identities, not positional handles; there is
    /// currently no accepted input-alias namespace. The snapshot is metadata, not a reusable
    /// schema or capability: refresh after load or reconfiguration. Allocates owned paths and
    /// definitions proportional to the schema. No Store calls or engine mutation occur.
    ///
    /// # Errors
    /// Returns `State(NoLoadedModel)` before a successful load, or `State(PendingParameterEdits)`
    /// while edits await resume. Does not panic on caller input.
    pub fn input_definitions(&self) -> Result<Vec<crate::InputDefinition>, OcError> {
        self.frame_preconditions()?;
        Ok(self.io.frame_definitions.clone())
    }

    /// Resolve an entire typed input frame without changing any engine or Store state.
    ///
    /// `time` is finite nondecreasing model seconds; equal time is allowed. Keys use the same
    /// canonical identity resolver as the input definitions; only executable boundary inputs are
    /// accepted here. All values are required exactly once, with exact types and declared domains.
    /// Unbounded Real signals admit nonfinite values; a declared bound requires its comparison
    /// to hold (thus rejects NaN). Values are never coerced through Store carriers.
    ///
    /// Returns an owned opaque plan, not a completed transition. Borrowed keys are not retained.
    /// Allocates in proportion to schema inputs and resolved fan-out targets, not key length.
    /// Collect all observations before preparation, then consume the plan with
    /// [`Self::execute_frame`]. No sparse staging or seed/hold-last substitution is available.
    ///
    /// ```
    /// use oce_api::{Engine, OcError, PreparedInputFrame, Value};
    /// fn prepare_observations(
    ///     engine: &Engine, time: f64, observed: &[(String, Value)],
    /// ) -> Result<PreparedInputFrame, OcError> {
    ///     // Obtain each required observation from the host, never from type seeds or bounds.
    ///     let entries: Vec<_> = observed.iter()
    ///         .map(|(key, value)| (key.as_str(), value.clone())).collect();
    ///     engine.prepare_frame(time, &entries)
    /// }
    /// ```
    ///
    /// # Errors
    /// Deterministic precedence: unloaded, pending edits, nonfinite/regressing/unrepresentable
    /// time; then unknown/non-boundary keys in lexical UTF-8 order; duplicate canonical input;
    /// missing canonical input; finally type/domain in canonical input order (type first).
    /// Only the first typed cause is returned. Unknown/non-boundary key errors retain at most
    /// 64 UTF-8 bytes plus the original byte count; schema-key errors name the canonical path.
    /// No staging prefix, time advance, diagnostic replacement or restore-window closure occurs
    /// on either success or refusal. Does not panic on caller input; allocation failure is excluded.
    pub fn prepare_frame(
        &self,
        time: f64,
        entries: &[(&str, Value)],
    ) -> Result<PreparedInputFrame, OcError> {
        let mut frame = PreparedInputFrame {
            generation: Arc::clone(&self.frame_generation),
            time,
            inputs: Vec::new(),
        };
        self.check_prepared_frame(&frame)?;
        let definitions = &self.io.frame_definitions;
        let mut values = vec![None; definitions.len()];
        let mut invalid: Option<(&str, bool)> = None;
        let mut duplicate: Option<usize> = None;
        for &(key, ref value) in entries {
            let binding = self.io.input_binding(key);
            if let Some(index) = binding.and_then(|binding| binding.boundary_index) {
                if values[index].replace(value).is_some() {
                    duplicate = Some(duplicate.map_or(index, |old| old.min(index)));
                }
            } else if invalid.is_none_or(|(old, _)| key < old) {
                invalid = Some((
                    key,
                    binding.is_some() || self.io.resolve_output(key).is_some(),
                ));
            }
        }
        if let Some((key, known)) = invalid {
            let mut end = key.len().min(64);
            while !key.is_char_boundary(end) {
                end -= 1;
            }
            let prefix = key[..end].to_owned();
            let bytes = key.len();
            return Err(if known {
                OcError::FrameNotInput { prefix, bytes }
            } else {
                OcError::FrameUnknownInput { prefix, bytes }
            });
        }
        if let Some(index) = duplicate {
            return Err(OcError::FrameDuplicateInput(
                definitions[index].path.clone(),
            ));
        }
        if let Some(index) = values.iter().position(Option::is_none) {
            return Err(OcError::FrameMissingInput(definitions[index].path.clone()));
        }
        let mut inputs = Vec::with_capacity(definitions.len());
        for (definition, value) in definitions.iter().zip(values) {
            let value = value.expect("completeness checked");
            let binding = self
                .io
                .input_binding(&definition.path)
                .expect("schema binding");
            if !binding.accepts_type(&self.model, value) {
                return Err(OcError::InputType(definition.path.clone()));
            }
            if !definition.accepts_domain(value) {
                return Err(OcError::InputDomain(definition.path.clone()));
            }
            inputs.push((binding.targets.clone(), value.clone()));
        }
        frame.inputs = inputs;
        Ok(frame)
    }

    fn frame_preconditions(&self) -> Result<(), OcError> {
        if !self.loaded {
            return Err(EngineStateError::NoLoadedModel.into());
        }
        if self.params_dirty {
            return Err(EngineStateError::PendingParameterEdits.into());
        }
        Ok(())
    }

    /// Shared compatibility/time preflight: no name rebinding, Store access or mutation.
    pub(crate) fn check_prepared_frame(&self, frame: &PreparedInputFrame) -> Result<(), OcError> {
        self.frame_preconditions()?;
        if !Arc::ptr_eq(&self.frame_generation, &frame.generation) {
            return Err(OcError::StalePreparedFrame);
        }
        self.validate_frame_time(frame.time)
    }

    fn validate_frame_time(&self, time: f64) -> Result<(), OcError> {
        if !time.is_finite() {
            return Err(OcError::NonFiniteTime { now: time });
        }
        if let Some(prev) = self.prev_t
            && time < prev
        {
            return Err(OcError::TimeRegression { now: time, prev });
        }
        if !self.time_is_representable(time) {
            return Err(OcError::ModelTimeUnrepresentable { now: time });
        }
        Ok(())
    }
}
