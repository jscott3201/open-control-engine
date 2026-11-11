//! Adversarial byte-level tests for the durable snapshot decoder.

use oce_graph::blob::{ENGINE_VERSION, EngineVersion};

use crate::state::{BlockKey, ConnectorKey, WireDir};
use crate::{Engine, EngineStateError, EngineStateSnapshot};

/// The engine-version field of the fixed header: major, minor, patch as little-endian `u16`.
const VERSION_FIELD: std::ops::Range<usize> = 16..22;
/// Offset of the body-length field, which every length refusal names.
const BODY_LENGTH: usize = 22;
/// Bytes in the fixed header; the body starts here with the 16-byte fingerprint.
const HEADER: usize = 30;
/// Bytes in the trailer.
const TRAILER: usize = 16;

const MINIMAL_LOOP: &[u8] = include_bytes!("../../../oce-cxf/tests/fixtures/minimal_loop.jsonld");

fn snapshot_bytes() -> Vec<u8> {
    let mut engine = Engine::new();
    engine.load_cxf(MINIMAL_LOOP).unwrap();
    super::common::advance(
        &mut engine,
        0.0,
        &[("http://example.org#MinLoop.uSet", crate::Value::Real(0.0))],
    )
    .unwrap();
    engine.state_snapshot().unwrap().into_bytes()
}

fn fnv(bytes: &[u8]) -> u128 {
    const OFFSET: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;
    bytes.iter().fold(OFFSET, |hash, byte| {
        (hash ^ u128::from(*byte)).wrapping_mul(PRIME)
    })
}

fn resign(bytes: &mut [u8]) {
    let trailer = bytes.len() - 16;
    let checksum = fnv(&bytes[..trailer]);
    bytes[trailer..].copy_from_slice(&checksum.to_le_bytes());
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

struct Offsets {
    model_bytes: std::ops::Range<usize>,
    state_t: usize,
    prev_tag: usize,
    manifest_length: usize,
    manifest: std::ops::Range<usize>,
    first_value_tag: usize,
    flags: usize,
}

fn offsets(bytes: &[u8]) -> Offsets {
    let model_length = u32_at(bytes, HEADER + 16) as usize;
    let model_bytes = HEADER + 20..HEADER + 20 + model_length;
    let state_t = model_bytes.end;
    let prev_tag = state_t + 8;
    let manifest_length = prev_tag + 1 + usize::from(bytes[prev_tag] == 1) * 8;
    let manifest_start = manifest_length + 8;
    let manifest_end = manifest_start + u64_at(bytes, manifest_length) as usize;
    let values_count = manifest_end;
    assert!(u32_at(bytes, values_count) > 0);
    let mut cursor = values_count + 4;
    cursor = skip_connector_key(bytes, cursor);
    let first_value_tag = cursor;
    Offsets {
        model_bytes,
        state_t,
        prev_tag,
        manifest_length,
        manifest: manifest_start..manifest_end,
        first_value_tag,
        flags: bytes.len() - 16 - 4,
    }
}

fn skip_string(bytes: &[u8], offset: usize) -> usize {
    offset + 4 + u32_at(bytes, offset) as usize
}

fn skip_connector_key(bytes: &[u8], offset: usize) -> usize {
    let mut cursor = offset + 1;
    match bytes[offset] {
        0 => cursor = skip_string(bytes, cursor),
        1 => {
            cursor = skip_string(bytes, cursor);
            cursor = skip_string(bytes, cursor);
        }
        tag => panic!("unexpected block-key tag {tag}"),
    }
    cursor + 1 + 4
}

#[test]
fn every_truncation_boundary_is_a_typed_refusal() {
    let bytes = snapshot_bytes();
    for length in 0..bytes.len() {
        // Host-owned partial durable write / power-loss model: no filesystem durability claim.
        let error = EngineStateSnapshot::from_bytes(&bytes[..length]).unwrap_err();
        assert!(
            matches!(error, EngineStateError::MalformedSnapshot { offset, .. }
            if offset == if length < HEADER + TRAILER { 0 } else { BODY_LENGTH as u64 }),
            "length {length}: {error:?}"
        );
    }
}

#[test]
fn fixed_header_refusals_follow_the_pinned_precedence() {
    let bytes = snapshot_bytes();

    let mut bad_magic = bytes.clone();
    bad_magic[0] ^= 1;
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&bad_magic),
        Err(EngineStateError::MalformedSnapshot { offset: 0, .. })
    ));

    let mut unsupported = bytes.clone();
    unsupported[8..12].copy_from_slice(&2u32.to_le_bytes());
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&unsupported),
        Err(EngineStateError::UnsupportedFormat { revision: 2 })
    ));

    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&trailing),
        Err(EngineStateError::MalformedSnapshot { offset, .. }) if offset == BODY_LENGTH as u64
    ));

    // The engine version sits between the format and the length: a wrong magic or format wins
    // over it, and it wins over a wrong length, a wrong trailer and a wrong body.
    let other = EngineVersion {
        patch: ENGINE_VERSION.patch.wrapping_add(1),
        ..ENGINE_VERSION
    };
    let mut other_version = bytes.clone();
    other_version[VERSION_FIELD.end - 2..VERSION_FIELD.end]
        .copy_from_slice(&other.patch.to_le_bytes());
    let edits: [fn(&mut Vec<u8>); 5] = [
        |_| {},
        |bytes| bytes.push(0),
        |bytes| bytes.truncate(bytes.len() - 1),
        |bytes| bytes[HEADER] ^= 1,
        |bytes| bytes[BODY_LENGTH..HEADER].fill(0xff),
    ];
    for edit in edits {
        let mut edited = other_version.clone();
        edit(&mut edited);
        assert!(matches!(
            EngineStateSnapshot::from_bytes(&edited),
            Err(EngineStateError::EngineVersionMismatch { found, expected })
                if found == other && expected == ENGINE_VERSION
        ));
    }
    let mut bad_magic = other_version.clone();
    bad_magic[7] ^= 1;
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&bad_magic),
        Err(EngineStateError::MalformedSnapshot { offset: 0, .. })
    ));
    let mut unsupported = other_version;
    unsupported[8..12].copy_from_slice(&0u32.to_le_bytes());
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&unsupported),
        Err(EngineStateError::UnsupportedFormat { revision: 0 })
    ));
}

