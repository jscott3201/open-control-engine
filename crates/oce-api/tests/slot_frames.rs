//! The slot-indexed execute entry against the named path over the whole G36 corpus (issue #444).
//!
//! * **Dual-entry equivalence.** For each of the 46 catalog fixtures the 160-frame schedule of
//!   `oce-graph`'s `tests/support/frame_schedule.rs` (the schedule whose run digests
//!   `blob_frame_digests.tsv` pins for the facade and the blob entry) runs through
//!   `prepare_frame`/`execute_frame` on one load and `execute_slots` on another. After every frame
//!   both must report the same sequence, time bits, output identities, output bits and warnings,
//!   and hold identical raw state words and latest outputs; every sixteenth frame (the last
//!   included) the durable state snapshot bytes must be identical too (`support/dual_entry.rs`).
//!   The private `blob_dual_path_tests.rs` compares the whole `RunState` after every frame. Bad
//!   frames between good ones are in `slot_frame_refusals.rs`.
//! * **Slot order is the blob's I/O order.** The map's input and output paths equal the
//!   compiled-schedule blob's I/O tables, entry for entry.
//! * **Allocation census.** Once the map exists, all 160 frames of every fixture, values built
//!   beforehand, run inside one measured region on a fresh load: zero allocations and zero bytes,
//!   with a counter positive control. `allocation-counter`'s counters are local to the measuring
//!   thread. The census prints per-fixture rows, which the PR pastes.

#![allow(clippy::print_stdout)] // The census table is the evidence the PR pastes.

#[path = "support/blob_corpus.rs"]
mod blob_corpus;
#[path = "support/dual_entry.rs"]
mod dual_entry;
#[path = "../../oce-graph/tests/support/frame_schedule.rs"]
mod frame_schedule;

use std::hint::black_box;

use allocation_counter::measure;
use blob_corpus::{FIXTURES, fixture_bytes, fixture_names_on_disk, load};
use dual_entry::{DualEntry, Outcome};
use frame_schedule::{TICKS, input_value, time_at};
use oce_api::{FrameSlots, Value};

/// Frame `k`'s values in slot order, from the map's own definitions.
fn frame(slots: &FrameSlots, k: u32) -> Vec<Value> {
    slots
        .inputs()
        .iter()
        .enumerate()
        .map(|(slot, d)| input_value(d.value_type, d.min.as_ref(), d.max.as_ref(), slot, k))
        .collect()
}

#[test]
fn the_corpus_is_the_46_catalog_fixtures() {
    let on_disk: Vec<String> = fixture_names_on_disk()
        .into_iter()
        .filter(|name| name != "member_list_interface")
        .collect();
    let mut listed: Vec<&str> = FIXTURES.to_vec();
    listed.sort_unstable();
    assert_eq!(on_disk, listed);
    assert_eq!(listed.len(), 46);
}

/// The snapshot-byte comparison interval, in frames; `TICKS` is a multiple of it.
const SNAPSHOT_EVERY: u32 = 16;

#[test]
fn both_entries_agree_bit_for_bit_after_every_frame_of_every_fixture() {
    assert_eq!(
        TICKS % SNAPSHOT_EVERY,
        0,
        "the last frame is snapshot-compared"
    );
    let mut frames = 0;
    let mut warned = 0;
    for name in FIXTURES {
        let mut dual = DualEntry::load(&fixture_bytes(name));
        dual.compare_state()
            .unwrap_or_else(|error| panic!("{name}: after load: {error}"));
        let mut outputs_moved = false;
        let mut first: Option<Vec<Value>> = None;
        for k in 0..TICKS {
            let values = frame(&dual.slots, k);
            match dual.step(time_at(k), &values) {
                Ok(Outcome::Accepted {
                    sequence,
                    outputs,
                    warnings,
                }) => {
                    assert_eq!(sequence, u64::from(k) + 1, "{name}: tick {k}");
                    warned += warnings;
                    match &first {
                        None => first = Some(outputs),
                        Some(first) => {
                            outputs_moved |= first.iter().zip(&outputs).any(|(a, b)| !a.bit_eq(b));
                        }
                    }
                }
                Ok(Outcome::Refused(error)) => panic!("{name}: tick {k}: refused {error}"),
                Err(error) => panic!("{name}: tick {k}: {error}"),
            }
            // Snapshot bytes every sixteenth frame and after the last (160 = 10 * 16).
            if k % SNAPSHOT_EVERY == SNAPSHOT_EVERY - 1 {
                dual.compare_state()
                    .unwrap_or_else(|error| panic!("{name}: tick {k}: {error}"));
            }
            frames += 1;
        }
        // The eleven fixed-limit economizer fixtures have no boundary input, so their outputs are
        // constant by construction; every other fixture's outputs must move.
        assert!(
            outputs_moved || dual.slots.inputs().is_empty(),
            "{name}: no output ever moved"
        );
    }
    assert_eq!(frames, 46 * TICKS);
    println!("dual-entry equivalence: {frames} frames, {warned} warnings, all bit-identical");
}

