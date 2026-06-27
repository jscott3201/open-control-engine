//! Periodic sampled/hold blocks from `CDL.Discrete`.

use oce_model::{ParamTable, Value, determinism::canonicalize_real};

use crate::{Block, BlockKind, BlockSignature, Ctx, PortKind, Time, emit_real, read_real};

const SAMPLED_HELD_WORD: usize = 0;
const SAMPLED_T0_WORD: usize = 1;
const SAMPLED_LAST_INDEX_WORD: usize = 2;
const SAMPLED_INITIALIZED_WORD: usize = 3;
const SAMPLED_STATE_WORDS: usize = 4;
const FIRST_ORDER_HOLD_INITIALIZED_WORD: usize = 0;
const FIRST_ORDER_HOLD_T0_WORD: usize = 1;
const FIRST_ORDER_HOLD_LAST_INDEX_WORD: usize = 2;
const FIRST_ORDER_HOLD_T_SAMPLE_WORD: usize = 3;
const FIRST_ORDER_HOLD_U_SAMPLE_WORD: usize = 4;
const FIRST_ORDER_HOLD_PRE_U_SAMPLE_WORD: usize = 5;
const FIRST_ORDER_HOLD_SLOPE_WORD: usize = 6;
const FIRST_ORDER_HOLD_STATE_WORDS: usize = 7;
const SAMPLE_INDEX_EPS: f64 = 1e-9;
pub(crate) const MIN_SAMPLE_PERIOD: f64 = 1e-3;

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

pub(crate) fn valid_sample_period(period: f64) -> f64 {
    if period.is_finite() && period >= MIN_SAMPLE_PERIOD {
        period
    } else {
        1.0
    }
}

fn buildings_round_six(x: f64) -> f64 {
    const FACTOR: f64 = 1_000_000.0;
    if x > 0.0 {
        (x * FACTOR + 0.5).floor() / FACTOR
    } else {
        (x * FACTOR - 0.5).ceil() / FACTOR
    }
}

pub(crate) fn initial_sample_time(t_start: Time, sample_period: f64) -> f64 {
    let period = valid_sample_period(sample_period);
    buildings_round_six((t_start / period).floor() * period)
}

fn sample_index(t_now: Time, t0: f64, sample_period: f64) -> i64 {
    ((t_now - t0) / valid_sample_period(sample_period) + SAMPLE_INDEX_EPS).floor() as i64
}

pub(crate) fn sample_due(t_now: Time, t0: f64, sample_period: f64, last_index: i64) -> (bool, i64) {
    let index = sample_index(t_now, t0, sample_period);
    (index > last_index, index)
}

/// `CDL.Discrete.Sampler` — sample `u` on `initial()` and periodic sample instants.
///
/// The source parameter is required `samplePeriod(min=1E-3)`. Buildings initializes the first
/// periodic instant as `round(integer(time/samplePeriod)*samplePeriod, n=6)`, so this block lazily
/// computes `t0` from the first tick time and then fires when the current tick crosses
/// `t0 + k*samplePeriod`. It owns four `[S]` words: held output, `t0`, last fired sample index, and
/// an initialization bit. It feeds through from `u` because the initial tick and sample instants emit
/// the current input. Invalid direct-construction periods degrade to `1.0`; validated models must
/// supply a finite period at least `1E-3`.
#[derive(Clone, Copy, Debug)]
pub struct Sampler {
    pub(crate) sample_period: f64,
}

impl Default for Sampler {
    fn default() -> Self {
        Self { sample_period: 1.0 }
    }
}

impl Sampler {
    fn period(self) -> f64 {
        valid_sample_period(self.sample_period)
    }

    fn initialized(region: &[u64]) -> bool {
        word_bool(region[SAMPLED_INITIALIZED_WORD])
    }

    fn output(self, ctx: &Ctx<'_>, inputs: &[Value], region: &[u64]) -> f64 {
        if !Self::initialized(region) {
            return read_real(inputs, 0);
        }
        let t0 = f64::from_bits(region[SAMPLED_T0_WORD]);
        let last = word_i64(region[SAMPLED_LAST_INDEX_WORD]);
        if sample_due(ctx.t(), t0, self.period(), last).0 {
            read_real(inputs, 0)
        } else {
            f64::from_bits(region[SAMPLED_HELD_WORD])
        }
    }

