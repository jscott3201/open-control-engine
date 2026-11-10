//! State snapshots earlier layouts wrote, presented to this build (issues #509 and #530).
//!
//! Every format stays at revision 1 until 1.0.0 and changes in place, so the format field cannot
//! tell an older layout from the current one: the engine version in the header does, and none of
//! these three fixtures has that field. Their headers put the body length at bytes 16..24, where
//! this build reads the version (16..22), so each is refused with `EngineVersionMismatch` naming
//! the low six bytes of its body length as the version it "found". None is ever re-blessed.
//!
//! - `fixtures/state_unstamped.hex`: revision 1 without the version field, the layout
//!   `development` wrote between the format reset (GitHub #518) and the change that stamped the
//!   version. Captured on `development` at commit `4c89577`, the parent of that change, with the
//!   recipe below. No release wrote this layout.
//! - `fixtures/state_revision_two.hex`: the same state as releases 0.2.0 to 0.4.0 wrote it, which
//!   numbered their snapshots format revision 2 and execution-state ABI revision 2. Captured with
//!   the same recipe on the build before the format reset (state codec of release 0.4.0), so it
//!   refuses by its format field first.
//! - `fixtures/state_legacy.hex`: the empty-authored-model vector (an in-memory model with no
//!   blocks, id `urn:test:empty-model`, before any frame) of a layout that predates the first
//!   release with a snapshot API. Its header carries revision 1 twice.
//!
//! **Capture recipe** of the two populated fixtures (`support/snapshot_bytes.rs` holds it as
//! code): load `fixtures/frame_delay.jsonld` (an `Add` into a `UnitDelay`, sample period 1 s);
//! accept two complete frames, at model time 0.0 and then 1.0, each with `urn:frame:a` = 0.5 and
//! `urn:frame:b` = 2.0; take `state_snapshot()` and write its bytes as one line of lowercase hex.
//! The state is then model time 1.0, `add.y` and `delay.y` 2.5 and five `UnitDelay` words. To
//! regenerate a fixture, run that recipe on the commit named for it.
//! `populated_fixtures_hold_one_state_under_two_numberings` checks that the two captures are the
//! same state.

#[path = "support/snapshot_bytes.rs"]
mod snapshot_bytes;

use oce_api::{EngineStateError, EngineStateSnapshot, EngineVersion};
use oce_graph::blob::ENGINE_VERSION;
use snapshot_bytes::{
    ABI_FIELD, BODY_LENGTH_FIELD, FORMAT_FIELD, HEADER_BYTES, TRAILER_BYTES,
    UNSTAMPED_HEADER_BYTES, VERSION_FIELD, computed_trailer, delay_target, manifest_range, reseal,
    trailer, unhex, unstamped_fingerprint, version_field,
};

/// What this build reads at the version field of the populated fixtures: their body length is
/// 1462 bytes (`b6 05 00 00 00 00 00 00`), and its low six bytes spell 1462.0.0.
const POPULATED_READS_AS: EngineVersion = EngineVersion {
    major: 1462,
    minor: 0,
    patch: 0,
};

/// The same for the empty-model vector, whose body is 114 bytes.
const EMPTY_READS_AS: EngineVersion = EngineVersion {
    major: 114,
    minor: 0,
    patch: 0,
};

/// The body length an unstamped header carries at bytes 16..24.
fn unstamped_body_length(bytes: &[u8]) -> usize {
    usize::try_from(u64::from_le_bytes(bytes[16..24].try_into().unwrap())).unwrap()
}

/// The fingerprint an unstamped body starts with.
fn unstamped_carried_fingerprint(bytes: &[u8]) -> u128 {
    u128::from_le_bytes(
        bytes[UNSTAMPED_HEADER_BYTES..UNSTAMPED_HEADER_BYTES + 16]
            .try_into()
            .unwrap(),
    )
}

/// Check that `bytes` are a whole, uncorrupted snapshot of the unstamped layout under the given
/// revision numbers: a consistent length, a valid trailer and a fingerprint that matches the
/// manifest. Nothing but the header layout and its numbers can then refuse them.
fn assert_sound_unstamped(bytes: &[u8], revisions: u32) {
    assert_eq!(&bytes[..8], b"OCESTAT\0");
    assert_eq!(bytes[FORMAT_FIELD], revisions.to_le_bytes());
    assert_eq!(bytes[ABI_FIELD], revisions.to_le_bytes());
    assert_eq!(
        unstamped_body_length(bytes) + UNSTAMPED_HEADER_BYTES + TRAILER_BYTES,
        bytes.len()
    );
    assert_eq!(trailer(bytes), computed_trailer(bytes));
    let manifest = manifest_range(bytes, UNSTAMPED_HEADER_BYTES);
    assert_eq!(
        unstamped_carried_fingerprint(bytes),
        unstamped_fingerprint(revisions, &bytes[manifest])
    );
}

