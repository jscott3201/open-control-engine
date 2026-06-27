//! `CDL.Discrete` scalar blocks.
//!
//! `UnitDelay` is the discrete one-sample loop-breaker. `TriggeredSampler` is event-stateful but
//! transparent on rising trigger instants: same-tick `u` and `trigger` can determine `y`, so it
//! must not be treated as a graph cut.

use oce_model::{
    ParamTable, Value,
    determinism::{canonicalize_real, det_max},
};

use crate::{Block, BlockKind, BlockSignature, Ctx, PortKind, emit_real, read_bool, read_real};

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

/// `CDL.Discrete.UnitDelay` — `y(k) = u(k−1)`, a one-sample `Real` delay and a discrete
/// loop-breaker (`03` §4.6): stateful with `feeds_through == false`, so it cuts the
/// direct-feedthrough DAG. The held value lives in one `[S]` state word (`f64::to_bits`), seeded
/// from the `y_start` parameter (default `0.0`).
#[derive(Clone, Copy, Debug, Default)]
pub struct UnitDelay {
    pub(crate) y_start: f64,
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
        1 // one word holds the one-sample-delayed Real (as f64 bits)
    }
    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region[0] = self.y_start.to_bits();
    }
    fn emit_from_state(
        &self,
        _ctx: &Ctx<'_>,
        _inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        emit_real(0, f64::from_bits(region[0]), emit); // the prior sample, held since last tick
    }
    fn update_state(&self, _ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        region[0] = read_real(inputs, 0).to_bits(); // latch the current input for next tick
    }
}

/// `CDL.Discrete.TriggeredSampler` — sample `u` on a rising Boolean `trigger`.
///
/// Before the first trigger instant, `y` is the parameter `y_start` (default `0`). The first tick
/// with `trigger = true` is a rising edge because the CDL source initializes `pre(trigger)` to
/// `false`. The block owns two `[S]` words: the held `Real` output (`f64::to_bits`) and the
/// previous trigger bit. It feeds through from both inputs to `y` because a same-tick rising trigger
/// emits the current `u`. Real outputs and stored samples are NaN-canonicalized for deterministic
/// traces. The block does not panic for finite or non-finite `Real` values; validated callers must
/// provide a `Real` then a `Boolean` input.
#[derive(Clone, Copy, Debug, Default)]
pub struct TriggeredSampler {
    pub(crate) y_start: f64,
}

impl TriggeredSampler {
    fn output(inputs: &[Value], region: &[u64]) -> f64 {
        let u = read_real(inputs, 0);
        let trigger = read_bool(inputs, 1);
        let prev_trigger = word_bool(region[TRIGGERED_SAMPLER_PREV_TRIGGER_WORD]);
        if trigger && !prev_trigger {
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

    fn state_len(&self) -> usize {
        2
    }

    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region[TRIGGERED_SAMPLER_HELD_WORD] = canonicalize_real(self.y_start).to_bits();
        region[TRIGGERED_SAMPLER_PREV_TRIGGER_WORD] = bool_word(false);
    }

    fn emit_from_state(
        &self,
        _ctx: &Ctx<'_>,
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        emit_real(0, Self::output(inputs, region), emit);
    }

    fn update_state(&self, _ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        region[TRIGGERED_SAMPLER_HELD_WORD] =
            canonicalize_real(Self::output(inputs, region)).to_bits();
        region[TRIGGERED_SAMPLER_PREV_TRIGGER_WORD] = bool_word(read_bool(inputs, 1));
    }
}

/// `CDL.Discrete.TriggeredMax` — track the maximum absolute sampled `u` on rising `trigger`.
///
/// The source has no current parameters. Its `initial equation y = u` means the first emitted value
/// is the current input unless the first tick also has a rising trigger, in which case the `when`
/// equation applies `max(pre(y), abs(u))` with `pre(y)` seeded by the initial value. The block owns
/// three `[S]` words: held `Real` output (`f64::to_bits`), previous trigger bit, and a
/// `has_history` bit that lets the first emit read current `u` as the initial value. It feeds
/// through from both inputs to `y` because the initial value and each rising-trigger event depend on
/// same-tick inputs. Real outputs and stored samples are NaN-canonicalized; `max` uses the same
/// deterministic NaN and signed-zero policy as `CDL.Reals.Max`. The block never panics for finite
/// or non-finite `Real` values; validated callers must provide a `Real` then a `Boolean` input.
#[derive(Clone, Copy, Debug, Default)]
pub struct TriggeredMax;

impl TriggeredMax {
    fn output(inputs: &[Value], region: &[u64]) -> f64 {
        let u = read_real(inputs, 0);
        let trigger = read_bool(inputs, 1);
        let has_history = word_bool(region[TRIGGERED_MAX_HAS_HISTORY_WORD]);
        let held = if has_history {
            f64::from_bits(region[TRIGGERED_MAX_HELD_WORD])
        } else {
            u
        };
        let prev_trigger = has_history && word_bool(region[TRIGGERED_MAX_PREV_TRIGGER_WORD]);
        if trigger && !prev_trigger {
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
        _ctx: &Ctx<'_>,
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        emit_real(0, Self::output(inputs, region), emit);
    }

    fn update_state(&self, _ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        region[TRIGGERED_MAX_HELD_WORD] = canonicalize_real(Self::output(inputs, region)).to_bits();
        region[TRIGGERED_MAX_PREV_TRIGGER_WORD] = bool_word(read_bool(inputs, 1));
        region[TRIGGERED_MAX_HAS_HISTORY_WORD] = bool_word(true);
    }
}

/// `CDL.Discrete.TriggeredMovingMean` — average the last `n` triggered samples of `u`.
///
/// The source parameter is `Integer n(min=1)` with no default; validation requires it, while the
/// registry's direct-construction fallback uses `n = 1`. The source samples on
/// `when {initial(), trigger}`: the first tick always samples exactly once, then later ticks sample
/// only on false-to-true trigger edges. The block owns `4 + n` `[S]` words: held output, previous
/// trigger bit, populated-sample counter, next zero-based ring slot, and the `n` sampled `Real`
/// words. It feeds through from both inputs to `y` because an initial event or same-tick rising
/// trigger emits a mean that includes the current `u`. Real samples and outputs are NaN
/// canonicalized for deterministic traces. The block never panics for finite or non-finite `Real`
/// values; validated callers must provide a `Real` then a `Boolean` input.
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

