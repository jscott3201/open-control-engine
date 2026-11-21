//! The blob summary reader over every checked-in golden (issues #448 and #680): `inspect` states
//! what `decode` builds from the same bytes, reports a class signature hash that `decode` refuses
//! with that refusal beside it, refuses what the decoder's structural pass refuses, and carries
//! in `decode_refusal` exactly what `decode` returns for every one-bit change of a golden.
//!
//! This file needs only the core crates, so it also runs against the `no_std + alloc` build
//! (`cargo test -p oce-graph --no-default-features --features alloc --test blob_summary`). The
//! hostile-byte, mutation and allocation suites are `oce-api`'s `tests/blob_summary*.rs`.

use std::path::PathBuf;

use oce_blocks::{BlockKind, class_signature_hash};
use oce_graph::blob::{
    BlobError, ENGINE_VERSION, FORMAT_VERSION, InstanceField, Summary, crc32c, decode, inspect,
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
        // The decoder accepted the blob, and the summary says so.
        assert_eq!(summary.decode_refusal, None, "{name}");

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
        let refusal = Some(BlobError::ClassSignatureMismatch {
            class: 0,
            expected: hash,
            found: hash ^ 1,
        });
        assert_eq!(decode(&changed).err(), refusal, "{name}");
        let summary = inspect(&changed).unwrap_or_else(|err| panic!("{name}: {err:?}"));
        assert_eq!(summary.classes[0].signature_hash, hash ^ 1, "{name}");
        // The summary carries the decoder's refusal, where the original carried none.
        assert_eq!(original.decode_refusal, None, "{name}");
        assert_eq!(summary.decode_refusal, refusal, "{name}");
        let mut expected = original.clone();
        expected.classes[0].signature_hash = hash ^ 1;
        expected.decode_refusal = refusal;
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

#[test]
fn every_one_bit_change_of_a_golden_carries_the_verdict_the_decoder_gives() {
    // The smallest golden and the smallest with a stateful block: every bit of every byte after
    // the engine version, one at a time, with the trailer recomputed so the change reaches the
    // checks behind the CRC. Whenever the bytes have a summary, its `decode_refusal` is what
    // `decode` returns for them, `None` exactly when it accepts them.
    let goldens = goldens();
    let smallest = goldens.iter().min_by_key(|(_, bytes)| bytes.len()).unwrap();
    let stateful = goldens
        .iter()
        .filter(|(_, bytes)| {
            let summary = inspect(bytes).unwrap();
            summary.instances.iter().any(|instance| instance.stateful)
        })
        .min_by_key(|(_, bytes)| bytes.len())
        .unwrap();
    assert_ne!(smallest.0, stateful.0);
    let (mut accepted, mut refused, mut no_summary, mut state_refusals) = (0u32, 0u32, 0u32, 0u32);
    for (name, bytes) in [smallest, stateful] {
        for at in 14..bytes.len() - 4 {
            for bit in 0..8 {
                let mut changed = bytes.clone();
                changed[at] ^= 1 << bit;
                rechecksum(&mut changed);
                let what = format!("{name}: byte {at} bit {bit}");
                let decoder = decode(&changed).err();
                match inspect(&changed) {
                    Ok(summary) => {
                        assert_eq!(summary.decode_refusal, decoder, "{what}");
                        // The summary's own refusals are never carried as the decoder's.
                        assert!(
                            !matches!(
                                summary.decode_refusal,
                                Some(
                                    BlobError::UnknownClass { .. }
                                        | BlobError::Malformed { .. }
                                        | BlobError::NonzeroFlags { .. }
                                        | BlobError::InstanceMismatch {
                                            field: InstanceField::ClassIndex,
                                            ..
                                        }
                                )
                            ),
                            "{what}"
                        );
                        match decoder {
                            None => accepted += 1,
                            Some(BlobError::InitialStateMismatch { .. }) => {
                                state_refusals += 1;
                                refused += 1;
                            }
                            Some(_) => refused += 1,
                        }
                    }
                    Err(error) => {
                        // No summary: the decoder refuses the bytes too.
                        assert!(decoder.is_some(), "{what}: {error:?}");
                        no_summary += 1;
                    }
                }
            }
        }
    }
    // All three outcomes occur many times over, and the last decoder step is among the refusals,
    // so the comparison above is not vacuous. (A changed content-id bit or I/O path byte is
    // accepted: the decoder carries both without checking them.)
    assert!(accepted >= 256, "{accepted} accepted");
    assert!(refused >= 256, "{refused} refused with a summary");
    assert!(no_summary >= 256, "{no_summary} without a summary");
    assert!(state_refusals >= 32, "{state_refusals} state refusals");
}
