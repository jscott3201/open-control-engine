//! A compiled-schedule blob and a state snapshot release 0.5.0 wrote, presented to this build
//! (issue #683, standing rule 10).
//!
//! Every format stays at revision 1 until 1.0.0, so the engine version in a blob's and a
//! snapshot's header is what separates one layout, or one meaning, from the next. The workspace
//! moved from 0.5.0 to 0.6.0 for a change of meaning with no change of layout: an Integer bound
//! to a Real parameter is held as a Real from 0.6.0 on, so the parameter table, the blob's
//! parameter record and the canonical content id of such a document are no longer what 0.5.0
//! wrote for it. Both fixtures are in this build's layout byte for byte and are refused with
//! `EngineVersionMismatch`, on `BlobError` and on `EngineStateError`. Neither is ever
//! re-blessed. `blob_old_layout.rs` and `state_old_layout.rs` hold the artefacts of the layouts
//! before 0.5.0.
//!
//! - `fixtures/blob_engine_0_5_0.oceb`: the `trim_and_respond_have_hol_false` blob exactly as
//!   release 0.5.0 encoded it (the golden at tag `v0.5.0`, path
//!   `crates/oce-api/tests/fixtures/blob/trim_and_respond_have_hol_false.oceb`; SHA-256
//!   `d0310a6bfab03d851bc1524e976a88befc5899ebc1a832b5bbd8fe6205331e46`). The source binds
//!   the Integer sequence parameter `numIgnReq = 2` to the Real parameter `numIgnReqCon.k`:
//!   release 0.5.0 encoded that parameter as the Integer 2, and this build encodes 2.0 there and
//!   mints another content id.
//! - `fixtures/state_engine_0_5_0.hex`: the snapshot of `fixtures/frame_delay.jsonld` after the
//!   two recipe frames, captured on tag `v0.5.0` (commit `713cbe5`) with the recipe
//!   `state_old_layout.rs` records. It carries the version field, and the field says 0.5.0.

use oce_api::{EngineStateError, EngineStateSnapshot, EngineVersion, OcError};
use oce_graph::blob::{BlobError, ENGINE_VERSION, Executable, FORMAT_VERSION, decode};
// The snapshot byte helpers are declared once in this binary, by `state_old_layout`.
use crate::state_old_layout::snapshot_bytes::{
    ABI_FIELD, BODY_LENGTH_FIELD, FORMAT_FIELD, HEADER_BYTES, TRAILER_BYTES, VERSION_FIELD,
    computed_trailer, delay_after_two_frames, delay_payload, delay_recipe_values,
    delay_recipe_words, delay_target, fingerprint, manifest_range, reseal, restamp,
    restore_from_bytes, stamped_version, trailer, unhex, version_field,
};

/// The versions of an `EngineVersionMismatch`, as `(found, expected)`; any other outcome fails.
fn version_refusal(bytes: &[u8]) -> (EngineVersion, EngineVersion) {
    match EngineStateSnapshot::from_bytes(bytes) {
        Err(EngineStateError::EngineVersionMismatch { found, expected }) => (found, expected),
        Err(other) => panic!("expected EngineVersionMismatch, got {other:?}"),
        Ok(_) => panic!("a snapshot of release 0.5.0 was accepted"),
    }
}

/// The `trim_and_respond_have_hol_false` blob as release 0.5.0 encoded it.
const RELEASE_0_5_0: &[u8] = include_bytes!("fixtures/blob_engine_0_5_0.oceb");
/// The same document's blob as this build encodes it (the checked-in golden).
const SAME_DOCUMENT_NOW: &[u8] =
    include_bytes!("fixtures/blob/trim_and_respond_have_hol_false.oceb");

const ENGINE_0_5_0: EngineVersion = EngineVersion {
    major: 0,
    minor: 5,
    patch: 0,
};