#[test]
fn slot_order_is_the_compiled_blob_io_order() {
    for name in FIXTURES {
        let engine = load(name);
        let slots = engine.frame_slots().unwrap();
        let blob = engine.compile_blob().unwrap();
        let executable = oce_graph::blob::Executable::from_blob(&blob.bytes).unwrap();
        let blob_inputs: Vec<&str> = executable.inputs().iter().map(|i| i.path).collect();
        let slot_inputs: Vec<&str> = slots.inputs().iter().map(|d| d.path.as_str()).collect();
        assert_eq!(blob_inputs, slot_inputs, "{name}: inputs");
        let blob_outputs: Vec<&str> = executable.outputs().iter().map(|o| o.path).collect();
        let slot_outputs: Vec<&str> = slots.outputs().iter().map(String::as_str).collect();
        assert_eq!(blob_outputs, slot_outputs, "{name}: outputs");
        for (slot, path) in slot_inputs.iter().enumerate() {
            assert_eq!(slots.input_slot(path), Some(slot), "{name}: {path}");
            assert_eq!(slots.output_slot(path), None, "{name}: {path}");
        }
        for (slot, path) in slot_outputs.iter().enumerate() {
            assert_eq!(slots.output_slot(path), Some(slot), "{name}: {path}");
            assert_eq!(slots.input_slot(path), None, "{name}: {path}");
        }
        assert_eq!(slots.input_slot(""), None);
        assert_eq!(slots.input_slot("urn:not-an-input"), None);
    }
}

/// The most warnings a frame may report; the census's length buffer is reserved to it up front.
const MAX_WARNINGS_PER_FRAME: usize = 64;

#[test]
fn the_slot_entry_allocates_nothing_per_frame_once_the_map_exists() {
    let positive = measure(|| {
        black_box(vec![0_u8; 64]);
    });
    assert_eq!(positive.count_total, 1, "the counter observes allocations");

    println!(
        "fixture\tinputs\toutputs\tframes\tmap allocations\twarnings\t\
         record-growing frames\tallocations on them\tallocations on all other frames"
    );
    let (mut all_frames, mut quiet_total, mut growing_total) = (0_u64, 0_u64, 0_u64);
    for name in FIXTURES {
        let mut engine = load(name);
        let mut slots = None;
        let map = measure(|| slots = Some(engine.frame_slots().unwrap()));
        let slots = slots.unwrap();
        let frames: Vec<(f64, Vec<Value>)> =
            (0..TICKS).map(|k| (time_at(k), frame(&slots, k))).collect();
        // Per warning-record index, the longest source and message seen so far: a frame that stays
        // within them reuses every record in place.
        let mut held: Vec<(usize, usize)> = Vec::new();
        let mut lengths: Vec<(usize, usize)> = Vec::with_capacity(MAX_WARNINGS_PER_FRAME);
        let (mut warnings, mut growing, mut on_growing, mut on_quiet) = (0, 0, 0, 0);
        for (k, (time, values)) in frames.iter().enumerate() {
            lengths.clear();
            let mut sequence = 0;
            let census = measure(|| {
                let frame = engine.execute_slots(&slots, *time, values).unwrap();
                black_box(frame.outputs().count());
                for event in frame.diagnostics().iter().take(MAX_WARNINGS_PER_FRAME) {
                    lengths.push((event.block.len(), event.message.len()));
                }
                sequence = frame.sequence();
            });
            assert_eq!(sequence, k as u64 + 1, "{name}");
            assert!(lengths.len() < MAX_WARNINGS_PER_FRAME, "{name}: frame {k}");
            warnings += lengths.len();
            let grows = lengths
                .iter()
                .enumerate()
                .any(|(index, &(block, message))| {
                    held.get(index)
                        .is_none_or(|&(b, m)| block > b || message > m)
                });
            for (index, &(block, message)) in lengths.iter().enumerate() {
                match held.get_mut(index) {
                    Some(record) => *record = (record.0.max(block), record.1.max(message)),
                    None => held.push((block, message)),
                }
            }
            if grows {
                growing += 1;
                on_growing += census.count_total;
            } else {
                assert_eq!(census.count_total, 0, "{name}: frame {k}: {census:?}");
                assert_eq!(census.bytes_total, 0, "{name}: frame {k}: {census:?}");
                on_quiet += census.count_total;
            }
        }
        println!(
            "{name}\t{}\t{}\t{TICKS}\t{}\t{warnings}\t{growing}\t{on_growing}\t{on_quiet}",
            slots.inputs().len(),
            slots.outputs().len(),
            map.count_total,
        );
        all_frames += u64::from(TICKS);
        quiet_total += on_quiet;
        growing_total += growing;
    }
    println!(
        "total: {all_frames} frames across {} fixtures; {growing_total} frames grew a warning \
         record; {quiet_total} allocations on the other {} frames",
        FIXTURES.len(),
        all_frames - growing_total
    );
    assert_eq!(quiet_total, 0);
}
