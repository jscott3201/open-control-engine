//! Scenario tests for `CDL.Logical.Proof`.

use oce_model::{ParamTable, Value};

use super::{Block, Ctx, NoopDiagnostics, Proof};

fn init_region(block: &dyn Block) -> Vec<u64> {
    let mut region = vec![0u64; block.state_len()];
    block.init_state(&mut region, &ParamTable::default());
    region
}

fn tick_at(block: &dyn Block, region: &mut [u64], t: f64, u_s: bool, u_m: bool) -> (bool, bool) {
    let diag = NoopDiagnostics;
    let cx = Ctx::new(t, &diag);
    let inputs = [Value::Boolean(u_s), Value::Boolean(u_m)];
    let mut out = Vec::new();
    block.emit_from_state(&cx, &inputs, region, &mut |idx, val| {
        assert_eq!(
            idx,
            out.len(),
            "Proof outputs must be emitted in port order"
        );
        out.push(val);
    });
    block.update_state(&cx, &inputs, region);
    bool_pair(&out)
}

fn emit_at(block: &dyn Block, region: &[u64], t: f64, u_s: bool, u_m: bool) -> (bool, bool) {
    let diag = NoopDiagnostics;
    let cx = Ctx::new(t, &diag);
    let inputs = [Value::Boolean(u_s), Value::Boolean(u_m)];
    let mut out = Vec::new();
    block.emit_from_state(&cx, &inputs, region, &mut |idx, val| {
        assert_eq!(
            idx,
            out.len(),
            "Proof outputs must be emitted in port order"
        );
        out.push(val);
    });
    bool_pair(&out)
}

fn run(block: &dyn Block, steps: &[(f64, bool, bool)]) -> (Vec<(bool, bool)>, Vec<u64>) {
    let mut region = init_region(block);
    let trace = steps
        .iter()
        .map(|&(t, u_s, u_m)| tick_at(block, &mut region, t, u_s, u_m))
        .collect();
    (trace, region)
}

fn bool_pair(values: &[Value]) -> (bool, bool) {
    assert_eq!(values.len(), 2);
    let Value::Boolean(y_loc_fal) = values[0] else {
        panic!("expected yLocFal Boolean, got {:?}", values[0]);
    };
    let Value::Boolean(y_loc_tru) = values[1] else {
        panic!("expected yLocTru Boolean, got {:?}", values[1]);
    };
    (y_loc_fal, y_loc_tru)
}

#[test]
fn stable_equality_outputs_remain_clear() {
    let proof = Proof {
        debounce: 2.0,
        feedback_delay: 5.0,
    };
    let (trace, _) = run(
        &proof,
        &[
            (0.0, false, false),
            (1.0, true, true),
            (2.99, true, true),
            (3.0, true, true),
            (4.0, false, false),
            (5.99, false, false),
            (6.0, false, false),
        ],
    );
    assert_eq!(
        trace,
        vec![(false, false); 7],
        "stable equality must never latch local status alarms"
    );
}

#[test]
fn stable_false_feedback_latches_false_alarm() {
    let proof = Proof {
        debounce: 2.0,
        feedback_delay: 5.0,
    };
    let (trace, _) = run(
        &proof,
        &[
            (0.0, true, false),
            (1.0, true, false),
            (2.0, true, true),
            (3.99, true, true),
            (4.0, true, true),
        ],
    );
    assert_eq!(
        trace,
        vec![
            (true, false),
            (true, false),
            (true, false),
            (true, false),
            (false, false),
        ],
        "stable false feedback must latch yLocFal until stable equality clears it"
    );
}

#[test]
fn stable_true_feedback_latches_true_alarm() {
    let proof = Proof {
        debounce: 2.0,
        feedback_delay: 5.0,
    };
    let (trace, _) = run(
        &proof,
        &[
            (0.0, false, true),
            (1.0, false, true),
            (2.0, false, false),
            (3.99, false, false),
            (4.0, false, false),
        ],
    );
    assert_eq!(
        trace,
        vec![
            (false, true),
            (false, true),
            (false, true),
            (false, true),
            (false, false),
        ],
        "stable true feedback must latch yLocTru until stable equality clears it"
    );
}

