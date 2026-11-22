//! Hostile bytes through `oce_api::inspect_blob` and `oce_graph::blob::inspect` (issue #448):
//! every refusal of the decoder's framing and structural pass is the summary's refusal too, the
//! identical typed error at the identical offset, and no input panics.
//!
//! The defects are the ones `blob_hostile_bytes.rs` and `blob_refusal_precedence.rs` feed the
//! decoder, built with the same test-side writer and re-checksummed so each reaches the check
//! under test. Every case goes through [`against_the_decoder`], which holds the summary, and the
//! `decode_refusal` it carries (issue #680), to the decoder's verdict.

// Shared helpers, declared once for this binary in its root `facade.rs`.
use crate::blob_corpus;
use crate::blob_fuzz::{XorShift, env_u64};
use crate::blob_hostile_bytes::{WORKED_COUNTS, rich as rich_golden};
use crate::blob_image;
use crate::blob_refusal_precedence::{Defect, REFUSAL_ORDER, refusal, with_defects};
use crate::blob_summary_facts;

use blob_corpus::{FIXTURES, golden};
use blob_image::{Image, VType, Val, rechecksum, reseal};
use blob_summary_facts::{Facts, assert_invariants, summarise, summarise_with_refusal};
use oce_api::{BlobError, ConnectorFault, ENGINE_VERSION, InstanceField, MalformedFault};
use oce_graph::blob::{HEADER_BYTES, MAX_BLOB_BYTES, decode};

const WORKED: &str = "generic_air_economizer_high_limits_ashrae_differential";

/// How a summary relates to the decoder's verdict on the same bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verdict {
    /// The decoder accepts the blob and the summary states what it decoded, with no refusal.
    Decoded,
    /// Both refuse with the identical error: every refusal of the decoder's steps 1 to 6, an
    /// unregistered class and a class index outside the table.
    SameRefusal,
    /// The decoder refuses at a later step; the summary reports the blob as written and carries
    /// that refusal.
    SummarisedAsWritten,
    /// The decoder refuses at a later step; the summary refuses an unregistered class or a class
    /// index further on in the table, which the decoder had not reached.
    RefusedFurtherOn,
}

/// Summarise `bytes` through the core and the facade and hold the result to the decoder's verdict.
fn against_the_decoder(bytes: &[u8], what: &str) -> Verdict {
    let summary = summarise_with_refusal(bytes);
    match decode(bytes) {
        Ok(decoded) => {
            let (facts, refusal) =
                summary.unwrap_or_else(|err| panic!("{what}: decodes, yet {err:?}"));
            assert_eq!(
                refusal, None,
                "{what}: decodes, yet the summary says refused"
            );
            assert_eq!(
                facts,
                Facts::of_image(&Image::from_decoded(&decoded)),
                "{what}"
            );
            assert_eq!(facts.target_bound, decoded.target_bound, "{what}");
            assert_invariants(&facts, what);
            Verdict::Decoded
        }
        Err(
            error @ (BlobError::TooLarge { .. }
            | BlobError::Malformed { .. }
            | BlobError::UnsupportedFormat { .. }
            | BlobError::EngineVersionMismatch { .. }
            | BlobError::IntegrityMismatch { .. }
            | BlobError::UnsupportedProfile { .. }
            | BlobError::NonzeroFlags { .. }
            | BlobError::UnknownClass { .. }
            | BlobError::InstanceMismatch {
                field: InstanceField::ClassIndex,
                ..
            }),
        ) => {
            assert_eq!(summary, Err(error), "{what}");
            Verdict::SameRefusal
        }
        Err(later) => match summary {
            Ok((facts, refusal)) => {
                assert_invariants(&facts, what);
                assert_eq!(refusal, Some(later), "{what}: the decoder's own refusal");
                Verdict::SummarisedAsWritten
            }
            Err(BlobError::UnknownClass { class }) => {
                assert_further_on(Reached::Class(class), later, what);
                Verdict::RefusedFurtherOn
            }
            Err(BlobError::InstanceMismatch {
                block,
                field: InstanceField::ClassIndex,
            }) => {
                assert_further_on(Reached::Block(block), later, what);
                Verdict::RefusedFurtherOn
            }
            Err(other) => panic!("{what}: the decoder says {later:?}, the summary {other:?}"),
        },
    }
}

