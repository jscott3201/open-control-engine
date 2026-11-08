//! Hostile compiled-schedule blob bytes (GitHub #400): every header, framing, count, UTF-8, tag,
//! ordering and trailing-byte defect is a typed refusal, never a panic. Each mutation is
//! re-checksummed so it reaches the check under test rather than the CRC.

#[path = "support/blob_corpus.rs"]
mod blob_corpus;
#[path = "support/blob_image.rs"]
mod blob_image;

use blob_corpus::{FIXTURES, golden};
use blob_image::{Image, VType, Val, rechecksum, reseal};
use oce_graph::blob::{
    BlobError, ENGINE_VERSION, HEADER_BYTES, MAX_BLOB_BYTES, MalformedFault, decode,
};

const WORKED: &str = "generic_air_economizer_high_limits_ashrae_differential";

fn malformed(bytes: &[u8]) -> (u32, MalformedFault) {
    match decode(bytes) {
        Err(BlobError::Malformed { offset, fault }) => (offset, fault),
        Err(other) => panic!("expected Malformed, got {other:?}"),
        Ok(_) => panic!("expected Malformed, decoded"),
    }
}

fn fault(bytes: &[u8]) -> MalformedFault {
    malformed(bytes).1
}

/// The smallest golden with at least two classes, connections, inputs and outputs, an input with
/// two targets and a stateful block, so every list can be put out of order.
fn rich() -> (String, Vec<u8>) {
    let mut best: Option<(String, Vec<u8>)> = None;
    for name in FIXTURES {
        let bytes = golden(name);
        let decoded = decode(&bytes).unwrap();
        let rich = decoded.model.connections.len() >= 2
            && decoded.inputs.len() >= 2
            && decoded.outputs.len() >= 2
            && decoded.inputs.iter().any(|i| i.targets.len() >= 2)
            && !decoded.state.slots.is_empty()
            && decoded
                .model
                .blocks
                .iter()
                .any(|b| b.class_iri != decoded.model.blocks[0].class_iri);
        if rich && best.as_ref().is_none_or(|(_, b)| bytes.len() < b.len()) {
            best = Some(((*name).to_owned(), bytes));
        }
    }
    best.expect("a rich fixture")
}

fn image_of(bytes: &[u8]) -> Image {
    Image::from_decoded(&decode(bytes).unwrap())
}

#[test]
fn the_independent_test_writer_reproduces_every_golden() {
    for name in FIXTURES {
        let bytes = golden(name);
        assert!(image_of(&bytes).to_bytes() == bytes, "{name}");
    }
}

#[test]
fn crc32c_matches_the_castagnoli_check_value() {
    assert_eq!(oce_graph::blob::crc32c(b"123456789"), 0xE306_9283);
    assert_eq!(oce_graph::blob::crc32c(b""), 0);
}

#[test]
fn every_prefix_of_a_blob_is_a_typed_refusal() {
    let (_, bytes) = rich();
    for length in 0..bytes.len() {
        let error = decode(&bytes[..length]).err().unwrap();
        assert!(
            matches!(error, BlobError::Malformed { .. }),
            "prefix {length}: {error:?}"
        );
    }
}

#[test]
fn every_resealed_body_truncation_reaches_the_structure_checks_and_refuses() {
    let (_, bytes) = rich();
    let body = &bytes[..bytes.len() - 4];
    for length in HEADER_BYTES..body.len() {
        let mut cut = body[..length].to_vec();
        reseal(&mut cut);
        let error = decode(&cut).err().unwrap();
        assert!(
            matches!(
                error,
                BlobError::Malformed {
                    fault: MalformedFault::Truncated
                        | MalformedFault::SectionId
                        | MalformedFault::SectionLength
                        | MalformedFault::CountExceedsInput
                        | MalformedFault::TrailingBytes,
                    ..
                }
            ),
            "body cut at {length}: {error:?}"
        );
    }
}

