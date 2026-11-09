//! Chunk sizes never change the trace (design note, resumable cursor; decision 6).
//!
//! The same run advanced in chunks of 1, 7 and the whole run (and `run`, which uses 4,096) gives
//! byte-identical `Trace::canonical_bytes()`, for a clean run (the controller's reference day), a
//! run with every status present (`every_status`, with state snapshots every 5 rows and at named
//! rows), a grid run with holds, and a run that stops on a refusal. After `Done` or `Stopped`,
//! `advance` keeps returning the same progress without reading further.

#[path = "support/controller.rs"]
mod controller;
#[path = "support/fixtures.rs"]
mod fixtures;
#[path = "support/scenarios.rs"]
mod scenarios;

use fixtures::load;
use oce_replay::{
    FrameSource, MissingPolicy, OnRefusal, Progress, RefusalCause, Replay, ReplayOptions, TimeBase,
};

fn in_chunks<S: FrameSource>(mut replay: Replay<S>, chunk: usize) -> (Vec<u8>, Progress) {
    let mut calls = 0_usize;
    let last = loop {
        calls += 1;
        match replay.advance(chunk).unwrap() {
            Progress::More { done } => {
                assert_eq!(done as usize, replay.trace().rows());
                assert!(done as usize <= calls.saturating_mul(chunk));
            }
            terminal => break terminal,
        }
    };
    // Terminal progress is sticky.
    assert_eq!(replay.advance(chunk).unwrap(), last);
    assert_eq!(replay.advance(1).unwrap(), last);
    (replay.finish().1.canonical_bytes(), last)
}

fn assert_independent<S: FrameSource>(make: impl Fn() -> Replay<S>, rows: usize) -> Progress {
    let (whole, last) = in_chunks(make(), usize::MAX);
    for chunk in [1, 7, rows, rows + 1] {
        let (bytes, progress) = in_chunks(make(), chunk);
        assert_eq!(progress, last, "chunk {chunk}");
        assert_eq!(bytes, whole, "chunk {chunk}: trace bytes differ");
    }
    let (_, trace) = make().run().unwrap();
    assert_eq!(trace.canonical_bytes(), whole, "run");
    last
}

fn controller_day() -> Replay<oce_replay::ColumnarSource> {
    let reference = controller::reference();
    let engine = load(controller::FIXTURE);
    let map = controller::map(&engine, &reference, &MissingPolicy::Refuse);
    let source = controller::columnar(&map, &reference);
    Replay::new(engine, map, source, ReplayOptions::new(TimeBase::Recorded)).unwrap()
}

#[test]
fn a_clean_run_is_byte_identical_for_chunks_of_one_seven_and_all() {
    let last = assert_independent(controller_day, 1_441);
    assert_eq!(last, Progress::Done { frames: 1_441 });
}

#[test]
fn a_run_with_every_status_and_snapshots_is_byte_identical_for_every_chunk_size() {
    let make = || {
        scenarios::every_status(|o| {
            o.trace.snapshot_every = Some(5);
            o.trace.snapshot_at = vec![0, 6, 9, 23];
        })
    };
    let last = assert_independent(make, 24);
    assert_eq!(last, Progress::Done { frames: 24 });
    let (_, trace) = make().run().unwrap();
    let rows: Vec<u64> = trace.snapshots().iter().map(|(r, _)| *r).collect();
    assert_eq!(rows, [0, 4, 6, 9, 14, 19, 23]);
    let mut seen = trace.status().to_vec();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen, [0, 1, 2, 3, 4]);
}

#[test]
fn a_grid_run_with_holds_is_byte_identical_for_every_chunk_size() {
    let last = assert_independent(scenarios::controller_grid_hold, 1_441);
    assert_eq!(last, Progress::Done { frames: 1_441 });
}

#[test]
fn a_stopped_run_is_byte_identical_for_every_chunk_size() {
    let make = || scenarios::every_status(|o| o.on_refusal = OnRefusal::Stop);
    let last = assert_independent(make, 24);
    assert_eq!(
        last,
        Progress::Stopped {
            frame: 6,
            cause: RefusalCause::MissingInput {
                slot: 2,
                path: "http://example.org#g36.source.multizone_vav_economizer_modulations_reliefs.uRetDam_max"
                    .to_owned(),
            },
        }
    );
    let (_, trace) = make().run().unwrap();
    assert_eq!(trace.rows(), 7, "the stopping row is recorded");
}

#[test]
fn advance_of_zero_frames_reads_nothing() {
    let mut replay = controller_day();
    assert_eq!(replay.advance(0).unwrap(), Progress::More { done: 0 });
    assert_eq!(replay.trace().rows(), 0);
    assert_eq!(replay.advance(3).unwrap(), Progress::More { done: 3 });
}

#[test]
fn reserving_room_mid_run_does_not_change_the_trace() {
    let (_, plain) = scenarios::every_status(|_| {}).run().unwrap();
    let mut replay = scenarios::every_status(|_| {});
    replay.advance(5).unwrap();
    replay.reserve(3).unwrap();
    replay.advance(2).unwrap();
    replay.reserve(1_000).unwrap();
    replay.reserve(0).unwrap();
    let (_, trace) = replay.run().unwrap();
    assert_eq!(trace.canonical_bytes(), plain.canonical_bytes());
}
