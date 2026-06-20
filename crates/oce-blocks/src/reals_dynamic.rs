//! Dynamic `CDL.Reals` blocks whose outputs are state-derived. These blocks define the
//! forward-Euler-on-tick convention that the rest of the integrating Reals family reuses: emit the
//! prior state first, then advance the state with `dt = t(k) - t(k-1)` in `update_state`.
//!
//! Non-finite inputs are deliberately not trapped here yet: NaN/Inf propagate through the
//! documented recurrence and join the deferred centralized non-finite validation/diagnostic seam.

use oce_model::{ParamTable, Value};

use crate::{Block, BlockKind, BlockSignature, Ctx, PortKind, read_bool, read_real};

const X_WORD: usize = 0;
const PREV_T_WORD: usize = 1;
const PREV_TRIGGER_WORD: usize = 2;
const STATE_WORDS: usize = 3;
const PREV_T_UNSET: u64 = u64::MAX;

/// `CDL.Reals.IntegratorWithReset` — forward-Euler integrator with a rising-edge reset:
///
/// - `[S]` loop cut: `y` is emitted from the prior accumulator and feeds through from no current
///   input (`y <- {}`).
/// - First tick integrates with `dt = 0` because there is no previous model time.
/// - A rising `trigger` stores `y_reset_in` during `update_state`, so the reset is visible at the
///   **next** emit; a held-high trigger does not reset again.
#[derive(Clone, Copy, Debug, Default)]
pub struct IntegratorWithReset {
    pub(crate) y_start: f64,
}

impl Block for IntegratorWithReset {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.IntegratorWithReset",
            inputs: &[PortKind::Real, PortKind::Real, PortKind::Boolean],
            outputs: &[PortKind::Real],
            stateful: true,
        };
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Stateful
    }

    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        false
    }

    fn state_len(&self) -> usize {
        STATE_WORDS
    }

    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region[X_WORD] = self.y_start.to_bits();
        region[PREV_T_WORD] = PREV_T_UNSET;
        region[PREV_TRIGGER_WORD] = 0;
    }

    fn emit_from_state(
        &self,
        _ctx: &Ctx<'_>,
        _inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        emit(0, Value::Real(f64::from_bits(region[X_WORD])));
    }

    fn update_state(&self, ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        let u = read_real(inputs, 0);
        let y_reset_in = read_real(inputs, 1);
        let trigger = read_bool(inputs, 2);
        let prev_trigger = region[PREV_TRIGGER_WORD] != 0;
        let x = f64::from_bits(region[X_WORD]);
        let dt = if region[PREV_T_WORD] == PREV_T_UNSET {
            0.0
        } else {
            ctx.t() - f64::from_bits(region[PREV_T_WORD])
        };

        let next_x = if trigger && !prev_trigger {
            y_reset_in
        } else {
            x + u * dt
        };
        region[X_WORD] = next_x.to_bits();
        region[PREV_T_WORD] = ctx.t().to_bits();
        region[PREV_TRIGGER_WORD] = u64::from(trigger);
    }
}