#[test]
fn a_header_cut_inside_the_version_field_is_too_short_or_another_version() {
    // Every prefix that ends inside the version field, alone and padded out to the shortest
    // length the decoder inspects: the first is shorter than a header and trailer, the second
    // reads a version the padding completed and refuses it as another engine's.
    let bytes = snapshot_bytes();
    for cut in VERSION_FIELD.start..VERSION_FIELD.end {
        assert!(matches!(
            EngineStateSnapshot::from_bytes(&bytes[..cut]),
            Err(EngineStateError::MalformedSnapshot { offset: 0, .. })
        ));
        let mut padded = bytes[..cut].to_vec();
        padded.resize(HEADER + TRAILER, 0xff);
        match EngineStateSnapshot::from_bytes(&padded) {
            Err(EngineStateError::EngineVersionMismatch { found, expected }) => {
                assert_eq!(expected, ENGINE_VERSION);
                assert_eq!(
                    [found.major, found.minor, found.patch],
                    [0, 2, 4].map(|at| u16::from_le_bytes([padded[16 + at], padded[17 + at]]))
                );
            }
            other => panic!("cut {cut}: {:?}", other.map(|_| ())),
        }
    }
}

#[test]
fn body_tags_utf8_lengths_and_flags_refuse_after_valid_integrity() {
    let bytes = snapshot_bytes();
    let layout = offsets(&bytes);

    let mut invalid_utf8 = bytes.clone();
    invalid_utf8[layout.model_bytes.start] = 0xff;
    resign(&mut invalid_utf8);
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&invalid_utf8),
        Err(EngineStateError::MalformedSnapshot { .. })
    ));

    let mut prev_tag = bytes.clone();
    prev_tag[layout.prev_tag] = 2;
    resign(&mut prev_tag);
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&prev_tag),
        Err(EngineStateError::MalformedSnapshot { .. })
    ));

    let mut manifest_length = bytes.clone();
    manifest_length[layout.manifest_length..layout.manifest_length + 8]
        .copy_from_slice(&u64::MAX.to_le_bytes());
    resign(&mut manifest_length);
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&manifest_length),
        Err(EngineStateError::MalformedSnapshot { .. })
    ));

    let mut portability_tag = bytes.clone();
    portability_tag[layout.manifest.start] = 9;
    resign(&mut portability_tag);
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&portability_tag),
        Err(EngineStateError::MalformedSnapshot { .. })
    ));

    let mut flags = bytes.clone();
    flags[layout.flags..layout.flags + 4].copy_from_slice(&1u32.to_le_bytes());
    resign(&mut flags);
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&flags),
        Err(EngineStateError::MalformedSnapshot { .. })
    ));
}

