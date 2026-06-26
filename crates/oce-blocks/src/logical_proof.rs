//! `CDL.Logical.Proof` status proof block.
//!
//! The Buildings source is a protected network of `TrueDelay`, `Timer`, `Latch`, `Edge`, Boolean
//! algebra, `BooleanToInteger`, and `Integers.Equal`. This native block keeps the same composite
//! same-tick behavior in one fixed state region: every internal output is emitted from the prior
//! internal state, then every internal state word is advanced together.

use oce_model::{ParamTable, Value, determinism::det_max};

use crate::dynamics::{PREV_T_UNSET, is_first_tick, tick_dt};
use crate::{Block, BlockKind, BlockSignature, Ctx, PortKind, read_bool};

fn bool_word(b: bool) -> u64 {
    u64::from(b)
}

fn word_bool(w: u64) -> bool {
    w != 0
}

fn positive_duration(x: f64) -> f64 {
    det_max(x, 0.0)
}

const TRUE_DELAY_LEN: usize = 4;
const TIMER_LEN: usize = 3;
const EDGE_LEN: usize = 1;
const LATCH_LEN: usize = 2;

const TD_TIMER_WORD: usize = 0;
const TD_PREV_T_WORD: usize = 1;
const TD_PREV_U_WORD: usize = 2;
const TD_HELD_WORD: usize = 3;

const TIMER_ENTRY_WORD: usize = 0;
const TIMER_PREV_T_WORD: usize = 1;
const TIMER_PREV_U_WORD: usize = 2;

const LATCH_HELD_WORD: usize = 0;
const LATCH_PREV_SET_WORD: usize = 1;

const VAL_TRU: usize = 0;
const TRU_DEL: usize = VAL_TRU + TRUE_DELAY_LEN;
const DEL_CHE_TRU: usize = TRU_DEL + TRUE_DELAY_LEN;
const DEL_CHE_FAL: usize = DEL_CHE_TRU + TRUE_DELAY_LEN;
const PAS_DEB: usize = DEL_CHE_FAL + TRUE_DELAY_LEN;
const SAM_INP_EDG: usize = PAS_DEB + TIMER_LEN;
const LOC_FAL: usize = SAM_INP_EDG + EDGE_LEN;
const LOC_TRU: usize = LOC_FAL + LATCH_LEN;
const STATE_LEN: usize = LOC_TRU + LATCH_LEN;

/// `CDL.Logical.Proof` — latches measured-status proof failures for a commanded status.
///
/// Inputs are `u_s` (commanded status setpoint) and `u_m` (measured status). Outputs are
/// `yLocFal` when the measured status is locked false while the setpoint is true, and `yLocTru`
/// when it is locked true while the setpoint is false. If the measured input remains unstable for
/// `feedbackDelay + debounce`, both outputs latch true. Stable equality clears both latches on the
/// same tick that equality becomes valid.
///
/// The upstream `feedbackDelay >= debounce` assertion is warning-only load validation; the runtime
/// uses the authored parameters without clamping.
#[derive(Clone, Copy, Debug, Default)]
pub struct Proof {
    /// Measured input debounce time, seconds.
    pub(crate) debounce: f64,
    /// Setpoint feedback delay, seconds.
    pub(crate) feedback_delay: f64,
}

#[derive(Clone, Copy)]
struct ProofSignals {
    y_loc_fal: bool,
    y_loc_tru: bool,
    u_m: bool,
    not_u_m: bool,
    u_s: bool,
    not_u_s: bool,
    val_equ: bool,
    sam_inp_edg: bool,
    che_sta_mea: bool,
    che_sta_mea_tru: bool,
    not_val_inp: bool,
}

fn init_true_delay(region: &mut [u64]) {
    region[TD_TIMER_WORD] = 0.0f64.to_bits();
    region[TD_PREV_T_WORD] = PREV_T_UNSET;
    region[TD_PREV_U_WORD] = 0;
    region[TD_HELD_WORD] = 0;
}

fn true_delay_output_and_timer(
    ctx: &Ctx<'_>,
    u: bool,
    delay_time: f64,
    region: &[u64],
) -> (bool, f64) {
    if !u {
        return (false, 0.0);
    }
    let first = is_first_tick(region[TD_PREV_T_WORD]);
    let held = word_bool(region[TD_HELD_WORD]);
    let prev_u = word_bool(region[TD_PREV_U_WORD]);
    let delay = positive_duration(delay_time);
    if first || held {
        (true, delay)
    } else if !prev_u {
        (delay <= 0.0, 0.0)
    } else {
        let timer =
            f64::from_bits(region[TD_TIMER_WORD]) + tick_dt(ctx.t(), region[TD_PREV_T_WORD]);
        (timer >= delay, timer)
    }
}

