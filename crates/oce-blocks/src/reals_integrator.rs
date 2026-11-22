//! Dynamic `CDL.Reals` blocks whose outputs are state-derived. These blocks define the
//! forward-Euler-on-tick convention that the rest of the integrating Reals family reuses: emit the
//! prior state first, then advance the state with `dt = t(k) - t(k-1)` in `update_state`. A reset
//! event is the one exception to emitting the prior state: it is applied at the frame where the
//! trigger rises, so that frame emits the reset value (the HostTick v1 rule for events on a
//! Boolean input, `docs/execution-profile.md`).
//!
//! Non-finite input *signals* are deliberately not trapped here: NaN/Inf propagate through the
//! documented recurrence, and runtime non-finite signal diagnostics remain deferred. Non-finite
//! *parameters* (`k`, `y_start`) are refused engine-wide at load and at `set_param` (issue #362).

use oce_model::{ParamTable, Value};

use crate::dynamics::{PREV_T_UNSET, forward_euler_accumulate, is_first_tick, tick_dt};
use crate::{
    Block, BlockKind, BlockSignature, Ctx, EventInputRole, PortKind, emit_real, read_bool,
    read_real,
};

const X_WORD: usize = 0;
const PREV_T_WORD: usize = 1;
const PREV_TRIGGER_WORD: usize = 2;
const STATE_WORDS: usize = 3;
const U_PORT: usize = 0;
const Y_RESET_IN_PORT: usize = 1;
const TRIGGER_PORT: usize = 2;

/// Whether this frame is a reset event: a frame after the first, with `trigger` true now and
/// false at the previous frame. Frame 0 never is (initialisation leaves the `when` inactive).
fn reset_event(inputs: &[Value], region: &[u64]) -> bool {
    !is_first_tick(region[PREV_T_WORD])
        && read_bool(inputs, TRIGGER_PORT)
        && region[PREV_TRIGGER_WORD] == 0
}

/// `CDL.Reals.IntegratorWithReset` — forward-Euler integrator with a rising-edge reset:
///
/// - Integrates `der(y) = k * u` (Buildings `Reals/IntegratorWithReset.mo`): the accumulator
///   advances by `k * u * dt`, with the gain `k` defaulting to `1` exactly as upstream.
/// - `[S]`, with `u` a loop cut: away from a reset, `y` is emitted from the prior accumulator.
///   `trigger` and `y_reset_in` feed through to `y` (`y <- {y_reset_in, trigger}`), because the
///   frame of a reset emits that frame's `y_reset_in`. Feedback from `y` into either of them is
///   therefore an algebraic loop; a `CDL.Logical.Pre` on the trigger path (or a
///   `CDL.Discrete.UnitDelay` on the `y_reset_in` path) breaks it.
/// - First tick integrates with `dt = 0` because there is no previous model time.
/// - Reset at the event (upstream `when trigger then reinit(y, y_reset_in)`): at a frame `k > 0`
///   where `trigger` is true and was false at frame `k - 1`, frame `k` emits `y_reset_in` and its
///   update stores it **without** an Euler step (the step of frame `k` covers `[t(k-1), t(k)]`,
///   which lies before the event). A held-high trigger does not reset again. The reset value is
///   **not** scaled by `k` (`reinit` assigns the state directly), and it is not trapped: a NaN or
///   infinite `y_reset_in` is emitted and stored as is (NaN canonicalized).
/// - Frame 0 is initialisation: the `when` is inactive there (Modelica 3.6 §8.6), so a `trigger`
///   already true at frame 0 does not reset; it becomes `pre(trigger)` for frame 1, and the first
///   reset is its next false-to-true transition.
/// - OpenModelica 1.25.1 agrees exactly at every frame with `u = 0`, with the trigger true from
///   frame 0 and with it rising at frame 1 (`oce-conformance`'s `open_modelica_event_triggers.rs`,
///   issue #670). With a nonzero `u`, at a uniform frame step with inputs held between frames,
///   this block trails it by exactly one frame of the input held at the last reset or at the
///   start, and equals it at each reset frame; the reset frame's skipped step is what keeps that
///   lag the same after a reset as after the start (`open_modelica_integrator_ramp.rs`). At
///   uneven steps the difference after the reset (or start) at frame `r` is
///   `k * [sum over j of u(j) * (dt(j) - dt(j+1)) - u(r) * dt(r+1)]`, the sum over
///   the frames after `r`.
#[derive(Clone, Copy, Debug)]
pub struct IntegratorWithReset {
    pub(crate) k: f64,
    pub(crate) y_start: f64,
}

impl Default for IntegratorWithReset {
    fn default() -> Self {
        Self {
            k: 1.0,
            y_start: 0.0,
        }
    }
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

    fn feeds_through(&self, in_idx: usize, _out_idx: usize) -> bool {
        in_idx == Y_RESET_IN_PORT || in_idx == TRIGGER_PORT
    }

    fn event_input_role(&self, in_idx: usize) -> Option<EventInputRole> {
        match in_idx {
            TRIGGER_PORT => Some(EventInputRole::Condition),
            Y_RESET_IN_PORT => Some(EventInputRole::ResetValue),
            _ => None,
        }
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
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        let y = if reset_event(inputs, region) {
            read_real(inputs, Y_RESET_IN_PORT)
        } else {
            f64::from_bits(region[X_WORD])
        };
        emit_real(0, y, emit);
    }

    fn update_state(&self, ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        let u = read_real(inputs, U_PORT);
        let trigger = read_bool(inputs, TRIGGER_PORT);
        let x = f64::from_bits(region[X_WORD]);
        let dt = tick_dt(ctx.t(), region[PREV_T_WORD]);

        let next_x = if reset_event(inputs, region) {
            read_real(inputs, Y_RESET_IN_PORT)
        } else {
            forward_euler_accumulate(x, self.k * u, dt)
        };
        region[X_WORD] = next_x.to_bits();
        region[PREV_T_WORD] = ctx.t().to_bits();
        region[PREV_TRIGGER_WORD] = u64::from(trigger);
    }
}