/// A place in the decoder's step 7, which reads the class table in order and then the blocks in
/// id order. The derived order is that reading order: every class comes before every block.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Reached {
    Class(u32),
    Block(u32),
}

/// The summary refused at `summary` where the decoder refused with `decoder` at a later step:
/// require the summary's place to lie strictly further on than the one the decoder stopped at.
/// The decoder had already passed its own place's registration or class-index check, so an equal
/// place would mean the two readers disagree about one record.
fn assert_further_on(summary: Reached, decoder: BlobError, what: &str) {
    match decoder {
        BlobError::ClassSignatureMismatch { class, .. } => {
            assert!(summary > Reached::Class(class), "{what}: {decoder:?}");
        }
        BlobError::DuplicateParameter { block, .. }
        | BlobError::UndeclaredParameter { block, .. }
        | BlobError::NonFiniteParameter { block, .. }
        | BlobError::ParameterKindMismatch { block, .. }
        | BlobError::InstanceMismatch { block, .. } => {
            assert!(summary > Reached::Block(block), "{what}: {decoder:?}");
        }
        // Raised inside a block and named by a byte offset or a connector id, not by the block.
        // Every class had passed by then, so only a block's class index can lie further on.
        BlobError::UnknownEnumClass { .. }
        | BlobError::EnumOrdinalOutOfRange { .. }
        | BlobError::InvalidConnector {
            fault: ConnectorFault::OutOfRange | ConnectorFault::Duplicated,
            ..
        } => assert!(
            matches!(summary, Reached::Block(_)),
            "{what}: {summary:?} against {decoder:?}"
        ),
        // Every other refusal comes after the decoder read every class and every block's class
        // index, so the summary has nothing left to refuse.
        _ => panic!("{what}: the summary refuses at {summary:?}, the decoder later: {decoder:?}"),
    }
}

/// A structural defect: the decoder's refusal is `Malformed` with `fault`, and the summary's is
/// the same error at the same offset.
fn refused_as_malformed(bytes: &[u8], fault: MalformedFault, what: &str) -> u32 {
    assert_eq!(
        against_the_decoder(bytes, what),
        Verdict::SameRefusal,
        "{what}"
    );
    match summarise(bytes) {
        Err(BlobError::Malformed {
            offset,
            fault: found,
        }) => {
            assert_eq!(found, fault, "{what}");
            offset
        }
        other => panic!("{what}: expected Malformed, got {other:?}"),
    }
}

/// The golden `blob_hostile_bytes.rs` puts out of order: the smallest with two classes,
/// connections, inputs and outputs, an input with two targets and a stateful block.
fn rich() -> Vec<u8> {
    rich_golden().1
}

fn image_of(bytes: &[u8]) -> Image {
    Image::from_decoded(&decode(bytes).unwrap())
}

#[test]
fn every_prefix_of_a_blob_is_refused_as_the_decoder_refuses_it() {
    let bytes = rich();
    for length in 0..bytes.len() {
        let what = format!("prefix {length}");
        assert_eq!(
            against_the_decoder(&bytes[..length], &what),
            Verdict::SameRefusal
        );
        assert!(
            matches!(
                summarise(&bytes[..length]),
                Err(BlobError::Malformed { .. })
            ),
            "{what}"
        );
    }
    assert_eq!(
        against_the_decoder(&bytes, "the whole blob"),
        Verdict::Decoded
    );
}

