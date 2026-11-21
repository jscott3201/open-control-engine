//! Peak decoder memory against blob length (GitHub #400 review): a hostile connector count or port
//! list cannot make `decode` allocate far more than the bytes it was given before it refuses, and
//! the cost of the shapes that do decode is pinned per encoded byte.
//!
//! Ratios are peak heap bytes held at once during `decode` (`allocation_counter`'s `bytes_max`)
//! divided by the blob length. The bounds are recorded in `oce_graph::blob`'s "Allocation"
//! documentation and in `docs/compiled-schedule-blob.md`.

// Shared helper, declared once for this binary in its root `facade_allocation_census.rs`.
use crate::blob_image;
use crate::blob_models;

use std::sync::Arc;

use allocation_counter::measure;
use blob_image::{Image, In, Inst, Out, VType, Val};
use blob_models::{REAL_TIME_TABLE, dense_matrix_gain, time_table_words, time_tables};
use oce_blocks::MAX_RESOLVED_ARITY;
use oce_graph::blob::{BlobError, InstanceField, MalformedFault, decode};
use oce_model::{ParamTable, Value};

fn class(path: &str) -> (Vec<u8>, u64) {
    (
        path.as_bytes().to_vec(),
        oce_blocks::class_signature_hash(path).unwrap(),
    )
}

fn stateless(class: u32, inputs: Vec<u32>, outputs: Vec<u32>, params: &[(&str, Value)]) -> Inst {
    Inst {
        class,
        kind: 0,
        state_len: 0,
        state_crc: 0,
        inputs,
        outputs,
        params: params
            .iter()
            .map(|(n, v)| (n.as_bytes().to_vec(), Val::from_value(v)))
            .collect(),
    }
}

/// Peak bytes held during `decode(bytes)`, the result, as a ratio of the blob length.
fn peak(bytes: &[u8]) -> (f64, Result<(), BlobError>) {
    let mut outcome = Ok(());
    let info = measure(|| {
        outcome = decode(bytes).map(|_| ());
    });
    (info.bytes_max as f64 / bytes.len() as f64, outcome)
}

/// One `Constant` (no inputs) whose record nonetheless lists `n` input ids, with a connector count
/// of `n + 1` so every id is in range: the shape that made the old decoder build a 144-byte
/// connector per id before the arity check.
pub(crate) fn constant_with_bogus_inputs(n: u32) -> Vec<u8> {
    let mut image = Image::new();
    image.classes = vec![class("CDL.Reals.Sources.Constant")];
    image.connectors = n + 1;
    image.instances = vec![stateless(0, (1..=n).collect(), vec![0], &[])];
    image.schedule = vec![0];
    image.to_bytes()
}

/// The old amplification shape is refused at its arity check holding at most 8× the blob.
const HOSTILE_PEAK_RATIO: f64 = 8.0;

#[test]
fn a_hostile_port_list_is_refused_within_eight_times_the_blob_length() {
    let bytes = constant_with_bogus_inputs(1_000_000);
    assert!(bytes.len() > 4_000_000);
    let (ratio, outcome) = peak(&bytes);
    assert_eq!(
        outcome,
        Err(BlobError::InstanceMismatch {
            block: 0,
            field: InstanceField::InputArity
        })
    );
    assert!(
        ratio <= HOSTILE_PEAK_RATIO,
        "peak {ratio:.2}x the blob length"
    );
}

#[test]
fn a_port_list_longer_than_any_class_resolves_is_refused_before_any_allocation_by_count() {
    let n = u32::try_from(MAX_RESOLVED_ARITY).unwrap() + 1;
    let bytes = constant_with_bogus_inputs(n);
    let (ratio, outcome) = peak(&bytes);
    assert!(matches!(
        outcome,
        Err(BlobError::Malformed {
            fault: MalformedFault::ArityTooLarge,
            ..
        })
    ));
    assert!(ratio < 0.01, "peak {ratio:.4}x the blob length");
    // At the bound itself the structure pass accepts the list and the arity check refuses it.
    let at_bound = constant_with_bogus_inputs(n - 1);
    assert!(matches!(
        decode(&at_bound),
        Err(BlobError::InstanceMismatch {
            field: InstanceField::InputArity,
            ..
        })
    ));
}