    fn update_sample_state(self, ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        let period = self.period();
        if !Self::initialized(region) {
            let t0 = initial_sample_time(ctx.t(), period);
            region[SAMPLED_HELD_WORD] = canonicalize_real(read_real(inputs, 0)).to_bits();
            region[SAMPLED_T0_WORD] = t0.to_bits();
            region[SAMPLED_LAST_INDEX_WORD] = i64_word(sample_index(ctx.t(), t0, period));
            region[SAMPLED_INITIALIZED_WORD] = bool_word(true);
            return;
        }
        let t0 = f64::from_bits(region[SAMPLED_T0_WORD]);
        let last = word_i64(region[SAMPLED_LAST_INDEX_WORD]);
        let (due, index) = sample_due(ctx.t(), t0, period, last);
        if due {
            region[SAMPLED_HELD_WORD] = canonicalize_real(read_real(inputs, 0)).to_bits();
            region[SAMPLED_LAST_INDEX_WORD] = i64_word(index);
        }
    }
}

impl Block for Sampler {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Discrete.Sampler",
            inputs: &[PortKind::Real],
            outputs: &[PortKind::Real],
            stateful: true,
        };
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Stateful
    }

    fn feeds_through(&self, in_idx: usize, out_idx: usize) -> bool {
        in_idx == 0 && out_idx == 0
    }

    fn state_len(&self) -> usize {
        SAMPLED_STATE_WORDS
    }

    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region[SAMPLED_HELD_WORD] = 0.0f64.to_bits();
        region[SAMPLED_T0_WORD] = 0.0f64.to_bits();
        region[SAMPLED_LAST_INDEX_WORD] = i64_word(-1);
        region[SAMPLED_INITIALIZED_WORD] = bool_word(false);
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
        self.update_sample_state(ctx, inputs, region);
    }
}

/// `CDL.Discrete.ZeroOrderHold` — hold the last periodically sampled `u`.
///
/// The sample clock matches [`Sampler`]. The first tick emits the current input because the
/// Buildings documentation states that the input feeds directly at initial time. After that, `y`
/// follows source equation `y = pre(ySample)`: a sample instant emits the previously held value and
/// stores current `u` for later ticks. The static feedthrough oracle is conservative because the
/// initial tick depends on current `u`; subsequent ticks act as a loop cut.
#[derive(Clone, Copy, Debug)]
pub struct ZeroOrderHold {
    pub(crate) sample_period: f64,
}

impl Default for ZeroOrderHold {
    fn default() -> Self {
        Self { sample_period: 1.0 }
    }
}

impl ZeroOrderHold {
    fn sampler(self) -> Sampler {
        Sampler {
            sample_period: self.sample_period,
        }
    }

    fn output(self, inputs: &[Value], region: &[u64]) -> f64 {
        if !Sampler::initialized(region) {
            read_real(inputs, 0)
        } else {
            f64::from_bits(region[SAMPLED_HELD_WORD])
        }
    }
}

impl Block for ZeroOrderHold {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Discrete.ZeroOrderHold",
            inputs: &[PortKind::Real],
            outputs: &[PortKind::Real],
            stateful: true,
        };
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Stateful
    }

    fn feeds_through(&self, in_idx: usize, out_idx: usize) -> bool {
        in_idx == 0 && out_idx == 0
    }

    fn state_len(&self) -> usize {
        SAMPLED_STATE_WORDS
    }

    fn init_state(&self, region: &mut [u64], params: &ParamTable) {
        self.sampler().init_state(region, params);
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

    fn update_state(&self, ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        self.sampler().update_sample_state(ctx, inputs, region);
    }
}

/// `CDL.Discrete.FirstOrderHold` — extrapolate from the last two periodically sampled inputs.
///
/// The sample clock matches [`Sampler`]. The first tick seeds `pre(tSample)`, `pre(uSample)`,
/// `pre(pre_uSample)`, and `pre(c)` from the current input and source-derived `t0`. A later sample
/// instant emits the last sampled value, then stores current `u` and the new slope
/// `(uSample - pre_uSample)/samplePeriod` for following ticks. Between sample instants, `y` is the
/// previous sample plus the stored slope times `time - tSample`. The static feedthrough oracle is
/// conservative because the initial tick depends on current `u`; after initialization, current `u`
/// only affects future output.
#[derive(Clone, Copy, Debug)]
pub struct FirstOrderHold {
    pub(crate) sample_period: f64,
}

impl Default for FirstOrderHold {
    fn default() -> Self {
        Self { sample_period: 1.0 }
    }
}

impl FirstOrderHold {
    fn period(self) -> f64 {
        valid_sample_period(self.sample_period)
    }

