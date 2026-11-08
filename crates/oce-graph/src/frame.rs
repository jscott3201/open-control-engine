//! The HostTick v1 frame driver (`docs/execution-profile.md`; design note
//! `docs/design/compiled-schedule-blob.md`, decision 11): the one implementation of what turns
//! [`eval_tick`] into a frame.
//!
//! [`eval_tick`] evaluates the frozen schedule once. A *frame* adds the rules
//! around it: time checks, per-input checks, the accepted-frame sequence, staging every value into
//! its fan-out targets, and the previous-time update. Those rules live here, slot-indexed and
//! name-free, so the `std` facade (`oce_api::Engine::prepare_frame` / `execute_frame`) and the
//! `no_std + alloc` blob entry ([`crate::blob::Executable`]) call the same code and HostTick v1
//! exists once.
//!
//! # Refusal order
//!
//! [`FrameDriver::execute`] refuses with the first failure in this order, and every refusal
//! precedes any mutation of the run state, the previous time or the sequence:
//!
//! 1. time ([`check_time`]): finite ([`FrameRefusal::NonFiniteTime`]); magnitude at most
//!    [`MAX_FRAME_TIME_MAGNITUDE`] ([`FrameRefusal::TimeOutOfRange`]); not before the previous
//!    accepted time ([`FrameRefusal::TimeRegression`]); a finite step from it
//!    ([`FrameRefusal::NonFiniteTimeStep`]); representable by every stateful block's state
//!    ([`FrameRefusal::ModelTimeUnrepresentable`]);
//! 2. one value per input slot ([`FrameRefusal::InputCount`]);
//! 3. per slot, in slot order ([`check_input`]): the exact type ([`FrameRefusal::InputType`]),
//!    then the [`NonFiniteInputPolicy`] ([`FrameRefusal::InputNonFinite`]), then the declared
//!    domain ([`FrameRefusal::InputDomain`]);
//! 4. the sequence ([`FrameRefusal::SequenceExhausted`]).
//!
//! The facade resolves names, refuses unknown, duplicate and missing keys between steps 1 and 3,
//! and maps each slot back to its canonical path; that is the facade's documented order.
//!
//! # Commit
//!
//! After the last check only infallible work remains: each value is staged into every fan-out
//! target, [`eval_tick`] runs once, the previous time becomes the frame's time
//! and the sequence advances. An equal timestamp is a new transition (HostTick v1 has no
//! same-time event iteration). The driver allocates nothing; the carve-outs in
//! `docs/architecture.md` (a `CDL.Reals.Sort` wider than 64 inputs, and whatever the diagnostics
//! sink does) still apply to the evaluation itself.

use alloc::boxed::Box;

use oce_blocks::{Block, Diagnostics, class_is_target_bound};
use oce_model::{ConnectorId, Model, Value, enum_descriptor};

use crate::{EvalContext, RunState, Schedule, eval_tick};

/// The largest accepted frame-time magnitude in model seconds: `f64::MAX / 4` (exactly
/// `2^1022 · (1 − 2^−53)`), about 4.49e307.
///
/// Guarantees for times `t` accepted within `[-MAX_FRAME_TIME_MAGNITUDE, MAX_FRAME_TIME_MAGNITUDE]`:
///
/// - **Single differences.** Any difference of two accepted times (one step, or the span since a
///   timer started) is at most `f64::MAX / 2` before rounding, and so finite after it.
/// - **Accumulations.** A running sum of rounded steps (an elapsed-time accumulator or a unit-slope
///   forward-Euler integrator) can round upward past the exact span. With round-to-nearest each
///   step and each addition gains at most a factor `1 + 2^−53`, so after `n` accumulated steps the
///   sum is at most `(f64::MAX / 2) · (1 + 2^−53)^(2n)`, finite for every `n` below about `3e15`
///   steps. The factor-of-two headroom over the exact span is what absorbs that drift; with
///   `f64::MAX / 2` there was none, and a five-step schedule overflowed.
///
/// Nothing bounds model maths applied to time (a slope other than one, a gain): that remains the
/// model's arithmetic. Frames outside the range are refused with
/// [`FrameRefusal::TimeOutOfRange`] (the facade's `OcError::TimeOutOfRange`) whatever the model
/// contains. UNIX epoch seconds, milliseconds and nanoseconds are all many orders of magnitude
/// inside it.
pub const MAX_FRAME_TIME_MAGNITUDE: f64 = f64::MAX / 4.0;