fn unstamped_snapshot() -> Vec<u8> {
    let bytes = unhex(include_str!("fixtures/state_unstamped.hex"));
    assert_sound_unstamped(&bytes, 1);
    bytes
}

fn release_snapshot() -> Vec<u8> {
    let bytes = unhex(include_str!("fixtures/state_revision_two.hex"));
    assert_sound_unstamped(&bytes, 2);
    bytes
}

/// The versions of an `EngineVersionMismatch`, as `(found, expected)`; any other outcome fails.
fn version_refusal(bytes: &[u8]) -> (EngineVersion, EngineVersion) {
    match EngineStateSnapshot::from_bytes(bytes) {
        Err(EngineStateError::EngineVersionMismatch { found, expected }) => (found, expected),
        Err(other) => panic!("expected EngineVersionMismatch, got {other:?}"),
        Ok(_) => panic!("a snapshot of an earlier layout was accepted"),
    }
}

/// An unstamped snapshot given this build's header by hand: the six version bytes inserted after
/// the ABI revision, everything else as it was.
fn with_version_field_inserted(unstamped: &[u8]) -> Vec<u8> {
    let mut bytes = unstamped[..VERSION_FIELD.start].to_vec();
    bytes.extend_from_slice(&version_field(ENGINE_VERSION));
    bytes.extend_from_slice(&unstamped[VERSION_FIELD.start..]);
    bytes
}

#[test]
fn unstamped_revision_one_snapshot_is_refused_at_the_version_field() {
    // Magic, both revision fields, length, trailer and fingerprint are all sound for the layout
    // that wrote it (checked by `unstamped_snapshot`), and both revisions are the ones this
    // build writes. The missing version field is what refuses it.
    let bytes = unstamped_snapshot();
    assert_eq!(bytes[VERSION_FIELD], version_field(POPULATED_READS_AS));
    assert_ne!(POPULATED_READS_AS, ENGINE_VERSION);
    assert_eq!(
        version_refusal(&bytes),
        (POPULATED_READS_AS, ENGINE_VERSION)
    );
}

#[test]
fn unstamped_snapshot_never_reaches_a_restore_target() {
    // A target that the fixture's state would fit exactly keeps its own state: the refusal
    // happens at parse, so there is no snapshot to restore.
    let target = delay_target();
    let before = target.state_snapshot().unwrap();
    version_refusal(&unstamped_snapshot());
    assert_eq!(
        target.state_snapshot().unwrap().as_bytes(),
        before.as_bytes()
    );
}

#[test]
fn unstamped_snapshot_given_a_version_field_fails_its_trailer_then_its_fingerprint() {
    // The body after the header is laid out as this build lays it out, so a host could try to
    // move an unstamped snapshot forward by inserting the version bytes. The trailer refuses the
    // edit; with the trailer recomputed the fingerprint does, because it now covers the version
    // and the one in the body was taken without it.
    let mut edited = with_version_field_inserted(&unstamped_snapshot());
    assert_eq!(
        usize::try_from(u64::from_le_bytes(
            edited[BODY_LENGTH_FIELD].try_into().unwrap()
        ))
        .unwrap()
            + HEADER_BYTES
            + TRAILER_BYTES,
        edited.len()
    );
    match EngineStateSnapshot::from_bytes(&edited) {
        Err(EngineStateError::IntegrityMismatch { expected, found }) => assert_eq!(
            (expected, found),
            (computed_trailer(&edited), trailer(&edited))
        ),
        Err(other) => panic!("expected IntegrityMismatch, got {other:?}"),
        Ok(_) => panic!("an edited unstamped snapshot was accepted"),
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
        Ok(_) => panic!("an edited and resealed unstamped snapshot was accepted"),
    }
}