/// Many minimal well-formed instances, refused only at the schedule: the decoder has built the
/// whole arena and every block by then.
const MANY_BLOCKS_PEAK_RATIO: f64 = 12.0;

#[test]
fn many_small_instances_refused_late_stay_within_their_recorded_ratio() {
    let blocks = 100_000u32;
    let mut image = Image::new();
    image.classes = vec![class("CDL.Reals.Sources.Constant")];
    image.connectors = blocks;
    image.instances = (0..blocks)
        .map(|b| stateless(0, vec![], vec![b], &[("k", Value::Real(1.0))]))
        .collect();
    image.schedule = vec![];
    let bytes = image.to_bytes();
    let (ratio, outcome) = peak(&bytes);
    assert!(matches!(outcome, Err(BlobError::InvalidSchedule { .. })));
    assert!(
        ratio <= MANY_BLOCKS_PEAK_RATIO,
        "peak {ratio:.2}x the blob length"
    );
}

/// A dense accepted shape: one wide block whose inputs all fan out from one
/// boundary input, so each connector costs eight encoded bytes (a port slot and a target).
pub(crate) const WIDE_ACCEPTED_PEAK_RATIO: f64 = 32.0;

/// The blob of that shape: one `MultiSum` of 200,000 inputs, all targets of one boundary input.
/// `blob_summary_allocations.rs` summarises the same bytes.
pub(crate) fn wide_accepted_block() -> Vec<u8> {
    let nin = 200_000u32;
    let params = ParamTable {
        values: vec![(Arc::from("nin"), Value::Integer(i64::from(nin)))],
    };
    let block = (oce_blocks::lookup("CDL.Reals.MultiSum").unwrap().make)(&params);
    assert_eq!(block.resolved_signature().inputs.len(), nin as usize);
    let mut image = Image::new();
    image.classes = vec![class("CDL.Reals.MultiSum")];
    image.connectors = nin + 1;
    image.instances = vec![stateless(
        0,
        (0..nin).collect(),
        vec![nin],
        &[("nin", Value::Integer(i64::from(nin)))],
    )];
    image.schedule = vec![0];
    image.inputs = vec![In {
        path: b"urn:test:u".to_vec(),
        vtype: VType::Real,
        min: None,
        max: None,
        targets: (0..nin).collect(),
        payload: Vec::new(),
    }];
    image.outputs = vec![Out {
        path: b"urn:test:y".to_vec(),
        source: nin,
        payload: Vec::new(),
    }];
    image.to_bytes()
}

#[test]
fn a_wide_accepted_block_stays_within_its_recorded_ratio() {
    let bytes = wide_accepted_block();
    let (ratio, outcome) = peak(&bytes);
    assert_eq!(outcome, Ok(()));
    assert!(
        ratio <= WIDE_ACCEPTED_PEAK_RATIO,
        "peak {ratio:.2}x the blob length"
    );
}

/// The linear term of the decoder's documented peak: bytes held per blob byte by everything the
/// decoder allocates outside block constructors and the state words. The costliest record is a
/// connector id in an output list: four encoded bytes for a 144-byte `Connector`, its value, its
/// driver entry, its ownership entry and its port slot.
pub(crate) const LINEAR_PEAK_RATIO: u64 = 64;

/// The constructed part of the decoder's documented peak, in bytes: the model-wide budget and
/// one block's construction on top of it.
pub(crate) const CONSTRUCTED_PEAK_BYTES: u64 =
    (8 * oce_graph::MAX_CONSTRUCTED_WORDS + oce_blocks::MAX_CONSTRUCTION_PEAK_BYTES) as u64;

