//! The blob execution entry over every checked-in golden: decode once, then 160 frames, with an
//! allocation census and a run digest (GitHub #401).
//!
//! This binary is the census harness for the blob path. `allocation-counter` installs a counting
//! global allocator in the test binary, whose counters are local to the measuring thread; the
//! library under test is whatever `oce-graph` build the binary links. The `no-std-build` CI job runs
//! it with `--no-default-features --features alloc`, so the measured library is the `no_std + alloc`
//! build a target links, while the harness itself is an ordinary `std` test.
//!
//! * **Decode** allocates what `blob::decode` documents (`oce-api`'s `blob_allocations` pins the
//!   exact count); it is counted and printed here, not bounded.
//! * **Frames** allocate nothing: all 160 frames of every golden, inputs built beforehand, run
//!   inside one measured region that must count zero allocations, matching `eval_tick`.
//! * **Digest.** Each fixture's run folds into one FNV-1a-64 digest (time, sequence, outputs, every
//!   connector value and state word after every frame), compared with
//!   `tests/fixtures/blob_frame_digests.tsv`. `oce-api`'s dual-path test checks the facade's run of
//!   the same frames against the same table, so the facade and this build of the entry agree.

#![allow(clippy::print_stdout)] // The census output is the evidence the PR pastes.

#[path = "support/frame_schedule.rs"]
mod frame_schedule;

use std::cell::Cell;
use std::hint::black_box;
use std::path::PathBuf;

use allocation_counter::measure;
use frame_schedule::{DIGEST_TABLE, RunDigest, TICKS, input_value, parse_digests, time_at};
use oce_blocks::Diagnostics;
use oce_graph::blob::Executable;
use oce_model::Value;

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../oce-api/tests/fixtures/blob")
}

/// Every golden blob, by fixture name, sorted.
fn goldens() -> Vec<(String, Vec<u8>)> {
    let mut names: Vec<String> = std::fs::read_dir(golden_dir())
        .unwrap()
        .filter_map(|entry| {
            let name = entry.unwrap().file_name().into_string().unwrap();
            name.strip_suffix(".oceb").map(str::to_owned)
        })
        .collect();
    names.sort_unstable();
    names
        .into_iter()
        .map(|name| {
            let bytes = std::fs::read(golden_dir().join(format!("{name}.oceb"))).unwrap();
            (name, bytes)
        })
        .collect()
}

/// A warning sink that counts without allocating.
#[derive(Default)]
struct CountingSink(Cell<u64>);

impl Diagnostics for CountingSink {
    fn warn(&self, _source: &str, _message: &str, _t: f64) {
        self.0.set(self.0.get() + 1);
    }
}

/// Every frame's input values for `executable`, built before any measurement.
fn frames(executable: &Executable<'_>) -> Vec<Vec<Value>> {
    (0..TICKS)
        .map(|k| {
            executable
                .inputs()
                .iter()
                .enumerate()
                .map(|(slot, input)| {
                    input_value(
                        input.value_type,
                        input.min.as_ref(),
                        input.max.as_ref(),
                        slot,
                        k,
                    )
                })
                .collect()
        })
        .collect()
}

#[test]
fn every_golden_decodes_once_then_runs_its_frames_with_zero_allocations() {
    let positive = measure(|| {
        black_box(vec![0_u8; 64]);
    });
    assert_eq!(positive.count_total, 1, "the counter observes allocations");

    let table =
        std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DIGEST_TABLE))
            .unwrap();
    let expected = parse_digests(&table);
    let goldens = goldens();
    assert_eq!(goldens.len(), 46);
    assert_eq!(expected.len(), goldens.len());

    let (mut decode_count, mut decode_bytes, mut warnings) = (0, 0, 0);
    for ((name, bytes), (digest_name, digest)) in goldens.iter().zip(&expected) {
        assert_eq!(name, digest_name);
        let mut executable = None;
        let decode = measure(|| {
            executable = Some(Executable::from_blob(bytes).unwrap());
        });
        let mut executable = executable.unwrap();
        assert!(
            decode.count_total > 0,
            "{name}: decode allocates what it returns"
        );

        let frames = frames(&executable);
        let times: Vec<f64> = (0..TICKS).map(time_at).collect();
        let sink = CountingSink::default();
        let mut sequences = Vec::with_capacity(frames.len());
        let census = measure(|| {
            for (time, values) in times.iter().zip(&frames) {
                sequences.push(executable.execute_frame(*time, values, &sink).unwrap());
            }
        });
        assert_eq!(
            census.count_total, 0,
            "{name}: {} allocations over {TICKS} frames",
            census.count_total
        );
        assert_eq!(census.bytes_total, 0, "{name}");
        assert_eq!(sequences, (1..=u64::from(TICKS)).collect::<Vec<_>>());

        // Replay the same frames on a fresh executable to fold the digest after every frame.
        let mut replay = Executable::from_blob(bytes).unwrap();
        let mut run = RunDigest::default();
        for (time, values) in times.iter().zip(&frames) {
            let sequence = replay
                .execute_frame(*time, values, &oce_blocks::NoopDiagnostics)
                .unwrap();
            let state = &replay.decoded().state;
            run.frame(
                *time,
                sequence,
                replay.output_values(),
                &state.values,
                &state.words,
            );
        }
        assert_eq!(
            &run.hex(),
            digest,
            "{name}: the blob entry's run digest moved; review the behaviour change, then bless \
             with OCE_BLESS_FRAME_DIGESTS=1 (written by oce-api's dual-path test)"
        );
        assert_eq!(
            replay.state_words(),
            executable.state_words(),
            "{name}: repeatable"
        );

        println!(
            "blob execution {name}: decode={}/{}B frames={TICKS} frame allocations={}/{}B warnings={}",
            decode.count_total,
            decode.bytes_total,
            census.count_total,
            census.bytes_total,
            sink.0.get()
        );
        decode_count += decode.count_total;
        decode_bytes += decode.bytes_total;
        warnings += sink.0.get();
    }
    println!(
        "blob execution total: {} goldens, decode={decode_count}/{decode_bytes}B, frames={}, \
         frame allocations=0/0B, warnings={warnings}",
        goldens.len(),
        goldens.len() as u64 * u64::from(TICKS)
    );
}