#[test]
fn unstamped_header_whose_length_spells_this_version_is_refused_at_the_length_field() {
    // The one unstamped header the version check cannot refuse: a body whose length, in its low
    // six bytes, is this build's version field (327680 bytes for 0.5.0). The top two bytes of that
    // length and the first six of the fingerprint are then read as the body length, which cannot
    // be the true one. When the spelled length is past the 64 MiB cap (any patch above zero), no
    // such snapshot can exist and the size check refuses a buffer that long.
    let mut spelled = [0u8; 8];
    spelled[..6].copy_from_slice(&version_field(ENGINE_VERSION));
    let body = u64::from_le_bytes(spelled);
    let total = body + (UNSTAMPED_HEADER_BYTES + TRAILER_BYTES) as u64;
    if total > 64 * 1024 * 1024 {
        return;
    }
    let mut bytes = b"OCESTAT\0".to_vec();
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&spelled);
    bytes.resize(usize::try_from(total).unwrap(), 0xee);
    reseal(&mut bytes);
    assert_eq!(bytes[VERSION_FIELD], version_field(ENGINE_VERSION));
    match EngineStateSnapshot::from_bytes(&bytes) {
        Err(EngineStateError::MalformedSnapshot { offset, .. }) => {
            assert_eq!(offset, BODY_LENGTH_FIELD.start as u64);
        }
        Err(other) => panic!("expected MalformedSnapshot, got {other:?}"),
        Ok(_) => panic!("an unstamped header was accepted"),
    }
}

#[test]
fn populated_release_snapshot_is_refused_by_its_format_revision() {
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&release_snapshot()),
        Err(EngineStateError::UnsupportedFormat { revision: 2 })
    ));
}

#[test]
fn release_snapshot_with_edited_revision_fields_is_refused_at_the_version_field() {
    // A host that rewrites the format field, or both revision fields, and recomputes the trailer
    // has bytes whose numbers are this build's. They are still an unstamped header.
    let mut format_only = release_snapshot();
    format_only[FORMAT_FIELD].copy_from_slice(&1u32.to_le_bytes());
    reseal(&mut format_only);

    let mut both = format_only.clone();
    both[ABI_FIELD].copy_from_slice(&1u32.to_le_bytes());
    reseal(&mut both);

    for bytes in [&format_only, &both] {
        assert_eq!(version_refusal(bytes), (POPULATED_READS_AS, ENGINE_VERSION));
    }
}

#[test]
fn pre_release_layout_vector_is_refused_at_the_version_field() {
    let bytes = unhex(include_str!("fixtures/state_legacy.hex"));
    // The header names the numbers this build writes, the length is consistent and the trailer
    // is valid. (Its manifest also ends before the input-definition count this build requires,
    // which no check reaches any more.)
    assert_eq!(&bytes[..8], b"OCESTAT\0");
    assert_eq!(bytes[FORMAT_FIELD], 1u32.to_le_bytes());
    assert_eq!(bytes[ABI_FIELD], 1u32.to_le_bytes());
    assert_eq!(
        unstamped_body_length(&bytes) + UNSTAMPED_HEADER_BYTES + TRAILER_BYTES,
        bytes.len()
    );
    assert_eq!(trailer(&bytes), computed_trailer(&bytes));
    assert_eq!(version_refusal(&bytes), (EMPTY_READS_AS, ENGINE_VERSION));
}

#[test]
fn populated_fixtures_hold_one_state_under_two_numberings() {
    // The recipe check: the two populated fixtures were captured from the same state by builds
    // whose layouts differ only in the two revision numbers, so they differ in exactly those two
    // fields, the fingerprint (which covers the ABI revision) and the trailer.
    let unstamped = unstamped_snapshot();
    let release = release_snapshot();
    assert_eq!(unstamped.len(), release.len());
    let trailer_start = unstamped.len() - TRAILER_BYTES;
    let fingerprint = UNSTAMPED_HEADER_BYTES..UNSTAMPED_HEADER_BYTES + 16;
    for index in 0..unstamped.len() {
        let may_differ = FORMAT_FIELD.contains(&index)
            || ABI_FIELD.contains(&index)
            || fingerprint.contains(&index)
            || index >= trailer_start;
        assert!(
            may_differ || unstamped[index] == release[index],
            "byte {index} differs outside the revisions, fingerprint and trailer"
        );
    }
    // The state the recipe produces: model time 1.0 with an equal prior time, straight after the
    // 15-byte model id `urn:frame:model`.
    let model_id = UNSTAMPED_HEADER_BYTES + 16;
    assert_eq!(unstamped[model_id..model_id + 4], 15u32.to_le_bytes());
    assert_eq!(&unstamped[model_id + 4..model_id + 19], b"urn:frame:model");
    let time = model_id + 19;
    assert_eq!(unstamped[time..time + 8], 1.0f64.to_bits().to_le_bytes());
    assert_eq!(unstamped[time + 8], 1);
    assert_eq!(
        unstamped[time + 9..time + 17],
        1.0f64.to_bits().to_le_bytes()
    );
}