#[test]
fn self_consistency_and_value_type_corruption_refuse() {
    let bytes = snapshot_bytes();
    let layout = offsets(&bytes);

    let mut fingerprint = bytes.clone();
    fingerprint[HEADER] ^= 1;
    resign(&mut fingerprint);
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&fingerprint),
        Err(EngineStateError::MalformedSnapshot { .. })
    ));

    let mut nonfinite_time = bytes.clone();
    nonfinite_time[layout.state_t..layout.state_t + 8]
        .copy_from_slice(&f64::INFINITY.to_bits().to_le_bytes());
    resign(&mut nonfinite_time);
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&nonfinite_time),
        Err(EngineStateError::MalformedSnapshot { .. })
    ));

    let mut wrong_value_tag = bytes.clone();
    wrong_value_tag[layout.first_value_tag] = if wrong_value_tag[layout.first_value_tag] == 0 {
        1
    } else {
        0
    };
    resign(&mut wrong_value_tag);
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&wrong_value_tag),
        Err(EngineStateError::MalformedSnapshot { .. })
    ));
}

#[test]
fn integrity_mismatch_reports_recomputed_and_carried_values() {
    let mut bytes = snapshot_bytes();
    bytes[HEADER] ^= 1;
    let expected = fnv(&bytes[..bytes.len() - 16]);
    let found = u128::from_le_bytes(bytes[bytes.len() - 16..].try_into().unwrap());
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&bytes),
        Err(EngineStateError::IntegrityMismatch {
            expected: actual_expected,
            found: actual_found
        }) if actual_expected == expected && actual_found == found
    ));
}

#[test]
fn key_order_is_lexicographic_over_complete_wire_bytes() {
    let short = BlockKey::Authored("z".into());
    let long = BlockKey::Authored("aa".into());
    assert!(
        short < long,
        "wire length prefix must sort before string bytes"
    );

    let low_numeric = ConnectorKey {
        owner: short.clone(),
        direction: WireDir::Out,
        port_index: 1,
    };
    let low_wire = ConnectorKey {
        owner: short,
        direction: WireDir::Out,
        port_index: 256,
    };
    assert!(
        low_wire < low_numeric,
        "little-endian port bytes must govern canonical order"
    );
}

#[test]
fn hostile_section_counts_refuse_before_large_allocation() {
    let mut bytes = snapshot_bytes();
    let layout = offsets(&bytes);
    let enum_count = layout.manifest.start + 1;
    let enum_remaining = layout.manifest.end - (enum_count + 4);
    bytes[enum_count..enum_count + 4].copy_from_slice(&(enum_remaining as u32).to_le_bytes());
    resign(&mut bytes);
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&bytes),
        Err(EngineStateError::MalformedSnapshot { .. })
    ));

    let mut bytes = snapshot_bytes();
    let layout = offsets(&bytes);
    let values_start = layout.manifest.end + 4;
    let values_remaining = bytes.len() - 16 - values_start;
    let value_count = (values_remaining / 2) as u32;
    assert!(value_count > 0 && value_count as usize * 2 <= values_remaining);
    bytes[layout.manifest.end..layout.manifest.end + 4].copy_from_slice(&value_count.to_le_bytes());
    resign(&mut bytes);
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&bytes),
        Err(EngineStateError::MalformedSnapshot { .. })
    ));
}