/// The decoder's documented peak for a blob of `length` bytes (`oce_graph::blob`, "Peak memory
/// against blob length"): the linear term, the constructed budget with one construction, and
/// the state budget.
pub(crate) fn documented_peak_bound(length: usize) -> u64 {
    LINEAR_PEAK_RATIO * length as u64
        + CONSTRUCTED_PEAK_BYTES
        + 8 * oce_graph::MAX_STATE_WORDS as u64
}

#[test]
fn the_documented_peak_is_the_linear_term_and_ninety_six_mebibytes() {
    assert_eq!(oce_graph::MAX_CONSTRUCTED_WORDS, 1 << 22);
    assert_eq!(oce_blocks::MAX_CONSTRUCTION_PEAK_BYTES, 32 << 20);
    assert_eq!(oce_graph::MAX_STATE_WORDS, 1 << 22);
    assert_eq!(documented_peak_bound(0), 96 << 20);
    assert_eq!(documented_peak_bound(1000), 64_000 + (96 << 20));
}

/// Peak bytes held during `decode(bytes)` and its outcome.
fn peak_bytes(bytes: &[u8]) -> (u64, Result<(), BlobError>) {
    let mut outcome = Ok(());
    let info = measure(|| {
        outcome = decode(bytes).map(|_| ());
    });
    (info.bytes_max, outcome)
}

/// The densest accepted shape per encoded byte: one block whose outputs nothing reads, so each
/// connector costs four encoded bytes (its port slot alone).
#[test]
fn a_wide_block_of_unread_outputs_stays_within_the_linear_term() {
    let nout = 200_000u32;
    let mut image = Image::new();
    image.classes = vec![class("CDL.Routing.RealScalarReplicator")];
    image.connectors = nout + 1;
    image.instances = vec![stateless(
        0,
        vec![0],
        (1..=nout).collect(),
        &[("nout", Value::Integer(i64::from(nout)))],
    )];
    image.schedule = vec![0];
    image.inputs = vec![In {
        path: b"urn:test:u".to_vec(),
        vtype: VType::Real,
        min: None,
        max: None,
        targets: vec![0],
        payload: Vec::new(),
    }];
    let bytes = image.to_bytes();
    let (ratio, outcome) = peak(&bytes);
    assert_eq!(outcome, Ok(()));
    assert!(
        (40.0..=LINEAR_PEAK_RATIO as f64).contains(&ratio),
        "peak {ratio:.2}x the blob length"
    );
}

/// The four shapes of issue #700, by the blob lengths it records. Each `table_524288_2` block
/// holds 8 MiB from one 60-byte record, and nothing summed them: 16 peaked at 134,225,920 bytes
/// and 64 at 536,903,536. The fourth such block now crosses the budget (4 x 1,048,578 words is
/// eight words past 2^22) and is refused, so 16 blocks and 64 peak alike.
#[test]
fn the_shapes_that_allocated_by_parameter_name_are_bounded_or_refused() {
    let table = "table_524288_2";
    let words = time_table_words(524_288);
    assert_eq!(words, 1_048_578);

    // One block: accepted, as before, holding its table and nothing else of note.
    let one = time_tables(&[table]).to_bytes();
    assert_eq!(one.len(), 223);
    let (peak, outcome) = peak_bytes(&one);
    assert_eq!(outcome, Ok(()));
    assert!(peak <= 8 * words + LINEAR_PEAK_RATIO * 223, "peak {peak}");
    assert!(peak >= 8 * words, "the table is held: peak {peak}");

    // 16 and 64 blocks: refused at the fourth, holding three tables and the one refused.
    let refusal = BlobError::ConstructedTooLarge {
        block: 3,
        requested: 4 * words,
    };
    assert_eq!(4 * words, 4_194_312);
    for (count, length) in [(16usize, 1_123usize), (64, 4_003)] {
        let bytes = time_tables(&vec![table; count]).to_bytes();
        assert_eq!(bytes.len(), length);
        let (peak, outcome) = peak_bytes(&bytes);
        assert_eq!(outcome, Err(refusal), "{count} blocks");
        assert!(
            peak <= 8 * 4 * words + LINEAR_PEAK_RATIO * length as u64,
            "{count} blocks peak {peak}"
        );
        assert!(peak <= documented_peak_bound(length), "{count} blocks");
    }

    // A dense 1024 x 1024 gain with no cell bound: accepted, as the ruling requires. It holds
    // 2^20 gains and 256 words of port kinds, and its constructor reads the cells through one
    // scratch table of the same 2^20 slots.
    let gain = dense_matrix_gain(1024, 1024).to_bytes();
    assert_eq!(gain.len(), 12_528);
    let (peak, outcome) = peak_bytes(&gain);
    assert_eq!(outcome, Ok(()));
    let held = 8 * ((1u64 << 20) + 256);
    let scratch = 8 * (1u64 << 20);
    assert!(
        peak <= held + scratch + LINEAR_PEAK_RATIO * 12_528,
        "peak {peak}"
    );
    assert!(peak <= documented_peak_bound(gain.len()));
}

