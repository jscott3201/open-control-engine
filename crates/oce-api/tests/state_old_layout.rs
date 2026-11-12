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
//!   version (issue #530). Captured with the recipe below on the format reset itself, GitHub #518
//!   (commit `4c89577` on `development`, workspace version 0.5.0). No release wrote this layout.
//! - `fixtures/state_revision_two.hex`: the same state as releases 0.2.0 to 0.4.0 wrote it, which
//!   numbered their snapshots format revision 2 and execution-state ABI revision 2, so it refuses
//!   by its format field first. Added by GitHub #518; the recipe below, run on the commit before
//!   the format reset (`b5d4f4b` on `development`, GitHub #512, workspace version 0.4.0),
//!   reproduces it byte for byte.
//! - `fixtures/state_legacy.hex`: the empty-authored-model vector (an in-memory model with no
//!   blocks, id `urn:test:empty-model`, before any frame) of a layout that predates the first
//!   release with a snapshot API. Its header carries revision 1 twice.
//!
//! **Capture recipe** of the two populated fixtures. On the commit named for the fixture, save
//! the test below as `crates/oce-api/tests/snapshot_capture.rs` (it uses only calls both commits
//! have) and run
//! `OCE_CAPTURE_OUT=<file> cargo test -p oce-api --test snapshot_capture --locked`.
//! The file it writes, plus a final newline, is the fixture. Both fixtures were regenerated this
//! way, byte for byte, when the recipe was recorded. Give that checkout its own `target`
//! directory: cargo reuses artefacts across checkouts of one workspace version that share one,
//! and the capture then comes from the wrong codec.
//!
//! ```text
//! //! Capture recipe of the old-layout snapshot fixtures. Scratch: never committed.
//! use oce_api::{Engine, Value};
//!
//! #[test]
//! fn write_the_capture() {
//!     // Model: an `Add` into a `UnitDelay` with a sample period of 1 s.
//!     let mut engine = Engine::new();
//!     engine.load_cxf(include_bytes!("fixtures/frame_delay.jsonld")).unwrap();
//!     // Two accepted frames, at model time 0.0 and then 1.0, with the same inputs.
//!     for time in [0.0, 1.0] {
//!         let inputs = [("urn:frame:a", Value::Real(0.5)), ("urn:frame:b", Value::Real(2.0))];
//!         let frame = engine.prepare_frame(time, &inputs).unwrap();
//!         engine.execute_frame(frame).unwrap();
//!     }
//!     // The bytes are `Engine::state_snapshot`'s, written as one line of lowercase hex.
//!     let bytes = engine.state_snapshot().unwrap().into_bytes();
//!     let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
//!     std::fs::write(std::env::var("OCE_CAPTURE_OUT").unwrap(), hex).unwrap();
//! }
//! ```
//!
//! `support/snapshot_bytes.rs` holds the same recipe for this build
//! (`delay_after_two_frames`). The state it leaves is model time 1.0 with an equal prior time,
//! five connector values (`add.u1` 0.5, `add.u2` 2.0, `add.y` 2.5, `delay.u` 0.0, `delay.y` 2.5)
//! and five `UnitDelay` state words. `populated_fixtures_hold_the_recorded_recipe_state` reads
//! exactly that out of both fixtures on every run, and `state_engine_version.rs` out of this
//! build's capture.

#[path = "support/snapshot_bytes.rs"]
mod snapshot_bytes;

use oce_api::{EngineStateError, EngineStateSnapshot, EngineVersion, OcError};
use oce_graph::blob::ENGINE_VERSION;
use snapshot_bytes::{
    ABI_FIELD, BODY_LENGTH_FIELD, FORMAT_FIELD, HEADER_BYTES, TRAILER_BYTES,
    UNSTAMPED_HEADER_BYTES, VERSION_FIELD, computed_trailer, delay_after_two_frames, delay_payload,
    delay_recipe_values, delay_recipe_words, delay_target, manifest_range, reseal,
    restore_from_bytes, trailer, unhex, unstamped_fingerprint, version_field,
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
fn host_restore_path_stops_at_decode_for_the_unstamped_snapshot() {
    // The path a host calls, into a freshly loaded target the fixture's state would fit exactly.
    // Decode refuses, so `restore_state` is not reached and the target keeps its load state.
    let mut target = delay_target();
    let loaded = target.state_snapshot().unwrap().into_bytes();
    match restore_from_bytes(&mut target, &unstamped_snapshot()) {
        Err(OcError::State(EngineStateError::EngineVersionMismatch { found, expected })) => {
            assert_eq!((found, expected), (POPULATED_READS_AS, ENGINE_VERSION));
        }
        other => panic!("expected EngineVersionMismatch, got {other:?}"),
    }
    assert_eq!(target.state_snapshot().unwrap().as_bytes(), loaded);

    // The control: the same state captured by this build goes through the same path and moves
    // the target, so the comparison above is one a restore would have changed, and the refusal
    // left the durable restore window open.
    let stamped = delay_after_two_frames()
        .state_snapshot()
        .unwrap()
        .into_bytes();
    assert_ne!(stamped, loaded);
    restore_from_bytes(&mut target, &stamped).unwrap();
    assert_eq!(target.state_snapshot().unwrap().as_bytes(), stamped);
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
fn populated_fixtures_hold_the_recorded_recipe_state() {
    // The recipe check. First, the two populated fixtures were captured from the same state by
    // builds whose layouts differ only in the two revision numbers, so they differ in exactly
    // those two fields, the fingerprint (which covers the ABI revision) and the trailer.
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
    // And after the manifest, read by hand out of each fixture: the five connector values
    // (0.5 + 2.0 = 2.5 on the `Add`, 2.5 delayed by one sample) and the five `UnitDelay` words.
    for fixture in [&unstamped, &release] {
        let (values, words) = delay_payload(fixture, UNSTAMPED_HEADER_BYTES);
        assert_eq!(values.len(), 5);
        assert_eq!(values, delay_recipe_values());
        assert_eq!(words.len(), 5);
        assert_eq!(words, delay_recipe_words());
    }
}