/// Where a blob header carries the engine version, the content id and the trailer.
const VERSION_BYTES: core::ops::Range<usize> = 8..14;
const CONTENT_ID_BYTES: core::ops::Range<usize> = 20..52;

#[test]
fn release_0_5_0_blob_is_sound_in_every_respect_but_its_engine_version() {
    assert_eq!(&RELEASE_0_5_0[..4], b"OCEB");
    assert_eq!(
        u16::from_le_bytes([RELEASE_0_5_0[4], RELEASE_0_5_0[5]]),
        FORMAT_VERSION
    );
    assert_eq!(RELEASE_0_5_0[VERSION_BYTES], [0, 0, 5, 0, 0, 0]);
    let total = u32::from_le_bytes(RELEASE_0_5_0[16..20].try_into().unwrap());
    assert_eq!(usize::try_from(total).unwrap(), RELEASE_0_5_0.len());
    let body_end = RELEASE_0_5_0.len() - 4;
    assert_eq!(
        u32::from_le_bytes(RELEASE_0_5_0[body_end..].try_into().unwrap()),
        oce_graph::blob::crc32c(&RELEASE_0_5_0[..body_end])
    );
}

#[test]
fn release_0_5_0_blob_is_refused_by_its_engine_version() {
    assert_ne!(ENGINE_0_5_0, ENGINE_VERSION);
    assert_eq!(
        decode(RELEASE_0_5_0).err(),
        Some(BlobError::EngineVersionMismatch {
            found: ENGINE_0_5_0,
            expected: ENGINE_VERSION,
        })
    );
    assert!(matches!(
        Executable::from_blob(RELEASE_0_5_0),
        Err(BlobError::EngineVersionMismatch { found, expected })
            if found == ENGINE_0_5_0 && expected == ENGINE_VERSION
    ));
    // The facade's summary reader refuses it the same way: it describes no other version's bytes.
    assert!(matches!(
        oce_api::inspect_blob(RELEASE_0_5_0),
        Err(oce_api::OcError::Blob(BlobError::EngineVersionMismatch { found, expected }))
            if found == ENGINE_0_5_0 && expected == ENGINE_VERSION
    ));
}

#[test]
fn release_0_5_0_blob_differs_from_this_builds_in_one_parameter_and_what_covers_it() {
    // Why the version had to move: the two encodings of one document have the same length and
    // layout, and outside the version, the content id and the trailer they differ in exactly one
    // parameter value, an Integer 2 (tag 1, little-endian `i64`) where this build writes the Real
    // 2.0 (tag 0, the bits of the `f64`).
    assert_eq!(RELEASE_0_5_0.len(), SAME_DOCUMENT_NOW.len());
    let trailer = RELEASE_0_5_0.len() - 4;
    let differing: Vec<usize> = (0..trailer)
        .filter(|index| !VERSION_BYTES.contains(index) && !CONTENT_ID_BYTES.contains(index))
        .filter(|&index| RELEASE_0_5_0[index] != SAME_DOCUMENT_NOW[index])
        .collect();
    let tag = *differing.first().expect("the parameter value moved");
    assert!(differing.iter().all(|index| (tag..tag + 9).contains(index)));
    assert_eq!(RELEASE_0_5_0[tag], 1);
    assert_eq!(RELEASE_0_5_0[tag + 1..tag + 9], 2i64.to_le_bytes());
    assert_eq!(SAME_DOCUMENT_NOW[tag], 0);
    assert_eq!(
        SAME_DOCUMENT_NOW[tag + 1..tag + 9],
        2.0f64.to_bits().to_le_bytes()
    );
    // The content id covers the parameter, so it moved with it.
    assert_ne!(
        RELEASE_0_5_0[CONTENT_ID_BYTES],
        SAME_DOCUMENT_NOW[CONTENT_ID_BYTES]
    );
    assert_eq!(SAME_DOCUMENT_NOW[VERSION_BYTES], {
        let version = ENGINE_VERSION;
        let mut field = [0u8; 6];
        field[..2].copy_from_slice(&version.major.to_le_bytes());
        field[2..4].copy_from_slice(&version.minor.to_le_bytes());
        field[4..].copy_from_slice(&version.patch.to_le_bytes());
        field
    });
}

