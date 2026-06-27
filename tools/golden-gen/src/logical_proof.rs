//! CDL.Logical.Proof oracle traces.
//!
//! The reference below re-derives the protected Buildings `Proof.mo` network from its Boolean
//! equations and discrete timing rules. It intentionally does not depend on `oce-blocks`.

use crate::oracle::{Golden, InputSeries, Sample, ValueKind};

const CLASS: &str = "CDL.Logical.Proof";
const SOURCE_COMMIT: &str = "a131864e4c4df22ebcd52bb8da439de0087ac365";
const DEBOUNCE: f64 = 2.0;
const FEEDBACK_DELAY: f64 = 5.0;

fn b(x: bool) -> Sample {
    Sample::Boolean(x)
}

fn input_b(name: &'static str, values: &[bool]) -> InputSeries {
    InputSeries::new(
        name,
        ValueKind::Boolean,
        values.iter().copied().map(b).collect(),
    )
}

#[derive(Clone, Copy, Default)]
struct DelayState {
    timer: f64,
    prev_t: Option<f64>,
    prev_u: bool,
    held: bool,
}

#[derive(Clone, Copy, Default)]
struct TimerState {
    entry_time: f64,
    prev_t: Option<f64>,
    prev_u: bool,
}

#[derive(Clone, Copy, Default)]
struct LatchState {
    held: bool,
    prev_set: bool,
}

#[derive(Clone, Copy)]
struct ProofState {
    measured_true: DelayState,
    measured_false: DelayState,
    setpoint_true: DelayState,
    setpoint_false: DelayState,
    invalid_timer: TimerState,
    prev_valid_equal: bool,
    loc_fal: LatchState,
    loc_tru: LatchState,
}

impl Default for ProofState {
    fn default() -> Self {
        Self {
            measured_true: DelayState::default(),
            measured_false: DelayState::default(),
            setpoint_true: DelayState::default(),
            setpoint_false: DelayState::default(),
            invalid_timer: TimerState::default(),
            prev_valid_equal: false,
            loc_fal: LatchState::default(),
            loc_tru: LatchState::default(),
        }
    }
}

/// Build all `CDL.Logical.Proof` goldens.
pub fn goldens() -> Vec<Golden> {
    let mut out = Vec::new();
    out.extend(proof_scenario(
        "stable_equal_no_alarm",
        DEBOUNCE,
        FEEDBACK_DELAY,
        vec![0.0, 1.0, 2.99, 3.0, 4.0, 5.99, 6.0],
        vec![false, true, true, true, false, false, false],
        vec![false, true, true, true, false, false, false],
        "stable equal inputs with exact debounce boundary probes",
    ));
    out.extend(proof_scenario(
        "mismatch_latches_clear_on_equal",
        DEBOUNCE,
        FEEDBACK_DELAY,
        vec![0.0, 1.0, 2.0, 4.0, 5.0, 6.0, 8.0],
        vec![false, true, true, true, false, false, false],
        vec![false, false, true, true, true, false, false],
        "stable mismatches latch each alarm and stable equality clears on the same tick",
    ));
    out.extend(proof_scenario(
        "debounce_before_feedback",
        DEBOUNCE,
        FEEDBACK_DELAY,
        vec![0.0, 1.0, 1.5, 3.49, 3.5, 8.0],
        vec![false, true, true, true, true, true],
        vec![false, true, false, false, false, false],
        "measured debounce proves locked-false feedback before the setpoint feedback timer lapses",
    ));
    out.extend(proof_scenario(
        "feedback_before_debounce_then_unstable_both",
        DEBOUNCE,
        FEEDBACK_DELAY,
        vec![
            0.0, 1.0, 3.0, 7.5, 8.0, 9.0, 10.5, 12.0, 13.5, 14.49, 14.5, 14.75,
            16.75,
        ],
        vec![
            false, true, true, true, true, true, true, true, true, true, true, true, true,
        ],
        vec![
            false, true, true, false, false, true, false, true, false, false, false, true, true,
        ],
        "feedback delay checks before measured debounce, then invalid measurement latches both alarms",
    ));
    out.extend(proof_scenario(
        "inverted_delay_warning_only",
        5.0,
        2.0,
        vec![0.0, 1.0, 3.0, 5.0, 7.99, 8.0, 9.0, 14.0],
        vec![false, true, true, true, true, true, true, true],
        vec![false, true, false, true, false, false, true, true],
        "feedbackDelay < debounce is warning-only; authored feedbackDelay+debounce=7s is not clamped",
    ));
    out
}

fn proof_scenario(
    scenario: &'static str,
    debounce: f64,
    feedback_delay: f64,
    time: Vec<f64>,
    u_s: Vec<bool>,
    u_m: Vec<bool>,
    desc: &'static str,
) -> Vec<Golden> {
    let (y_loc_fal, y_loc_tru) = proof_reference(&time, &u_s, &u_m, debounce, feedback_delay);
    let inputs = vec![input_b("u_s", &u_s), input_b("u_m", &u_m)];
    let input_desc = format!("debounce={debounce}, feedbackDelay={feedback_delay}; {desc}");
    let rule_desc = "Buildings Proof.mo: measured stability via debounce TrueDelay pair; \
                     invalid timer threshold feedbackDelay+debounce; clear-dominant latch outputs \
                     clear on rising valid-equality edge";

    vec![
        Golden::new(
            CLASS,
            "yLocFal",
            ValueKind::Boolean,
            time.clone(),
            y_loc_fal,
            input_desc.clone(),
            rule_desc,
        )
        .with_scenario(scenario)
        .with_inputs(inputs.clone())
        .with_provenance("source_commit", SOURCE_COMMIT),
        Golden::new(
            CLASS,
            "yLocTru",
            ValueKind::Boolean,
            time,
            y_loc_tru,
            input_desc,
            rule_desc,
        )
        .with_scenario(scenario)
        .with_inputs(inputs)
        .with_provenance("source_commit", SOURCE_COMMIT),
    ]
}

