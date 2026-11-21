//! What reading a blob summary allocates (issues #448 and #680): nothing for any refusal, and
//! for a summary what the decoder allocates for the same bytes, freed, and then the lists.
//!
//! `oce_graph::blob::inspect` documents that every check runs before anything is allocated; that
//! a summary then costs the decoder's building pass (exactly `decode`'s allocations for the same
//! bytes, all freed) and after it one allocation per non-empty list, at most 2 bytes held per
//! blob byte; and that the two are never held together, so the peak is the larger of them.
//! `oce_api::inspect_blob` documents the same with at most 4 requested bytes per blob byte for
//! its owned result. Peaks are `allocation_counter`'s `bytes_max`, as in
//! `blob_peak_allocation.rs`: bytes requested of the allocator, not its per-allocation overhead.
//! Every expected count and peak here is an equality against a measurement of `decode` on the
//! same bytes, so an allocation the documentation does not list fails a test.

// Shared helpers, declared once for this binary in its root `facade_allocation_census.rs`.
use crate::blob_corpus;
use crate::blob_image;
use crate::blob_models;
use crate::blob_peak_allocation::{
    WIDE_ACCEPTED_PEAK_RATIO, constant_with_bogus_inputs, wide_accepted_block,
};

use std::mem::size_of;

use allocation_counter::measure;
use blob_corpus::{FIXTURES, golden};
use blob_image::{Image, Inst, Out};
use blob_models::{chain, moving_mean, time_tables};
use oce_api::{BlobError, InstanceField, OcError, inspect_blob};
use oce_graph::blob::{Summary, SummaryClass, SummaryInstance, decode, inspect};
use oce_graph::{MAX_CONSTRUCTED_WORDS, MAX_STATE_WORDS};

/// The core reader's documented bound on the bytes its four lists hold per blob byte.
const CORE_LISTS_RATIO: f64 = 2.0;

/// The facade's documented bound on its result: the core's lists and the owned copies, held
/// together.
const FACADE_RESULT_RATIO: f64 = 4.0;

fn some(n: usize) -> u64 {
    u64::from(n > 0)
}

/// What `decode` costs for `bytes`: its allocation count and its peak bytes, whether it returns
/// an executable or a refusal. This is the building pass's share of a summary.
fn decoder_cost(bytes: &[u8]) -> (u64, u64) {
    let info = measure(|| drop(decode(bytes)));
    (info.count_total, info.bytes_max)
}

/// The allocation count and bytes of the four lists of `summary`, each allocated once at its
/// exact length.
fn lists_cost(summary: &Summary<'_>) -> (u64, u64) {
    let count = some(summary.classes.len())
        + some(summary.instances.len())
        + some(summary.inputs.len())
        + some(summary.outputs.len());
    let bytes = summary.classes.len() * size_of::<SummaryClass<'_>>()
        + summary.instances.len() * size_of::<SummaryInstance>()
        + (summary.inputs.len() + summary.outputs.len()) * size_of::<&str>();
    (count, bytes as u64)
}

/// What the core reader costs for `bytes`, which it summarises, checked against the documented
/// cost exactly: the decoder's allocations and then one per non-empty list, and a peak that is
/// the larger of the decoder's and the lists', never their sum. Returns the decoder's peak and
/// the lists' bytes as ratios of the blob length, and the summary's verdict.
fn core_cost_is_the_decoders_then_the_lists(
    bytes: &[u8],
    what: &str,
) -> (f64, f64, Option<BlobError>) {
    let (decoder_count, decoder_peak) = decoder_cost(bytes);
    let mut lists = (0, 0);
    let mut refusal = None;
    let info = measure(|| {
        let summary = inspect(bytes).unwrap();
        lists = lists_cost(&summary);
        refusal = summary.decode_refusal;
    });
    assert_eq!(info.count_total, decoder_count + lists.0, "{what}: count");
    assert_eq!(info.bytes_max, decoder_peak.max(lists.1), "{what}: peak");
    assert_eq!(refusal, decode(bytes).err(), "{what}: verdict");
    let length = bytes.len() as f64;
    assert!(
        lists.1 as f64 <= CORE_LISTS_RATIO * length,
        "{what}: the lists hold {:.2}x the blob",
        lists.1 as f64 / length
    );
    (
        decoder_peak as f64 / length,
        lists.1 as f64 / length,
        refusal,
    )
}

