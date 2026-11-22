//! Tests for the `CDL.Integers` edge and counter blocks. `OnCounter` traces are hand-derived
//! from `OnCounter.mo` under the event-at-the-frame rule (`docs/design/reset-timing.md` §2): the
//! frame where `trigger` or `reset` rises emits the new count, and frame 0 has no event.

use oce_model::{ParamTable, Value};

use super::{Block, Ctx, EventInputRole, IntegerChange, NoopDiagnostics, OnCounter};

fn init_region(block: &dyn Block) -> Vec<u64> {
    let mut region = vec![0u64; block.state_len()];
    block.init_state(&mut region, &ParamTable::default());
    region
}

fn tick_at(block: &dyn Block, region: &mut [u64], t: f64, inputs: Vec<Value>) -> Vec<Value> {
    let diag = NoopDiagnostics;
    let cx = Ctx::new(t, &diag);
    let mut out = Vec::new();
    block.emit_from_state(&cx, &inputs, region, &mut |idx, val| {
        assert_eq!(idx, out.len(), "outputs must be emitted in port order");
        out.push(val);
    });
    block.update_state(&cx, &inputs, region);
    out
}

fn run(block: &dyn Block, steps: &[(f64, Vec<Value>)]) -> (Vec<Vec<Value>>, Vec<u64>) {
    let mut region = init_region(block);
    let trace = steps
        .iter()
        .map(|(t, inputs)| tick_at(block, &mut region, *t, inputs.clone()))
        .collect();
    (trace, region)
}

fn assert_bool(value: &Value, want: bool) {
    assert!(
        value.bit_eq(&Value::Boolean(want)),
        "got {value:?}, want {want}"
    );
}

fn assert_int(value: &Value, want: i64) {
    assert!(
        value.bit_eq(&Value::Integer(want)),
        "got {value:?}, want {want}"
    );
}

fn bool_inputs(trigger: bool, reset: bool) -> Vec<Value> {
    vec![Value::Boolean(trigger), Value::Boolean(reset)]
}

fn assert_int_trace(trace: &[Vec<Value>], expected: &[i64]) {
    assert_eq!(trace.len(), expected.len());
    for (got, want) in trace.iter().zip(expected) {
        assert_int(&got[0], *want);
    }
}

#[test]
fn integer_edge_and_counter_goldens() {
    let counter = OnCounter { y_start: 5 };
    let (trace, _) = run(
        &counter,
        &[
            (0.0, bool_inputs(false, false)),
            (1.0, bool_inputs(true, false)),
            (2.0, bool_inputs(true, false)),
            (3.0, bool_inputs(false, false)),
            (4.0, bool_inputs(true, true)),
            (5.0, bool_inputs(false, true)),
            (6.0, bool_inputs(false, false)),
        ],
    );
    // t=1: trigger rises, emits 6. t=4: trigger and reset rise together; reset has priority and
    // that frame emits y_start = 5.
    assert_int_trace(&trace, &[5, 6, 6, 6, 5, 5, 5]);

    let change = IntegerChange { pre_u_start: 10 };
    let (trace, _) = run(
        &change,
        &[
            (0.0, vec![Value::Integer(10)]),
            (0.0, vec![Value::Integer(12)]),
            (0.0, vec![Value::Integer(9)]),
            (0.0, vec![Value::Integer(9)]),
        ],
    );
    let expected = [
        (false, false, false),
        (true, true, false),
        (true, false, true),
        (false, false, false),
    ];
    for (got, (changed, up, down)) in trace.iter().zip(expected) {
        assert_bool(&got[0], changed);
        assert_bool(&got[1], up);
        assert_bool(&got[2], down);
    }
}

#[test]
fn held_reset_level_suppresses_trigger_rising_edges() {
    let counter = OnCounter { y_start: 3 };
    let (trace, _) = run(
        &counter,
        &[
            (0.0, bool_inputs(false, true)),
            (1.0, bool_inputs(false, true)),
            (2.0, bool_inputs(true, true)),
            (3.0, bool_inputs(true, true)),
            (4.0, bool_inputs(false, true)),
            (5.0, bool_inputs(true, true)),
            (6.0, bool_inputs(true, true)),
        ],
    );
    assert_int_trace(&trace, &[3, 3, 3, 3, 3, 3, 3]);
}

