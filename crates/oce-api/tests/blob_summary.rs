//! What `oce_api::inspect_blob` and `oce_graph::blob::inspect` report about a compiled-schedule
//! blob (issue #448): the summary of every golden states what the decoded executable holds, and
//! a changed class signature hash, initial-state CRC, state length, kind, content id or I/O path
//! shows in the summary and nowhere else. Every golden's verdict is acceptance, and each change
//! the decoder refuses carries the decoder's refusal in `decode_refusal` (issue #680).
//!
//! The oracle is the test-side blob writer: `Facts::of_image` reads the lists a blob was written
//! from, and `Image::from_decoded` derives those lists from a decoded executable without the
//! production encoder. What a summary refuses, and what it reports for a blob the decoder refuses,
//! is in `blob_summary_acceptance.rs`; hostile bytes are in `blob_summary_hostile.rs`.

// Shared helpers, declared once for this binary in its root `facade.rs`.
use crate::blob_corpus;
use crate::blob_image;
use crate::blob_models;
use crate::blob_summary_facts;

use blob_corpus::{FIXTURES, compile, golden, load};
use blob_image::{Image, words_crc};
use blob_models::{adder, chain, delay};
use blob_summary_facts::{Facts, assert_invariants, summarise, summarise_with_refusal};
use oce_api::{BlobError, ENGINE_VERSION, InstanceField};
use oce_graph::blob::decode;

fn image_of(bytes: &[u8]) -> Image {
    Image::from_decoded(&decode(bytes).unwrap())
}

/// The smallest golden with at least three classes and at least two stateful and two stateless
/// instances, so a change to one entry can be told from a change to its neighbours.
fn rich() -> (String, Vec<u8>) {
    let mut best: Option<(String, Vec<u8>)> = None;
    for name in FIXTURES {
        let bytes = golden(name);
        let facts = summarise(&bytes).unwrap();
        let stateful = facts.instances.iter().filter(|i| i.2).count();
        let rich = facts.classes.len() >= 3
            && stateful >= 2
            && facts.instances.len() - stateful >= 2
            && facts.inputs.len() >= 2
            && facts.outputs.len() >= 2;
        if rich && best.as_ref().is_none_or(|(_, b)| bytes.len() < b.len()) {
            best = Some(((*name).to_owned(), bytes));
        }
    }
    best.expect("a rich fixture")
}

#[test]
fn every_golden_summary_states_what_the_decoded_executable_holds() {
    let (mut instances, mut stateful, mut classes) = (0usize, 0usize, 0usize);
    for name in FIXTURES {
        let bytes = golden(name);
        let decoded = decode(&bytes).unwrap_or_else(|err| panic!("{name}: {err}"));
        let (facts, refusal) =
            summarise_with_refusal(&bytes).unwrap_or_else(|err| panic!("{name}: {err:?}"));
        assert_invariants(&facts, name);
        // This build's decoder accepts every golden, and the summary says so.
        assert_eq!(refusal, None, "{name}");

        // The lists the blob is written from, derived without the production encoder.
        assert_eq!(
            facts,
            Facts::of_image(&Image::from_decoded(&decoded)),
            "{name}"
        );

        // The header fields, read straight from the bytes.
        let header = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
        assert_eq!(facts.format_version, header(4), "{name}");
        assert_eq!(
            facts.engine_version,
            [header(8), header(10), header(12)],
            "{name}"
        );
        assert_eq!(facts.content_id.unwrap(), bytes[20..52], "{name}");

        // The decoded executable itself: class per block, the flag, the I/O tables.
        assert_eq!(facts.target_bound, decoded.target_bound, "{name}");
        assert_eq!(facts.instances.len(), decoded.model.blocks.len(), "{name}");
        for (block, instance) in decoded.model.blocks.iter().zip(&facts.instances) {
            let (path, hash) = &facts.classes[instance.1 as usize];
            assert_eq!(path.as_str(), block.class_iri.as_ref(), "{name}");
            assert_eq!(
                Some(*hash),
                oce_blocks::class_signature_hash(path),
                "{name}: {path}"
            );
        }
        let paths = |list: Vec<&str>| list.into_iter().map(str::to_owned).collect::<Vec<_>>();
        assert_eq!(
            facts.inputs,
            paths(decoded.inputs.iter().map(|i| i.path).collect()),
            "{name}"
        );
        assert_eq!(
            facts.outputs,
            paths(decoded.outputs.iter().map(|o| o.path).collect()),
            "{name}"
        );

        // Each recorded CRC is the CRC of the words the decoder allocated for that block, and a
        // block without a state region records no words and a zero CRC.
        let mut regions = vec![None; facts.instances.len()];
        for slot in &decoded.state.slots {
            regions[slot.block.0 as usize] =
                Some(&decoded.state.words[slot.offset..slot.offset + slot.len]);
        }
        for (instance, region) in facts.instances.iter().zip(regions) {
            let (block, _, is_stateful, words, crc) = *instance;
            match region {
                Some(region) => {
                    assert!(is_stateful, "{name}: block {block}");
                    assert_eq!(words as usize, region.len(), "{name}: block {block}");
                    assert_eq!(crc, words_crc(region), "{name}: block {block}");
                }
                None => assert_eq!(
                    (is_stateful, words, crc),
                    (false, 0, 0),
                    "{name}: block {block}"
                ),
            }
        }
        instances += facts.instances.len();
        stateful += facts.instances.iter().filter(|i| i.2).count();
        classes += facts.classes.len();
    }
    // The corpus is not vacuous: the 46 goldens hold this many instances and class entries.
    assert_eq!((instances, stateful, classes), (1_204, 203, 406));
}