#[test]
fn header_defects_refuse_in_the_documented_order() {
    let bytes = golden(WORKED);
    decode(&bytes).unwrap();

    let oversized = vec![0u8; MAX_BLOB_BYTES + 1];
    assert!(matches!(
        decode(&oversized),
        Err(BlobError::TooLarge { length, limit }) if length == MAX_BLOB_BYTES as u64 + 1
            && limit == MAX_BLOB_BYTES as u64
    ));

    let mut magic = bytes.clone();
    magic[3] = b'C';
    rechecksum(&mut magic);
    assert_eq!(malformed(&magic), (0, MalformedFault::BadMagic));

    for delta in [-1i64, 1] {
        let mut length = bytes.clone();
        let total = i64::from(u32::from_le_bytes(length[16..20].try_into().unwrap())) + delta;
        length[16..20].copy_from_slice(&u32::try_from(total).unwrap().to_le_bytes());
        assert_eq!(malformed(&length), (16, MalformedFault::LengthMismatch));
    }

    for version in [0u16, 2, u16::MAX] {
        let mut format = bytes.clone();
        format[4..6].copy_from_slice(&version.to_le_bytes());
        rechecksum(&mut format);
        assert_eq!(
            decode(&format).err(),
            Some(BlobError::UnsupportedFormat {
                found: version,
                supported: 1
            })
        );
    }
    // A wrong version is refused before the CRC is even read.
    let mut format = bytes.clone();
    format[4] = 2;
    assert!(matches!(
        decode(&format),
        Err(BlobError::UnsupportedFormat { .. })
    ));

    for byte in [0, 20, 100, bytes.len() - 5, bytes.len() - 1] {
        let mut flipped = bytes.clone();
        flipped[byte] ^= 0x40;
        if byte < 4 {
            continue;
        }
        let error = decode(&flipped).err().unwrap();
        assert!(
            matches!(error, BlobError::IntegrityMismatch { .. }),
            "byte {byte}: {error:?}"
        );
    }

    let mut profile = bytes.clone();
    profile[6] = 2;
    rechecksum(&mut profile);
    assert_eq!(
        decode(&profile).err(),
        Some(BlobError::UnsupportedProfile {
            found: 2,
            supported: 1
        })
    );

    for (offset, field) in [(8usize, "major"), (10, "minor"), (12, "patch")] {
        let mut version = bytes.clone();
        version[offset] ^= 1;
        rechecksum(&mut version);
        assert!(
            matches!(
                decode(&version),
                Err(BlobError::EngineVersionMismatch { expected, .. }) if expected == ENGINE_VERSION
            ),
            "{field}"
        );
    }

    for flags in [1u16, 0x8000] {
        let mut flagged = bytes.clone();
        flagged[14..16].copy_from_slice(&flags.to_le_bytes());
        rechecksum(&mut flagged);
        assert_eq!(
            decode(&flagged).err(),
            Some(BlobError::NonzeroFlags { flags })
        );
    }
}

/// The worked example's count and length fields, at the offsets the design note's dump shows.
const WORKED_COUNTS: &[(usize, &str, MalformedFault)] = &[
    (
        0x38,
        "classes section length",
        MalformedFault::SectionLength,
    ),
    (0x3c, "class count", MalformedFault::CountExceedsInput),
    (0x40, "class path length", MalformedFault::CountExceedsInput),
    (
        0x66,
        "instances section length",
        MalformedFault::SectionLength,
    ),
    (0x6a, "block count", MalformedFault::CountExceedsInput),
    (0x6e, "connector count", MalformedFault::CountExceedsInput),
    (0x7f, "input arity", MalformedFault::CountExceedsInput),
    (0x87, "output arity", MalformedFault::CountExceedsInput),
    (0x8f, "parameter count", MalformedFault::CountExceedsInput),
    (
        0x93,
        "parameter name length",
        MalformedFault::CountExceedsInput,
    ),
    (
        0xa5,
        "schedule section length",
        MalformedFault::SectionLength,
    ),
    (0xa9, "schedule count", MalformedFault::CountExceedsInput),
    (
        0xb5,
        "connections section length",
        MalformedFault::SectionLength,
    ),
    (0xb9, "connection count", MalformedFault::CountExceedsInput),
    (0xc1, "I/O section length", MalformedFault::SectionLength),
    (0xc5, "input count", MalformedFault::CountExceedsInput),
    (0xc9, "input path length", MalformedFault::CountExceedsInput),
    (0x129, "target count", MalformedFault::CountExceedsInput),
    (
        0x131,
        "input payload length",
        MalformedFault::CountExceedsInput,
    ),
    (0x135, "output count", MalformedFault::CountExceedsInput),
    (
        0x139,
        "output path length",
        MalformedFault::CountExceedsInput,
    ),
    (
        0x19a,
        "output payload length",
        MalformedFault::CountExceedsInput,
    ),
];