/// Peak bytes held while the facade summarises `bytes`, as a ratio of the blob length, checked
/// against the documented bound: the larger of the decoder's peak and 4 bytes per blob byte.
fn facade_peak_is_within_the_decoders_or_the_result_bound(bytes: &[u8], what: &str) -> f64 {
    let (_, decoder_peak) = decoder_cost(bytes);
    let info = measure(|| {
        inspect_blob(bytes).unwrap();
    });
    let length = bytes.len() as f64;
    let bound = (decoder_peak as f64).max(FACADE_RESULT_RATIO * length);
    assert!(
        info.bytes_max as f64 <= bound,
        "{what}: facade peak {:.2}x, decoder peak {:.2}x",
        info.bytes_max as f64 / length,
        decoder_peak as f64 / length
    );
    info.bytes_max as f64 / length
}

#[test]
fn a_summary_of_every_golden_costs_the_decoders_allocations_then_one_per_non_empty_list() {
    let positive = measure(|| drop(std::hint::black_box(vec![0_u8; 64])));
    assert_eq!(positive.count_total, 1, "the counter observes allocations");
    for name in FIXTURES {
        let bytes = golden(name);
        let (decoder, lists, refusal) = core_cost_is_the_decoders_then_the_lists(&bytes, name);
        assert_eq!(refusal, None, "{name}");
        // For a real executable the decoder's share is the larger one.
        assert!(decoder > lists, "{name}: {decoder:.2}x against {lists:.2}x");
        facade_peak_is_within_the_decoders_or_the_result_bound(&bytes, name);
    }
}

#[test]
fn a_summary_of_a_blob_the_decoder_refuses_costs_what_the_decoder_spent_before_refusing() {
    // Refused at the first class's hash: the decoder allocates one default-parameter block for
    // the hash and stops, and the summary costs that and its lists.
    let mut early = chain();
    early.classes[0].1 ^= 1;
    // Refused at the schedule, the last check but the state: everything is built by then.
    let mut late = chain();
    late.schedule = vec![1, 0];
    // Refused at block 0's arity with a million-entry port list in hand.
    let hostile = constant_with_bogus_inputs(1_000_000);
    for (what, bytes) in [
        ("a wrong hash", early.to_bytes()),
        ("a wrong schedule", late.to_bytes()),
        ("a hostile port list", hostile),
    ] {
        let (_, _, refusal) = core_cost_is_the_decoders_then_the_lists(&bytes, what);
        assert!(refusal.is_some(), "{what}");
        facade_peak_is_within_the_decoders_or_the_result_bound(&bytes, what);
    }
}

#[test]
fn a_summary_of_a_wide_accepted_blob_peaks_where_the_decoder_peaks() {
    // A wide accepted shape of the decoder's (`blob_peak_allocation.rs`): the building pass holds
    // far more than the lists, and all of it is freed before they are allocated, so the summary
    // peaks exactly where `decode` does and within the decoder's recorded bound.
    let bytes = wide_accepted_block();
    let (decoder, lists, refusal) = core_cost_is_the_decoders_then_the_lists(&bytes, "wide");
    assert_eq!(refusal, None);
    assert!(
        decoder > 20.0 && decoder <= WIDE_ACCEPTED_PEAK_RATIO,
        "{decoder:.2}x"
    );
    assert!(lists < 0.01, "{lists:.4}x");
    let facade = facade_peak_is_within_the_decoders_or_the_result_bound(&bytes, "wide");
    assert!(facade <= WIDE_ACCEPTED_PEAK_RATIO, "{facade:.2}x");
}

#[test]
fn a_summary_of_a_blob_at_the_state_budget_holds_the_state_the_decoder_allocates() {
    // A blob of a few hundred bytes whose one block owns the whole state budget. The verdict is
    // the decoder's, so the summary allocates that state, checks its CRC and frees it: its peak
    // is the decoder's, bounded by the budget and not by the blob length.
    let overhead = moving_mean(1).instances[0].state_len as usize - 1;
    let bytes = moving_mean(i64::try_from(MAX_STATE_WORDS - overhead).unwrap()).to_bytes();
    assert!(bytes.len() < 1_000);
    let (_, _, refusal) = core_cost_is_the_decoders_then_the_lists(&bytes, "the state budget");
    assert_eq!(refusal, None);
    let (_, decoder_peak) = decoder_cost(&bytes);
    let info = measure(|| drop(inspect(&bytes)));
    assert_eq!(info.bytes_max, decoder_peak);
    let words = 8 * MAX_STATE_WORDS as u64;
    assert!(info.bytes_max >= words, "the state words were allocated");
    assert!(
        info.bytes_max <= words + 64 * 1024,
        "peak {} bytes against {words} of state",
        info.bytes_max
    );
    // Nothing of it outlives the call.
    assert_eq!(info.bytes_current, 0);
}

