//! State snapshots earlier builds wrote, presented to this build (issue #509).
//!
//! Every format stays at revision 1 until 1.0.0 and changes in place, and a snapshot carries no
//! engine version, so these fixtures pin what refuses older bytes today. Neither is re-blessed.
//!
//! - `fixtures/state_legacy.hex`: the empty-authored-model vector of a layout that predates the
//!   first release with a snapshot API. Its header carries the numbers this build writes
//!   (format revision 1, execution-state ABI revision 1), so only its structure can refuse it.
//! - `fixtures/state_revision_two.hex`: a populated snapshot (`frame_delay.jsonld` after two
//!   accepted frames, a stateful `UnitDelay`, bounded inputs) as the build before the reset
//!   wrote it, with the state codec of release 0.4.0. Releases 0.2.0 to 0.4.0 numbered their
//!   snapshots format revision 2 and execution-state ABI revision 2, so its header refuses it.

use oce_api::{Engine, EngineStateError, EngineStateSnapshot, OcError};

const DELAY_MODEL: &[u8] = include_bytes!("fixtures/frame_delay.jsonld");

fn unhex(text: &str) -> Vec<u8> {
    let hex = text.trim();
    (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap())
        .collect()
}

/// FNV-1a-128 as `docs/state-compatibility.md` specifies it, written here independently.
fn fnv(bytes: &[u8]) -> u128 {
    bytes.iter().fold(
        0x6c62_272e_07bb_0142_62b8_2175_6295_c58d_u128,
        |hash, byte| {
            (hash ^ u128::from(*byte)).wrapping_mul(0x0000_0000_0100_0000_0000_0000_0000_013b)
        },
    )
}

/// Recompute the 16-byte trailer after a header edit, as a host editing the bytes would.
fn reseal(bytes: &mut [u8]) {
    let trailer = bytes.len() - 16;
    let checksum = fnv(&bytes[..trailer]);
    bytes[trailer..].copy_from_slice(&checksum.to_le_bytes());
}

fn release_snapshot() -> Vec<u8> {
    let bytes = unhex(include_str!("fixtures/state_revision_two.hex"));
    // Format revision 2, execution-state ABI revision 2, a consistent length and a valid trailer.
    assert_eq!(bytes[8..16], [2, 0, 0, 0, 2, 0, 0, 0]);
    let body = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
    assert_eq!(usize::try_from(body).unwrap() + 24 + 16, bytes.len());
    let trailer = bytes.len() - 16;
    assert_eq!(
        u128::from_le_bytes(bytes[trailer..].try_into().unwrap()),
        fnv(&bytes[..trailer])
    );
    bytes
}

/// A freshly loaded target for the populated snapshot, inside its durable restore window.
fn delay_target() -> Engine {
    let mut engine = Engine::new();
    engine.load_cxf(DELAY_MODEL).unwrap();
    engine
}

#[test]
fn old_layout_snapshot_sharing_the_format_revision_is_refused_by_structure() {
    let bytes = unhex(include_str!("fixtures/state_legacy.hex"));
    // The header names the numbers this build writes, the length is consistent and the trailer
    // is valid, so the version, length and integrity checks all pass.
    assert_eq!(bytes[8..16], [1, 0, 0, 0, 1, 0, 0, 0]);
    let trailer = bytes.len() - 16;
    assert_eq!(
        u128::from_le_bytes(bytes[trailer..].try_into().unwrap()),
        fnv(&bytes[..trailer])
    );
    // The old manifest ends before the input-definition count the current layout requires.
    match EngineStateSnapshot::from_bytes(&bytes) {
        Err(EngineStateError::MalformedSnapshot { offset, detail }) => {
            assert_eq!(
                (offset, detail.as_str()),
                (122, "unexpected end of snapshot")
            );
        }
        Err(other) => panic!("expected MalformedSnapshot, got {other:?}"),
        Ok(_) => panic!("old-layout snapshot was accepted"),
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
fn release_snapshot_with_an_edited_format_field_does_not_restore() {
    // A host that rewrites only the format field and reseals the trailer gets bytes that parse
    // (an unknown execution ABI is inspectable) and that no target restores.
    let mut bytes = release_snapshot();
    bytes[8..12].copy_from_slice(&1u32.to_le_bytes());
    reseal(&mut bytes);
    let parsed = EngineStateSnapshot::from_bytes(&bytes).unwrap();
    let mut target = delay_target();
    let before = target.state_snapshot().unwrap();
    match target.restore_state(&parsed) {
        Err(OcError::State(EngineStateError::IncompatibleExecution {
            subject,
            snapshot,
            target,
        })) => assert_eq!(
            (subject.as_str(), snapshot.as_str(), target.as_str()),
            ("execution-state ABI revision", "2", "1")
        ),
        other => panic!("expected IncompatibleExecution, got {other:?}"),
    }
    assert_eq!(
        target.state_snapshot().unwrap().as_bytes(),
        before.as_bytes()
    );
}

#[test]
fn release_snapshot_with_both_revision_fields_edited_fails_its_fingerprint() {
    // The execution fingerprint covers the ABI revision, so rewriting both header numbers and
    // resealing the trailer still leaves bytes the decoder refuses.
    let mut bytes = release_snapshot();
    bytes[8..12].copy_from_slice(&1u32.to_le_bytes());
    bytes[12..16].copy_from_slice(&1u32.to_le_bytes());
    reseal(&mut bytes);
    match EngineStateSnapshot::from_bytes(&bytes) {
        Err(EngineStateError::MalformedSnapshot { offset, detail }) => assert_eq!(
            (offset, detail.as_str()),
            (24, "execution fingerprint does not match its manifest")
        ),
        Err(other) => panic!("expected MalformedSnapshot, got {other:?}"),
        Ok(_) => panic!("edited release snapshot was accepted"),
    }
}
