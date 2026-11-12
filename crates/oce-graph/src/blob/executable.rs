//! The `no_std + alloc` execution entry: a decoded blob plus the HostTick v1 frame driver
//! ([`crate::frame`]), with inputs and outputs by slot and the state as raw words.

use alloc::vec;

use oce_blocks::{BlockKind, Diagnostics};
use oce_model::Value;

use super::{BlobError, BlobInput, BlobOutput, Decoded, decode};
use crate::frame::{FrameDriver, FrameInputs, FrameRefusal, InputSlot, NonFiniteInputPolicy};

/// A compiled-schedule blob decoded into a runnable executable: what a bare-metal or WASM target
/// links instead of the `oce-api` facade.
///
/// [`Self::from_blob`] runs [`decode()`]; [`Self::execute_frame`] runs one HostTick v1 frame
/// through the same driver the facade's `Engine::execute_frame` calls ([`crate::frame`]), so time
/// checks, input checks, the non-finite policy, the sequence, staging and evaluation are one
/// implementation on both sides. Equal timestamps advance state again, as on the facade.
///
/// Inputs and outputs are addressed by slot: a slot is the record's position in the blob's I/O
/// table ([`Self::inputs`], [`Self::outputs`]), sorted by UTF-8 path. A host resolves names to
/// slots once, at startup ([`Self::input_slot`], [`Self::output_slot`]), and passes one value per
/// input slot each frame; no name is looked up at run time.
///
/// The non-finite Real input policy starts at the default, [`NonFiniteInputPolicy::Admit`], as on
/// the facade; a production host is recommended to select [`NonFiniteInputPolicy::Refuse`] with
/// [`Self::set_non_finite_input_policy`] before its first frame. The policy is host
/// configuration; the blob does not carry it.
///
/// State is exposed as raw words ([`Self::state_words`], [`Self::restore_state_words`]): the
/// stateful blocks' `RunState::words` in the decoded layout, with no portability tag, manifest or
/// checksum. They are not the facade's portable state snapshot, which stays in `oce-api`.
///
/// # Allocation
///
/// Construction allocates what [`decode()`] allocates. [`Self::execute_frame`] allocates nothing
/// in the driver or the scheduler (pinned at zero for every golden by the `blob_execution` test);
/// the architecture's carve-outs still apply (a `CDL.Reals.Sort` wider than 64 inputs, and the
/// diagnostics sink you pass). [`Self::restore_state_words`] allocates one scratch word vector
/// before the first accepted frame only.
///
/// # Example
///
/// The module below is the shape of a `#![no_std]` firmware crate's library: it names only
/// `core` and the engine crates, and allocates nothing after boot (the `no_implicit_prelude`
/// attribute keeps `std`'s prelude out of it, so every path is spelled from its crate root).
/// Doctests always build with `std`, so it runs here inside an ordinary host test; the
/// `no-std-build` CI job builds `oce-graph` itself for `thumbv7em-none-eabihf` and
/// `wasm32-unknown-unknown`. The blob is one of the checked-in goldens: the ASHRAE differential
/// economizer high limit, whose one input is the return-air temperature `TRet` in K and whose one
/// output is the high-limit cutoff temperature `TCut`.
///
/// ```
/// #[no_implicit_prelude]
/// mod firmware {
///     use ::core::option::Option::{self, None, Some};
///     use ::core::result::Result::{self, Err, Ok};
///     use ::oce_blocks::NoopDiagnostics;
///     use ::oce_graph::blob::{BlobError, Executable};
///     use ::oce_graph::frame::{FrameRefusal, NonFiniteInputPolicy};
///     use ::oce_model::Value;
///
///     /// The blob, linked into flash as `.rodata`.
///     static BLOB: &[u8] = ::core::include_bytes!(
///         "../../../oce-api/tests/fixtures/blob/generic_air_economizer_high_limits_ashrae_differential.oceb"
///     );
///     const T_RET: &str = ::core::concat!(
///         "http://example.org#g36.source.",
///         "generic_air_economizer_high_limits_ashrae_differential.TRet"
///     );
///     const T_CUT: &str = ::core::concat!(
///         "http://example.org#g36.source.",
///         "generic_air_economizer_high_limits_ashrae_differential.TCut"
///     );
///
///     /// The executable and the two slots the host resolved once, at boot.
///     pub struct Controller {
///         pub executable: Executable<'static>,
///         t_ret: usize,
///         t_cut: usize,
///     }
///
///     /// Decode once at boot, refuse non-finite inputs, and resolve the I/O slots by path.
///     pub fn boot() -> Result<Option<Controller>, BlobError> {
///         let mut executable = Executable::from_blob(BLOB)?;
///         executable.set_non_finite_input_policy(NonFiniteInputPolicy::Refuse);
///         let (Some(t_ret), Some(t_cut)) =
///             (executable.input_slot(T_RET), executable.output_slot(T_CUT))
///         else {
///             return Ok(None);
///         };
///         Ok(Some(Controller { executable, t_ret, t_cut }))
///     }
///
///     /// One frame: the return-air temperature in, the cutoff temperature out.
///     pub fn frame(
///         controller: &mut Controller,
///         time: f64,
///         t_ret: f64,
///     ) -> Result<Option<f64>, FrameRefusal> {
///         let mut inputs = [Value::Real(0.0)];
///         if let Some(slot) = inputs.get_mut(controller.t_ret) {
///             *slot = Value::Real(t_ret);
///         }
///         controller.executable.execute_frame(time, &inputs, &NoopDiagnostics)?;
///         Ok(match controller.executable.output(controller.t_cut) {
///             Some(Value::Real(t_cut)) => Some(*t_cut),
///             _ => None,
///         })
///     }
/// }
///
/// let mut controller = firmware::boot().unwrap().expect("both paths resolve");
/// assert_eq!(firmware::frame(&mut controller, 0.0, 297.15), Ok(Some(297.15)));
/// assert_eq!(controller.executable.sequence(), 1);
/// // Refusals change nothing: time going backwards, then a NaN under `Refuse`.
/// assert!(matches!(
///     firmware::frame(&mut controller, -1.0, 297.15),
///     Err(oce_graph::frame::FrameRefusal::TimeRegression { .. })
/// ));
/// assert_eq!(
///     firmware::frame(&mut controller, 1.0, f64::NAN),
///     Err(oce_graph::frame::FrameRefusal::InputNonFinite { slot: 0 })
/// );
/// assert_eq!(controller.executable.sequence(), 1);
/// ```
pub struct Executable<'a> {
    decoded: Decoded<'a>,
    prev_t: Option<f64>,
    sequence: u64,
    policy: NonFiniteInputPolicy,
}

