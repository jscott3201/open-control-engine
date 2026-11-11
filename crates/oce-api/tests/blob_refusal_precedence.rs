//! Which refusal wins when a compiled-schedule blob is wrong in more than one way (issue #535).
//!
//! The decoder's header checks run in one order: size, a header and trailer's worth of bytes,
//! magic, format, engine version, total length, CRC, profile, flags, then the structure. The
//! engine version comes straight after the format, before anything another engine version may
//! lay out differently under the same format number, as in the state snapshot decoder: a blob
//! another engine version wrote is refused by its version however else it is damaged.
//!
//! Each defect here is an edit of its own header bytes, written from the header table in
//! `docs/design/compiled-schedule-blob.md`. `with_defects` applies any set of them to a golden so
//! that every one is present at once, and `present` re-reads the bytes to prove it, so a pair
//! test cannot pass because one edit undid the other. The expected refusals are spelled out from
//! the table, not read back from the decoder.

// Shared helpers, declared once for this binary in its root `facade.rs`.
use crate::blob_corpus;

use blob_corpus::golden;
use oce_graph::blob::{
    BlobError, ENGINE_VERSION, EXECUTION_PROFILE, EngineVersion, FORMAT_VERSION, HEADER_BYTES,
    MAX_BLOB_BYTES, MalformedFault, TRAILER_BYTES, crc32c, decode,
};

const WORKED: &str = "generic_air_economizer_high_limits_ashrae_differential";

/// The engine-version field: major, minor and patch, each a little-endian `u16`.
const VERSION_FIELD: std::ops::Range<usize> = 8..14;
/// The total-length field, a little-endian `u32`.
const LENGTH_FIELD: std::ops::Range<usize> = 16..20;
/// The shortest input the decoder reads a header from: a header and a trailer with no body.
const SHORTEST_INSPECTED: usize = HEADER_BYTES + TRAILER_BYTES;

/// One defect, in the decoder's refusal order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Defect {
    /// One byte longer than [`MAX_BLOB_BYTES`].
    Oversized,
    /// One byte shorter than a header and a trailer.
    Short,
    /// A magic other than `OCEB`.
    Magic,
    /// Format version 2.
    Format,
    /// The engine version with its patch one higher.
    Version,
    /// A total length one more than the input's.
    Length,
    /// A trailer that does not match the bytes before it.
    Crc,
    /// Execution profile 2.
    Profile,
    /// Reserved flag bit 0 set.
    Flags,
    /// A first section id of 9, the first structural fault.
    Section,
}

/// Every defect, in the order the decoder checks for it.
const REFUSAL_ORDER: [Defect; 10] = [
    Defect::Oversized,
    Defect::Short,
    Defect::Magic,
    Defect::Format,
    Defect::Version,
    Defect::Length,
    Defect::Crc,
    Defect::Profile,
    Defect::Flags,
    Defect::Section,
];

/// The engine version the `Version` defect writes.
fn other_version() -> EngineVersion {
    EngineVersion {
        patch: ENGINE_VERSION.patch.wrapping_add(1),
        ..ENGINE_VERSION
    }
}