#[test]
fn unstable_measurement_latches_both_alarms_after_timeout() {
    let proof = Proof {
        debounce: 2.0,
        feedback_delay: 1.0,
    };
    let (trace, _) = run(
        &proof,
        &[
            (0.0, true, true),
            (1.0, true, false),
            (2.5, true, true),
            (4.0, true, true),
            (5.0, true, true),
        ],
    );
    assert_eq!(
        trace,
        vec![
            (false, false),
            (true, false),
            (true, false),
            (true, true),
            (false, false),
        ],
        "setpoint-delay checks can latch one side before invalid timeout latches both"
    );
}

#[test]
fn measurement_stabilizes_before_timeout_avoids_opposite_alarm() {
    let proof = Proof {
        debounce: 2.0,
        feedback_delay: 5.0,
    };
    let (trace, _) = run(
        &proof,
        &[
            (0.0, false, false),
            (1.0, true, true),
            (1.5, true, false),
            (3.49, true, false),
            (3.5, true, false),
            (8.0, true, false),
        ],
    );
    assert_eq!(
        trace,
        vec![
            (false, false),
            (false, false),
            (false, false),
            (false, false),
            (true, false),
            (true, false),
        ],
        "measured debounce can prove a one-sided mismatch before the invalid-input timeout"
    );
}

#[test]
fn stable_equality_edge_clears_prior_alarm() {
    let proof = Proof {
        debounce: 2.0,
        feedback_delay: 5.0,
    };
    let (trace, _) = run(
        &proof,
        &[
            (0.0, false, false),
            (1.0, true, false),
            (2.0, true, true),
            (4.0, true, true),
            (5.0, false, true),
            (6.0, false, false),
            (8.0, false, false),
        ],
    );
    assert_eq!(
        trace,
        vec![
            (false, false),
            (true, false),
            (true, false),
            (false, false),
            (false, true),
            (false, true),
            (false, false),
        ],
        "rising valid-equality edge must clear both held alarms on the same tick"
    );
}

#[test]
fn zero_durations_clear_on_same_tick_boundary() {
    let proof = Proof {
        debounce: 0.0,
        feedback_delay: 0.0,
    };
    let (trace, _) = run(
        &proof,
        &[
            (0.0, true, false),
            (0.0, true, true),
            (0.0, false, true),
            (0.0, false, false),
        ],
    );
    assert_eq!(
        trace,
        vec![(true, false), (false, false), (false, true), (false, false)],
        "zero debounce and feedbackDelay use the same >= boundary as source timing blocks"
    );
}

#[test]
fn state_region_is_deterministic() {
    let proof = Proof {
        debounce: 2.0,
        feedback_delay: 5.0,
    };
    let steps = [
        (0.0, false, false),
        (1.0, true, false),
        (2.0, true, true),
        (4.0, true, true),
        (5.0, false, true),
        (6.0, false, false),
        (8.0, false, false),
    ];
    let (trace_a, state_a) = run(&proof, &steps);
    let (trace_b, state_b) = run(&proof, &steps);
    assert_eq!(trace_a, trace_b);
    assert_eq!(state_a, state_b);
    assert_eq!(state_a.len(), 24);
}

#[test]
fn feedthrough_surface_tracks_current_inputs() {
    let proof = Proof {
        debounce: 0.0,
        feedback_delay: 0.0,
    };
    assert!(proof.feeds_through(0, 0));
    assert!(proof.feeds_through(0, 1));
    assert!(proof.feeds_through(1, 0));
    assert!(proof.feeds_through(1, 1));

    let mut region = init_region(&proof);
    assert_eq!(
        tick_at(&proof, &mut region, 0.0, true, false),
        (true, false)
    );
    assert_eq!(
        emit_at(&proof, &region, 0.0, true, true),
        (false, false),
        "same prior state must clear on current equal inputs without waiting for update_state"
    );
}