#[test]
fn every_resealed_body_truncation_is_refused_as_the_decoder_refuses_it() {
    let bytes = rich();
    let body = &bytes[..bytes.len() - 4];
    for length in HEADER_BYTES..body.len() {
        let mut cut = body[..length].to_vec();
        reseal(&mut cut);
        let what = format!("body cut at {length}");
        assert_eq!(against_the_decoder(&cut, &what), Verdict::SameRefusal);
        assert!(
            matches!(
                summarise(&cut),
                Err(BlobError::Malformed {
                    fault: MalformedFault::Truncated
                        | MalformedFault::SectionId
                        | MalformedFault::SectionLength
                        | MalformedFault::CountExceedsInput
                        | MalformedFault::TrailingBytes,
                    ..
                })
            ),
            "{what}"
        );
    }
}

#[test]
fn header_defects_are_the_decoders_refusals_in_the_decoders_order() {
    let bytes = golden(WORKED);
    assert_eq!(against_the_decoder(&bytes, "the golden"), Verdict::Decoded);

    // The size limit is inclusive: one byte over is too large, the limit itself goes on to the
    // magic.
    let mut oversized = vec![0u8; MAX_BLOB_BYTES + 1];
    assert_eq!(
        summarise(&oversized),
        Err(BlobError::TooLarge {
            length: MAX_BLOB_BYTES as u64 + 1,
            limit: MAX_BLOB_BYTES as u64
        })
    );
    oversized.pop();
    assert_eq!(
        summarise(&oversized),
        Err(BlobError::Malformed {
            offset: 0,
            fault: MalformedFault::BadMagic
        })
    );
    drop(oversized);

    let mut magic = bytes.clone();
    magic[3] = b'C';
    rechecksum(&mut magic);
    assert_eq!(
        refused_as_malformed(&magic, MalformedFault::BadMagic, "magic"),
        0
    );

    for version in [0u16, 2, 3, u16::MAX] {
        let mut format = bytes.clone();
        format[4..6].copy_from_slice(&version.to_le_bytes());
        // Refused before the CRC is read, re-checksummed or not.
        for resealed in [false, true] {
            if resealed {
                rechecksum(&mut format);
            }
            assert_eq!(
                summarise(&format),
                Err(BlobError::UnsupportedFormat {
                    found: version,
                    supported: 1
                })
            );
            assert_eq!(against_the_decoder(&format, "format"), Verdict::SameRefusal);
        }
    }

    for (offset, field) in [(8usize, "major"), (10, "minor"), (12, "patch")] {
        let mut version = bytes.clone();
        version[offset] ^= 1;
        // Refused before the length and the CRC, re-checksummed or not.
        for resealed in [false, true] {
            if resealed {
                rechecksum(&mut version);
            }
            assert!(
                matches!(
                    summarise(&version),
                    Err(BlobError::EngineVersionMismatch { expected, .. })
                        if expected == ENGINE_VERSION
                ),
                "{field}"
            );
            assert_eq!(against_the_decoder(&version, field), Verdict::SameRefusal);
        }
    }

    for delta in [-1i64, 1] {
        let mut length = bytes.clone();
        let total = i64::from(u32::from_le_bytes(length[16..20].try_into().unwrap())) + delta;
        length[16..20].copy_from_slice(&u32::try_from(total).unwrap().to_le_bytes());
        assert_eq!(
            refused_as_malformed(&length, MalformedFault::LengthMismatch, "length"),
            16
        );
    }

    // One flipped bit anywhere after the version, left unsealed, is an integrity refusal: the
    // content id, a class path, the last body byte and the trailer itself.
    for byte in [20, 51, 100, bytes.len() - 5, bytes.len() - 1] {
        let mut flipped = bytes.clone();
        flipped[byte] ^= 0x40;
        let what = format!("byte {byte}");
        assert_eq!(against_the_decoder(&flipped, &what), Verdict::SameRefusal);
        assert!(
            matches!(
                summarise(&flipped),
                Err(BlobError::IntegrityMismatch { stored, computed }) if stored != computed
            ),
            "{what}"
        );
    }

    let mut profile = bytes.clone();
    profile[6] = 2;
    rechecksum(&mut profile);
    assert_eq!(
        summarise(&profile),
        Err(BlobError::UnsupportedProfile {
            found: 2,
            supported: 1
        })
    );
    assert_eq!(
        against_the_decoder(&profile, "profile"),
        Verdict::SameRefusal
    );

    for flags in [1u16, 0x8000] {
        let mut flagged = bytes.clone();
        flagged[14..16].copy_from_slice(&flags.to_le_bytes());
        rechecksum(&mut flagged);
        assert_eq!(summarise(&flagged), Err(BlobError::NonzeroFlags { flags }));
        assert_eq!(against_the_decoder(&flagged, "flags"), Verdict::SameRefusal);
    }
}

