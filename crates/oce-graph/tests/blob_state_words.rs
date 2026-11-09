//! The blob entry's slot table and raw state words over every checked-in golden (GitHub #401):
//! every path resolves to its own slot and nothing else does; words restore all or nothing,
//! continue a run identically, and are refused with a typed reason when their length, their
//! pre-frame value or a block's own state contract says they cannot belong to this executable.

#[path = "support/frame_schedule.rs"]
mod frame_schedule;

use std::path::PathBuf;

use frame_schedule::{TICKS, input_value, time_at};
use oce_blocks::NoopDiagnostics;
use oce_graph::blob::{Executable, StateWordsRefusal};
use oce_model::Value;

fn goldens() -> Vec<(String, Vec<u8>)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../oce-api/tests/fixtures/blob");
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

/// Frame `k`'s values for `executable`, shifted by `offset` frames of the shared schedule.
fn values(executable: &Executable<'_>, k: u32, offset: u32) -> Vec<Value> {
    executable
        .inputs()
        .iter()
        .enumerate()
        .map(|(slot, i)| {
            input_value(
                i.value_type,
                i.min.as_ref(),
                i.max.as_ref(),
                slot,
                k + offset,
            )
        })
        .collect()
}

/// Run frames `range` of the schedule, with inputs shifted by `offset`.
fn run(executable: &mut Executable<'_>, range: std::ops::Range<u32>, offset: u32) {
    for k in range {
        let values = values(executable, k, offset);
        executable
            .execute_frame(time_at(k), &values, &NoopDiagnostics)
            .unwrap();
    }
}

fn values_bits(executable: &Executable<'_>) -> Vec<String> {
    executable
        .decoded()
        .state
        .values
        .iter()
        .map(|value| match value {
            Value::Real(x) => format!("{:016x}", x.to_bits()),
            other => format!("{other:?}"),
        })
        .collect()
}

#[test]
fn every_path_resolves_to_its_own_slot_and_nothing_else_does() {
    for (name, bytes) in goldens() {
        let executable = Executable::from_blob(&bytes).unwrap();
        for (slot, input) in executable.inputs().iter().enumerate() {
            assert_eq!(executable.input_slot(input.path), Some(slot), "{name}");
            assert_eq!(executable.output_slot(input.path), None, "{name}");
            let truncated = &input.path[..input.path.len() - 1];
            assert_eq!(executable.input_slot(truncated), None, "{name}");
        }
        for (slot, output) in executable.outputs().iter().enumerate() {
            assert_eq!(executable.output_slot(output.path), Some(slot), "{name}");
            assert_eq!(executable.input_slot(output.path), None, "{name}");
        }
        assert_eq!(executable.input_slot(""), None);
        assert!(executable.output(executable.outputs().len()).is_none());
        assert_eq!(executable.output_values().len(), executable.outputs().len());
        assert!(
            !executable.target_bound(),
            "{name}: no G36 fixture is target-bound"
        );
        assert_eq!(executable.content_id(), executable.decoded().content_id);
        // Format 2: every golden carries its canonical content id's 32 digest bytes verbatim
        // (the 13 partial exports included), and the decoder never interprets them.
        assert_eq!(
            executable.content_id(),
            Some(bytes[20..52].try_into().unwrap()),
            "{name}"
        );
        assert_eq!(executable.time(), None);
        assert_eq!(executable.sequence(), 0);
    }
}

#[test]
fn restored_words_continue_a_run_identically() {
    let half = TICKS / 2;
    let (mut stateful, mut differed) = (0, 0);
    for (name, bytes) in goldens() {
        let mut reference = Executable::from_blob(&bytes).unwrap();
        // Before the first frame the initial words are the only valid ones, and restore them.
        let initial = reference.state_words().to_vec();
        assert_eq!(reference.restore_state_words(&initial), Ok(()), "{name}");
        run(&mut reference, 0..half, 0);
        if reference.state_words().is_empty() {
            continue;
        }
        stateful += 1;
        // A twin at the same clock but fed other inputs takes the reference's words.
        let mut twin = Executable::from_blob(&bytes).unwrap();
        run(&mut twin, 0..half, 7);
        let words = reference.state_words().to_vec();
        differed += usize::from(twin.state_words() != words.as_slice());
        assert_eq!(twin.restore_state_words(&words), Ok(()), "{name}");
        assert_eq!(twin.state_words(), words.as_slice(), "{name}");
        // Restoring a run's own words changes nothing.
        assert_eq!(reference.restore_state_words(&words), Ok(()), "{name}");
        // From the next frame on both runs agree on every connector value and word.
        for k in half..TICKS {
            let frame = values(&reference, k, 0);
            reference
                .execute_frame(time_at(k), &frame, &NoopDiagnostics)
                .unwrap();
            twin.execute_frame(time_at(k), &frame, &NoopDiagnostics)
                .unwrap();
            assert_eq!(twin.state_words(), reference.state_words(), "{name} {k}");
            assert_eq!(values_bits(&twin), values_bits(&reference), "{name} {k}");
        }
    }
    assert!(stateful > 20, "{stateful} stateful goldens");
    assert!(
        differed * 2 > stateful,
        "only {differed} of {stateful} twins held other words before the restore"
    );
}

#[test]
fn refused_words_change_nothing() {
    let mut invalid_regions = 0;
    let mut stateful = 0;
    for (name, bytes) in goldens() {
        let mut executable = Executable::from_blob(&bytes).unwrap();
        let initial = executable.state_words().to_vec();
        let length = initial.len();
        for found in [length + 1, length.saturating_sub(1)] {
            if found == length {
                continue;
            }
            assert_eq!(
                executable.restore_state_words(&vec![0; found]),
                Err(StateWordsRefusal::LengthMismatch {
                    expected: length,
                    found
                }),
                "{name}"
            );
        }
        if length == 0 {
            continue;
        }
        stateful += 1;
        // Before the first frame anything but the initial words is refused.
        let mut flipped = initial.clone();
        flipped[length - 1] ^= 1;
        assert_eq!(
            executable.restore_state_words(&flipped),
            Err(StateWordsRefusal::NotInitial),
            "{name}"
        );
        assert_eq!(executable.state_words(), initial.as_slice(), "{name}");

        run(&mut executable, 0..TICKS / 2, 0);
        let before = executable.state_words().to_vec();
        // All-ones words fail the first stateful block's own contract (a previous-time word, a
        // flag or a ring index).
        let refusal = executable.restore_state_words(&vec![u64::MAX; length]);
        assert!(
            matches!(refusal, Err(StateWordsRefusal::InvalidRegion { .. })),
            "{name}: {refusal:?}"
        );
        assert_eq!(executable.state_words(), before.as_slice(), "{name}");
        // A moving-average ring index far outside the ring is refused, not executed.
        for slot in &executable.decoded().state.slots.clone() {
            let class = &executable.decoded().model.blocks[slot.block.0 as usize].class_iri;
            if class.as_ref() != "CDL.Reals.MovingAverage" {
                continue;
            }
            let mut words = before.clone();
            words[slot.offset + 3] = u64::MAX;
            assert_eq!(
                executable.restore_state_words(&words),
                Err(StateWordsRefusal::InvalidRegion {
                    block: slot.block.0
                }),
                "{name}"
            );
            assert_eq!(executable.state_words(), before.as_slice(), "{name}");
            invalid_regions += 1;
        }
    }
    assert!(stateful > 20, "{stateful} stateful goldens");
    assert!(
        invalid_regions > 0,
        "a golden with a moving average was exercised"
    );
}