fn version_field(version: EngineVersion) -> [u8; 6] {
    let mut field = [0u8; 6];
    for (at, part) in [version.major, version.minor, version.patch]
        .into_iter()
        .enumerate()
    {
        field[2 * at..2 * at + 2].copy_from_slice(&part.to_le_bytes());
    }
    field
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

/// The CRC-32C trailer the bytes carry and the one computed over the bytes before it.
fn trailer_pair(bytes: &[u8]) -> (u32, u32) {
    let end = bytes.len() - TRAILER_BYTES;
    (u32_at(bytes, end), crc32c(&bytes[..end]))
}

/// `valid` with every defect in `defects`. The field edits come first, each in its own header
/// bytes; then a trailer over them, broken afterwards for `Crc`; then the size edits, which leave
/// the header bytes in place. A defect absent from the set is absent from the bytes.
fn with_defects(valid: &[u8], defects: &[Defect]) -> Vec<u8> {
    let mut bytes = valid.to_vec();
    for defect in defects {
        match defect {
            Defect::Magic => bytes[3] = b'C',
            Defect::Format => bytes[4..6].copy_from_slice(&2u16.to_le_bytes()),
            Defect::Profile => bytes[6..8].copy_from_slice(&2u16.to_le_bytes()),
            Defect::Version => {
                bytes[VERSION_FIELD].copy_from_slice(&version_field(other_version()))
            }
            Defect::Flags => bytes[14..16].copy_from_slice(&1u16.to_le_bytes()),
            Defect::Length => {
                let total = u32_at(&bytes, LENGTH_FIELD.start) + 1;
                bytes[LENGTH_FIELD].copy_from_slice(&total.to_le_bytes());
            }
            Defect::Section => {
                bytes[HEADER_BYTES..HEADER_BYTES + 4].copy_from_slice(&9u32.to_le_bytes());
            }
            Defect::Oversized | Defect::Short | Defect::Crc => {}
        }
    }
    let body_end = bytes.len() - TRAILER_BYTES;
    let crc = crc32c(&bytes[..body_end]);
    bytes[body_end..].copy_from_slice(&crc.to_le_bytes());
    if defects.contains(&Defect::Crc) {
        bytes[body_end] ^= 0x80;
    }
    if defects.contains(&Defect::Short) {
        bytes.truncate(SHORTEST_INSPECTED - 1);
    }
    if defects.contains(&Defect::Oversized) {
        // Zeroed in one allocation, then the edited blob copied to the front.
        let mut oversized = vec![0u8; MAX_BLOB_BYTES + 1];
        oversized[..bytes.len()].copy_from_slice(&bytes);
        bytes = oversized;
    }
    bytes
}

/// Whether `bytes` carry `defect`, read from the header table without the decoder. Not asked of
/// an oversized input, whose trailer would cost a 64 MiB CRC to inspect.
fn present(defect: Defect, bytes: &[u8]) -> bool {
    assert!(
        bytes.len() <= MAX_BLOB_BYTES,
        "not inspected when oversized"
    );
    match defect {
        Defect::Oversized => false,
        Defect::Short => bytes.len() < SHORTEST_INSPECTED,
        Defect::Magic => bytes[..4] != *b"OCEB",
        Defect::Format => u16_at(bytes, 4) != FORMAT_VERSION,
        Defect::Profile => u16_at(bytes, 6) != EXECUTION_PROFILE,
        Defect::Version => bytes[VERSION_FIELD] != version_field(ENGINE_VERSION),
        Defect::Flags => u16_at(bytes, 14) != 0,
        Defect::Length => {
            usize::try_from(u32_at(bytes, LENGTH_FIELD.start)).unwrap() != bytes.len()
        }
        Defect::Crc => {
            let (stored, computed) = trailer_pair(bytes);
            stored != computed
        }
        Defect::Section => bytes.len() >= HEADER_BYTES + 4 && u32_at(bytes, HEADER_BYTES) != 1,
    }
}

/// The refusal `defect` earns when it is the first defect of `bytes`.
fn refusal(defect: Defect, bytes: &[u8]) -> BlobError {
    match defect {
        Defect::Oversized => BlobError::TooLarge {
            length: MAX_BLOB_BYTES as u64 + 1,
            limit: MAX_BLOB_BYTES as u64,
        },
        Defect::Short => BlobError::Malformed {
            offset: u32::try_from(SHORTEST_INSPECTED - 1).unwrap(),
            fault: MalformedFault::Truncated,
        },
        Defect::Magic => BlobError::Malformed {
            offset: 0,
            fault: MalformedFault::BadMagic,
        },
        Defect::Format => BlobError::UnsupportedFormat {
            found: 2,
            supported: 1,
        },
        Defect::Version => BlobError::EngineVersionMismatch {
            found: other_version(),
            expected: ENGINE_VERSION,
        },
        Defect::Length => BlobError::Malformed {
            offset: u32::try_from(LENGTH_FIELD.start).unwrap(),
            fault: MalformedFault::LengthMismatch,
        },
        Defect::Crc => {
            let (stored, computed) = trailer_pair(bytes);
            BlobError::IntegrityMismatch { stored, computed }
        }
        Defect::Profile => BlobError::UnsupportedProfile {
            found: 2,
            supported: 1,
        },
        Defect::Flags => BlobError::NonzeroFlags { flags: 1 },
        Defect::Section => BlobError::Malformed {
            offset: u32::try_from(HEADER_BYTES).unwrap(),
            fault: MalformedFault::SectionId,
        },
    }
}

#[test]
fn each_defect_alone_earns_its_own_refusal() {
    let valid = golden(WORKED);
    decode(&valid).unwrap();
    for defect in REFUSAL_ORDER {
        let bytes = with_defects(&valid, &[defect]);
        // A size defect moves the total length and the trailer with it; every other defect is the
        // only one its bytes carry.
        match defect {
            Defect::Oversized => {}
            Defect::Short => assert!(present(Defect::Short, &bytes)),
            _ => {
                for other in REFUSAL_ORDER
                    .into_iter()
                    .filter(|&o| o != Defect::Oversized)
                {
                    assert_eq!(
                        present(other, &bytes),
                        other == defect,
                        "{defect:?}: {other:?}"
                    );
                }
            }
        }
        assert_eq!(
            decode(&bytes).err(),
            Some(refusal(defect, &bytes)),
            "{defect:?}"
        );
    }
}

#[test]
fn of_every_two_defects_the_earlier_check_wins() {
    // Every pair, so every two adjacent checks among them. Two pairs cannot occur: an input is not
    // both larger than the limit and shorter than a header, and one shorter than a header and a
    // trailer has no byte where the first section id would be.
    let valid = golden(WORKED);
    for (index, &earlier) in REFUSAL_ORDER.iter().enumerate() {
        for &later in &REFUSAL_ORDER[index + 1..] {
            if matches!(
                (earlier, later),
                (Defect::Oversized, Defect::Short) | (Defect::Short, Defect::Section)
            ) {
                continue;
            }
            let bytes = with_defects(&valid, &[earlier, later]);
            if earlier != Defect::Oversized {
                assert!(present(earlier, &bytes), "{earlier:?} with {later:?}");
                assert!(present(later, &bytes), "{later:?} with {earlier:?}");
            }
            assert_eq!(
                decode(&bytes).err(),
                Some(refusal(earlier, &bytes)),
                "{earlier:?} with {later:?}"
            );
        }
    }
}

#[test]
fn another_engine_version_wins_over_every_later_defect_at_once() {
    let valid = golden(WORKED);
    let later = [
        Defect::Version,
        Defect::Length,
        Defect::Crc,
        Defect::Profile,
        Defect::Flags,
        Defect::Section,
    ];
    let bytes = with_defects(&valid, &later);
    for defect in later {
        assert!(present(defect, &bytes), "{defect:?}");
    }
    assert_eq!(decode(&bytes).err(), Some(refusal(Defect::Version, &bytes)));
    // And only the size, the magic and the format win over it.
    for earlier in [
        Defect::Oversized,
        Defect::Short,
        Defect::Magic,
        Defect::Format,
    ] {
        let mut all = vec![earlier];
        all.extend(later);
        let bytes = with_defects(&valid, &all);
        assert_eq!(
            decode(&bytes).err(),
            Some(refusal(earlier, &bytes)),
            "{earlier:?}"
        );
    }
}

#[test]
fn a_blob_of_another_engine_version_is_refused_by_its_version_at_every_truncation() {
    // Every truncation and every extension of a blob another version wrote: below a header and a
    // trailer the bytes are too short to be a blob of any version, and from there on the version
    // refuses them, though the total length and the CRC no longer match either.
    let mut extended = with_defects(&golden(WORKED), &[Defect::Version]);
    extended.extend_from_slice(&[0; 64]);
    for length in 0..=extended.len() {
        let bytes = &extended[..length];
        let expected = if length < SHORTEST_INSPECTED {
            BlobError::Malformed {
                offset: u32::try_from(length).unwrap(),
                fault: MalformedFault::Truncated,
            }
        } else {
            refusal(Defect::Version, bytes)
        };
        assert_eq!(decode(bytes).err(), Some(expected), "length {length}");
    }
}

#[test]
fn this_builds_blob_meets_the_length_check_at_every_truncation() {
    // The control for the test above: with this build's version in place the same truncations and
    // extensions reach the total-length field, which refuses them before the CRC is read.
    let valid = golden(WORKED);
    let mut extended = valid.clone();
    extended.extend_from_slice(&[0; 64]);
    for length in SHORTEST_INSPECTED..=extended.len() {
        if length == valid.len() {
            continue;
        }
        assert_eq!(
            decode(&extended[..length]).err(),
            Some(refusal(Defect::Length, &extended[..length])),
            "length {length}"
        );
    }
}

#[test]
fn every_flipped_bit_of_the_version_field_is_a_version_refusal() {
    // No trailer is recomputed: the version is checked before integrity, so a corrupted version
    // field reports the version it now spells, as a snapshot's does.
    let valid = golden(WORKED);
    for bit in 0..VERSION_FIELD.len() * 8 {
        let mut bytes = valid.clone();
        bytes[VERSION_FIELD.start + bit / 8] ^= 1 << (bit % 8);
        let found = EngineVersion {
            major: u16_at(&bytes, 8),
            minor: u16_at(&bytes, 10),
            patch: u16_at(&bytes, 12),
        };
        assert_ne!(found, ENGINE_VERSION);
        assert!(present(Defect::Crc, &bytes), "bit {bit}");
        assert_eq!(
            decode(&bytes).err(),
            Some(BlobError::EngineVersionMismatch {
                found,
                expected: ENGINE_VERSION
            }),
            "bit {bit}"
        );
    }
}

#[test]
fn a_foreign_version_field_rewritten_to_this_builds_fails_the_crc() {
    // A blob another version wrote, consistent in every field, with only its six version bytes
    // rewritten to claim this build: the trailer covers the header, so the edit is caught as
    // corruption, as it is in a snapshot.
    let foreign = with_defects(&golden(WORKED), &[Defect::Version]);
    assert!(!present(Defect::Crc, &foreign));
    let mut patched = foreign.clone();
    patched[VERSION_FIELD].copy_from_slice(&version_field(ENGINE_VERSION));
    let changed: Vec<usize> = (0..patched.len())
        .filter(|&index| patched[index] != foreign[index])
        .collect();
    assert!(!changed.is_empty() && changed.iter().all(|index| VERSION_FIELD.contains(index)));
    assert_eq!(decode(&patched).err(), Some(refusal(Defect::Crc, &patched)));
}

#[test]
fn every_flipped_bit_of_the_profile_or_flags_is_still_an_integrity_refusal() {
    // The fields after the version keep their place behind the CRC: a corrupted profile or flags
    // field of this build's blob is reported as corruption, not as the value it now spells.
    let valid = golden(WORKED);
    for byte in [6usize, 7, 14, 15] {
        for bit in 0..8 {
            let mut bytes = valid.clone();
            bytes[byte] ^= 1 << bit;
            assert_eq!(
                decode(&bytes).err(),
                Some(refusal(Defect::Crc, &bytes)),
                "byte {byte} bit {bit}"
            );
        }
    }
}