    fn initialized(region: &[u64]) -> bool {
        word_bool(region[FIRST_ORDER_HOLD_INITIALIZED_WORD])
    }

    fn output(self, ctx: &Ctx<'_>, inputs: &[Value], region: &[u64]) -> f64 {
        if !Self::initialized(region) {
            return read_real(inputs, 0);
        }
        let t0 = f64::from_bits(region[FIRST_ORDER_HOLD_T0_WORD]);
        let last = word_i64(region[FIRST_ORDER_HOLD_LAST_INDEX_WORD]);
        if sample_due(ctx.t(), t0, self.period(), last).0 {
            f64::from_bits(region[FIRST_ORDER_HOLD_U_SAMPLE_WORD])
        } else {
            let pre_u = f64::from_bits(region[FIRST_ORDER_HOLD_PRE_U_SAMPLE_WORD]);
            let slope = f64::from_bits(region[FIRST_ORDER_HOLD_SLOPE_WORD]);
            let t_sample = f64::from_bits(region[FIRST_ORDER_HOLD_T_SAMPLE_WORD]);
            pre_u + slope * (ctx.t() - t_sample)
        }
    }
}

impl Block for FirstOrderHold {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Discrete.FirstOrderHold",
            inputs: &[PortKind::Real],
            outputs: &[PortKind::Real],
            stateful: true,
        };
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Stateful
    }

    fn feeds_through(&self, in_idx: usize, out_idx: usize) -> bool {
        in_idx == 0 && out_idx == 0
    }

    fn state_len(&self) -> usize {
        FIRST_ORDER_HOLD_STATE_WORDS
    }

    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region[FIRST_ORDER_HOLD_INITIALIZED_WORD] = bool_word(false);
        region[FIRST_ORDER_HOLD_T0_WORD] = 0.0f64.to_bits();
        region[FIRST_ORDER_HOLD_LAST_INDEX_WORD] = i64_word(-1);
        region[FIRST_ORDER_HOLD_T_SAMPLE_WORD] = 0.0f64.to_bits();
        region[FIRST_ORDER_HOLD_U_SAMPLE_WORD] = 0.0f64.to_bits();
        region[FIRST_ORDER_HOLD_PRE_U_SAMPLE_WORD] = 0.0f64.to_bits();
        region[FIRST_ORDER_HOLD_SLOPE_WORD] = 0.0f64.to_bits();
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
        let period = self.period();
        let u = canonicalize_real(read_real(inputs, 0));
        if !Self::initialized(region) {
            let t0 = initial_sample_time(ctx.t(), period);
            region[FIRST_ORDER_HOLD_INITIALIZED_WORD] = bool_word(true);
            region[FIRST_ORDER_HOLD_T0_WORD] = t0.to_bits();
            region[FIRST_ORDER_HOLD_LAST_INDEX_WORD] = i64_word(sample_index(ctx.t(), t0, period));
            region[FIRST_ORDER_HOLD_T_SAMPLE_WORD] = t0.to_bits();
            region[FIRST_ORDER_HOLD_U_SAMPLE_WORD] = u.to_bits();
            region[FIRST_ORDER_HOLD_PRE_U_SAMPLE_WORD] = u.to_bits();
            region[FIRST_ORDER_HOLD_SLOPE_WORD] = 0.0f64.to_bits();
            return;
        }

        let t0 = f64::from_bits(region[FIRST_ORDER_HOLD_T0_WORD]);
        let last = word_i64(region[FIRST_ORDER_HOLD_LAST_INDEX_WORD]);
        let (due, index) = sample_due(ctx.t(), t0, period, last);
        if due {
            let previous_u = f64::from_bits(region[FIRST_ORDER_HOLD_U_SAMPLE_WORD]);
            let first_trigger = ctx.t() <= t0 + period / 2.0;
            region[FIRST_ORDER_HOLD_LAST_INDEX_WORD] = i64_word(index);
            region[FIRST_ORDER_HOLD_T_SAMPLE_WORD] = ctx.t().to_bits();
            region[FIRST_ORDER_HOLD_U_SAMPLE_WORD] = u.to_bits();
            region[FIRST_ORDER_HOLD_PRE_U_SAMPLE_WORD] = previous_u.to_bits();
            region[FIRST_ORDER_HOLD_SLOPE_WORD] = if first_trigger {
                0.0f64.to_bits()
            } else {
                canonicalize_real((u - previous_u) / period).to_bits()
            };
        }
    }
}