/// What frame admission does with a NaN (either sign, any payload) or infinite value supplied for
/// a Real boundary input.
///
/// The policy governs only which frames are accepted; it never changes how an accepted frame
/// evolves state, and blocks compute exactly the same bits under either variant. It is host
/// configuration, not model or blob content: the facade keeps it across loads, reloads, parameter
/// resumes and state restores, and no snapshot, checkpoint, replay record or blob carries it.
///
/// The default is [`Self::Admit`], which preserves the existing contract that the bound
/// strict-bit conformance evidence was captured under. **Production hosts are recommended to
/// select [`Self::Refuse`]**, on the facade and on a target alike. The default is planned to
/// become `Refuse` at the next native-evidence refresh (GitHub issue #363). Non-exhaustive so
/// further policies can be added.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum NonFiniteInputPolicy {
    /// Accept non-finite Real values on unbounded inputs and pass them to blocks unchanged; a
    /// declared comparison bound still refuses NaN and the infinity beyond it with
    /// [`FrameRefusal::InputDomain`] (the facade's `OcError::InputDomain`). This is the default.
    #[default]
    Admit,
    /// Refuse every non-finite Real input value, bounded or not, with
    /// [`FrameRefusal::InputNonFinite`] (the facade's `OcError::InputNonFinite`, naming the
    /// canonical input). It is checked after the input's type and before its domain, at
    /// preparation and again at execution, so a facade plan prepared under `Admit` cannot carry a
    /// non-finite value past a later switch to `Refuse`.
    Refuse,
}

/// Why the frame driver refused a frame. Slot-indexed and name-free: the caller maps a slot to
/// its identity (the facade to the canonical input path, a target to its own I/O table).
///
/// Variants are listed in the driver's refusal order (see the [module documentation](self)).
/// Times are the supplied and previous model seconds with their exact bits. The set is closed on
/// purpose: a new refusal changes the HostTick v1 profile, so callers match it exhaustively.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FrameRefusal {
    /// The frame time is NaN or infinite (CDL §7.16 time is a finite real).
    NonFiniteTime {
        /// The supplied time.
        now: f64,
    },
    /// The frame time is finite but its magnitude exceeds [`MAX_FRAME_TIME_MAGNITUDE`].
    TimeOutOfRange {
        /// The supplied time.
        now: f64,
        /// The accepted magnitude limit, [`MAX_FRAME_TIME_MAGNITUDE`].
        limit: f64,
    },
    /// The frame time is before the previous accepted frame's time (CDL §7.16 monotonic time).
    TimeRegression {
        /// The supplied time.
        now: f64,
        /// The previous accepted time.
        prev: f64,
    },
    /// Both times are finite and nondecreasing, but `now - prev` overflows. Defensive: accepted
    /// clocks stay within [`MAX_FRAME_TIME_MAGNITUDE`], so only a clock set from elsewhere (the
    /// facade's restore refuses one beyond the limit) could reach it.
    NonFiniteTimeStep {
        /// The supplied time.
        now: f64,
        /// The previous accepted time.
        prev: f64,
    },
    /// A finite time that one stateful block's state words cannot represent (a sampled clock's
    /// index would overflow).
    ModelTimeUnrepresentable {
        /// The supplied time.
        now: f64,
    },
    /// The caller supplied a different number of values than the executable has input slots.
    /// The facade cannot produce it: its plan always carries one value per input definition.
    InputCount {
        /// The executable's input slot count.
        expected: usize,
        /// The number of values supplied.
        found: usize,
    },
    /// The value's type is not exactly every fan-out target's type (no coercion).
    InputType {
        /// The input slot.
        slot: usize,
    },
    /// A NaN or infinite Real under [`NonFiniteInputPolicy::Refuse`].
    InputNonFinite {
        /// The input slot.
        slot: usize,
    },
    /// The value is outside the slot's inclusive acceptance domain.
    InputDomain {
        /// The input slot.
        slot: usize,
    },
    /// The accepted-frame sequence has no next position (`u64::MAX` frames were accepted).
    SequenceExhausted,
}