#[test]
fn blocks_holding_exactly_the_constructed_budget_decode_and_one_more_block_is_refused() {
    // Four tables of 524,287 rows report 1,048,576 words each: exactly the budget.
    let at_budget = "table_524287_2";
    assert_eq!(4 * time_table_words(524_287), 1 << 22);
    let bytes = time_tables(&[at_budget; 4]).to_bytes();
    let (peak, outcome) = peak_bytes(&bytes);
    assert_eq!(outcome, Ok(()));
    assert!(
        peak <= 8 * (1 << 22) + LINEAR_PEAK_RATIO * bytes.len() as u64,
        "peak {peak}"
    );

    // A fifth block that holds anything at all crosses it: the smallest table is four words.
    assert_eq!(time_table_words(1), 4);
    let bytes = time_tables(&[at_budget, at_budget, at_budget, at_budget, "table_1_2"]).to_bytes();
    let (peak, outcome) = peak_bytes(&bytes);
    assert_eq!(
        outcome,
        Err(BlobError::ConstructedTooLarge {
            block: 4,
            requested: (1 << 22) + 4
        })
    );
    assert!(
        peak <= 8 * ((1 << 22) + 4) + LINEAR_PEAK_RATIO * bytes.len() as u64,
        "peak {peak}"
    );

    // The refused block need not be the last: the first to cross is named, in id order.
    let bytes = time_tables(&[at_budget, at_budget, "table_1_2", at_budget, at_budget]).to_bytes();
    assert_eq!(
        peak_bytes(&bytes).1,
        Err(BlobError::ConstructedTooLarge {
            block: 4,
            requested: (1 << 22) + 4
        })
    );
}

#[test]
fn a_blob_that_spends_the_constructed_and_the_state_budget_in_full_decodes_within_their_sum() {
    // Four tables at the constructed budget beside one moving mean whose window owns the whole
    // state budget: `n` sample words and four fixed ones.
    let window = i64::try_from(oce_graph::MAX_STATE_WORDS).unwrap() - 4;
    let mut image = time_tables(&["table_524287_2"; 4]);
    let mean = blob_models::moving_mean(window);
    image.classes = vec![mean.classes[0].clone(), class(REAL_TIME_TABLE)];
    for instance in &mut image.instances {
        instance.class = 1;
    }
    let mut instance = mean.instances[0].clone();
    instance.inputs = vec![4, 5];
    instance.outputs = vec![6];
    image.instances.push(instance);
    image.connectors = 7;
    image.schedule = (0..5).collect();
    image.inputs = mean.inputs.clone();
    image.inputs[0].targets = vec![5];
    image.inputs[1].targets = vec![4];
    let bytes = image.to_bytes();
    let (peak, outcome) = peak_bytes(&bytes);
    assert_eq!(outcome, Ok(()));
    let both = 8 * (oce_graph::MAX_CONSTRUCTED_WORDS + oce_graph::MAX_STATE_WORDS) as u64;
    assert!(peak >= both, "both budgets are held: peak {peak}");
    assert!(
        peak <= both + LINEAR_PEAK_RATIO * bytes.len() as u64,
        "peak {peak}"
    );
    assert!(peak <= documented_peak_bound(bytes.len()));
}