/// Why [`Executable::restore_state_words`] refused a word slice. Nothing was written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateWordsRefusal {
    /// The slice length differs from the decoded state layout's word count.
    LengthMismatch {
        /// The decoded layout's word count.
        expected: usize,
        /// The supplied word count.
        found: usize,
    },
    /// Before the first accepted frame, the only valid words are the initial ones the decoder
    /// allocated, and these differ.
    NotInitial,
    /// A stateful block has no registered state-contract revision, so its words cannot be
    /// validated (no registered class has this today).
    NoStateContract {
        /// The block's index.
        block: u32,
    },
    /// A stateful block's region fails its own state-contract validation at the executable's
    /// current model time.
    InvalidRegion {
        /// The block's index.
        block: u32,
    },
}

#[cfg(feature = "std")]
impl core::fmt::Display for StateWordsRefusal {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::LengthMismatch { expected, found } => {
                write!(f, "{found} state words supplied for a layout of {expected}")
            }
            Self::NotInitial => f.write_str("state words before the first frame are not initial"),
            Self::NoStateContract { block } => {
                write!(f, "block {block} has no registered state contract")
            }
            Self::InvalidRegion { block } => {
                write!(f, "block {block} state words fail validation")
            }
        }
    }
}

#[cfg(feature = "std")]
impl core::error::Error for StateWordsRefusal {}

/// The decoded input table beside one frame's value slice, as the driver reads it.
struct SlotValues<'s, 'a> {
    table: &'s [BlobInput<'a>],
    values: &'s [Value],
}

impl FrameInputs for SlotValues<'_, '_> {
    fn slot_count(&self) -> usize {
        self.table.len()
    }

    fn value_count(&self) -> usize {
        self.values.len()
    }

