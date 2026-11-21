//! What reading a blob summary allocates (issue #448), by count for every golden and by peak
//! bytes against the blob length for the hostile shapes that make each list as long as its bytes
//! allow.
//!
//! `oce_graph::blob::inspect` documents four allocations at most, one per non-empty list, and at
//! most 2 bytes held per blob byte; `oce_api::inspect_blob` documents at most 4. Peaks are
//! `allocation_counter`'s `bytes_max` divided by the blob length, as in `blob_peak_allocation.rs`.

// Shared helpers, declared once for this binary in its root `facade_allocation_census.rs`.
use crate::blob_corpus;
use crate::blob_image;

use allocation_counter::measure;
use blob_corpus::{FIXTURES, golden};
use blob_image::{Image, Inst, Out};
use oce_api::{BlobError, OcError, inspect_blob};
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

#[test]
fn a_refusal_before_the_structural_pass_ends_allocates_nothing() {
    let bytes = golden(FIXTURES[0]);
    let mut cases: Vec<Vec<u8>> = vec![Vec::new(), b"OCEB".to_vec(), bytes[..60].to_vec()];
    // A flipped body byte: refused at the CRC.
    let mut flipped = bytes.clone();
    flipped[100] ^= 1;
    cases.push(flipped);
    // A trailing byte in the last section: refused by the structural pass.
    let mut image = Image::from_decoded(&oce_graph::blob::decode(&bytes).unwrap());
    image.section_tail[4] = vec![0];
    cases.push(image.to_bytes());
    for (index, case) in cases.iter().enumerate() {
        let mut refused = false;
        let core = measure(|| refused = inspect(case).is_err());
        assert!(refused, "case {index}");
        assert_eq!(core.count_total, 0, "case {index}: the core reader");
        let facade = measure(|| refused = inspect_blob(case).is_err());
        assert!(refused, "case {index}");
        assert_eq!(facade.count_total, 0, "case {index}: the facade");
    }
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
fn the_longest_class_table_is_refused_within_the_documented_ratios() {
    // 200,000 distinct three-letter paths in canonical order, none registered: the table is
    // sized by its proven count, then refused at its first entry.
    let count = 200_000u32;
    let letter = |digit: u32| b'A' + u8::try_from(digit % 59).unwrap();
    let mut image = Image::new();
    image.classes = (0..count)
        .map(|n| (vec![letter(n / 59 / 59), letter(n / 59), letter(n)], 0))
        .collect();
    let bytes = image.to_bytes();
    assert!(bytes.len() >= 15 * count as usize);
    let refusal = Some(BlobError::UnknownClass { class: 0 });
    let (ratio, found) = core_peak(&bytes);
    assert_eq!(found, refusal);
    assert!(ratio <= CORE_PEAK_RATIO, "core peak {ratio:.2}x");
    let (ratio, found) = facade_peak(&bytes);
    assert_eq!(found, refusal);
    assert!(ratio <= FACADE_PEAK_RATIO, "facade peak {ratio:.2}x");
}

#[test]
fn the_longest_instance_list_stays_within_the_documented_ratios() {
    // 100,000 instances of 25 bytes each: no ports, no parameters, no state.
    let count = 100_000;
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
    let bytes = image.to_bytes();
    let (ratio, refusal) = core_peak(&bytes);
    assert_eq!(refusal, None);
    assert!(ratio <= CORE_PEAK_RATIO, "core peak {ratio:.2}x");
    let (ratio, refusal) = facade_peak(&bytes);
    assert_eq!(refusal, None);
    assert!(ratio <= FACADE_PEAK_RATIO, "facade peak {ratio:.2}x");
    assert_eq!(inspect_blob(&bytes).unwrap().instances.len(), count);
}
