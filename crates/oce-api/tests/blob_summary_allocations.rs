//! What reading a blob summary allocates (issue #448): nothing for any refusal, by count for
//! every golden, and by peak bytes against the blob length for the hostile shapes that make each
//! list as long as its bytes allow.
//!
//! `oce_graph::blob::inspect` documents that every check runs before anything is allocated, four
//! allocations at most for a summary (one per non-empty list) and at most 2 bytes held per blob
//! byte; `oce_api::inspect_blob` documents at most 4 requested bytes. Peaks are
//! `allocation_counter`'s `bytes_max` divided by the blob length, as in
//! `blob_peak_allocation.rs`: bytes requested of the allocator, not its per-allocation overhead.

// Shared helpers, declared once for this binary in its root `facade_allocation_census.rs`.
use crate::blob_corpus;
use crate::blob_image;

use allocation_counter::measure;
use blob_corpus::{FIXTURES, golden};
use blob_image::{Image, Inst, Out};
use oce_api::{BlobError, InstanceField, OcError, inspect_blob};
use oce_graph::blob::inspect;

/// The core reader's documented bound on bytes held per blob byte.
const CORE_PEAK_RATIO: f64 = 2.0;

/// The facade's documented bound: the core's lists and the owned copies, held together.
const FACADE_PEAK_RATIO: f64 = 4.0;

fn some(n: usize) -> u64 {
    u64::from(n > 0)
}

/// Peak bytes held while the core reads `bytes`, as a ratio of the blob length, and the refusal.
fn core_peak(bytes: &[u8]) -> (f64, Option<BlobError>) {
    let mut refusal = None;
    let info = measure(|| {
        refusal = inspect(bytes).err();
    });
    (info.bytes_max as f64 / bytes.len() as f64, refusal)
}

/// The same for the facade's owned summary.
fn facade_peak(bytes: &[u8]) -> (f64, Option<BlobError>) {
    let mut refusal = None;
    let info = measure(|| {
        refusal = match inspect_blob(bytes) {
            Ok(_) => None,
            Err(OcError::Blob(error)) => Some(error),
            Err(other) => panic!("a refusal that is not OcError::Blob: {other:?}"),
        };
    });
    (info.bytes_max as f64 / bytes.len() as f64, refusal)
}

#[test]
fn the_core_reader_allocates_one_vector_per_non_empty_list_for_every_golden() {
    for name in FIXTURES {
        let bytes = golden(name);
        let mut lists = 0;
        let info = measure(|| {
            let summary = inspect(&bytes).unwrap();
            lists = some(summary.classes.len())
                + some(summary.instances.len())
                + some(summary.inputs.len())
                + some(summary.outputs.len());
        });
        assert_eq!(info.count_total, lists, "{name}");
        assert!(
            info.bytes_max as f64 <= CORE_PEAK_RATIO * bytes.len() as f64,
            "{name}"
        );
    }
}

#[test]
fn the_facade_summary_of_every_golden_stays_within_four_times_the_blob_length() {
    for name in FIXTURES {
        let bytes = golden(name);
        let (ratio, refusal) = facade_peak(&bytes);
        assert_eq!(refusal, None, "{name}");
        assert!(ratio <= FACADE_PEAK_RATIO, "{name}: peak {ratio:.2}x");
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
/// and an empty payload. The structural pass accepts a repeated output path and the summary
/// checks no source, so this blob summarises; it is the shape that holds the most per blob byte.
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
    let (ratio, refusal) = core_peak(&bytes);
    assert_eq!(refusal, None);
    assert!(ratio <= CORE_PEAK_RATIO, "core peak {ratio:.2}x");
    let (ratio, refusal) = facade_peak(&bytes);
    assert_eq!(refusal, None);
    assert!(ratio <= FACADE_PEAK_RATIO, "facade peak {ratio:.2}x");
    assert_eq!(inspect_blob(&bytes).unwrap().outputs.len(), count as usize);
}

#[test]
fn the_longest_instance_list_stays_within_the_documented_ratios() {
    let count = 100_000;
    let bytes = minimal_instances(count).to_bytes();
    let (ratio, refusal) = core_peak(&bytes);
    assert_eq!(refusal, None);
    assert!(ratio <= CORE_PEAK_RATIO, "core peak {ratio:.2}x");
    let (ratio, refusal) = facade_peak(&bytes);
    assert_eq!(refusal, None);
    assert!(ratio <= FACADE_PEAK_RATIO, "facade peak {ratio:.2}x");
    assert_eq!(inspect_blob(&bytes).unwrap().instances.len(), count);
}