    fn slot(&self, slot: usize) -> InputSlot<'_> {
        let input = &self.table[slot];
        InputSlot {
            targets: &input.targets,
            min: input.min.as_ref(),
            max: input.max.as_ref(),
        }
    }

    fn value(&self, slot: usize) -> &Value {
        &self.values[slot]
    }
}

impl<'a> Executable<'a> {
    /// Decode `bytes` ([`decode()`]) into an executable at model time zero, before its first
    /// frame, with sequence zero and the default [`NonFiniteInputPolicy::Admit`].
    ///
    /// # Errors
    /// The decoder's [`BlobError`]; never a panic.
    pub fn from_blob(bytes: &'a [u8]) -> Result<Self, BlobError> {
        decode(bytes).map(Self::from_decoded)
    }

    /// Wrap an already decoded executable, before its first frame.
    #[must_use]
    pub fn from_decoded(decoded: Decoded<'a>) -> Self {
        Self {
            decoded,
            prev_t: None,
            sequence: 0,
            policy: NonFiniteInputPolicy::default(),
        }
    }

    /// The boundary input table, in slot (UTF-8 path) order: each record's path, exact type,
    /// acceptance domain and fan-out targets.
    #[must_use]
    pub fn inputs(&self) -> &[BlobInput<'a>] {
        &self.decoded.inputs
    }

    /// The boundary output table, in slot (UTF-8 path) order.
    #[must_use]
    pub fn outputs(&self) -> &[BlobOutput<'a>] {
        &self.decoded.outputs
    }

    /// The input slot whose path is exactly `path`, by binary search over the sorted table.
    #[must_use]
    pub fn input_slot(&self, path: &str) -> Option<usize> {
        self.decoded
            .inputs
            .binary_search_by(|input| input.path.as_bytes().cmp(path.as_bytes()))
            .ok()
    }

    /// The output slot whose path is exactly `path`, by binary search over the sorted table.
    #[must_use]
    pub fn output_slot(&self, path: &str) -> Option<usize> {
        self.decoded
            .outputs
            .binary_search_by(|output| output.path.as_bytes().cmp(path.as_bytes()))
            .ok()
    }

    /// The non-finite Real input policy in force; [`NonFiniteInputPolicy::Admit`] until set.
    #[must_use]
    pub fn non_finite_input_policy(&self) -> NonFiniteInputPolicy {
        self.policy
    }

    /// Select the non-finite Real input policy for every later frame. Touches no state.
    pub fn set_non_finite_input_policy(&mut self, policy: NonFiniteInputPolicy) {
        self.policy = policy;
    }

    /// Run one HostTick v1 frame at `time` model seconds with one value per input slot, and return
    /// its accepted sequence number (one for the first accepted frame).
    ///
    /// The checks and their order are the frame driver's ([`crate::frame`]): time, then one value
    /// per slot, then per slot its exact type, the non-finite policy and its domain, then the
    /// sequence. Every refusal precedes any mutation. On acceptance the values are staged into
    /// their targets, the schedule is evaluated once with `diagnostics` as the warning sink, and
    /// the outputs ([`Self::output`]) hold the frame's results.
    ///
    /// # Errors
    /// The first [`FrameRefusal`]; nothing was changed.
    pub fn execute_frame(
        &mut self,
        time: f64,
        inputs: &[Value],
        diagnostics: &dyn Diagnostics,
    ) -> Result<u64, FrameRefusal> {
        let Decoded {
            model,
            schedule,
            blocks,
            state,
            inputs: table,
            ..
        } = &mut self.decoded;
        FrameDriver {
            model,
            schedule,
            blocks,
            state,
            prev_t: &mut self.prev_t,
            sequence: &mut self.sequence,
        }
        .execute(
            time,
            &SlotValues {
                table,
                values: inputs,
            },
            self.policy,
            diagnostics,
        )
    }

    /// The value of output `slot`, borrowed from the run state; `None` past the last slot. Before
    /// the first frame it is the connector's seed (its type zero, or a source block's value).
    #[must_use]
    pub fn output(&self, slot: usize) -> Option<&Value> {
        let output = self.decoded.outputs.get(slot)?;
        Some(crate::frame::output_value(
            &self.decoded.state,
            output.source,
        ))
    }