#[test]
fn of_every_two_header_defects_the_summary_reports_the_one_the_decoder_reports() {
    // The decoder's ten header checks in their order, each defect alone and every pair that can
    // occur together (`blob_refusal_precedence.rs`): the summary's refusal is the earlier check's,
    // spelled out from the header table, which is also what the decoder says.
    let valid = golden(WORKED);
    for (index, &earlier) in REFUSAL_ORDER.iter().enumerate() {
        let alone = with_defects(&valid, &[earlier]);
        assert_eq!(
            summarise(&alone),
            Err(refusal(earlier, &alone)),
            "{earlier:?}"
        );
        for &later in &REFUSAL_ORDER[index + 1..] {
            if matches!(
                (earlier, later),
                (Defect::Oversized, Defect::Short) | (Defect::Short, Defect::Section)
            ) {
                continue;
            }
            let bytes = with_defects(&valid, &[earlier, later]);
            let expected = refusal(earlier, &bytes);
            assert_eq!(
                summarise(&bytes),
                Err(expected),
                "{earlier:?} with {later:?}"
            );
            assert_eq!(
                decode(&bytes).err(),
                Some(expected),
                "{earlier:?} with {later:?}"
            );
        }
    }
}

#[test]
fn u32_max_in_every_count_and_length_is_refused_at_its_own_offset() {
    let bytes = golden(WORKED);
    for &(offset, what, expected) in WORKED_COUNTS {
        for value in [u32::MAX, u32::MAX / 2, 0x0100_0000] {
            let mut hostile = bytes.clone();
            hostile[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            rechecksum(&mut hostile);
            let what = format!("{what} = {value:#x}");
            let at = refused_as_malformed(&hostile, expected, &what);
            assert_eq!(at as usize, offset, "{what}: refusal offset");
        }
    }
}

#[test]
fn misframed_sections_and_trailing_bytes_are_refused_as_the_decoder_refuses_them() {
    let image = image_of(&golden(WORKED));
    let sections = image.sections();
    let framed = |order: [usize; 5], count: usize| {
        let mut body = Vec::new();
        for source in order.into_iter().take(count) {
            body.extend_from_slice(&(source as u32 + 1).to_le_bytes());
            body.extend_from_slice(&(sections[source].len() as u32).to_le_bytes());
            body.extend_from_slice(&sections[source]);
        }
        image.assemble(&body)
    };
    for order in [[1, 0, 2, 3, 4], [0, 2, 1, 3, 4], [0, 1, 2, 4, 3]] {
        refused_as_malformed(&framed(order, 5), MalformedFault::SectionId, "swapped");
    }
    refused_as_malformed(
        &framed([0, 1, 2, 3, 4], 4),
        MalformedFault::Truncated,
        "a missing fifth section",
    );
    assert_eq!(
        against_the_decoder(&framed([0, 1, 2, 3, 4], 5), "in order"),
        Verdict::Decoded
    );

    for section in 0..5 {
        let mut hostile = image.clone();
        hostile.section_tail[section] = vec![0];
        let what = format!("a byte after section {}", section + 1);
        refused_as_malformed(&hostile.to_bytes(), MalformedFault::TrailingBytes, &what);
    }
    let mut hostile = image.clone();
    hostile.body_tail = vec![0; 8];
    refused_as_malformed(
        &hostile.to_bytes(),
        MalformedFault::TrailingBytes,
        "bytes after the last section",
    );
}

#[test]
fn invalid_utf8_and_tags_outside_their_tables_are_refused_as_the_decoder_refuses_them() {
    let image = image_of(&rich());
    let bad = vec![b'C', 0xff, 0xfe];
    let holder = image
        .instances
        .iter()
        .position(|i| !i.params.is_empty())
        .unwrap();
    let mut utf8: Vec<(&str, Image)> = Vec::new();
    let mut class = image.clone();
    class.classes[0].0 = bad.clone();
    utf8.push(("class path", class));
    let mut name = image.clone();
    name.instances[holder].params[0].0 = bad.clone();
    utf8.push(("parameter name", name));
    let mut string = image.clone();
    string.instances[holder].params[0].1 = Val::Str(bad.clone());
    utf8.push(("String parameter", string));
    let mut input = image.clone();
    input.inputs[0].path = bad.clone();
    utf8.push(("input path", input));
    let mut output = image.clone();
    output.outputs[0].path = bad.clone();
    utf8.push(("output path", output));
    let mut enum_type = image.clone();
    enum_type.inputs[0].vtype = VType::Enum(bad.clone());
    utf8.push(("Enum type path", enum_type));
    for (what, edit) in &utf8 {
        refused_as_malformed(&edit.to_bytes(), MalformedFault::InvalidUtf8, what);
    }

    let mut tags: Vec<(&str, Image)> = Vec::new();
    for tag in [5u8, 9, 0xff] {
        let mut value = image.clone();
        value.instances[holder].params[0].1 = Val::Tag(tag);
        tags.push(("value tag", value));
        let mut vtype = image.clone();
        vtype.inputs[0].vtype = VType::Tag(tag);
        tags.push(("vtype tag", vtype));
    }
    for byte in [2u8, 0xff] {
        let mut boolean = image.clone();
        boolean.instances[holder].params[0].1 = Val::Boolean(byte);
        tags.push(("Boolean byte", boolean));
        let mut kind = image.clone();
        kind.instances[0].kind = byte;
        tags.push(("block kind", kind));
    }
    for (what, edit) in &tags {
        refused_as_malformed(&edit.to_bytes(), MalformedFault::InvalidTag, what);
    }
}

#[test]
fn every_list_out_of_canonical_order_is_refused_as_the_decoder_refuses_it() {
    let image = image_of(&rich());
    let fan = image
        .inputs
        .iter()
        .position(|i| i.targets.len() >= 2)
        .unwrap();
    let mut edits: Vec<(&str, Image)> = Vec::new();

    let mut classes = image.clone();
    classes.classes.swap(0, 1);
    edits.push(("classes", classes));
    let mut duplicate_class = image.clone();
    let first = duplicate_class.classes[0].clone();
    duplicate_class.classes.insert(1, first);
    edits.push(("a class twice", duplicate_class));
    let mut connections = image.clone();
    connections.connections.reverse();
    edits.push(("connections", connections));
    let mut inputs = image.clone();
    inputs.inputs.swap(0, 1);
    edits.push(("inputs", inputs));
    let mut duplicate_input = image.clone();
    let first = duplicate_input.inputs[0].path.clone();
    duplicate_input.inputs[1].path = first;
    edits.push(("an input path twice", duplicate_input));
    let mut outputs = image.clone();
    outputs.outputs.reverse();
    edits.push(("outputs", outputs));
    let mut targets = image.clone();
    targets.inputs[fan].targets.reverse();
    edits.push(("targets", targets));
    let mut repeated_target = image.clone();
    let first = repeated_target.inputs[fan].targets[0];
    repeated_target.inputs[fan].targets[1] = first;
    edits.push(("a target twice", repeated_target));

    for (what, edit) in &edits {
        refused_as_malformed(&edit.to_bytes(), MalformedFault::NotCanonical, what);
    }
}

#[test]
fn random_byte_mutations_never_panic_and_never_part_from_the_decoder() {
    let cases = env_u64("OCE_BLOB_FUZZ_CASES", 10_000);
    let mut rng = XorShift(env_u64("OCE_BLOB_FUZZ_SEED", 0x0CEB_0448_5A11_5EED).max(1));
    let goldens: Vec<Vec<u8>> = FIXTURES.iter().map(|name| golden(name)).collect();
    let mut verdicts = [0u64; 4];
    for case in 0..cases {
        let source = &goldens[rng.below(goldens.len())];
        let mut bytes = source.clone();
        let flips = 1 + rng.below(4);
        // Mostly mutate the body after the header, where the structural checks live.
        for _ in 0..flips {
            let at = if rng.below(8) == 0 {
                rng.below(bytes.len())
            } else {
                HEADER_BYTES + rng.below(bytes.len() - HEADER_BYTES - 4)
            };
            bytes[at] = rng.next() as u8;
        }
        if rng.below(10) != 0 && bytes[16..20] == source[16..20] {
            rechecksum(&mut bytes);
        }
        let verdict = against_the_decoder(&bytes, &format!("case {case}"));
        verdicts[verdict as usize] += 1;
    }
    assert_eq!(verdicts.iter().sum::<u64>(), cases);
    if cases >= 10_000 {
        // Every relation occurs, so none of the four arms of `against_the_decoder` goes untested.
        let [decoded, same, as_written, further_on] = verdicts;
        assert!(decoded > 0 && further_on > 0, "{verdicts:?}");
        assert!(same > cases / 2, "{verdicts:?}");
        assert!(as_written > cases / 20, "{verdicts:?}");
    }
}

#[test]
fn a_summary_refusal_further_on_in_the_table_is_told_from_the_decoders_earlier_one() {
    // The decoder constructs block 0 and stops at its kind tag. The summary constructs nothing,
    // reads on, and stops at block 1, whose class index is outside the table.
    let mut image = crate::blob_models::chain();
    image.instances[0].kind = 1;
    image.instances[1].class = 7;
    let bytes = image.to_bytes();
    assert_eq!(
        decode(&bytes).err(),
        Some(BlobError::InstanceMismatch {
            block: 0,
            field: InstanceField::Kind
        })
    );
    assert_eq!(
        summarise(&bytes),
        Err(BlobError::InstanceMismatch {
            block: 1,
            field: InstanceField::ClassIndex
        })
    );
    assert_eq!(
        against_the_decoder(&bytes, "kind, then class index"),
        Verdict::RefusedFurtherOn
    );

    // The same in the class table: the decoder stops at class 0's hash, the summary at class 1,
    // which this build does not register.
    let mut image = crate::blob_models::chain();
    image.classes[0].1 ^= 1;
    image.classes[1].0 = b"CDL.Reals.Sources.Constanz".to_vec();
    let bytes = image.to_bytes();
    assert!(matches!(
        decode(&bytes),
        Err(BlobError::ClassSignatureMismatch { class: 0, .. })
    ));
    assert_eq!(summarise(&bytes), Err(BlobError::UnknownClass { class: 1 }));
    assert_eq!(
        against_the_decoder(&bytes, "hash, then unregistered class"),
        Verdict::RefusedFurtherOn
    );
}

#[test]
#[should_panic(expected = "the summary refuses at Block(0)")]
fn a_summary_refusal_where_the_decoder_had_passed_every_block_fails_the_comparison() {
    // The control for `assert_further_on`: a decoder refusal from after the instance loop leaves
    // the summary nothing to refuse, so a summary refusal beside it is a disagreement.
    assert_further_on(
        Reached::Block(0),
        BlobError::UndrivenInput { connector: 1 },
        "control",
    );
}

#[test]
#[should_panic(expected = "equal place")]
fn a_summary_refusal_at_the_decoders_own_place_fails_the_comparison() {
    assert!(Reached::Class(3) < Reached::Block(0));
    assert_further_on(
        Reached::Block(2),
        BlobError::NonFiniteParameter {
            block: 2,
            parameter: 0,
        },
        "equal place",
    );
}
