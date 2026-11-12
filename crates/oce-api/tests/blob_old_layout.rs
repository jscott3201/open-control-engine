//! A compiled-schedule blob an earlier release wrote under the same format number (issue #509).
//!
//! Every format stays at revision 1 until 1.0.0 and changes in place, so the format field cannot
//! tell an older layout from the current one: the engine version in the header does.
//! `fixtures/blob_legacy.oceb` is the `generic_air_economizer_high_limits_ashrae_differential`
//! blob exactly as release 0.4.0 encoded it: format 1, with the retired FNV-1a-128 export tag in
//! the 32-byte content-id field where the current layout carries the canonical content id's
//! SHA-256 digest. Its sections, class hashes and CRC are all valid for this build, and it is
//! never re-blessed. The decoder refuses it at the engine-version check, and the execution entry
//! builds no executable from it.

use oce_graph::blob::{
    BlobError, ENGINE_VERSION, EngineVersion, Executable, FORMAT_VERSION, decode,
};

const OLD_LAYOUT: &[u8] = include_bytes!("fixtures/blob_legacy.oceb");

/// The engine version the fixture's encoder wrote into its header.
const OLD_ENGINE: EngineVersion = EngineVersion {
    major: 0,
    minor: 4,
    patch: 0,
};

#[test]
fn old_layout_fixture_passes_every_header_check_before_the_engine_version() {
    // The precondition of the refusal tests below: magic, format, length and CRC are all valid, so
    // the engine version refuses these bytes whatever order the header checks run in.
    assert_eq!(&OLD_LAYOUT[..4], b"OCEB");
    assert_eq!(
        u16::from_le_bytes([OLD_LAYOUT[4], OLD_LAYOUT[5]]),
        FORMAT_VERSION
    );
    let total = u32::from_le_bytes(OLD_LAYOUT[16..20].try_into().unwrap());
    assert_eq!(usize::try_from(total).unwrap(), OLD_LAYOUT.len());
    let body_end = OLD_LAYOUT.len() - 4;
    assert_eq!(
        u32::from_le_bytes(OLD_LAYOUT[body_end..].try_into().unwrap()),
        oce_graph::blob::crc32c(&OLD_LAYOUT[..body_end])
    );
    // The old layout's content-id field: a 16-byte export tag, then 16 zero bytes.
    assert!(OLD_LAYOUT[20..36].iter().any(|&byte| byte != 0));
    assert_eq!(OLD_LAYOUT[36..52], [0u8; 16]);
}

#[test]
fn old_layout_blob_sharing_the_format_number_is_refused_by_its_engine_version() {
    assert_ne!(OLD_ENGINE, ENGINE_VERSION);
    assert_eq!(
        decode(OLD_LAYOUT).err(),
        Some(BlobError::EngineVersionMismatch {
            found: OLD_ENGINE,
            expected: ENGINE_VERSION,
        })
    );
}

#[test]
fn old_layout_blob_never_becomes_an_executable() {
    assert!(matches!(
        Executable::from_blob(OLD_LAYOUT),
        Err(BlobError::EngineVersionMismatch { found, expected })
            if found == OLD_ENGINE && expected == ENGINE_VERSION
    ));
}

#[test]
fn old_layout_blob_damaged_after_its_version_field_is_still_refused_by_its_version() {
    // The engine version is checked straight after the format (issue #535), so damage to what
    // another version laid out (its length, its trailer, its profile, its body) never turns the
    // refusal into a length or integrity one. None of these edits recomputes the CRC.
    let mut short = OLD_LAYOUT.to_vec();
    short.pop();
    let mut long = OLD_LAYOUT.to_vec();
    long.push(0);
    let mut trailer = OLD_LAYOUT.to_vec();
    *trailer.last_mut().unwrap() ^= 0x01;
    let mut length = OLD_LAYOUT.to_vec();
    length[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    let mut profile = OLD_LAYOUT.to_vec();
    profile[6] = 2;
    let mut body = OLD_LAYOUT.to_vec();
    body[52..].fill(0xa5);
    for (what, bytes) in [
        ("one byte short", short),
        ("one byte long", long),
        ("a flipped trailer bit", trailer),
        ("an absurd total length", length),
        ("another profile", profile),
        ("a body of noise", body),
    ] {
        assert_eq!(
            decode(&bytes).err(),
            Some(BlobError::EngineVersionMismatch {
                found: OLD_ENGINE,
                expected: ENGINE_VERSION,
            }),
            "{what}"
        );
    }
}