#[test]
fn a_summary_of_blobs_at_and_past_the_constructed_budget_holds_what_the_decoder_holds() {
    // Blobs of a few hundred bytes whose blocks hold megabytes by one parameter name each
    // (issue #700). The verdict is the decoder's, so the summary constructs the blocks, holds
    // them to the budget and frees them: its peak is the decoder's, bounded by the budget.
    let budget = 8 * MAX_CONSTRUCTED_WORDS as u64;
    let at_budget = time_tables(&["table_524287_2"; 4]).to_bytes();
    assert_eq!(at_budget.len(), 403);
    let past = time_tables(&["table_524288_2"; 64]).to_bytes();
    assert_eq!(past.len(), 4_003);
    for (what, bytes, verdict, lists_bytes) in [
        ("at the constructed budget", &at_budget, None, 104),
        (
            "past the constructed budget",
            &past,
            Some(BlobError::ConstructedTooLarge {
                block: 3,
                requested: MAX_CONSTRUCTED_WORDS as u64 + 8,
            }),
            1_304,
        ),
    ] {
        let (_, _, refusal) = core_cost_is_the_decoders_then_the_lists(bytes, what);
        assert_eq!(refusal, verdict, "{what}");
        let (_, decoder_peak) = decoder_cost(bytes);
        let mut lists = (0, 0);
        let info = measure(|| lists = lists_cost(&inspect(bytes).unwrap()));
        assert_eq!(lists.1, lists_bytes, "{what}: the lists");
        assert_eq!(info.bytes_max, decoder_peak, "{what}");
        assert!(
            info.bytes_max >= budget - 64,
            "{what}: the tables were held"
        );
        assert!(
            info.bytes_max <= budget + 64 + 64 * bytes.len() as u64,
            "{what}: peak {} bytes against a budget of {budget}",
            info.bytes_max
        );
        // Nothing of it outlives the call.
        assert_eq!(info.bytes_current, 0, "{what}");
        facade_peak_is_within_the_decoders_or_the_result_bound(bytes, what);
        let summary = inspect_blob(bytes).unwrap();
        assert_eq!(
            summary.decode_refusal, verdict,
            "{what}: the facade's verdict"
        );
    }
}

/// Zero allocations in the core reader and in the facade for `bytes`, which both refuse with
/// `refusal`.
fn refused_without_allocating(bytes: &[u8], refusal: Option<BlobError>, what: &str) {
    let mut found = None;
    let core = measure(|| found = inspect(bytes).err());
    assert!(found.is_some(), "{what}: the core reader summarised");
    if refusal.is_some() {
        assert_eq!(found, refusal, "{what}");
    }
    assert_eq!(core.count_total, 0, "{what}: the core reader allocated");
    let expected = found;
    let facade = measure(|| {
        found = match inspect_blob(bytes) {
            Ok(_) => None,
            Err(OcError::Blob(error)) => Some(error),
            Err(other) => panic!("a refusal that is not OcError::Blob: {other:?}"),
        };
    });
    assert_eq!(found, expected, "{what}: the facade's refusal");
    assert_eq!(facade.count_total, 0, "{what}: the facade allocated");
}

#[test]
fn a_refusal_of_the_framing_or_the_structural_pass_allocates_nothing() {
    let bytes = golden(FIXTURES[0]);
    refused_without_allocating(&[], None, "no bytes");
    refused_without_allocating(b"OCEB", None, "the magic alone");
    refused_without_allocating(&bytes[..60], None, "a 60-byte prefix");
    // A flipped body byte: refused at the CRC.
    let mut flipped = bytes.clone();
    flipped[100] ^= 1;
    refused_without_allocating(&flipped, None, "a flipped byte");
    // A trailing byte in the last section: refused by the structural pass.
    let mut image = Image::from_decoded(&oce_graph::blob::decode(&bytes).unwrap());
    image.section_tail[4] = vec![0];
    refused_without_allocating(&image.to_bytes(), None, "a trailing byte");
}