#[test]
fn trigger_true_at_frame_zero_is_initialisation_not_a_count() {
    let counter = OnCounter { y_start: 2 };
    let (trace, _) = run(
        &counter,
        &[
            (0.0, bool_inputs(true, false)),
            (1.0, bool_inputs(true, false)),
            (2.0, bool_inputs(false, false)),
            (3.0, bool_inputs(true, false)),
            (4.0, bool_inputs(true, false)),
        ],
    );
    // Frame 0 captures the high trigger as history; held at t=1 is no event; the rise at t=3
    // counts at t=3.
    assert_int_trace(&trace, &[2, 2, 2, 3, 3]);
}

#[test]
fn simultaneous_trigger_and_reset_rise_stores_start_value() {
    let counter = OnCounter { y_start: 4 };
    let (trace, _) = run(
        &counter,
        &[
            (0.0, bool_inputs(false, false)),
            (1.0, bool_inputs(true, false)),
            (2.0, bool_inputs(false, false)),
            (3.0, bool_inputs(true, true)),
            (4.0, bool_inputs(true, true)),
        ],
    );
    // The trigger rise at t=1 emits 5 at t=1; the simultaneous rise at t=3 emits y_start = 4.
    assert_int_trace(&trace, &[4, 5, 5, 4, 4]);
}

#[test]
fn on_counter_inputs_feed_through_as_event_inputs() {
    let counter = OnCounter { y_start: 0 };
    for input in 0..2 {
        assert!(
            counter.feeds_through(input, 0),
            "input {input} must feed through"
        );
        assert_eq!(
            counter.event_input_role(input),
            Some(EventInputRole::Condition)
        );
    }
    assert_eq!(counter.event_input_role(2), None);
}

#[test]
fn counter_events_on_consecutive_frames_count_at_each_rise() {
    // F,T,F,T,F,T: rises at frames 1, 3 and 5 count at those frames.
    let counter = OnCounter { y_start: 0 };
    let steps: Vec<_> = [false, true, false, true, false, true]
        .iter()
        .enumerate()
        .map(|(i, &trigger)| (i as f64, bool_inputs(trigger, false)))
        .collect();
    let (trace, region) = run(&counter, &steps);
    assert_int_trace(&trace, &[0, 1, 1, 2, 2, 3]);
    assert_eq!(region[0], 3);
}

#[test]
fn reset_rising_alone_restores_the_start_value_at_its_frame() {
    // Two counts, then reset rises at t=3 (trigger low): that frame emits y_start = 7. A reset
    // high from frame 0 is no event (held_reset_level_suppresses_trigger_rising_edges).
    let counter = OnCounter { y_start: 7 };
    let (trace, _) = run(
        &counter,
        &[
            (0.0, bool_inputs(false, false)),
            (1.0, bool_inputs(true, false)),
            (2.0, bool_inputs(false, false)),
            (2.0, bool_inputs(true, false)),
            (3.0, bool_inputs(false, true)),
            (4.0, bool_inputs(false, false)),
        ],
    );
    // The repeated timestamp t=2 changes nothing: the counter has no time dependence.
    assert_int_trace(&trace, &[7, 8, 8, 9, 7, 7]);
}

#[test]
fn count_overflow_wraps_at_the_event_frame() {
    let counter = OnCounter { y_start: i64::MAX };
    let (trace, _) = run(
        &counter,
        &[
            (0.0, bool_inputs(false, false)),
            (1.0, bool_inputs(true, false)),
        ],
    );
    assert_int_trace(&trace, &[i64::MAX, i64::MIN]);
}

#[test]
fn counter_continues_identically_from_a_copied_state() {
    let counter = OnCounter { y_start: 1 };
    let steps: Vec<_> = [
        (true, false),
        (false, false),
        (true, false),
        (true, true),
        (false, false),
        (true, false),
    ]
    .iter()
    .enumerate()
    .map(|(i, &(trigger, reset))| (i as f64, bool_inputs(trigger, reset)))
    .collect();
    let (whole, _) = run(&counter, &steps);
    for split in 1..steps.len() {
        let (head, mut region) = run(&counter, &steps[..split]);
        let tail: Vec<_> = steps[split..]
            .iter()
            .map(|(t, inputs)| tick_at(&counter, &mut region, *t, inputs.clone()))
            .collect();
        let joined: Vec<_> = head.into_iter().chain(tail).collect();
        for (a, b) in joined.iter().zip(&whole) {
            assert!(a[0].bit_eq(&b[0]), "split {split}: {a:?} vs {b:?}");
        }
    }
}