/// One snapshot defect, in the decoder's refusal order (issue #535 pins the same rule for the
/// compiled-schedule blob, in `tests/blob_refusal_precedence.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Defect {
    /// One byte longer than the snapshot cap.
    Oversized,
    /// One byte shorter than a header and a trailer.
    Short,
    /// A magic other than `OCESTAT\0`.
    Magic,
    /// Format revision 2.
    Format,
    /// The engine version with its patch one higher. The fingerprint, which covers the version,
    /// is left as this build wrote it: no check after the version is reached.
    Version,
    /// A body length one more than the bytes hold.
    BodyLength,
    /// A trailer that does not match the bytes before it.
    Trailer,
    /// A fingerprint that does not match the manifest, the first check of the body.
    Fingerprint,
}

const REFUSAL_ORDER: [Defect; 8] = [
    Defect::Oversized,
    Defect::Short,
    Defect::Magic,
    Defect::Format,
    Defect::Version,
    Defect::BodyLength,
    Defect::Trailer,
    Defect::Fingerprint,
];

fn other_engine_version() -> EngineVersion {
    EngineVersion {
        patch: ENGINE_VERSION.patch.wrapping_add(1),
        ..ENGINE_VERSION
    }
}

fn version_bytes(version: EngineVersion) -> Vec<u8> {
    [version.major, version.minor, version.patch]
        .iter()
        .flat_map(|part| part.to_le_bytes())
        .collect()
}

fn snapshot_cap() -> usize {
    usize::try_from(crate::state::MAX_SNAPSHOT_BYTES).unwrap()
}

/// `valid` with every defect in `defects`: the field edits, each in its own bytes, then a trailer
/// over them (broken afterwards for `Trailer`), then the size edits.
fn with_defects(valid: &[u8], defects: &[Defect]) -> Vec<u8> {
    let mut bytes = valid.to_vec();
    for defect in defects {
        match defect {
            Defect::Magic => bytes[0] ^= 1,
            Defect::Format => bytes[8..12].copy_from_slice(&2u32.to_le_bytes()),
            Defect::Version => {
                bytes[VERSION_FIELD].copy_from_slice(&version_bytes(other_engine_version()));
            }
            Defect::BodyLength => {
                let body = u64_at(&bytes, BODY_LENGTH) + 1;
                bytes[BODY_LENGTH..HEADER].copy_from_slice(&body.to_le_bytes());
            }
            Defect::Fingerprint => bytes[HEADER] ^= 1,
            Defect::Oversized | Defect::Short | Defect::Trailer => {}
        }
    }
    resign(&mut bytes);
    if defects.contains(&Defect::Trailer) {
        *bytes.last_mut().unwrap() ^= 0x80;
    }
    if defects.contains(&Defect::Short) {
        bytes.truncate(HEADER + TRAILER - 1);
    }
    if defects.contains(&Defect::Oversized) {
        let mut oversized = vec![0u8; snapshot_cap() + 1];
        oversized[..bytes.len()].copy_from_slice(&bytes);
        bytes = oversized;
    }
    bytes
}

/// Whether `bytes` carry `defect`, read without the codec; `valid` is the snapshot they were
/// edited from. Not asked of an oversized input, whose trailer would cost a 64 MiB hash.
fn present(defect: Defect, bytes: &[u8], valid: &[u8]) -> bool {
    assert!(
        bytes.len() <= snapshot_cap(),
        "not inspected when oversized"
    );
    let len = bytes.len();
    match defect {
        Defect::Oversized => false,
        Defect::Short => len < HEADER + TRAILER,
        Defect::Magic => bytes[..8] != *b"OCESTAT\0",
        Defect::Format => u32_at(bytes, 8) != 1,
        Defect::Version => bytes[VERSION_FIELD] != *version_bytes(ENGINE_VERSION),
        Defect::BodyLength => {
            usize::try_from(u64_at(bytes, BODY_LENGTH)).unwrap() + HEADER + TRAILER != len
        }
        Defect::Trailer => {
            fnv(&bytes[..len - TRAILER])
                != u128::from_le_bytes(bytes[len - TRAILER..].try_into().unwrap())
        }
        Defect::Fingerprint => {
            len >= HEADER + 16 && bytes[HEADER..HEADER + 16] != valid[HEADER..HEADER + 16]
        }
    }
}

