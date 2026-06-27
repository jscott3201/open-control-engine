//! Typed `CDL.*.Sources.Pulse` blocks.
//!
//! The three pulse sources are source roots: they have no input connectors and derive output only
//! from model time plus resolved parameters. `Reals` and `Integers` are CDL composites over the
//! logical pulse and the corresponding Boolean conversion block, so this module keeps one shared
//! timing predicate and typed output wrappers.

use oce_model::Value;

use crate::{Block, BlockKind, BlockSignature, Ctx, ParamRule, PortKind, Time, emit_real};

/// Minimum valid `period` and `width` bound for source pulses, from `CDL.Constants.small`.
pub(crate) const MIN_SOURCE_PULSE_BOUND: f64 = 1e-37;

/// Parameter validation rules shared by Logical/Reals/Integers `Sources.Pulse`.
pub(crate) const SOURCE_PULSE_PARAM_RULES: &[ParamRule] = &[
    ParamRule::Required { name: "period" },
    ParamRule::RealGreaterOrEqual {
        name: "period",
        min: MIN_SOURCE_PULSE_BOUND,
    },
    ParamRule::RealGreaterOrEqual {
        name: "width",
        min: MIN_SOURCE_PULSE_BOUND,
    },
    ParamRule::RealLessOrEqualConstant {
        name: "width",
        max: 1.0,
    },
];

/// `CDL.Logical.Sources.Pulse` - periodic Boolean pulse source.
///
/// `period` is measured in seconds and must be at least `CDL.Constants.small` in validated models.
/// `width` is the fraction of the period for which the output is true, in `(0, 1]`. `shift` is the
/// model-time phase anchor in seconds. Direct construction with invalid `period` or `width`
/// degrades to finite effective values so ticking is total and panic-free.
#[derive(Clone, Copy, Debug)]
pub struct LogicalPulse {
    /// Fraction of each period for which the pulse is true.
    pub(crate) width: f64,
    /// Pulse period in seconds.
    pub(crate) period: f64,
    /// Phase anchor in seconds.
    pub(crate) shift: f64,
}

impl Default for LogicalPulse {
    fn default() -> Self {
        Self {
            width: 0.5,
            period: 1.0,
            shift: 0.0,
        }
    }
}

impl LogicalPulse {
    fn high_at(self, t: Time) -> bool {
        source_pulse_high_at(t, self.width, self.period, self.shift)
    }
}

impl Block for LogicalPulse {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Logical.Sources.Pulse",
            inputs: &[],
            outputs: &[PortKind::Boolean],
            stateful: false,
        };
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }

    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        false
    }

    fn step_algebraic(&self, ctx: &Ctx<'_>, _inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit(0, Value::Boolean(self.high_at(ctx.t())));
    }
}

/// `CDL.Reals.Sources.Pulse` - typed Real pulse source.
///
/// This is equivalent to `Logical.Sources.Pulse` followed by `BooleanToReal` with
/// `realTrue = offset + amplitude` and `realFalse = offset`. Non-finite Real parameters follow the
/// normal Real-output canonicalization path. The block has no inputs, no state, and never panics.
#[derive(Clone, Copy, Debug)]
pub struct RealPulse {
    /// Value added to `offset` while the logical pulse is true.
    pub(crate) amplitude: f64,
    /// Fraction of each period for which the pulse is true.
    pub(crate) width: f64,
    /// Pulse period in seconds.
    pub(crate) period: f64,
    /// Phase anchor in seconds.
    pub(crate) shift: f64,
    /// Output while the logical pulse is false.
    pub(crate) offset: f64,
}

impl Default for RealPulse {
    fn default() -> Self {
        Self {
            amplitude: 1.0,
            width: 0.5,
            period: 1.0,
            shift: 0.0,
            offset: 0.0,
        }
    }
}

impl RealPulse {
    fn output(self, t: Time) -> f64 {
        if source_pulse_high_at(t, self.width, self.period, self.shift) {
            self.offset + self.amplitude
        } else {
            self.offset
        }
    }
}

impl Block for RealPulse {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.Sources.Pulse",
            inputs: &[],
            outputs: &[PortKind::Real],
            stateful: false,
        };
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }

    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        false
    }

    fn step_algebraic(&self, ctx: &Ctx<'_>, _inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit_real(0, self.output(ctx.t()), emit);
    }
}

/// `CDL.Integers.Sources.Pulse` - typed Integer pulse source.
///
/// This is equivalent to `Logical.Sources.Pulse` followed by `BooleanToInteger` with
/// `integerTrue = offset + amplitude` and `integerFalse = offset`. The sum uses the crate's
/// deterministic two's-complement wrapping convention for Integer arithmetic. The block has no
/// inputs, no state, and never panics.
#[derive(Clone, Copy, Debug)]
pub struct IntegerPulse {
    /// Value added to `offset` while the logical pulse is true.
    pub(crate) amplitude: i64,
    /// Fraction of each period for which the pulse is true.
    pub(crate) width: f64,
    /// Pulse period in seconds.
    pub(crate) period: f64,
    /// Phase anchor in seconds.
    pub(crate) shift: f64,
    /// Output while the logical pulse is false.
    pub(crate) offset: i64,
}

impl Default for IntegerPulse {
    fn default() -> Self {
        Self {
            amplitude: 1,
            width: 0.5,
            period: 1.0,
            shift: 0.0,
            offset: 0,
        }
    }
}

impl IntegerPulse {
    fn output(self, t: Time) -> i64 {
        if source_pulse_high_at(t, self.width, self.period, self.shift) {
            self.offset.wrapping_add(self.amplitude)
        } else {
            self.offset
        }
    }
}

impl Block for IntegerPulse {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Integers.Sources.Pulse",
            inputs: &[],
            outputs: &[PortKind::Integer],
            stateful: false,
        };
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }

    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        false
    }

    fn step_algebraic(&self, ctx: &Ctx<'_>, _inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit(0, Value::Integer(self.output(ctx.t())));
    }
}

fn source_pulse_high_at(t: Time, width: f64, period: f64, shift: f64) -> bool {
    if !t.is_finite() {
        return false;
    }
    let period = valid_pulse_period(period);
    let width = valid_pulse_width(width);
    if width >= 1.0 {
        return true;
    }

    let phase = modelica_mod(finite_shift(shift), period);
    let mut t0 = buildings_round_six((t / period).floor() * period + phase);
    let mut t1 = t0 + width * period;

    if t + period < t1 {
        t0 -= period;
        t1 -= period;
    }
    if t >= t1 {
        t0 += period;
    } else if t < t0 {
        t1 -= period;
    }

    if t0 < t1 {
        t >= t0 && t < t1
    } else {
        !(t >= t1 && t < t0)
    }
}

fn valid_pulse_period(period: f64) -> f64 {
    if period.is_finite() && period >= MIN_SOURCE_PULSE_BOUND {
        period
    } else {
        1.0
    }
}

fn valid_pulse_width(width: f64) -> f64 {
    if !width.is_finite() {
        return 0.5;
    }
    width.clamp(MIN_SOURCE_PULSE_BOUND, 1.0)
}

fn finite_shift(shift: f64) -> f64 {
    if shift.is_finite() { shift } else { 0.0 }
}

fn modelica_mod(x: f64, y: f64) -> f64 {
    x - (x / y).floor() * y
}

fn buildings_round_six(x: f64) -> f64 {
    const FACTOR: f64 = 1_000_000.0;
    if x > 0.0 {
        (x * FACTOR + 0.5).floor() / FACTOR
    } else {
        (x * FACTOR - 0.5).ceil() / FACTOR
    }
}