#[test]
fn release_0_5_0_blob_restamped_with_this_version_decodes_to_the_integer_this_build_never_writes() {
    // The control for the refusal: with only the version bytes and the CRC rewritten, the 0.5.0
    // bytes pass every check this build makes, and carry the Integer on the Real parameter. So
    // the version field is the one thing between an 0.5.0 blob and a silent second reading.
    let mut restamped = RELEASE_0_5_0.to_vec();
    restamped[VERSION_BYTES].copy_from_slice(&SAME_DOCUMENT_NOW[VERSION_BYTES]);
    let body_end = restamped.len() - 4;
    let crc = oce_graph::blob::crc32c(&restamped[..body_end]);
    restamped[body_end..].copy_from_slice(&crc.to_le_bytes());
    assert!(decode(&restamped).is_ok());
    assert_ne!(restamped, SAME_DOCUMENT_NOW);
}

#[test]
fn release_0_5_0_blob_damaged_after_its_version_field_is_still_refused_by_its_version() {
    let mut short = RELEASE_0_5_0.to_vec();
    short.pop();
    let mut trailer = RELEASE_0_5_0.to_vec();
    *trailer.last_mut().unwrap() ^= 0x01;
    let mut body = RELEASE_0_5_0.to_vec();
    body[52..].fill(0xa5);
    for (what, bytes) in [
        ("one byte short", short),
        ("a flipped trailer bit", trailer),
        ("a body of noise", body),
    ] {
        assert_eq!(
            decode(&bytes).err(),
            Some(BlobError::EngineVersionMismatch {
                found: ENGINE_0_5_0,
                expected: ENGINE_VERSION,
            }),
            "{what}"
        );
    }
}

fn release_0_5_0_snapshot() -> Vec<u8> {
    unhex(include_str!("fixtures/state_engine_0_5_0.hex"))
}

#[test]
fn release_0_5_0_snapshot_is_sound_in_every_respect_but_its_engine_version() {
    // A whole, uncorrupted snapshot in this build's layout: both revisions, a consistent length,
    // a valid trailer and a fingerprint that matches the manifest under the version it carries.
    let bytes = release_0_5_0_snapshot();
    assert_eq!(&bytes[..8], b"OCESTAT\0");
    assert_eq!(bytes[FORMAT_FIELD], 1u32.to_le_bytes());
    assert_eq!(bytes[ABI_FIELD], 1u32.to_le_bytes());
    assert_eq!(bytes[VERSION_FIELD], [0, 0, 5, 0, 0, 0]);
    assert_eq!(stamped_version(&bytes), ENGINE_0_5_0);
    let body = u64::from_le_bytes(bytes[BODY_LENGTH_FIELD].try_into().unwrap());
    assert_eq!(
        usize::try_from(body).unwrap() + HEADER_BYTES + TRAILER_BYTES,
        bytes.len()
    );
    assert_eq!(trailer(&bytes), computed_trailer(&bytes));
    let manifest = manifest_range(&bytes, HEADER_BYTES);
    assert_eq!(
        u128::from_le_bytes(bytes[HEADER_BYTES..HEADER_BYTES + 16].try_into().unwrap()),
        fingerprint(1, ENGINE_0_5_0, &bytes[manifest])
    );
    // And it holds the state the recipe produces.
    let (values, words) = delay_payload(&bytes, HEADER_BYTES);
    assert_eq!(values, delay_recipe_values());
    assert_eq!(words, delay_recipe_words());
}

#[test]
fn release_0_5_0_snapshot_is_refused_by_its_engine_version() {
    assert_ne!(ENGINE_0_5_0, ENGINE_VERSION);
    assert_eq!(
        version_refusal(&release_0_5_0_snapshot()),
        (ENGINE_0_5_0, ENGINE_VERSION)
    );
}

