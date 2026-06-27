//! Stateless `CDL.Reals.Sources` blocks whose outputs are derived from model time or parameters.
//!
//! These are source roots: they have no input connectors, carry no per-tick state, and read time
//! only from the scheduler-provided [`Ctx`]. They intentionally do not consult wall-clock,
//! calendar, timezone, or daylight-saving APIs.

use oce_model::Value;

use crate::{Block, BlockKind, BlockSignature, Ctx, PortKind, emit_real};

/// Minimum valid `CDL.Reals.Sources.Ramp.duration`, from `CDL.Constants.small`.
pub(crate) const MIN_SOURCE_RAMP_DURATION: f64 = 1e-37;

/// `CDL.Reals.Sources.CivilTime` - emit the host scheduler's model time in seconds.
///
/// The block has no parameters, inputs, state, panics, or wall-clock dependency. Negative
/// simulation starts are preserved because the CDL source equation is exactly `y = time`.
#[derive(Clone, Copy, Debug, Default)]
pub struct CivilTime;

impl Block for CivilTime {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.Sources.CivilTime",
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
        emit_real(0, ctx.t(), emit);
    }
}

/// `CDL.Reals.Sources.Ramp` - parameterized time ramp source.
///
/// The CDL equation is `offset` before `startTime`, then a linear interpolation to
/// `offset + height` over `duration`, and finally `offset + height`. Validation requires
/// `duration >= CDL.Constants.small`; direct construction falls back to `1.0 s` when given an
/// invalid duration so the tick path remains total and never divides by zero.
#[derive(Clone, Copy, Debug)]
pub struct SourceRamp {
    /// Ramp height added after the duration elapses.
    pub(crate) height: f64,
    /// Ramp duration in seconds.
    pub(crate) duration: f64,
    /// Output before the ramp starts.
    pub(crate) offset: f64,
    /// Model time at which the ramp begins.
    pub(crate) start_time: f64,
}

impl Default for SourceRamp {
    fn default() -> Self {
        Self {
            height: 1.0,
            duration: 1.0,
            offset: 0.0,
            start_time: 0.0,
        }
    }
}

impl SourceRamp {
    fn duration_eff(self) -> f64 {
        if self.duration.is_finite() && self.duration >= MIN_SOURCE_RAMP_DURATION {
            self.duration
        } else {
            1.0
        }
    }

    fn output(self, t: f64) -> f64 {
        let duration = self.duration_eff();
        self.offset
            + if t < self.start_time {
                0.0
            } else if t < self.start_time + duration {
                (t - self.start_time) * self.height / duration
            } else {
                self.height
            }
    }
}

impl Block for SourceRamp {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.Sources.Ramp",
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
