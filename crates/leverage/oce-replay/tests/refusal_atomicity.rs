//! A refused frame leaves no trace in the engine (design note, test plan: refusal atomicity).
//!
//! Under `RecordAndContinue`, a run that refuses frames (an engine refusal: +∞ under the default
//! `Refuse` non-finite policy, and an out-of-domain value; a replay refusal: a missing input) or
//! skips them must end with the same engine as a run that never saw those frames: identical state
//! snapshot bytes, the same next accepted sequence, and the same outputs on every frame both
//! executed. Checked after every executed frame by snapshot, not just at the end.

#[path = "support/controller.rs"]
mod controller;
#[path = "support/fixtures.rs"]
mod fixtures;
#[path = "support/scenarios.rs"]
mod scenarios;

use fixtures::{Frames, columnar, columnar_with_missing, load, path_specs};
use oce_api::{Engine, Value};
use oce_replay::{
    ColumnMap, InputMap, MissingPolicy, OnRefusal, Replay, ReplayOptions, TimeBase, Trace,
};

const FIXTURE: &str = "multizone_vav_economizer_modulations_reliefs";

fn frames() -> Frames {
    scenarios::every_status_frames()
}

fn map(engine: &Engine) -> InputMap {
    let specs = path_specs(engine);
    InputMap::resolve(
        &engine.input_definitions().unwrap(),
        &specs,
        &ColumnMap::by_name(&specs),
        &MissingPolicy::Refuse,
        &[("uOutDam_min".to_owned(), MissingPolicy::SkipFrame)],
    )
    .unwrap()
}

fn options() -> ReplayOptions {
    let mut options = ReplayOptions::new(TimeBase::Recorded);
    options.on_refusal = OnRefusal::RecordAndContinue;
    options.trace.snapshot_every = Some(1);
    options
}

fn next_sequence(mut engine: Engine, t: f64, values: &[Value]) -> u64 {
    let slots = engine.frame_slots().unwrap();
    engine.execute_slots(&slots, t, values).unwrap().sequence()
}

#[test]
fn refused_and_skipped_frames_leave_the_engine_as_a_run_that_never_saw_them() {
    let mut all = frames();
    all[4].1[4] = Value::Real(f64::INFINITY); // RefusedEngine: InputNonFinite
    // all[9] is out of domain already: RefusedEngine: InputDomain
    let missing = [(6, 2), (11, 1), (15, 3)]; // RefusedMissing, Skipped, RefusedMissing
    let dropped = [4_usize, 6, 9, 11, 15];

    let engine = load(FIXTURE);
    let map_a = map(&engine);
    let source = columnar_with_missing(&map_a, &all, &missing);
    let (engine_a, trace_a) = Replay::new(engine, map_a, source, options())
        .unwrap()
        .run()
        .unwrap();
    assert_eq!(
        trace_a.status(),
        [
            0, 0, 0, 0, 4, 0, 3, 0, 0, 4, 0, 2, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0
        ]
    );

    let kept: Frames = all
        .iter()
        .enumerate()
        .filter(|(k, _)| !dropped.contains(k))
        .map(|(_, f)| f.clone())
        .collect();
    let engine = load(FIXTURE);
    let map_b = map(&engine);
    let source = columnar(&map_b, &kept);
    let (engine_b, trace_b) = Replay::new(engine, map_b, source, options())
        .unwrap()
        .run()
        .unwrap();
    assert!(trace_b.is_clean());

    assert_eq!(
        engine_a.state_snapshot().unwrap().as_bytes(),
        engine_b.state_snapshot().unwrap().as_bytes(),
        "final state differs"
    );
    let probe = (1_000.0, kept[0].1.clone());
    assert_eq!(
        next_sequence(engine_a, probe.0, &probe.1),
        next_sequence(engine_b, probe.0, &probe.1),
        "the next accepted sequence differs"
    );
    assert_eq!(next_sequence(load(FIXTURE), 0.0, &probe.1), 1);

    // Row by row: every executed row of A matches B's corresponding row, outputs and snapshot.
    let executed: Vec<usize> = (0..all.len()).filter(|k| !dropped.contains(k)).collect();
    compare_rows(&trace_a, &trace_b, &executed);
}

fn compare_rows(a: &Trace, b: &Trace, executed: &[usize]) {
    let snapshot = |t: &Trace, row: usize| {
        t.snapshots()
            .iter()
            .find(|(r, _)| *r as usize == row)
            .map(|(_, s)| s.as_bytes().to_vec())
            .unwrap()
    };
    for (row_b, &row_a) in executed.iter().enumerate() {
        assert_eq!(a.times()[row_a].to_bits(), b.times()[row_b].to_bits());
        for ((_, col_a), (_, col_b)) in a.outputs().iter().zip(b.outputs()) {
            let (oce_replay::TraceColumn::Real(x), oce_replay::TraceColumn::Real(y)) =
                (col_a, col_b)
            else {
                panic!("the reliefs outputs are Real");
            };
            assert_eq!(x[row_a].to_bits(), y[row_b].to_bits(), "row {row_a}");
        }
        assert_eq!(
            snapshot(a, row_a),
            snapshot(b, row_b),
            "state after row {row_a}"
        );
    }
    // The snapshot after a refused row equals the one before it.
    for row in 1..a.rows() {
        if !executed.contains(&row) {
            assert_eq!(
                snapshot(a, row),
                snapshot(a, row - 1),
                "refused row {row} changed state"
            );
        }
    }
}
