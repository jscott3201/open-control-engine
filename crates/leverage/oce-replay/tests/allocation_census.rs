//! Allocation census of the replay loop (design note, "Size budget per frame"; issue #445).
//!
//! Once the map exists and the trace columns have room, a frame allocates nothing: the source
//! lends its row, the policy stage fills a reused slot-ordered buffer, `Engine::execute_slots`
//! commits it, and its outputs are appended to preallocated columns. The documented exceptions:
//! the first accepted frame creates the output columns (decision 16), a frame whose engine emits
//! warnings appends owned diagnostic rows (and the engine may grow its reused warning records),
//! and a refused frame appends its refusal row.
//!
//! Measured per frame with `allocation-counter` (counters local to the measuring thread), after a
//! positive control, as `oce-api`'s slot-entry census does: every one of the 160 frames of every
//! G36 fixture through `ColumnarSource` and `StepSource` on the `Recorded` time base, the
//! controller's two-hour 5 s grid with every input held, and the controller's reference day read
//! from CSV text. Each quiet frame (no warning) must allocate zero times and zero bytes; a warning
//! frame is bounded by its warnings. The table it prints is the PR's census.

#![allow(clippy::print_stdout)] // The census table is the evidence the PR pastes.

extern crate oce_api as oce_model;

#[path = "../../../oce-api/tests/support/blob_corpus.rs"]
mod blob_corpus;
#[path = "support/controller.rs"]
mod controller;
#[path = "support/fixtures.rs"]
mod fixtures;
#[path = "../../../oce-graph/tests/support/frame_schedule.rs"]
mod frame_schedule;
#[path = "support/scenarios.rs"]
mod scenarios;

use std::hint::black_box;

use allocation_counter::measure;
use blob_corpus::FIXTURES;
use fixtures::{Frames, columnar, load, path_map, steps};
use frame_schedule::{TICKS, input_value, time_at};
use oce_replay::{
    CsvFormat, CsvSource, FrameSource, MissingPolicy, Progress, Replay, ReplayLimits,
    ReplayOptions, TimeBase,
};

struct Census {
    frames: u64,
    quiet: u64,
    warning_frames: u64,
    warning_allocations: u64,
    first_frame: u64,
}

/// Advance one frame at a time, measuring each; the frame count is known up front.
fn census<S: FrameSource>(name: &str, mut replay: Replay<S>, frames: usize) -> Census {
    replay.reserve(frames).unwrap();
    let mut c = Census {
        frames: 0,
        quiet: 0,
        warning_frames: 0,
        warning_allocations: 0,
        first_frame: 0,
    };
    for k in 0..frames {
        let before = replay.trace().diagnostics().len();
        let mut progress = None;
        let info = measure(|| progress = Some(replay.advance(1)));
        let progress = progress.unwrap().unwrap();
        assert!(
            matches!(progress, Progress::More { .. }),
            "{name}: frame {k}: {progress:?}"
        );
        assert!(replay.trace().status()[k] <= 2, "{name}: frame {k} refused");
        let warnings = (replay.trace().diagnostics().len() - before) as u64;
        c.frames += 1;
        if k == 0 {
            c.first_frame = info.count_total;
            if warnings == 0 {
                // Per output: its column and its path twice (column and metadata); plus the two
                // vectors that hold them.
                let outputs = replay.trace().outputs().len() as u64;
                assert!(
                    info.count_total <= 3 * outputs + 2,
                    "{name}: first frame {info:?}"
                );
            }
        } else if warnings == 0 {
            assert_eq!(info.count_total, 0, "{name}: frame {k}: {info:?}");
            assert_eq!(info.bytes_total, 0, "{name}: frame {k}: {info:?}");
            c.quiet += 1;
        } else {
            // Per warning: a block and a message string here, and at most a source and a message
            // buffer in the engine's reused records; plus geometric growth of both record vectors.
            let bound = 4 * warnings + 2 * (64 - (c.frames + warnings).leading_zeros()) as u64 + 2;
            assert!(
                info.count_total <= bound,
                "{name}: frame {k}: {} allocations, {warnings} warnings",
                info.count_total
            );
            c.warning_frames += 1;
            c.warning_allocations += info.count_total;
        }
    }
    black_box(replay.finish());
    c
}

fn schedule(engine: &oce_api::Engine) -> Frames {
    let definitions = engine.input_definitions().unwrap();
    (0..TICKS)
        .map(|k| {
            let values = definitions
                .iter()
                .enumerate()
                .map(|(slot, d)| input_value(d.value_type, d.min.as_ref(), d.max.as_ref(), slot, k))
                .collect();
            (time_at(k), values)
        })
        .collect()
}

fn row(name: &str, how: &str, c: &Census) {
    println!(
        "{name}\t{how}\t{}\t{}\t{}\t{}\t{}",
        c.frames, c.first_frame, c.quiet, c.warning_frames, c.warning_allocations
    );
}

#[test]
fn a_replayed_frame_allocates_nothing_once_the_map_and_columns_exist() {
    let positive = measure(|| {
        black_box(vec![0_u8; 64]);
    });
    assert_eq!(positive.count_total, 1, "the counter observes allocations");
    println!(
        "fixture\tsource\tframes\tfirst-frame allocations\tquiet frames (0 allocations)\twarning frames\tallocations on warning frames"
    );
    let (mut frames, mut quiet) = (0, 0);
    for name in FIXTURES {
        let engine = load(name);
        let schedule = schedule(&engine);
        let map = path_map(&engine, &MissingPolicy::Refuse);
        for how in ["columnar", "steps"] {
            let options = ReplayOptions::new(TimeBase::Recorded);
            let c = if how == "columnar" {
                census(
                    name,
                    Replay::new(load(name), map.clone(), columnar(&map, &schedule), options)
                        .unwrap(),
                    schedule.len(),
                )
            } else {
                census(
                    name,
                    Replay::new(load(name), map.clone(), steps(&map, &schedule), options).unwrap(),
                    schedule.len(),
                )
            };
            row(name, how, &c);
            frames += c.frames;
            quiet += c.quiet;
        }
    }
    let c = census(
        "controller_grid_hold",
        scenarios::controller_grid_hold(),
        1_441,
    );
    row("cooling_only_controller", "grid 5 s, HoldLast", &c);
    frames += c.frames;
    quiet += c.quiet;

    // The reference day as CSV text, Booleans and Integers written as 0/1 and integers.
    let reference = controller::reference();
    let mut text = String::from("time");
    for (name, _, _) in controller::MAPPING {
        text.push(',');
        text.push_str(name);
    }
    text.push('\n');
    for r in 0..reference.rows.len() {
        text.push_str(&format!("{}", reference.rows[r][0]));
        for (name, _, code) in controller::MAPPING {
            let cell = reference.column(name)[r];
            match code {
                'R' => text.push_str(&format!(",{cell:?}")),
                _ => text.push_str(&format!(",{}", cell as i64)),
            }
        }
        text.push('\n');
    }
    let engine = load(controller::FIXTURE);
    let map = controller::map(&engine, &reference, &MissingPolicy::Refuse);
    let source = CsvSource::new(
        &map,
        text.as_bytes(),
        CsvFormat::seconds("time"),
        &ReplayLimits::default(),
    )
    .unwrap();
    let replay = Replay::new(engine, map, source, ReplayOptions::new(TimeBase::Recorded)).unwrap();
    let c = census("controller_csv", replay, 1_441);
    row("cooling_only_controller", "csv", &c);
    frames += c.frames;
    quiet += c.quiet;
    println!("total: {frames} frames; {quiet} quiet frames allocated nothing");
}