fn proof_reference(
    time: &[f64],
    u_s: &[bool],
    u_m: &[bool],
    debounce: f64,
    feedback_delay: f64,
) -> (Vec<Sample>, Vec<Sample>) {
    assert_eq!(time.len(), u_s.len(), "Proof u_s length");
    assert_eq!(time.len(), u_m.len(), "Proof u_m length");

    let feedback_window = feedback_delay + debounce;
    let mut state = ProofState::default();
    let mut y_loc_fal = Vec::with_capacity(time.len());
    let mut y_loc_tru = Vec::with_capacity(time.len());

    for idx in 0..time.len() {
        let t = time[idx];
        let setpoint = u_s[idx];
        let measured = u_m[idx];
        let not_setpoint = !setpoint;
        let not_measured = !measured;

        let measured_true = true_delay_emit(&state.measured_true, t, measured, debounce);
        let measured_false = true_delay_emit(&state.measured_false, t, not_measured, debounce);
        let valid_measured = measured_true || measured_false;
        let measured_for_equal = if measured_false { measured } else { true };
        let invalid_measured = !valid_measured;

        let setpoint_true = true_delay_emit(&state.setpoint_true, t, setpoint, feedback_window);
        let setpoint_false =
            true_delay_emit(&state.setpoint_false, t, not_setpoint, feedback_window);
        let check_now = setpoint_true || setpoint_false || valid_measured;
        let invalid_timeout = invalid_measured
            && timer_elapsed(&state.invalid_timer, t, invalid_measured) >= feedback_window;

        let status_equal = measured_for_equal == setpoint;
        let both_true = measured_true && setpoint;
        let both_false = not_setpoint && status_equal;
        let loc_fal_set = if invalid_timeout {
            true
        } else {
            check_now && setpoint && !both_true
        };
        let loc_tru_set = if invalid_timeout {
            true
        } else {
            check_now && not_setpoint && !both_false
        };

        let valid_equal = valid_measured && (both_true || status_equal);
        let clear = valid_equal && !state.prev_valid_equal;
        let next_fal = latch_emit(&state.loc_fal, loc_fal_set, clear);
        let next_tru = latch_emit(&state.loc_tru, loc_tru_set, clear);
        y_loc_fal.push(b(next_fal));
        y_loc_tru.push(b(next_tru));

        state.measured_true = true_delay_update(&state.measured_true, t, measured, debounce);
        state.measured_false = true_delay_update(&state.measured_false, t, not_measured, debounce);
        state.setpoint_true = true_delay_update(&state.setpoint_true, t, setpoint, feedback_window);
        state.setpoint_false =
            true_delay_update(&state.setpoint_false, t, not_setpoint, feedback_window);
        state.invalid_timer = timer_update(&state.invalid_timer, t, invalid_measured);
        state.prev_valid_equal = valid_equal;
        state.loc_fal = latch_update(&state.loc_fal, loc_fal_set, clear);
        state.loc_tru = latch_update(&state.loc_tru, loc_tru_set, clear);
    }

    (y_loc_fal, y_loc_tru)
}

fn true_delay_emit(state: &DelayState, t: f64, u: bool, delay_time: f64) -> bool {
    true_delay_output_and_timer(state, t, u, delay_time).0
}

fn true_delay_update(state: &DelayState, t: f64, u: bool, delay_time: f64) -> DelayState {
    let (held, timer) = true_delay_output_and_timer(state, t, u, delay_time);
    DelayState {
        timer,
        prev_t: Some(t),
        prev_u: u,
        held,
    }
}

fn true_delay_output_and_timer(
    state: &DelayState,
    t: f64,
    u: bool,
    delay_time: f64,
) -> (bool, f64) {
    if !u {
        return (false, 0.0);
    }
    let delay = delay_time.max(0.0);
    match state.prev_t {
        None => (true, delay),
        Some(_) if state.held => (true, delay),
        Some(_) if !state.prev_u => (delay <= 0.0, 0.0),
        Some(prev_t) => {
            let timer = state.timer + (t - prev_t);
            (timer >= delay, timer)
        }
    }
}

fn timer_elapsed(state: &TimerState, t: f64, u: bool) -> f64 {
    if !u {
        0.0
    } else if state.prev_t.is_none() || !state.prev_u {
        0.0
    } else {
        t - state.entry_time
    }
}

fn timer_update(state: &TimerState, t: f64, u: bool) -> TimerState {
    TimerState {
        entry_time: if u && (state.prev_t.is_none() || !state.prev_u) {
            t
        } else {
            state.entry_time
        },
        prev_t: Some(t),
        prev_u: u,
    }
}

fn latch_emit(state: &LatchState, set: bool, clear: bool) -> bool {
    !clear && ((set && !state.prev_set) || state.held)
}

fn latch_update(state: &LatchState, set: bool, clear: bool) -> LatchState {
    LatchState {
        held: latch_emit(state, set, clear),
        prev_set: set,
    }
}