#[cfg(feature = "std")]
impl core::fmt::Display for FrameRefusal {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NonFiniteTime { now } => write!(f, "non-finite frame time: t_now={now}"),
            Self::TimeOutOfRange { now, limit } => {
                write!(
                    f,
                    "frame time out of range: |t_now={now:e}| exceeds {limit:e}"
                )
            }
            Self::TimeRegression { now, prev } => {
                write!(f, "time regression: t_now={now} < previous {prev}")
            }
            Self::NonFiniteTimeStep { now, prev } => {
                write!(f, "non-finite time step: t_now={now} - previous {prev}")
            }
            Self::ModelTimeUnrepresentable { now } => {
                write!(
                    f,
                    "model time cannot be represented by block state: t_now={now}"
                )
            }
            Self::InputCount { expected, found } => {
                write!(
                    f,
                    "{found} input values supplied for {expected} input slots"
                )
            }
            Self::InputType { slot } => write!(f, "input slot {slot}: type mismatch"),
            Self::InputNonFinite { slot } => write!(f, "input slot {slot}: non-finite Real"),
            Self::InputDomain { slot } => write!(f, "input slot {slot}: outside its domain"),
            Self::SequenceExhausted => f.write_str("accepted frame sequence is exhausted"),
        }
    }
}

#[cfg(feature = "std")]
impl core::error::Error for FrameRefusal {}

/// One boundary input slot as the driver checks and stages it.
#[derive(Clone, Copy, Debug)]
pub struct InputSlot<'a> {
    /// Every input connector the value is staged into (at least one on a loaded or decoded
    /// executable).
    pub targets: &'a [ConnectorId],
    /// Inclusive lower bound of the acceptance domain, if any.
    pub min: Option<&'a Value>,
    /// Inclusive upper bound of the acceptance domain, if any.
    pub max: Option<&'a Value>,
}

/// One frame's input values beside the executable's slot table, as the driver reads them.
///
/// The facade implements it over its prepared plan and input definitions; the blob entry over its
/// decoded I/O table and the caller's value slice. [`Self::slot`] and [`Self::value`] are only
/// called for `slot < slot_count()` after the driver has checked that both counts are equal.
pub trait FrameInputs {
    /// The executable's number of input slots.
    fn slot_count(&self) -> usize;
    /// The number of values this frame supplies.
    fn value_count(&self) -> usize;
    /// The targets and domain of input `slot`.
    fn slot(&self, slot: usize) -> InputSlot<'_>;
    /// The value supplied for input `slot`.
    fn value(&self, slot: usize) -> &Value;
}

/// Check the frame time against the previous accepted time and every stateful block, without
/// mutation. The first of the five time refusals in the [module order](self) is returned.
///
/// # Errors
/// [`FrameRefusal::NonFiniteTime`], [`FrameRefusal::TimeOutOfRange`],
/// [`FrameRefusal::TimeRegression`], [`FrameRefusal::NonFiniteTimeStep`] or
/// [`FrameRefusal::ModelTimeUnrepresentable`].
pub fn check_time(
    blocks: &[Box<dyn Block>],
    state: &RunState,
    prev_t: Option<f64>,
    time: f64,
) -> Result<(), FrameRefusal> {
    if !time.is_finite() {
        return Err(FrameRefusal::NonFiniteTime { now: time });
    }
    if time.abs() > MAX_FRAME_TIME_MAGNITUDE {
        return Err(FrameRefusal::TimeOutOfRange {
            now: time,
            limit: MAX_FRAME_TIME_MAGNITUDE,
        });
    }
    if let Some(prev) = prev_t {
        if time < prev {
            return Err(FrameRefusal::TimeRegression { now: time, prev });
        }
        if !(time - prev).is_finite() {
            return Err(FrameRefusal::NonFiniteTimeStep { now: time, prev });
        }
    }
    if !time_is_representable(blocks, state, time) {
        return Err(FrameRefusal::ModelTimeUnrepresentable { now: time });
    }
    Ok(())
}

