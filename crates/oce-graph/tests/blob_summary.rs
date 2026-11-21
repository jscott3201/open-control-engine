//! The blob summary reader over every checked-in golden (issue #448): `inspect` states what
//! `decode` builds from the same bytes, reports a class signature hash and an initial-state CRC
//! that `decode` refuses, and refuses what the decoder's structural pass refuses.
//!
//! This file needs only the core crates, so it also runs against the `no_std + alloc` build
//! (`cargo test -p oce-graph --no-default-features --features alloc --test blob_summary`). The
//! hostile-byte, mutation and allocation suites are `oce-api`'s `tests/blob_summary*.rs`.

use std::path::PathBuf;

use oce_blocks::{BlockKind, class_signature_hash};
use oce_graph::blob::{
    BlobError, ENGINE_VERSION, FORMAT_VERSION, Summary, crc32c, decode, inspect,
};

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

/// Replace the CRC-32C trailer after editing bytes in place.
fn rechecksum(blob: &mut [u8]) {
    let body = blob.len() - 4;
    let crc = crc32c(&blob[..body]);
    blob[body..].copy_from_slice(&crc.to_le_bytes());
}

/// The CRC-32C of state words, each as its eight little-endian bytes.
fn words_crc(words: &[u64]) -> u32 {
    let bytes: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
    crc32c(&bytes)
}

#[test]
fn every_golden_summary_states_what_the_decoder_builds() {
    let goldens = goldens();
    assert_eq!(goldens.len(), 46);
    for (name, bytes) in &goldens {
        let decoded = decode(bytes).unwrap_or_else(|err| panic!("{name}: {err:?}"));
        let summary = inspect(bytes).unwrap_or_else(|err| panic!("{name}: {err:?}"));
        assert_eq!(summary, inspect(bytes).unwrap(), "{name}: read twice");

        assert_eq!(summary.format_version, FORMAT_VERSION, "{name}");
        assert_eq!(summary.engine_version, ENGINE_VERSION, "{name}");
        assert_eq!(summary.content_id, decoded.content_id, "{name}");
        assert_eq!(summary.target_bound, decoded.target_bound, "{name}");

        // The class table is the sorted set of the classes the blocks use, each with this
        // build's hash, because the decoder accepted the blob.
        let mut paths: Vec<&str> = decoded
            .model
            .blocks
            .iter()
            .map(|block| block.class_iri.as_ref())
            .collect();
        paths.sort_unstable();
        paths.dedup();
        let table: Vec<(&str, u64)> = summary
            .classes
            .iter()
            .map(|class| (class.class_path, class.signature_hash))
            .collect();
        let expected: Vec<(&str, u64)> = paths
            .iter()
            .map(|path| (*path, class_signature_hash(path).unwrap()))
            .collect();
        assert_eq!(table, expected, "{name}");

        // One instance per block, in block id order, with the constructed block's kind and
        // state length and the CRC of the words the decoder allocated for it.
        assert_eq!(summary.instances.len(), decoded.blocks.len(), "{name}");
        let mut regions = vec![None; decoded.blocks.len()];
        for slot in &decoded.state.slots {
            regions[slot.block.0 as usize] =
                Some(&decoded.state.words[slot.offset..slot.offset + slot.len]);
        }
        for (index, instance) in summary.instances.iter().enumerate() {
            let block = &decoded.blocks[index];
            let what = format!("{name}: block {index}");
            assert_eq!(instance.block as usize, index, "{what}");
            assert_eq!(
                summary.classes[instance.class as usize].class_path,
                decoded.model.blocks[index].class_iri.as_ref(),
                "{what}"
            );
            assert_eq!(
                instance.stateful,
                block.kind() == BlockKind::Stateful,
                "{what}"
            );
            assert_eq!(instance.state_words as usize, block.state_len(), "{what}");
            assert_eq!(
                instance.initial_state_crc,
                regions[index].map_or(0, words_crc),
                "{what}"
            );
        }

        let inputs: Vec<&str> = decoded.inputs.iter().map(|input| input.path).collect();
        let outputs: Vec<&str> = decoded.outputs.iter().map(|output| output.path).collect();
        assert_eq!(summary.inputs, inputs, "{name}");
        assert_eq!(summary.outputs, outputs, "{name}");
    }
}

/// The byte offset of the first class's signature hash: the body starts with section 1's id and
/// length, the class count, then the first path's length and bytes.
fn first_hash_offset(summary: &Summary<'_>) -> usize {
    52 + 4 + 4 + 4 + 4 + summary.classes[0].class_path.len()
}

#[test]
fn a_changed_class_signature_hash_is_reported_where_the_decoder_refuses_the_blob() {
    for (name, bytes) in &goldens() {
        let original = inspect(bytes).unwrap();
        let at = first_hash_offset(&original);
        let hash = original.classes[0].signature_hash;
        assert_eq!(bytes[at..at + 8], hash.to_le_bytes(), "{name}");

        let mut changed = bytes.clone();
        changed[at] ^= 1;
        rechecksum(&mut changed);
        assert_eq!(
            decode(&changed).err(),
            Some(BlobError::ClassSignatureMismatch {
                class: 0,
                expected: hash,
                found: hash ^ 1
            }),
            "{name}"
        );
        let summary = inspect(&changed).unwrap_or_else(|err| panic!("{name}: {err:?}"));
        assert_eq!(summary.classes[0].signature_hash, hash ^ 1, "{name}");
        let mut expected = original.clone();
        expected.classes[0].signature_hash = hash ^ 1;
        assert_eq!(summary, expected, "{name}: nothing else moved");
    }
}

#[test]
fn bytes_the_structural_pass_refuses_have_no_summary() {
    let (_, bytes) = goldens().swap_remove(0);
    // Every prefix, the decoder's own refusal.
    for length in 0..bytes.len() {
        let refusal = inspect(&bytes[..length]).err();
        assert!(
            matches!(refusal, Some(BlobError::Malformed { .. })),
            "prefix {length}: {refusal:?}"
        );
        assert_eq!(refusal, decode(&bytes[..length]).err(), "prefix {length}");
    }
    // A flipped byte without a new trailer.
    let mut flipped = bytes.clone();
    flipped[60] ^= 1;
    assert!(matches!(
        inspect(&flipped),
        Err(BlobError::IntegrityMismatch { .. })
    ));
    // Another engine version, whatever else the bytes hold.
    let mut other = bytes.clone();
    other[8] ^= 1;
    assert!(matches!(
        inspect(&other),
        Err(BlobError::EngineVersionMismatch { expected, .. }) if expected == ENGINE_VERSION
    ));
    // The one class path of a one-class blob made unregistered: the decoder's own refusal.
    let (_, one_class) = goldens()
        .into_iter()
        .find(|(name, _)| name == "generic_air_economizer_high_limits_ashrae_differential")
        .unwrap();
    let original = inspect(&one_class).unwrap();
    assert_eq!(original.classes.len(), 1);
    let at = first_hash_offset(&original) - 1;
    let mut unknown = one_class.clone();
    unknown[at] = b'~';
    rechecksum(&mut unknown);
    let refusal = Some(BlobError::UnknownClass { class: 0 });
    assert_eq!(inspect(&unknown).err(), refusal);
    assert_eq!(decode(&unknown).err(), refusal);
}
