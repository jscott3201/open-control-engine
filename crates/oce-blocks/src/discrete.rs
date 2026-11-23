//! `CDL.Discrete` scalar blocks.
//!
//! `UnitDelay` is the discrete one-sample loop-breaker. `TriggeredSampler` is event-stateful but
//! transparent on rising trigger instants: same-tick `u` and `trigger` can determine `y`, so it
//! must not be treated as a graph cut. `TriggeredSampler` and `TriggeredMax` fire no event at
//! frame 0: initialisation leaves their `when trigger` inactive (Modelica 3.6 §8.6), and a trigger
//! already true there becomes `pre(trigger)` for frame 1.

use alloc::string::String;
use oce_model::{
    ParamTable, Value,
    determinism::{canonicalize_real, det_max},
};

use crate::discrete_sampled::{
    at_sample_instant, initial_sample_time, sample_due, valid_sample_period,
};
use crate::settled::{fire, settled_input};
use crate::{Block, BlockKind, BlockSignature, Ctx, PortKind, emit_real, read_real};

const TRIGGERED_SAMPLER_HELD_WORD: usize = 0;
const TRIGGERED_SAMPLER_PREV_TRIGGER_WORD: usize = 1;
const TRIGGERED_MAX_HELD_WORD: usize = 0;
const TRIGGERED_MAX_PREV_TRIGGER_WORD: usize = 1;
const TRIGGERED_MAX_HAS_HISTORY_WORD: usize = 2;
const TRIGGERED_MOVING_MEAN_HELD_WORD: usize = 0;
const TRIGGERED_MOVING_MEAN_PREV_TRIGGER_WORD: usize = 1;
const TRIGGERED_MOVING_MEAN_COUNTER_WORD: usize = 2;
const TRIGGERED_MOVING_MEAN_NEXT_INDEX_WORD: usize = 3;
const TRIGGERED_MOVING_MEAN_SAMPLE_WORDS_START: usize = 4;

fn bool_word(value: bool) -> u64 {
    u64::from(value)
}

fn word_bool(word: u64) -> bool {
    word != 0
}

fn i64_word(value: i64) -> u64 {
    value.cast_unsigned()
}

fn word_i64(word: u64) -> i64 {
    word as i64
}

const UNIT_DELAY_HELD_WORD: usize = 0;
const UNIT_DELAY_STAGED_WORD: usize = 1;
const UNIT_DELAY_T0_WORD: usize = 2;
const UNIT_DELAY_LAST_INDEX_WORD: usize = 3;
const UNIT_DELAY_INITIALIZED_WORD: usize = 4;
const UNIT_DELAY_STATE_WORDS: usize = 5;

/// `CDL.Discrete.UnitDelay` — `y = pre(u_internal)` on the `samplePeriod` clock, a one-sample
/// `Real` delay and a discrete loop-breaker (`03` §4.6): stateful with `feeds_through == false`,
/// so it cuts the direct-feedthrough DAG (the sample-instant check in `emit_from_state` reads only
/// the tick time and prior state, never a current input).
///
/// Mirrors Buildings `Discrete/UnitDelay.mo` exactly: two state values track the upstream
/// discrete pair — the held output `y` (the input sampled at the *previous* instant) and the
/// staged `u_internal` (the input sampled at the *most recent* instant). At each sample instant
/// `y` becomes the staged sample and the current input is staged; between instants `y` holds.
/// Both are seeded from `y_start`, so `y` is identical to `y_start` until the **second** sample
/// instant, per the upstream documentation. Invalid direct-construction periods degrade to `1.0`;
/// validated models must supply a finite period at least `1E-3`.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnitDelay {
    pub(crate) y_start: f64,
    pub(crate) sample_period: f64,
}