    fn should_sample(self, inputs: &[Value], region: &[u64]) -> bool {
        let trigger = read_bool(inputs, 1);
        let prev_trigger = word_bool(region[TRIGGERED_MOVING_MEAN_PREV_TRIGGER_WORD]);
        self.sample_count(region) == 0 || (trigger && !prev_trigger)
    }

    fn sampled_mean(self, u: f64, region: &[u64]) -> f64 {
        let n = self.window();
        let sample_index = self.next_sample_index(region);
        let next_counter = (self.sample_count(region) + 1).min(n);
        let sample = canonicalize_real(u);
        let mut sum = 0.0;
        for i in 0..n {
            let value = if i == sample_index {
                sample
            } else {
                f64::from_bits(region[TRIGGERED_MOVING_MEAN_SAMPLE_WORDS_START + i])
            };
            sum += value;
        }
        sum / next_counter as f64
    }

    fn output(self, inputs: &[Value], region: &[u64]) -> f64 {
        if self.should_sample(inputs, region) {
            self.sampled_mean(read_real(inputs, 0), region)
        } else {
            f64::from_bits(region[TRIGGERED_MOVING_MEAN_HELD_WORD])
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

    fn state_len(&self) -> usize {
        TRIGGERED_MOVING_MEAN_SAMPLE_WORDS_START + self.window()
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
        _ctx: &Ctx<'_>,
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        emit_real(0, self.output(inputs, region), emit);
    }

    fn update_state(&self, _ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        if self.should_sample(inputs, region) {
            let n = self.window();
            let sample_index = self.next_sample_index(region);
            let next_counter = (self.sample_count(region) + 1).min(n);
            let output = self.sampled_mean(read_real(inputs, 0), region);
            region[TRIGGERED_MOVING_MEAN_HELD_WORD] = canonicalize_real(output).to_bits();
            region[TRIGGERED_MOVING_MEAN_SAMPLE_WORDS_START + sample_index] =
                canonicalize_real(read_real(inputs, 0)).to_bits();
            region[TRIGGERED_MOVING_MEAN_COUNTER_WORD] = next_counter as u64;
            region[TRIGGERED_MOVING_MEAN_NEXT_INDEX_WORD] = ((sample_index + 1) % n) as u64;
        }
        region[TRIGGERED_MOVING_MEAN_PREV_TRIGGER_WORD] = bool_word(read_bool(inputs, 1));
    }
}
