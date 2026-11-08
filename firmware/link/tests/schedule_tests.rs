//! The firmware schedule is the host schedule, bit for bit (issue #402).
//!
//! `src/schedule.rs` restates `crates/oce-graph/tests/support/frame_schedule.rs` without `std` so
//! a bare-metal image can link it. That file is the convention the facade's dual-path test and
//! `oce-graph`'s `blob_execution` test drive, so this test includes it by path and requires the two
//! to agree on every constant, every frame time and every input value of every checked-in golden,
//! by exact bits, and on the digest each folds.

#[path = "../../../crates/oce-graph/tests/support/frame_schedule.rs"]
mod frame_schedule;

use std::path::PathBuf;

use oce_firmware_link::schedule;
use oce_graph::blob::Executable;

/// Every golden blob, by fixture name, sorted.
fn goldens() -> Vec<(String, Vec<u8>)> {
    let dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/oce-api/tests/fixtures/blob");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
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
            let bytes = std::fs::read(dir.join(format!("{name}.oceb"))).unwrap();
            (name, bytes)
        })
        .collect()
}

/// A value's tag and exact bits, as both digests fold it: a host `RunDigest` over the one value.
fn bits(value: &oce_model::Value) -> String {
    let mut digest = frame_schedule::RunDigest::default();
    digest.value(value);
    digest.hex()
}

#[test]
fn frame_count_and_steps_equal_the_host_schedule() {
    assert_eq!(schedule::TICKS, frame_schedule::TICKS);
    assert_eq!(
        schedule::STEPS.map(f64::to_bits),
        frame_schedule::STEPS.map(f64::to_bits)
    );
}

#[test]
fn every_frame_time_equals_the_host_schedule_by_bits() {
    for k in 0..=schedule::TICKS + 1 {
        assert_eq!(
            schedule::time_at(k).to_bits(),
            frame_schedule::time_at(k).to_bits(),
            "frame {k}"
        );
    }
}

#[test]
fn every_input_of_every_golden_equals_the_host_schedule_by_bits() {
    let goldens = goldens();
    assert_eq!(goldens.len(), 46);
    let mut compared = 0_usize;
    for (name, bytes) in &goldens {
        let executable = Executable::from_blob(bytes).unwrap();
        for k in 0..schedule::TICKS {
            for (slot, input) in executable.inputs().iter().enumerate() {
                let (min, max) = (input.min.as_ref(), input.max.as_ref());
                let ours = schedule::input_value(input.value_type, min, max, slot, k);
                let host = frame_schedule::input_value(input.value_type, min, max, slot, k);
                assert_eq!(bits(&ours), bits(&host), "{name} frame {k} slot {slot}");
                compared += 1;
            }
        }
    }
    assert!(compared > 46 * 160, "every golden has inputs");
}

#[test]
fn both_digests_fold_a_frame_to_the_same_value() {
    use oce_model::{EnumClassId, Value};

    let values = [
        Value::Real(-0.0),
        Value::Real(f64::NAN),
        Value::Integer(i64::MIN),
        Value::Boolean(true),
        Value::String("supply fan".into()),
        Value::Enum {
            class: EnumClassId(7),
            ordinal: 3,
        },
    ];
    let words = [0, u64::MAX, 0x0123_4567_89ab_cdef];
    let mut ours = schedule::RunDigest::default();
    let mut host = frame_schedule::RunDigest::default();
    for (k, time) in [0.0, 0.75, 1.75, 1e9].into_iter().enumerate() {
        ours.frame(time, k as u64 + 1, &values[..2], &values, &words);
        host.frame(time, k as u64 + 1, &values[..2], &values, &words);
        assert_eq!(
            format!("{:016x}", ours.get()),
            host.hex(),
            "after frame {k}"
        );
    }
}