#[test]
fn u32_max_in_every_count_and_length_is_refused_before_any_allocation() {
    let bytes = golden(WORKED);
    for &(offset, what, expected) in WORKED_COUNTS {
        for value in [u32::MAX, u32::MAX / 2, 0x0100_0000] {
            let mut hostile = bytes.clone();
            hostile[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            rechecksum(&mut hostile);
            let (at, found) = malformed(&hostile);
            assert_eq!(found, expected, "{what} = {value:#x}");
            assert_eq!(at as usize, offset, "{what}: refusal offset");
        }
    }
}

#[test]
fn a_count_one_past_its_records_is_refused_and_the_exact_count_is_accepted() {
    // The worked example's single input record is 108 bytes; claiming two inputs leaves the second
    // without bytes.
    let bytes = golden(WORKED);
    let mut two = bytes.clone();
    two[0xc5..0xc9].copy_from_slice(&2u32.to_le_bytes());
    rechecksum(&mut two);
    assert!(matches!(decode(&two), Err(BlobError::Malformed { .. })));
    decode(&bytes).unwrap();
}

#[test]
fn sections_out_of_order_or_misframed_are_refused() {
    let image = image_of(&golden(WORKED));
    let sections = image.sections();
    for (a, b) in [(0usize, 1usize), (1, 2), (3, 4)] {
        let mut body = Vec::new();
        for index in 0..5 {
            let source = if index == a {
                b
            } else if index == b {
                a
            } else {
                index
            };
            body.extend_from_slice(&(source as u32 + 1).to_le_bytes());
            body.extend_from_slice(&(sections[source].len() as u32).to_le_bytes());
            body.extend_from_slice(&sections[source]);
        }
        assert_eq!(fault(&image.assemble(&body)), MalformedFault::SectionId);
    }
    // A missing fifth section.
    let mut body = Vec::new();
    for (index, payload) in sections.iter().take(4).enumerate() {
        body.extend_from_slice(&(index as u32 + 1).to_le_bytes());
        body.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        body.extend_from_slice(payload);
    }
    assert_eq!(fault(&image.assemble(&body)), MalformedFault::Truncated);
}

#[test]
fn trailing_bytes_in_any_section_or_after_the_last_are_refused() {
    let image = image_of(&golden(WORKED));
    for section in 0..5 {
        let mut hostile = image.clone();
        hostile.section_tail[section] = vec![0];
        assert_eq!(
            fault(&hostile.to_bytes()),
            MalformedFault::TrailingBytes,
            "section {}",
            section + 1
        );
    }
    let mut hostile = image.clone();
    hostile.body_tail = vec![0; 8];
    assert_eq!(fault(&hostile.to_bytes()), MalformedFault::TrailingBytes);
}

#[test]
fn invalid_utf8_anywhere_a_string_sits_is_refused() {
    let (_, bytes) = rich();
    let image = image_of(&bytes);
    let bad = vec![b'C', 0xff, 0xfe];
    let mut edits: Vec<Image> = Vec::new();
    let mut class = image.clone();
    class.classes[0].0 = bad.clone();
    edits.push(class);
    let mut param = image.clone();
    let holder = param
        .instances
        .iter()
        .position(|i| !i.params.is_empty())
        .unwrap();
    param.instances[holder].params[0].0 = bad.clone();
    edits.push(param);
    let mut string = image.clone();
    string.instances[holder].params[0].1 = Val::Str(bad.clone());
    edits.push(string);
    let mut input = image.clone();
    input.inputs[0].path = bad.clone();
    edits.push(input);
    let mut output = image.clone();
    output.outputs[0].path = bad.clone();
    edits.push(output);
    let mut enum_type = image.clone();
    enum_type.inputs[0].vtype = VType::Enum(bad.clone());
    edits.push(enum_type);
    for (index, edit) in edits.iter().enumerate() {
        assert_eq!(
            fault(&edit.to_bytes()),
            MalformedFault::InvalidUtf8,
            "edit {index}"
        );
    }
}

#[test]
fn every_tag_outside_its_table_is_refused() {
    let (_, bytes) = rich();
    let image = image_of(&bytes);
    let holder = image
        .instances
        .iter()
        .position(|i| !i.params.is_empty())
        .unwrap();
    let mut edits: Vec<(&str, Image)> = Vec::new();
    for tag in [5u8, 9, 0xff] {
        let mut value = image.clone();
        value.instances[holder].params[0].1 = Val::Tag(tag);
        edits.push(("value tag", value));
        let mut vtype = image.clone();
        vtype.inputs[0].vtype = VType::Tag(tag);
        edits.push(("vtype tag", vtype));
    }
    for byte in [2u8, 0xff] {
        let mut boolean = image.clone();
        boolean.instances[holder].params[0].1 = Val::Boolean(byte);
        edits.push(("Boolean byte", boolean));
        let mut kind = image.clone();
        kind.instances[0].kind = byte;
        edits.push(("block kind", kind));
    }
    for (what, edit) in &edits {
        assert_eq!(
            fault(&edit.to_bytes()),
            MalformedFault::InvalidTag,
            "{what}"
        );
    }
    // An optional-bound tag other than 0/1: the min bound byte of the first input.
    let mut sections = image.sections();
    let path_len = image.inputs[0].path.len();
    let vtype_len = 1;
    let min_tag = 4 + 4 + path_len + vtype_len;
    sections[4][min_tag] = 2;
    let mut body = Vec::new();
    for (index, payload) in sections.iter().enumerate() {
        body.extend_from_slice(&(index as u32 + 1).to_le_bytes());
        body.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        body.extend_from_slice(payload);
    }
    assert_eq!(fault(&image.assemble(&body)), MalformedFault::InvalidTag);
}

#[test]
fn every_list_out_of_canonical_order_is_refused() {
    let (name, bytes) = rich();
    let image = image_of(&bytes);
    let mut edits: Vec<(&str, Image)> = Vec::new();

    let mut classes = image.clone();
    classes.classes.swap(0, 1);
    // Structure is checked before any class index is read, so the instances need no remap.
    edits.push(("classes", classes));

    let mut duplicate_class = image.clone();
    let first = duplicate_class.classes[0].clone();
    duplicate_class.classes.insert(1, first);
    edits.push(("duplicated class", duplicate_class));

    let mut connections = image.clone();
    connections.connections.reverse();
    edits.push(("connections", connections));

    let mut inputs = image.clone();
    inputs.inputs.swap(0, 1);
    edits.push(("inputs", inputs));

    let mut duplicate_input = image.clone();
    let first = duplicate_input.inputs[0].path.clone();
    duplicate_input.inputs[1].path = first;
    edits.push(("duplicated input path", duplicate_input));

    let mut outputs = image.clone();
    outputs.outputs.reverse();
    edits.push(("outputs", outputs));

    let mut targets = image.clone();
    let fan = targets
        .inputs
        .iter()
        .position(|i| i.targets.len() >= 2)
        .unwrap();
    targets.inputs[fan].targets.reverse();
    edits.push(("targets", targets));

    let mut repeated_target = image.clone();
    let first = repeated_target.inputs[fan].targets[0];
    repeated_target.inputs[fan].targets[1] = first;
    edits.push(("repeated target", repeated_target));

    for (what, edit) in &edits {
        assert_eq!(
            fault(&edit.to_bytes()),
            MalformedFault::NotCanonical,
            "{name}: {what}"
        );
    }
}