impl Block for UnitDelay {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Discrete.UnitDelay",
            inputs: &[PortKind::Real],
            outputs: &[PortKind::Real],
            stateful: true,
        };
        &SIG
    }
    fn kind(&self) -> BlockKind {
        BlockKind::Stateful
    }
    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        false // the discrete loop cut: output is the prior sample, not the current input
    }
    fn state_len(&self) -> usize {
        UNIT_DELAY_STATE_WORDS
    }
    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region[UNIT_DELAY_HELD_WORD] = canonicalize_real(self.y_start).to_bits();
        region[UNIT_DELAY_STAGED_WORD] = canonicalize_real(self.y_start).to_bits();
        region[UNIT_DELAY_T0_WORD] = 0.0f64.to_bits();
        region[UNIT_DELAY_LAST_INDEX_WORD] = i64_word(-1);
        region[UNIT_DELAY_INITIALIZED_WORD] = bool_word(false);
    }
    fn validate_state(&self, region: &[u64], state_t: f64, _prev_t: f64) -> Result<(), String> {
        crate::state_contract::validate_sampled(
            region,
            state_t,
            valid_sample_period(self.sample_period),
            false,
            true,
        )
    }
    fn time_is_representable(&self, t_now: f64, region: &[u64]) -> bool {
        crate::state_contract::sampled_time_representable(
            t_now,
            region,
            valid_sample_period(self.sample_period),
            UNIT_DELAY_INITIALIZED_WORD,
            UNIT_DELAY_T0_WORD,
        )
    }
    fn simulation_time_is_representable(&self, first: f64, last: f64, _region: &[u64]) -> bool {
        crate::state_contract::sampled_horizon_representable(
            first,
            last,
            valid_sample_period(self.sample_period),
        )
    }
    fn emit_from_state(
        &self,
        ctx: &Ctx<'_>,
        _inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        if !word_bool(region[UNIT_DELAY_INITIALIZED_WORD]) {
            emit_real(0, f64::from_bits(region[UNIT_DELAY_HELD_WORD]), emit);
            return;
        }
        // At a sample instant the upstream `when` fires before the output is read: `y` becomes
        // the previously staged sample. Between instants the held word is unchanged. This check
        // reads only the tick time and prior state, so `feeds_through` stays `false`.
        let period = valid_sample_period(self.sample_period);
        let t0 = f64::from_bits(region[UNIT_DELAY_T0_WORD]);
        let last = word_i64(region[UNIT_DELAY_LAST_INDEX_WORD]);
        let (due, _) = sample_due(ctx.t(), t0, period, last);
        let word = if due {
            UNIT_DELAY_STAGED_WORD
        } else {
            UNIT_DELAY_HELD_WORD
        };
        emit_real(0, f64::from_bits(region[word]), emit);
    }
    fn update_state(&self, ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        let period = valid_sample_period(self.sample_period);
        if !word_bool(region[UNIT_DELAY_INITIALIZED_WORD]) {
            let t0 = initial_sample_time(ctx.t(), period);
            let (_, index) = sample_due(ctx.t(), t0, period, -1);
            // Upstream fires on `when sampleTrigger` with NO `initial()` (unlike the
            // Sampler/ZeroOrderHold/FirstOrderHold family): the first tick samples only when it
            // lands exactly on a sample instant, firing `y = pre(u_internal) = y_start` (held
            // word unchanged) and staging `u_internal = u`. A mid-interval start holds BOTH
            // words at `y_start` until the next true instant (`last_index` still advances so
            // the next boundary fires).
            if at_sample_instant(ctx.t(), t0, period, index) {
                region[UNIT_DELAY_STAGED_WORD] = canonicalize_real(read_real(inputs, 0)).to_bits();
            }
            region[UNIT_DELAY_T0_WORD] = t0.to_bits();
            region[UNIT_DELAY_LAST_INDEX_WORD] = i64_word(index);
            region[UNIT_DELAY_INITIALIZED_WORD] = bool_word(true);
            return;
        }
        let t0 = f64::from_bits(region[UNIT_DELAY_T0_WORD]);
        let last = word_i64(region[UNIT_DELAY_LAST_INDEX_WORD]);
        let (due, index) = sample_due(ctx.t(), t0, period, last);
        if due {
            region[UNIT_DELAY_HELD_WORD] = region[UNIT_DELAY_STAGED_WORD];
            region[UNIT_DELAY_STAGED_WORD] = canonicalize_real(read_real(inputs, 0)).to_bits();
            region[UNIT_DELAY_LAST_INDEX_WORD] = i64_word(index);
        }
    }
}