/// Whether every stateful block can represent `time` in its current state words
/// ([`Block::time_is_representable`]). A slot outside the block list or the word arena counts as
/// unrepresentable rather than panicking; BUILD and the blob decoder never produce one.
#[must_use]
pub fn time_is_representable(blocks: &[Box<dyn Block>], state: &RunState, time: f64) -> bool {
    state.slots.iter().all(|slot| {
        let region = slot
            .offset
            .checked_add(slot.len)
            .and_then(|end| state.words.get(slot.offset..end));
        match (blocks.get(slot.block.0 as usize), region) {
            (Some(block), Some(region)) => block.time_is_representable(time, region),
            _ => false,
        }
    })
}

/// Whether `value` has exactly the type of every target connector (no coercion; an Enum value
/// must name the targets' class). A target outside the connector arena does not accept.
#[must_use]
pub fn accepts_type(model: &Model, targets: &[ConnectorId], value: &Value) -> bool {
    let value_type = value.value_type();
    targets.iter().all(|id| {
        model
            .connectors
            .get(id.0 as usize)
            .is_some_and(|connector| connector.value_type == value_type)
    })
}

/// Whether an already type-checked `value` lies in the inclusive domain `[min, max]`.
///
/// A Real compares with `>=`/`<=`, so NaN fails any present bound and a NaN bound accepts
/// nothing; an absent bound admits everything on its side, infinities included. A bound of a
/// different type than the value accepts nothing. An Enum ordinal must lie in `1..=members` of
/// its class. Booleans and Strings have no domain.
#[must_use]
pub fn accepts_domain(value: &Value, min: Option<&Value>, max: Option<&Value>) -> bool {
    match value {
        Value::Real(value) => {
            min.is_none_or(|min| matches!(min, Value::Real(min) if value >= min))
                && max.is_none_or(|max| matches!(max, Value::Real(max) if value <= max))
        }
        Value::Integer(value) => {
            min.is_none_or(|min| matches!(min, Value::Integer(min) if value >= min))
                && max.is_none_or(|max| matches!(max, Value::Integer(max) if value <= max))
        }
        Value::Enum { class, ordinal } => enum_descriptor(*class).is_some_and(|descriptor| {
            *ordinal > 0 && (*ordinal as usize) <= descriptor.members.len()
        }),
        Value::Boolean(_) | Value::String(_) => true,
    }
}

/// Check one input value for `slot`: exact type, then the non-finite policy, then the domain.
///
/// # Errors
/// [`FrameRefusal::InputType`], [`FrameRefusal::InputNonFinite`] (only under
/// [`NonFiniteInputPolicy::Refuse`]) or [`FrameRefusal::InputDomain`], each naming `slot`.
pub fn check_input(
    model: &Model,
    slot: usize,
    input: &InputSlot<'_>,
    value: &Value,
    policy: NonFiniteInputPolicy,
) -> Result<(), FrameRefusal> {
    if !accepts_type(model, input.targets, value) {
        return Err(FrameRefusal::InputType { slot });
    }
    // Exhaustive on purpose: a new policy variant must decide here, never fall back to `Admit`.
    let refuses_non_finite = match policy {
        NonFiniteInputPolicy::Admit => false,
        NonFiniteInputPolicy::Refuse => true,
    };
    if refuses_non_finite && matches!(value, Value::Real(x) if !x.is_finite()) {
        return Err(FrameRefusal::InputNonFinite { slot });
    }
    if !accepts_domain(value, input.min, input.max) {
        return Err(FrameRefusal::InputDomain { slot });
    }
    Ok(())
}