fn update_true_delay(ctx: &Ctx<'_>, u: bool, delay_time: f64, region: &mut [u64]) {
    let (y, timer) = true_delay_output_and_timer(ctx, u, delay_time, region);
    region[TD_TIMER_WORD] = timer.to_bits();
    region[TD_PREV_T_WORD] = ctx.t().to_bits();
    region[TD_PREV_U_WORD] = bool_word(u);
    region[TD_HELD_WORD] = bool_word(y);
}

fn timer_y_now(ctx: &Ctx<'_>, u: bool, region: &[u64]) -> f64 {
    if !u {
        return 0.0;
    }
    if is_first_tick(region[TIMER_PREV_T_WORD]) || !word_bool(region[TIMER_PREV_U_WORD]) {
        0.0
    } else {
        ctx.t() - f64::from_bits(region[TIMER_ENTRY_WORD])
    }
}

fn timer_passed(ctx: &Ctx<'_>, u: bool, threshold: f64, region: &[u64]) -> bool {
    u && timer_y_now(ctx, u, region) >= threshold
}

fn update_timer(ctx: &Ctx<'_>, u: bool, region: &mut [u64]) {
    if u && (is_first_tick(region[TIMER_PREV_T_WORD]) || !word_bool(region[TIMER_PREV_U_WORD])) {
        region[TIMER_ENTRY_WORD] = ctx.t().to_bits();
    }
    region[TIMER_PREV_T_WORD] = ctx.t().to_bits();
    region[TIMER_PREV_U_WORD] = bool_word(u);
}

fn edge_output(u: bool, region: &[u64]) -> bool {
    u && !word_bool(region[0])
}

fn update_edge(u: bool, region: &mut [u64]) {
    region[0] = bool_word(u);
}

fn latch_output(set: bool, clear: bool, region: &[u64]) -> bool {
    let held = word_bool(region[LATCH_HELD_WORD]);
    let prev_set = word_bool(region[LATCH_PREV_SET_WORD]);
    !clear && ((set && !prev_set) || held)
}

fn update_latch(set: bool, clear: bool, region: &mut [u64]) {
    region[LATCH_HELD_WORD] = bool_word(latch_output(set, clear, region));
    region[LATCH_PREV_SET_WORD] = bool_word(set);
}

impl Proof {
    fn feedback_window(self) -> f64 {
        self.feedback_delay + self.debounce
    }

    fn signals(self, ctx: &Ctx<'_>, inputs: &[Value], region: &[u64]) -> ProofSignals {
        let u_s = read_bool(inputs, 0);
        let u_m = read_bool(inputs, 1);
        let not_u_s = !u_s;
        let not_u_m = !u_m;
        let feedback_window = self.feedback_window();

        let val_tru = true_delay_output_and_timer(
            ctx,
            u_m,
            self.debounce,
            &region[VAL_TRU..VAL_TRU + TRUE_DELAY_LEN],
        )
        .0;
        let tru_del = true_delay_output_and_timer(
            ctx,
            not_u_m,
            self.debounce,
            &region[TRU_DEL..TRU_DEL + TRUE_DELAY_LEN],
        )
        .0;
        let val_inp = val_tru || tru_del;
        let val_fal = if tru_del { u_m } else { true };
        let not_val_inp = !val_inp;

        let del_che_tru = true_delay_output_and_timer(
            ctx,
            u_s,
            feedback_window,
            &region[DEL_CHE_TRU..DEL_CHE_TRU + TRUE_DELAY_LEN],
        )
        .0;
        let del_che_fal = true_delay_output_and_timer(
            ctx,
            not_u_s,
            feedback_window,
            &region[DEL_CHE_FAL..DEL_CHE_FAL + TRUE_DELAY_LEN],
        )
        .0;
        let pas_del = del_che_tru || del_che_fal;
        let che_dif = pas_del || val_inp;

        let inv_inp = not_val_inp
            && timer_passed(
                ctx,
                not_val_inp,
                feedback_window,
                &region[PAS_DEB..PAS_DEB + TIMER_LEN],
            );

        let equ_sta = val_fal == u_s;
        let bot_tru = val_tru && u_s;
        let tru_fal = !bot_tru && u_s;
        let bot_fal = not_u_s && equ_sta;
        let fal_tru = !bot_fal && not_u_s;

        let che_dif_fal = if che_dif { tru_fal } else { false };
        let che_dif_tru = if che_dif { fal_tru } else { false };
        let che_sta_mea = if inv_inp { true } else { che_dif_fal };
        let che_sta_mea_tru = if inv_inp { true } else { che_dif_tru };

        let equ_inp = bot_tru || equ_sta;
        let val_equ = equ_inp && val_inp;
        let same_input_edge = edge_output(val_equ, &region[SAM_INP_EDG..SAM_INP_EDG + EDGE_LEN]);

        ProofSignals {
            y_loc_fal: latch_output(
                che_sta_mea,
                same_input_edge,
                &region[LOC_FAL..LOC_FAL + LATCH_LEN],
            ),
            y_loc_tru: latch_output(
                che_sta_mea_tru,
                same_input_edge,
                &region[LOC_TRU..LOC_TRU + LATCH_LEN],
            ),
            u_m,
            not_u_m,
            u_s,
            not_u_s,
            val_equ,
            sam_inp_edg: same_input_edge,
            che_sta_mea,
            che_sta_mea_tru,
            not_val_inp,
        }
    }
}