/// `CDL.Discrete.TriggeredSampler` — sample `u` on each `trigger` event.
///
/// Before the first trigger instant, `y` is the parameter `y_start` (default `0`): the source's
/// `initial equation y = y_start`. Frame 0 is initialisation, where the `when trigger` clause is
/// inactive, so frame 0 emits `y_start` whatever `trigger` is, and its `trigger` becomes
/// `pre(trigger)` for frame 1; the first sample is the first false-to-true transition after
/// frame 0. The block owns two `[S]` words: the held `Real` output (`f64::to_bits`) and the
/// previous trigger bit. [`Block::init_state`] seeds that bit `true`, so no false-to-true
/// transition can be seen at frame 0 whatever `trigger` is there, and the frame-0 update
/// overwrites it with frame 0's `trigger`. It feeds through from both inputs to `y` because a
/// same-tick rising trigger emits the current `u`. Real outputs and stored samples are NaN-canonicalized for deterministic
/// traces. The block does not panic for finite or non-finite `Real` values; validated callers must
/// provide a `Real` then a `Boolean` input.
/// `trigger` takes the settled-value rule of issue #800 (`docs/design/pulse-event-inputs.md`): with
/// a pulse or a sample upstream the event plan appends `w` and `s`, the block fires on
/// `(v and not w(k-1)) or (not v and s)` and stores `w` in its previous-trigger word, which nothing
/// but that test reads. At the initial frame
/// only a sample instant samples (the frame driver's initial flag; the seeded word answers a
/// direct call).
/// OpenModelica 1.25.1 agrees exactly at every frame, with the trigger true from frame 0 and with
/// it rising at frame 1 (`oce-conformance`'s `open_modelica_event_triggers.rs`, issue #670), and
/// behind `Change` and `SampleTrigger` pulses (`open_modelica_pulse_events.rs`, issue #800).
#[derive(Clone, Copy, Debug, Default)]
pub struct TriggeredSampler {
    pub(crate) y_start: f64,
}

impl TriggeredSampler {
    fn output(ctx: &Ctx<'_>, inputs: &[Value], region: &[u64]) -> f64 {
        let u = read_real(inputs, 0);
        let trigger = settled_input(inputs, 1, 2, 0);
        let prev_trigger = word_bool(region[TRIGGERED_SAMPLER_PREV_TRIGGER_WORD]);
        if fire(ctx.is_initial(), prev_trigger, trigger).any() {
            u
        } else {
            f64::from_bits(region[TRIGGERED_SAMPLER_HELD_WORD])
        }
    }
}

impl Block for TriggeredSampler {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Discrete.TriggeredSampler",
            inputs: &[PortKind::Real, PortKind::Boolean],
            outputs: &[PortKind::Real],
            stateful: true,
        };
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Stateful
    }

    fn feeds_through(&self, in_idx: usize, out_idx: usize) -> bool {
        in_idx < 2 && out_idx == 0
    }

    fn settled_event_inputs(&self) -> &[usize] {
        &[1]
    }

    fn state_len(&self) -> usize {
        2
    }

    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region[TRIGGERED_SAMPLER_HELD_WORD] = canonicalize_real(self.y_start).to_bits();
        // Seeded `true`: frame 0 is initialisation, where the `when trigger` clause is inactive.
        region[TRIGGERED_SAMPLER_PREV_TRIGGER_WORD] = bool_word(true);
    }

    fn emit_from_state(
        &self,
        ctx: &Ctx<'_>,
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        emit_real(0, Self::output(ctx, inputs, region), emit);
    }

    fn update_state(&self, ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        region[TRIGGERED_SAMPLER_HELD_WORD] =
            canonicalize_real(Self::output(ctx, inputs, region)).to_bits();
        region[TRIGGERED_SAMPLER_PREV_TRIGGER_WORD] = bool_word(settled_input(inputs, 1, 2, 0).w);
    }
}

/// `CDL.Discrete.TriggeredMax` — track the maximum absolute sampled `u` at `trigger` events.
///
/// The source has no current parameters. Its `initial equation y = u` makes frame 0 emit the
/// current input exactly (a negative `u` or `-0.0` included): frame 0 is initialisation, where the
/// `when trigger` clause is inactive, and its `trigger` becomes `pre(trigger)` for frame 1. Each
/// later false-to-true transition applies `max(pre(y), abs(u))`. The block owns three `[S]` words:
/// held `Real` output (`f64::to_bits`), previous trigger bit, and a `has_history` bit that marks
/// frame 0. It feeds through from both inputs to `y` because the initial value and each
/// rising-trigger event depend on same-tick inputs. Real outputs and stored samples are NaN-canonicalized; `max` uses the same
/// deterministic NaN and signed-zero policy as `CDL.Reals.Max`. The block never panics for finite
/// or non-finite `Real` values; validated callers must provide a `Real` then a `Boolean` input.
/// `trigger` takes the settled-value rule of issue #800 (`docs/design/pulse-event-inputs.md`): with
/// a pulse or a sample upstream the event plan appends `w` and `s`, the block fires on
/// `(v and not w(k-1)) or (not v and s)` and stores `w` in its previous-trigger word, which nothing
/// but that test reads. At the initial frame
/// a sample instant applies `max(u, abs(u))` to the initial `y = u`, as OpenModelica does.
/// OpenModelica 1.25.1 agrees exactly at every frame, with the trigger true from frame 0 and with
/// it rising at frame 1 (`oce-conformance`'s `open_modelica_event_triggers.rs`, issue #670), and
/// behind `Change` and `SampleTrigger` pulses (`open_modelica_pulse_events.rs`, issue #800).
#[derive(Clone, Copy, Debug, Default)]
pub struct TriggeredMax;