#[test]
fn the_summary_content_id_and_inputs_are_the_loaded_engines() {
    // The content-id pin: `<content id> TAB <source path>` for every corpus document.
    let pin = include_str!("fixtures/content_id/pin.tsv");
    for name in FIXTURES {
        let engine = load(name);
        let compiled = engine.compile_blob().unwrap();
        let facts = summarise(&compiled.bytes).unwrap();
        let id = engine.content_id().unwrap();
        assert_eq!(facts.content_id, Some(*id.digest()), "{name}");
        assert_eq!(compiled.content_id.as_ref(), Some(&id), "{name}");
        // The summary carries the id as the engine's own type, equal to the one it minted and
        // to the one the pin records for the document.
        let summary = oce_api::inspect_blob(&compiled.bytes).unwrap();
        assert_eq!(summary.content_id.as_ref(), Some(&id), "{name}");
        let pinned = pin
            .lines()
            .find_map(|line| {
                let (text, path) = line.split_once('\t')?;
                path.ends_with(&format!("/g36/{name}.jsonld"))
                    .then_some(text)
            })
            .unwrap_or_else(|| panic!("{name}: no pinned content id"));
        assert_eq!(summary.content_id.unwrap().as_str(), pinned, "{name}");
        let mut defined: Vec<String> = engine
            .input_definitions()
            .unwrap()
            .into_iter()
            .map(|definition| definition.path)
            .collect();
        defined.sort_unstable();
        assert_eq!(facts.inputs, defined, "{name}");
        // The engine's own blob and the checked-in golden summarise alike, and a blob
        // `compile_blob` returned is one this build accepts.
        let accepted = Ok((facts, None));
        assert_eq!(summarise_with_refusal(&compiled.bytes), accepted, "{name}");
        assert_eq!(
            summarise_with_refusal(&compile(name).bytes),
            accepted,
            "{name}"
        );
        assert_eq!(summarise_with_refusal(&golden(name)), accepted, "{name}");
    }
}