impl Block for Proof {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Logical.Proof",
            inputs: &[PortKind::Boolean, PortKind::Boolean],
            outputs: &[PortKind::Boolean, PortKind::Boolean],
            stateful: true,
        };
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Stateful
    }

    fn feeds_through(&self, in_idx: usize, out_idx: usize) -> bool {
        in_idx < 2 && out_idx < 2
    }

    fn state_len(&self) -> usize {
        STATE_LEN
    }

    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        init_true_delay(&mut region[VAL_TRU..VAL_TRU + TRUE_DELAY_LEN]);
        init_true_delay(&mut region[TRU_DEL..TRU_DEL + TRUE_DELAY_LEN]);
        init_true_delay(&mut region[DEL_CHE_TRU..DEL_CHE_TRU + TRUE_DELAY_LEN]);
        init_true_delay(&mut region[DEL_CHE_FAL..DEL_CHE_FAL + TRUE_DELAY_LEN]);
        region[PAS_DEB + TIMER_ENTRY_WORD] = 0.0f64.to_bits();
        region[PAS_DEB + TIMER_PREV_T_WORD] = PREV_T_UNSET;
        region[PAS_DEB + TIMER_PREV_U_WORD] = 0;
        region[SAM_INP_EDG] = 0;
        region[LOC_FAL + LATCH_HELD_WORD] = 0;
        region[LOC_FAL + LATCH_PREV_SET_WORD] = 0;
        region[LOC_TRU + LATCH_HELD_WORD] = 0;
        region[LOC_TRU + LATCH_PREV_SET_WORD] = 0;
    }

    fn emit_from_state(
        &self,
        ctx: &Ctx<'_>,
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        let signals = self.signals(ctx, inputs, region);
        emit(0, Value::Boolean(signals.y_loc_fal));
        emit(1, Value::Boolean(signals.y_loc_tru));
    }

    fn update_state(&self, ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        let signals = self.signals(ctx, inputs, region);
        let feedback_window = self.feedback_window();

        update_true_delay(
            ctx,
            signals.u_m,
            self.debounce,
            &mut region[VAL_TRU..VAL_TRU + TRUE_DELAY_LEN],
        );
        update_true_delay(
            ctx,
            signals.not_u_m,
            self.debounce,
            &mut region[TRU_DEL..TRU_DEL + TRUE_DELAY_LEN],
        );
        update_true_delay(
            ctx,
            signals.u_s,
            feedback_window,
            &mut region[DEL_CHE_TRU..DEL_CHE_TRU + TRUE_DELAY_LEN],
        );
        update_true_delay(
            ctx,
            signals.not_u_s,
            feedback_window,
            &mut region[DEL_CHE_FAL..DEL_CHE_FAL + TRUE_DELAY_LEN],
        );
        update_timer(
            ctx,
            signals.not_val_inp,
            &mut region[PAS_DEB..PAS_DEB + TIMER_LEN],
        );
        update_edge(
            signals.val_equ,
            &mut region[SAM_INP_EDG..SAM_INP_EDG + EDGE_LEN],
        );
        update_latch(
            signals.che_sta_mea,
            signals.sam_inp_edg,
            &mut region[LOC_FAL..LOC_FAL + LATCH_LEN],
        );
        update_latch(
            signals.che_sta_mea_tru,
            signals.sam_inp_edg,
            &mut region[LOC_TRU..LOC_TRU + LATCH_LEN],
        );
    }
}