impl TriggeredMax {
    fn output(ctx: &Ctx<'_>, inputs: &[Value], region: &[u64]) -> f64 {
        let u = read_real(inputs, 0);
        let trigger = settled_input(inputs, 1, 2, 0);
        let has_history = word_bool(region[TRIGGERED_MAX_HAS_HISTORY_WORD]);
        // `initial equation y = u`: at the initial frame `pre(y)` is the current input.
        let held = if has_history {
            f64::from_bits(region[TRIGGERED_MAX_HELD_WORD])
        } else {
            u
        };
        let initial = ctx.is_initial() || !has_history;
        let prev_trigger = word_bool(region[TRIGGERED_MAX_PREV_TRIGGER_WORD]);
        if fire(initial, prev_trigger, trigger).any() {
            det_max(held, u.abs())
        } else {
            held
        }
    }
}

impl Block for TriggeredMax {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Discrete.TriggeredMax",
            inputs: &[PortKind::Real, PortKind::Boolean],
            outputs: &[PortKind::Real],
            stateful: true,
        };
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Stateful
    }

    fn feeds_through(&self, in_idx: usize, out_idx: usize) -> bool {
        in_idx < 2 && out_idx == 0
    }

    fn settled_event_inputs(&self) -> &[usize] {
        &[1]
    }

    fn state_len(&self) -> usize {
        3
    }

    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region[TRIGGERED_MAX_HELD_WORD] = 0;
        region[TRIGGERED_MAX_PREV_TRIGGER_WORD] = bool_word(false);
        region[TRIGGERED_MAX_HAS_HISTORY_WORD] = bool_word(false);
    }

    fn emit_from_state(
        &self,
        ctx: &Ctx<'_>,
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        emit_real(0, Self::output(ctx, inputs, region), emit);
    }

    fn update_state(&self, ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        region[TRIGGERED_MAX_HELD_WORD] =
            canonicalize_real(Self::output(ctx, inputs, region)).to_bits();
        region[TRIGGERED_MAX_PREV_TRIGGER_WORD] = bool_word(settled_input(inputs, 1, 2, 0).w);
        region[TRIGGERED_MAX_HAS_HISTORY_WORD] = bool_word(true);
    }
}

/// `CDL.Discrete.TriggeredMovingMean` — average the last `n` triggered samples of `u`.
///
/// The source parameter is `Integer n(min=1)` with no default; validation requires it, while the
/// registry's direct-construction fallback uses `n = 1`. The source samples on
/// `when {initial(), trigger}`: the first tick samples once for `initial()` and once more when a
/// sample instant fires there (`s and not w`; the `when` fires again in the start's event
/// iteration, as OpenModelica 1.25.1 shows), then later ticks sample on `trigger` events by the
/// settled-value rule of issue #800 (`docs/design/pulse-event-inputs.md`). The block owns `4 + n` `[S]` words: held output, previous
/// trigger bit, populated-sample counter, next zero-based ring slot, and the `n` sampled `Real`
/// words. `n` is input-derived and has no class-level upper bound; the window is bounded by
/// `oce-graph`'s model-wide `MAX_STATE_WORDS` budget, which refuses an over-large window at BUILD
/// with a typed `StateTooLarge` (each sampling event costs `O(n)`). It feeds through from both
/// inputs to `y` because an initial event or same-tick rising trigger emits a mean that includes
/// the current `u`. Real samples and outputs are NaN canonicalized for deterministic traces. The
/// block never panics for finite or non-finite `Real` values; validated callers must provide a
/// `Real` then a `Boolean` input.
#[derive(Clone, Copy, Debug)]
pub struct TriggeredMovingMean {
    pub(crate) n: usize,
}

impl Default for TriggeredMovingMean {
    fn default() -> Self {
        Self { n: 1 }
    }
}

impl TriggeredMovingMean {
    fn window(self) -> usize {
        self.n.max(1)
    }

    fn sample_count(self, region: &[u64]) -> usize {
        (region[TRIGGERED_MOVING_MEAN_COUNTER_WORD] as usize).min(self.window())
    }

    fn next_sample_index(self, region: &[u64]) -> usize {
        (region[TRIGGERED_MOVING_MEAN_NEXT_INDEX_WORD] as usize) % self.window()
    }