#[test]
fn host_restore_path_stops_at_decode_for_the_release_0_5_0_snapshot() {
    let mut target = delay_target();
    let loaded = target.state_snapshot().unwrap().into_bytes();
    match restore_from_bytes(&mut target, &release_0_5_0_snapshot()) {
        Err(OcError::State(EngineStateError::EngineVersionMismatch { found, expected })) => {
            assert_eq!((found, expected), (ENGINE_0_5_0, ENGINE_VERSION));
        }
        other => panic!("expected EngineVersionMismatch, got {other:?}"),
    }
    // The target keeps its load state and its restore window: this build's own capture of the
    // same state still restores into it.
    assert_eq!(target.state_snapshot().unwrap().as_bytes(), loaded);
    let stamped = delay_after_two_frames()
        .state_snapshot()
        .unwrap()
        .into_bytes();
    restore_from_bytes(&mut target, &stamped).unwrap();
    assert_eq!(target.state_snapshot().unwrap().as_bytes(), stamped);
}

#[test]
fn release_0_5_0_snapshot_differs_from_this_builds_capture_only_where_the_version_reaches() {
    // The recipe's model binds no Integer to a Real parameter, so release 0.5.0 and this build
    // capture the same state in the same bytes apart from the version field and the two digests
    // that cover it. Stamping this build's capture 0.5.0 by hand (version field, fingerprint,
    // trailer; `restamp` shares no code with the codec) gives the fixture byte for byte: nothing
    // but the version separates the two, which is why the version is what refuses it.
    let fixture = release_0_5_0_snapshot();
    let captured = delay_after_two_frames()
        .state_snapshot()
        .unwrap()
        .into_bytes();
    assert_eq!(restamp(&captured, ENGINE_0_5_0), fixture);
    assert_eq!(fixture.len(), captured.len());
    let fingerprint_field = HEADER_BYTES..HEADER_BYTES + 16;
    let trailer_start = fixture.len() - TRAILER_BYTES;
    for index in 0..fixture.len() {
        let may_differ = VERSION_FIELD.contains(&index)
            || fingerprint_field.contains(&index)
            || index >= trailer_start;
        assert!(
            may_differ || fixture[index] == captured[index],
            "byte {index} differs outside the version, fingerprint and trailer"
        );
    }
    assert_ne!(fixture[VERSION_FIELD], captured[VERSION_FIELD]);
}

#[test]
fn release_0_5_0_snapshot_with_only_its_version_bytes_edited_fails_its_trailer_then_fingerprint() {
    // A host cannot move the snapshot forward by rewriting the six version bytes: the trailer
    // covers them, and with the trailer recomputed the fingerprint, which covers them too, does
    // not match its manifest.
    let mut edited = release_0_5_0_snapshot();
    edited[VERSION_FIELD].copy_from_slice(&version_field(ENGINE_VERSION));
    match EngineStateSnapshot::from_bytes(&edited) {
        Err(EngineStateError::IntegrityMismatch { expected, found }) => assert_eq!(
            (expected, found),
            (computed_trailer(&edited), trailer(&edited))
        ),
        Err(other) => panic!("expected IntegrityMismatch, got {other:?}"),
        Ok(_) => panic!("an edited 0.5.0 snapshot was accepted"),
    }
    reseal(&mut edited);
    match EngineStateSnapshot::from_bytes(&edited) {
        Err(EngineStateError::MalformedSnapshot { offset, detail }) => assert_eq!(
            (offset, detail.as_str()),
            (
                HEADER_BYTES as u64,
                "execution fingerprint does not match its manifest"
            )
        ),
        Err(other) => panic!("expected MalformedSnapshot, got {other:?}"),
        Ok(_) => panic!("an edited and resealed 0.5.0 snapshot was accepted"),
    }
}