#[test]
fn a_changed_class_signature_hash_shows_in_the_summary_and_nowhere_else() {
    let (name, bytes) = rich();
    let image = image_of(&bytes);
    let original = summarise(&bytes).unwrap();
    for class in 0..image.classes.len() {
        for bit in 0..64 {
            let mut changed = image.clone();
            changed.classes[class].1 ^= 1 << bit;
            let hostile = changed.to_bytes();
            // The decoder refuses the blob: the hash is not this build's for the class.
            let refusal = Some(BlobError::ClassSignatureMismatch {
                class: u32::try_from(class).unwrap(),
                expected: image.classes[class].1,
                found: changed.classes[class].1,
            });
            assert_eq!(
                decode(&hostile).err(),
                refusal,
                "{name}: class {class} bit {bit}"
            );
            // The summary reports the hash the blob carries, in that entry only, and the
            // decoder's refusal beside it.
            let mut expected = original.clone();
            expected.classes[class].1 = changed.classes[class].1;
            assert_ne!(expected, original);
            assert_eq!(
                summarise_with_refusal(&hostile),
                Ok((expected, refusal)),
                "{name}: class {class} bit {bit}"
            );
        }
    }
}

#[test]
fn a_changed_initial_state_crc_shows_in_the_summary_and_nowhere_else() {
    let (name, bytes) = rich();
    let image = image_of(&bytes);
    let original = summarise(&bytes).unwrap();
    let (mut stateful, mut stateless) = (0, 0);
    for block in 0..image.instances.len() {
        for bit in 0..32 {
            let mut changed = image.clone();
            changed.instances[block].state_crc ^= 1 << bit;
            let hostile = changed.to_bytes();
            let id = u32::try_from(block).unwrap();
            // The decoder refuses the blob: at the state check for a stateful block, and at the
            // instance check for a block without state, whose CRC must be zero.
            let refusal = decode(&hostile).err().unwrap();
            if image.instances[block].kind == 1 {
                stateful += 1;
                assert_eq!(
                    refusal,
                    BlobError::InitialStateMismatch {
                        block: id,
                        expected: changed.instances[block].state_crc,
                        found: image.instances[block].state_crc,
                    },
                    "{name}: block {block} bit {bit}"
                );
            } else {
                stateless += 1;
                assert_eq!(
                    refusal,
                    BlobError::InstanceMismatch {
                        block: id,
                        field: InstanceField::StateCrc
                    },
                    "{name}: block {block} bit {bit}"
                );
            }
            let mut expected = original.clone();
            expected.instances[block].4 = changed.instances[block].state_crc;
            assert_ne!(expected, original);
            assert_eq!(
                summarise_with_refusal(&hostile),
                Ok((expected, Some(refusal))),
                "{name}: block {block} bit {bit}"
            );
        }
    }
    assert!(stateful >= 64 && stateless >= 64, "{stateful} {stateless}");
}

#[test]
fn a_changed_state_length_or_kind_shows_in_the_summary_and_nowhere_else() {
    let (name, bytes) = rich();
    let image = image_of(&bytes);
    let original = summarise(&bytes).unwrap();
    for block in 0..image.instances.len() {
        let id = u32::try_from(block).unwrap();
        for words in [0, 1, image.instances[block].state_len + 1, u32::MAX] {
            if words == image.instances[block].state_len {
                continue;
            }
            let mut changed = image.clone();
            changed.instances[block].state_len = words;
            let hostile = changed.to_bytes();
            let refusal = Some(BlobError::InstanceMismatch {
                block: id,
                field: InstanceField::StateLen,
            });
            assert_eq!(
                decode(&hostile).err(),
                refusal,
                "{name}: block {block} words {words}"
            );
            let mut expected = original.clone();
            expected.instances[block].3 = words;
            assert_eq!(
                summarise_with_refusal(&hostile),
                Ok((expected, refusal)),
                "{name}: block {block}"
            );
        }

        let mut changed = image.clone();
        changed.instances[block].kind ^= 1;
        let hostile = changed.to_bytes();
        let refusal = Some(BlobError::InstanceMismatch {
            block: id,
            field: InstanceField::Kind,
        });
        assert_eq!(
            decode(&hostile).err(),
            refusal,
            "{name}: block {block} kind"
        );
        let mut expected = original.clone();
        expected.instances[block].2 = !original.instances[block].2;
        assert_eq!(
            summarise_with_refusal(&hostile),
            Ok((expected, refusal)),
            "{name}: block {block}"
        );
    }
}