#[test]
fn a_hostile_class_table_is_refused_before_anything_is_sized_by_its_count() {
    // 200,000 distinct three-letter paths in canonical order, none registered: 3 MB of class
    // table whose proven count would size a 4.8 MB list. It is refused at its first entry with
    // nothing allocated.
    let count = 200_000u32;
    let letter = |digit: u32| b'A' + u8::try_from(digit % 59).unwrap();
    let mut image = Image::new();
    image.classes = (0..count)
        .map(|n| (vec![letter(n / 59 / 59), letter(n / 59), letter(n)], 0))
        .collect();
    let bytes = image.to_bytes();
    assert!(bytes.len() >= 15 * count as usize);
    refused_without_allocating(
        &bytes,
        Some(BlobError::UnknownClass { class: 0 }),
        "200,000 unregistered classes",
    );

    // The unregistered class last in a real table, behind every registered one.
    let golden_bytes = golden(FIXTURES[0]);
    let mut image = Image::from_decoded(&oce_graph::blob::decode(&golden_bytes).unwrap());
    let last = image.classes.len() - 1;
    assert!(last >= 1);
    image.classes[last].0.push(b'~');
    refused_without_allocating(
        &image.to_bytes(),
        Some(BlobError::UnknownClass {
            class: u32::try_from(last).unwrap(),
        }),
        "the last class unregistered",
    );
}

/// `count` instances of 25 bytes each, the shortest record: no ports, no parameters, no state.
fn minimal_instances(count: usize) -> Image {
    let mut image = Image::new();
    image.classes = vec![(
        b"CDL.Reals.Sources.Constant".to_vec(),
        oce_blocks::class_signature_hash("CDL.Reals.Sources.Constant").unwrap(),
    )];
    image.instances = (0..count)
        .map(|_| Inst {
            class: 0,
            kind: 0,
            state_len: 0,
            state_crc: 0,
            inputs: Vec::new(),
            outputs: Vec::new(),
            params: Vec::new(),
        })
        .collect();
    image
}

#[test]
fn a_class_index_outside_the_table_on_the_last_of_many_blocks_allocates_nothing() {
    // 100,000 instances, every class index in range but the last block's: the instance list is
    // not allocated until every index has been checked.
    let count = 100_000;
    let mut image = minimal_instances(count);
    image.instances[count - 1].class = 1;
    refused_without_allocating(
        &image.to_bytes(),
        Some(BlobError::InstanceMismatch {
            block: u32::try_from(count - 1).unwrap(),
            field: InstanceField::ClassIndex,
        }),
        "the last block's class index",
    );
}

/// The longest output list its bytes allow: every record is 12 bytes, an empty path, a source id
/// and an empty payload. The structural pass accepts a repeated output path and no source is
/// checked for the lists, so this blob summarises; it is the shape whose lists hold the most per
/// blob byte. The decoder refuses it at its first output, having sized its own output table.
fn outputs_with_empty_paths(count: u32) -> Vec<u8> {
    let mut image = Image::new();
    image.outputs = (0..count)
        .map(|_| Out {
            path: Vec::new(),
            source: 0,
            payload: Vec::new(),
        })
        .collect();
    image.to_bytes()
}

#[test]
fn the_longest_output_list_stays_within_the_documented_ratios() {
    let count = 500_000;
    let bytes = outputs_with_empty_paths(count);
    assert!(bytes.len() >= 12 * count as usize);
    let (decoder, lists, refusal) = core_cost_is_the_decoders_then_the_lists(&bytes, "outputs");
    assert!(matches!(
        refusal,
        Some(BlobError::InvalidOutput { output: 0, .. })
    ));
    // 16 bytes of list per 12-byte record; the decoder's own table of the same count is larger.
    assert!(lists > 1.3 && lists <= CORE_LISTS_RATIO, "{lists:.2}x");
    assert!(decoder > lists && decoder <= 4.0, "{decoder:.2}x");
    let facade = facade_peak_is_within_the_decoders_or_the_result_bound(&bytes, "outputs");
    assert!(facade <= FACADE_RESULT_RATIO, "{facade:.2}x");
    assert_eq!(inspect_blob(&bytes).unwrap().outputs.len(), count as usize);
}

#[test]
fn the_longest_instance_list_stays_within_the_documented_ratios() {
    let count = 100_000;
    let bytes = minimal_instances(count).to_bytes();
    let (decoder, lists, refusal) = core_cost_is_the_decoders_then_the_lists(&bytes, "instances");
    assert_eq!(
        refusal,
        Some(BlobError::InstanceMismatch {
            block: 0,
            field: InstanceField::OutputArity
        })
    );
    assert!(lists > 0.7 && lists <= CORE_LISTS_RATIO, "{lists:.2}x");
    assert!(decoder > lists && decoder <= 8.0, "{decoder:.2}x");
    facade_peak_is_within_the_decoders_or_the_result_bound(&bytes, "instances");
    assert_eq!(inspect_blob(&bytes).unwrap().instances.len(), count);
}