    /// How many samples of the current `u` this frame takes, by `when {initial(), trigger}`. The
    /// initial frame (no sample yet) takes one for `initial()`, and a second when a sample instant
    /// fires there (`s and not w`): the `when` fires at initialisation and again in the start's
    /// event iteration, as OpenModelica does (`Discrete/TriggeredMovingMean.mo:36`; issue #800,
    /// a refinement of decision 3). A later frame takes one when the settled-value rule fires.
    fn samples(self, ctx: &Ctx<'_>, inputs: &[Value], region: &[u64]) -> usize {
        let trigger = settled_input(inputs, 1, 2, 0);
        let prev_trigger = word_bool(region[TRIGGERED_MOVING_MEAN_PREV_TRIGGER_WORD]);
        if self.sample_count(region) == 0 {
            1 + usize::from(fire(true, prev_trigger, trigger).first)
        } else {
            usize::from(fire(ctx.is_initial(), prev_trigger, trigger).any())
        }
    }

    /// Whether slot `i` is among the `samples` slots written from the next one on (`samples` is
    /// at most 2, so at most two distinct slots).
    fn takes(self, i: usize, samples: usize, region: &[u64]) -> bool {
        let first = self.next_sample_index(region);
        (0..samples).any(|j| (first + j) % self.window() == i)
    }

    fn sampled_mean(self, u: f64, samples: usize, region: &[u64]) -> f64 {
        let n = self.window();
        let next_counter = (self.sample_count(region) + samples).min(n);
        let sample = canonicalize_real(u);
        let mut sum = 0.0;
        for i in 0..n {
            let value = if self.takes(i, samples, region) {
                sample
            } else {
                f64::from_bits(region[TRIGGERED_MOVING_MEAN_SAMPLE_WORDS_START + i])
            };
            sum += value;
        }
        sum / next_counter as f64
    }

    fn output(self, ctx: &Ctx<'_>, inputs: &[Value], region: &[u64]) -> f64 {
        match self.samples(ctx, inputs, region) {
            0 => f64::from_bits(region[TRIGGERED_MOVING_MEAN_HELD_WORD]),
            samples => self.sampled_mean(read_real(inputs, 0), samples, region),
        }
    }
}

impl Block for TriggeredMovingMean {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Discrete.TriggeredMovingMean",
            inputs: &[PortKind::Real, PortKind::Boolean],
            outputs: &[PortKind::Real],
            stateful: true,
        };
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Stateful
    }

    fn feeds_through(&self, in_idx: usize, out_idx: usize) -> bool {
        in_idx < 2 && out_idx == 0
    }

    fn settled_event_inputs(&self) -> &[usize] {
        &[1]
    }

    fn state_len(&self) -> usize {
        // Saturating: an unvalidated huge window reports `usize::MAX`, which the checked BUILD
        // allocation refuses as over budget instead of overflowing here.
        TRIGGERED_MOVING_MEAN_SAMPLE_WORDS_START.saturating_add(self.window())
    }

    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region[TRIGGERED_MOVING_MEAN_HELD_WORD] = 0.0f64.to_bits();
        region[TRIGGERED_MOVING_MEAN_PREV_TRIGGER_WORD] = bool_word(false);
        region[TRIGGERED_MOVING_MEAN_COUNTER_WORD] = 0;
        region[TRIGGERED_MOVING_MEAN_NEXT_INDEX_WORD] = 0;
        for word in &mut region[TRIGGERED_MOVING_MEAN_SAMPLE_WORDS_START..] {
            *word = 0.0f64.to_bits();
        }
    }

    fn emit_from_state(
        &self,
        ctx: &Ctx<'_>,
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        emit_real(0, self.output(ctx, inputs, region), emit);
    }

    fn update_state(&self, ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        let samples = self.samples(ctx, inputs, region);
        if samples > 0 {
            let n = self.window();
            let sample_index = self.next_sample_index(region);
            let next_counter = (self.sample_count(region) + samples).min(n);
            let output = self.sampled_mean(read_real(inputs, 0), samples, region);
            region[TRIGGERED_MOVING_MEAN_HELD_WORD] = canonicalize_real(output).to_bits();
            for j in 0..samples {
                region[TRIGGERED_MOVING_MEAN_SAMPLE_WORDS_START + (sample_index + j) % n] =
                    canonicalize_real(read_real(inputs, 0)).to_bits();
            }
            region[TRIGGERED_MOVING_MEAN_COUNTER_WORD] = next_counter as u64;
            region[TRIGGERED_MOVING_MEAN_NEXT_INDEX_WORD] = ((sample_index + samples) % n) as u64;
        }
        region[TRIGGERED_MOVING_MEAN_PREV_TRIGGER_WORD] =
            bool_word(settled_input(inputs, 1, 2, 0).w);
    }
}