#[test]
fn the_content_id_field_is_reported_verbatim_and_all_zero_bytes_are_none() {
    let image = image_of(&golden(FIXTURES[0]));
    let original = Facts::of_image(&image);
    assert!(original.content_id.is_some());

    // The decoder carries the field without checking it, and so does the summary: any one
    // non-zero byte is an id, reported byte for byte.
    for at in 0..32 {
        for value in [1u8, 0x80, 0xff] {
            let mut changed = image.clone();
            changed.content_id = [0; 32];
            changed.content_id[at] = value;
            let bytes = changed.to_bytes();
            let mut expected = original.clone();
            expected.content_id = Some(changed.content_id);
            // The decoder checks the field against nothing, so the verdict stays acceptance.
            assert_eq!(
                summarise_with_refusal(&bytes),
                Ok((expected, None)),
                "byte {at} = {value:#x}"
            );
            assert_eq!(decode(&bytes).unwrap().content_id, Some(changed.content_id));
        }
    }

    // Thirty-two zero bytes are "no content id": what a hand-built model's blob records.
    let mut none = image.clone();
    none.content_id = [0; 32];
    let bytes = none.to_bytes();
    let mut expected = original.clone();
    expected.content_id = None;
    assert_eq!(summarise_with_refusal(&bytes), Ok((expected, None)));
    assert_eq!(decode(&bytes).unwrap().content_id, None);
    for model in [chain(), adder(), delay()] {
        assert_eq!(summarise(&model.to_bytes()).unwrap().content_id, None);
    }
}

#[test]
fn a_changed_boundary_path_shows_in_the_summary_and_nowhere_else() {
    let image = adder();
    let original = summarise(&image.to_bytes()).unwrap();
    assert_eq!(original, Facts::of_image(&image));
    assert_eq!(original.inputs, ["urn:test:a"]);
    assert_eq!(original.outputs, ["urn:test:y"]);

    // An empty path, a path with every kind of character a JSON writer must escape, and a
    // multi-byte path are all paths: the blob bounds them by length and UTF-8 only.
    for path in [
        "",
        "urn:test:a\"\\\n\t\u{1}",
        "urn:test:\u{00e9}\u{4e2d}\u{1f600}",
    ] {
        let mut renamed = image.clone();
        renamed.inputs[0].path = path.as_bytes().to_vec();
        let bytes = renamed.to_bytes();
        decode(&bytes).unwrap();
        let mut expected = original.clone();
        expected.inputs = vec![path.to_owned()];
        assert_eq!(
            summarise_with_refusal(&bytes),
            Ok((expected.clone(), None)),
            "input {path:?}"
        );
        // The golden spelling carries the path through an independent JSON reader unchanged.
        assert_eq!(Facts::from_json(&expected.to_json()), expected, "{path:?}");

        let mut renamed = image.clone();
        renamed.outputs[0].path = path.as_bytes().to_vec();
        let bytes = renamed.to_bytes();
        decode(&bytes).unwrap();
        let mut expected = original.clone();
        expected.outputs = vec![path.to_owned()];
        assert_eq!(
            summarise_with_refusal(&bytes),
            Ok((expected, None)),
            "output {path:?}"
        );
    }
}

#[test]
fn an_executable_with_no_blocks_summarises_to_empty_lists() {
    let image = Image::new();
    let bytes = image.to_bytes();
    // Header, five empty sections and the trailer: the smallest blob there is.
    assert_eq!(bytes.len(), 52 + 5 * 8 + 4 + 4 + 4 + 4 + 4 + 4 + 4 + 4);
    decode(&bytes).unwrap();
    let (facts, refusal) = summarise_with_refusal(&bytes).unwrap();
    // The smallest blob is one this build accepts: nothing to build is nothing to refuse.
    assert_eq!(refusal, None);
    assert_eq!(
        facts,
        Facts {
            format_version: 1,
            engine_version: [
                ENGINE_VERSION.major,
                ENGINE_VERSION.minor,
                ENGINE_VERSION.patch
            ],
            content_id: None,
            target_bound: false,
            classes: vec![],
            instances: vec![],
            inputs: vec![],
            outputs: vec![],
        }
    );
    assert_eq!(Facts::from_json(&facts.to_json()), facts);
}