/// Check that one value is supplied per slot, then every slot in slot order with
/// [`check_input`].
///
/// # Errors
/// [`FrameRefusal::InputCount`], or the first slot's refusal from [`check_input`].
pub fn check_inputs<I: FrameInputs + ?Sized>(
    model: &Model,
    inputs: &I,
    policy: NonFiniteInputPolicy,
) -> Result<(), FrameRefusal> {
    let expected = inputs.slot_count();
    let found = inputs.value_count();
    if found != expected {
        return Err(FrameRefusal::InputCount { expected, found });
    }
    for slot in 0..expected {
        check_input(model, slot, &inputs.slot(slot), inputs.value(slot), policy)?;
    }
    Ok(())
}

/// The value an output connector holds, borrowed from the run state. `source` must be a connector
/// of the executable the state was allocated for (load and the blob decoder guarantee it for every
/// boundary output).
#[must_use]
pub fn output_value(state: &RunState, source: ConnectorId) -> &Value {
    &state.values[source.0 as usize]
}

/// Whether any block's class is target-bound ([`oce_blocks::class_is_target_bound`]): its outputs
/// may depend on the platform's transcendental math library.
#[must_use]
pub fn target_bound(blocks: &[Box<dyn Block>]) -> bool {
    blocks
        .iter()
        .any(|block| class_is_target_bound(block.signature().class_path))
}

/// A borrowed view of one executable and its frame clock: the HostTick v1 frame driver.
///
/// `model`, `schedule`, `blocks` and `state` are the arenas BUILD (or the blob decoder) produced,
/// with their invariants intact. `prev_t` is the previous accepted frame's time (`None` before the
/// first) and `sequence` the number of frames accepted so far; the driver advances both on
/// acceptance only. The facade keeps them as engine fields and the blob entry as its own.
pub struct FrameDriver<'a> {
    /// The flattened model the schedule was compiled from.
    pub model: &'a Model,
    /// The frozen schedule.
    pub schedule: &'a Schedule,
    /// Instantiated blocks, indexed by `BlockId.0`.
    pub blocks: &'a [Box<dyn Block>],
    /// The mutable run state.
    pub state: &'a mut RunState,
    /// The previous accepted frame time, `None` before the first accepted frame.
    pub prev_t: &'a mut Option<f64>,
    /// The number of frames accepted so far; the next accepted frame is `sequence + 1`.
    pub sequence: &'a mut u64,
}

impl FrameDriver<'_> {
    /// Check and commit one HostTick v1 frame at `time` model seconds, returning its accepted
    /// sequence number (one for the first accepted frame).
    ///
    /// Every check in the [module order](self) runs before anything is written; on refusal the
    /// run state, the previous time and the sequence are untouched. On acceptance each value is
    /// staged into every fan-out target, the schedule is evaluated once with `diagnostics` as the
    /// sink, the previous time becomes `time` and the sequence advances. Allocates nothing itself.
    ///
    /// # Errors
    /// The first [`FrameRefusal`] in the module order.
    pub fn execute<I: FrameInputs + ?Sized>(
        &mut self,
        time: f64,
        inputs: &I,
        policy: NonFiniteInputPolicy,
        diagnostics: &dyn Diagnostics,
    ) -> Result<u64, FrameRefusal> {
        check_time(self.blocks, self.state, *self.prev_t, time)?;
        check_inputs(self.model, inputs, policy)?;
        let next = self
            .sequence
            .checked_add(1)
            .ok_or(FrameRefusal::SequenceExhausted)?;

        // COMMIT: only infallible operations remain. Every target was bounds-checked by
        // `accepts_type`.
        for slot in 0..inputs.slot_count() {
            let value = inputs.value(slot);
            for target in inputs.slot(slot).targets {
                self.state.values[target.0 as usize] = value.clone();
            }
        }
        let mut ctx = EvalContext {
            model: self.model,
            schedule: self.schedule,
            blocks: self.blocks,
            diagnostics,
            state: self.state,
        };
        eval_tick(&mut ctx, time);
        *self.prev_t = Some(time);
        *self.sequence = next;
        Ok(next)
    }
}