    /// Every output value in slot order, borrowed from the run state.
    pub fn output_values(&self) -> impl ExactSizeIterator<Item = &Value> + '_ {
        self.decoded
            .outputs
            .iter()
            .map(|output| crate::frame::output_value(&self.decoded.state, output.source))
    }

    /// The previous accepted frame's time in model seconds, `None` before the first.
    #[must_use]
    pub fn time(&self) -> Option<f64> {
        self.prev_t
    }

    /// The number of frames accepted so far.
    #[must_use]
    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Whether any block's class is target-bound: the outputs may depend on this platform's
    /// transcendental math library ([`crate::frame::target_bound`]).
    #[must_use]
    pub fn target_bound(&self) -> bool {
        crate::frame::target_bound(&self.decoded.blocks)
    }

    /// The blob's content-id field verbatim: the 32 SHA-256 digest bytes of the facade's
    /// `oce:canon1:` canonical content id, or `None` when the blob carries none. Never interpreted
    /// or checked here.
    #[must_use]
    pub fn content_id(&self) -> Option<[u8; 32]> {
        self.decoded.content_id
    }

    /// The decoded executable: model arena, schedule, blocks and run state, read-only.
    #[must_use]
    pub fn decoded(&self) -> &Decoded<'a> {
        &self.decoded
    }

    /// The stateful blocks' state words in the decoded layout (`RunState::words`), raw.
    #[must_use]
    pub fn state_words(&self) -> &[u64] {
        &self.decoded.state.words
    }

    /// Replace the state words with `words`, all or nothing.
    ///
    /// The slice must have the decoded layout's length. Before the first accepted frame it must
    /// equal the initial words the decoder allocated. After it, every stateful block's region must
    /// pass that block's own state-contract validation at the executable's current model time, the
    /// check the facade's restore applies, so words that would make a block index outside its
    /// buffers are refused rather than executed. The model time, sequence, policy and connector
    /// values are unchanged; the next accepted frame recomputes the outputs from the restored
    /// words and its inputs.
    ///
    /// This is raw state, not a snapshot: no portability tag, manifest or checksum travels, and
    /// the caller keeps the words with the executable and model time they belong to.
    ///
    /// # Errors
    /// A [`StateWordsRefusal`]; nothing was written.
    pub fn restore_state_words(&mut self, words: &[u64]) -> Result<(), StateWordsRefusal> {
        let state = &self.decoded.state;
        if words.len() != state.words.len() {
            return Err(StateWordsRefusal::LengthMismatch {
                expected: state.words.len(),
                found: words.len(),
            });
        }
        match self.prev_t {
            None => {
                let mut initial = vec![0u64; words.len()];
                for slot in &state.slots {
                    let index = slot.block.0 as usize;
                    let (Some(block), Some(instance)) = (
                        self.decoded.blocks.get(index),
                        self.decoded.model.blocks.get(index),
                    ) else {
                        return Err(StateWordsRefusal::NotInitial);
                    };
                    let Some(region) = region_mut(&mut initial, slot.offset, slot.len) else {
                        return Err(StateWordsRefusal::NotInitial);
                    };
                    block.init_state(region, &instance.params);
                }
                if initial != words {
                    return Err(StateWordsRefusal::NotInitial);
                }
            }
            Some(time) => {
                for slot in &state.slots {
                    let block = slot.block.0;
                    let refused = StateWordsRefusal::InvalidRegion { block };
                    let Some(implementation) = self.decoded.blocks.get(block as usize) else {
                        return Err(refused);
                    };
                    if implementation.kind() != BlockKind::Stateful
                        || implementation.state_contract_revision() == 0
                    {
                        return Err(StateWordsRefusal::NoStateContract { block });
                    }
                    let region = slot
                        .offset
                        .checked_add(slot.len)
                        .and_then(|end| words.get(slot.offset..end))
                        .ok_or(refused)?;
                    implementation
                        .validate_state(region, time, time)
                        .map_err(|_| refused)?;
                }
            }
        }
        self.decoded.state.words.copy_from_slice(words);
        Ok(())
    }
}

/// `words[offset..offset + len]`, or `None` when the range overflows or leaves the slice.
fn region_mut(words: &mut [u64], offset: usize, len: usize) -> Option<&mut [u64]> {
    let end = offset.checked_add(len)?;
    words.get_mut(offset..end)
}
