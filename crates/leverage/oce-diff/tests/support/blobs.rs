//! Helpers for the blob comparison's tests: the facade's checked-in blobs and blob summaries read
//! in place, blobs compiled from authoring text, and the few edits a test makes to a blob.
//!
//! Nothing here parses a blob. An edit finds the bytes it changes by searching for what the
//! facade's summary says the blob records (a class path followed by its hash, the content id's
//! 32 digest bytes, an initial-state CRC) and requires exactly one occurrence; it then rewrites
//! the CRC-32C trailer, which the format defines over every byte before it, so the edit reaches
//! the field instead of the integrity check.

use std::path::PathBuf;

use oce_api::{BlobSummary, inspect_blob};
use oce_diff::{BlobComparison, compare_blobs};

use super::loaded::{emitted, load};
use super::parse;

/// A path under the facade's test fixtures, read in place (`crates/oce-api/tests/fixtures`).
pub fn facade_fixture(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../oce-api/tests/fixtures")
        .join(relative)
}

fn read_bytes(path: &PathBuf) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// The facade's checked-in blob of a G36 catalog fixture (`tests/fixtures/blob/<name>.oceb`).
pub fn golden_blob(name: &str) -> Vec<u8> {
    read_bytes(&facade_fixture(&format!("blob/{name}.oceb")))
}

/// The facade's checked-in summary of that blob (`tests/fixtures/blob_summary/<name>.json`): what
/// `inspect_blob` reports for it, written by the facade's own test code.
pub fn golden_summary(name: &str) -> serde_json::Value {
    let path = facade_fixture(&format!("blob_summary/{name}.json"));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// The release-0.4.0 blob the facade keeps unblessed to prove that another engine version's blob
/// is refused (`tests/fixtures/blob_legacy.oceb`).
pub fn legacy_blob() -> Vec<u8> {
    read_bytes(&facade_fixture("blob_legacy.oceb"))
}

/// The blob of an authoring text, emitted, loaded and compiled through the facade.
pub fn compiled(text: &str) -> Vec<u8> {
    load(&emitted(&parse(text)))
        .engine
        .compile_blob()
        .unwrap_or_else(|error| panic!("the model compiles to a blob: {error}"))
        .bytes
}

/// What the facade reads from bytes it must read.
pub fn summary(blob: &[u8]) -> BlobSummary {
    inspect_blob(blob).unwrap_or_else(|error| panic!("the blob has a summary: {error}"))
}

/// The comparison of two blobs that must both be read.
pub fn compare(base: &[u8], head: &[u8]) -> BlobComparison {
    compare_blobs(base, head).unwrap_or_else(|error| panic!("the blobs compare: {error}"))
}

/// CRC-32C (Castagnoli, reflected polynomial `0x82F6_3B78`), restated from its definition.
pub fn crc32c(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0x82F6_3B78 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

/// Rewrite the trailer: the CRC-32C of every byte before it, little-endian.
pub fn reseal(blob: &mut [u8]) {
    let body = blob.len() - 4;
    let crc = crc32c(&blob[..body]);
    blob[body..].copy_from_slice(&crc.to_le_bytes());
}

/// The offsets at which `needle` starts in `haystack`.
pub fn occurrences(haystack: &[u8], needle: &[u8]) -> Vec<usize> {
    haystack
        .windows(needle.len())
        .enumerate()
        .filter(|(_, window)| *window == needle)
        .map(|(at, _)| at)
        .collect()
}

/// The one offset at which `needle` starts in `haystack`.
pub fn only_occurrence(haystack: &[u8], needle: &[u8], what: &str) -> usize {
    let found = occurrences(haystack, needle);
    assert_eq!(found.len(), 1, "{what} occurs once: {found:?}");
    found[0]
}

/// The blob with one bit of class `class`'s recorded signature hash flipped, resealed: what a
/// build of the same engine version with another record for that class would have written.
pub fn with_signature_flipped(blob: &[u8], class: usize, bit: u32) -> Vec<u8> {
    let recorded = &summary(blob).classes[class];
    let mut needle = recorded.class_path.as_bytes().to_vec();
    needle.extend_from_slice(&recorded.signature_hash.to_le_bytes());
    let at = only_occurrence(blob, &needle, &recorded.class_path) + recorded.class_path.len();
    let mut changed = blob.to_vec();
    let flipped = recorded.signature_hash ^ (1u64 << bit);
    changed[at..at + 8].copy_from_slice(&flipped.to_le_bytes());
    reseal(&mut changed);
    changed
}

/// The blob with its recorded content id cleared to the 32 zero bytes that mean "none",
/// resealed: what a hand-built model with no id records.
pub fn with_content_id_cleared(blob: &[u8]) -> Vec<u8> {
    let recorded = summary(blob)
        .content_id
        .expect("the blob records a content id");
    let digest = recorded.digest();
    let at = only_occurrence(blob, digest, "the content id's digest");
    let mut changed = blob.to_vec();
    changed[at..at + 32].fill(0);
    reseal(&mut changed);
    changed
}

/// The blob with one bit of block `block`'s recorded initial-state CRC flipped, resealed, or
/// `None` when the recorded CRC's four bytes occur more than once in the blob, so the field
/// cannot be found by its value alone.
pub fn with_initial_state_crc_flipped(blob: &[u8], block: usize) -> Option<Vec<u8>> {
    let recorded = summary(blob).instances[block].initial_state_crc;
    let found = occurrences(blob, &recorded.to_le_bytes());
    let [at] = found[..] else {
        return None;
    };
    let mut changed = blob.to_vec();
    changed[at] ^= 1;
    reseal(&mut changed);
    Some(changed)
}
