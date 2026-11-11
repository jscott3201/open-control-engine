//! The compiled-schedule decoder's allocation count, pinned exactly for every golden (GitHub #400).
//!
//! `oce_graph::blob` documents what `decode` allocates: what it returns, one default-parameter block
//! per class for its signature hash, and four scratch vectors.
//! This test turns that list into a formula over the decoded executable and requires the measured
//! allocation count to equal it for all 46 goldens, so an undocumented allocation fails here. Block
//! constructors allocate whatever they allocate on the host too; their share is measured by
//! constructing each block again with its decoded parameters.

// Shared helper, declared once for this binary in its root `facade_allocation_census.rs`.
use crate::blob_corpus;

use std::hint::black_box;

use allocation_counter::measure;
use blob_corpus::{FIXTURES, golden};
use oce_graph::StateSlot;
use oce_graph::blob::{Decoded, decode};
use oce_model::{BlockId, Value};

fn some(n: usize) -> u64 {
    u64::from(n > 0)
}

/// The documented allocation count of `decode` for this executable.
fn documented(decoded: &Decoded<'_>) -> u64 {
    let model = &decoded.model;
    let blocks = model.blocks.len();
    let connectors = model.connectors.len();
    let mut classes: Vec<&str> = model.blocks.iter().map(|b| b.class_iri.as_ref()).collect();
    classes.sort_unstable();
    classes.dedup();
    let largest_params = model
        .blocks
        .iter()
        .map(|b| b.params.values.len())
        .max()
        .unwrap_or(0);
    let max_arity = model
        .blocks
        .iter()
        .map(|b| b.inputs.len())
        .max()
        .unwrap_or(0);

    // Returned: the model arenas.
    let mut count = some(blocks) + some(connectors) + some(model.connections.len());
    // One shared Arc<str> per class.
    count += classes.len() as u64;
    // Per block: port and parameter vectors when non-empty, one Arc per name and String value.
    for block in &model.blocks {
        count += some(block.inputs.len()) + some(block.outputs.len());
        count += some(block.params.values.len()) + block.params.values.len() as u64;
        count += block
            .params
            .values
            .iter()
            .filter(|(_, v)| matches!(v, Value::String(_)))
            .count() as u64;
    }
    // The block vector, then each constructor (its Box included), measured by re-construction.
    count += some(blocks);
    for block in &model.blocks {
        let entry = oce_blocks::lookup(&block.class_iri).unwrap();
        count += measure(|| {
            black_box((entry.make)(&block.params));
        })
        .count_total;
    }
    // Schedule: order and driver_of.
    count += some(blocks) + some(connectors);
    // RunState: words (one reservation), slots (pushed one at a time), slot_of, values, scratch.
    count += some(decoded.state.words.len());
    let stateful = decoded.state.slots.len();
    count += measure(|| {
        let mut slots = Vec::new();
        for _ in 0..stateful {
            slots.push(StateSlot {
                block: BlockId(0),
                offset: 0,
                len: 0,
            });
        }
        black_box(slots);
    })
    .count_total;
    count += some(blocks) + some(connectors) + some(max_arity);
    // I/O tables: the two lists, one target vector per input; bounds are never Strings.
    count += some(decoded.inputs.len()) + decoded.inputs.len() as u64 + some(decoded.outputs.len());
    // Scratch: one default-parameter construction per class for its signature hash (freed at
    // once), measured by re-construction.
    for class in &classes {
        let entry = oce_blocks::lookup(class).unwrap();
        count += measure(|| {
            black_box((entry.make)(&oce_model::ParamTable::default()));
        })
        .count_total;
    }
    // Scratch: the class table, the parameter-name buffer, the per-connector table and the
    // schedule-position table.
    count += some(classes.len()) + some(largest_params) + some(connectors) + some(blocks);
    count
}

#[test]
fn decode_allocates_exactly_what_its_documentation_lists() {
    let positive = measure(|| {
        black_box(vec![0_u8; 64]);
    });
    assert_eq!(positive.count_total, 1, "the counter observes allocations");
    for name in FIXTURES {
        let bytes = golden(name);
        let mut decoded = None;
        let measured = measure(|| {
            decoded = Some(decode(&bytes).unwrap());
        });
        let decoded = decoded.unwrap();
        assert_eq!(measured.count_total, documented(&decoded), "{name}");
        // Nothing the decoder returns copies the input: every path and payload borrows `bytes`.
        let range = bytes.as_ptr_range();
        for path in decoded
            .inputs
            .iter()
            .map(|i| i.path)
            .chain(decoded.outputs.iter().map(|o| o.path))
        {
            assert!(range.contains(&path.as_ptr()), "{name}: {path} is copied");
        }
    }
}