/// The refusal `defect` earns when it is the first defect of `bytes`, as its `Debug` text: the
/// error type has no `PartialEq`, and the text names the variant and every field.
fn refusal(defect: Defect, bytes: &[u8]) -> String {
    let len = bytes.len();
    let error = match defect {
        Defect::Oversized => EngineStateError::SnapshotTooLarge {
            actual_bytes: crate::state::MAX_SNAPSHOT_BYTES + 1,
            max_bytes: crate::state::MAX_SNAPSHOT_BYTES,
        },
        Defect::Short => EngineStateError::MalformedSnapshot {
            offset: 0,
            detail: "snapshot is shorter than the fixed header and trailer".into(),
        },
        Defect::Magic => EngineStateError::MalformedSnapshot {
            offset: 0,
            detail: "invalid state-snapshot magic".into(),
        },
        Defect::Format => EngineStateError::UnsupportedFormat { revision: 2 },
        Defect::Version => EngineStateError::EngineVersionMismatch {
            found: other_engine_version(),
            expected: ENGINE_VERSION,
        },
        Defect::BodyLength => EngineStateError::MalformedSnapshot {
            offset: BODY_LENGTH as u64,
            detail: format!("body length names {} total bytes, found {len}", len + 1),
        },
        Defect::Trailer => EngineStateError::IntegrityMismatch {
            expected: fnv(&bytes[..len - TRAILER]),
            found: u128::from_le_bytes(bytes[len - TRAILER..].try_into().unwrap()),
        },
        Defect::Fingerprint => EngineStateError::MalformedSnapshot {
            offset: HEADER as u64,
            detail: "execution fingerprint does not match its manifest".into(),
        },
    };
    format!("{error:?}")
}

fn outcome(bytes: &[u8]) -> String {
    match EngineStateSnapshot::from_bytes(bytes) {
        Ok(_) => "accepted".into(),
        Err(error) => format!("{error:?}"),
    }
}

#[test]
fn each_snapshot_defect_alone_earns_its_own_refusal() {
    let valid = snapshot_bytes();
    assert_eq!(outcome(&valid), "accepted");
    for defect in REFUSAL_ORDER {
        let bytes = with_defects(&valid, &[defect]);
        // A size defect moves the body length and the trailer with it; every other defect is the
        // only one its bytes carry.
        match defect {
            Defect::Oversized => {}
            Defect::Short => assert!(present(Defect::Short, &bytes, &valid)),
            _ => {
                for other in REFUSAL_ORDER
                    .into_iter()
                    .filter(|&o| o != Defect::Oversized)
                {
                    assert_eq!(
                        present(other, &bytes, &valid),
                        other == defect,
                        "{defect:?}: {other:?}"
                    );
                }
            }
        }
        assert_eq!(outcome(&bytes), refusal(defect, &bytes), "{defect:?}");
    }
}

#[test]
fn of_every_two_snapshot_defects_the_earlier_check_wins() {
    // Every pair, so every two adjacent checks among them. Two pairs cannot occur: an input is not
    // both larger than the cap and shorter than a header, and one shorter than a header and a
    // trailer has no whole fingerprint field.
    let valid = snapshot_bytes();
    for (index, &earlier) in REFUSAL_ORDER.iter().enumerate() {
        for &later in &REFUSAL_ORDER[index + 1..] {
            if matches!(
                (earlier, later),
                (Defect::Oversized, Defect::Short) | (Defect::Short, Defect::Fingerprint)
            ) {
                continue;
            }
            let bytes = with_defects(&valid, &[earlier, later]);
            if earlier != Defect::Oversized {
                assert!(
                    present(earlier, &bytes, &valid),
                    "{earlier:?} with {later:?}"
                );
                assert!(present(later, &bytes, &valid), "{later:?} with {earlier:?}");
            }
            assert_eq!(
                outcome(&bytes),
                refusal(earlier, &bytes),
                "{earlier:?} with {later:?}"
            );
        }
    }
}
