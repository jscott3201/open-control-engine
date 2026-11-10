//! The engine version in the state snapshot header (GitHub #530).
//!
//! The format revision and the execution-state ABI revision stay 1 until 1.0.0, so the six bytes
//! after them are what tell one build's layout from another's: major, minor and patch of the
//! engine version, the same constant a compiled-schedule blob records. `from_bytes` accepts only
//! this build's version and refuses any other with `EngineVersionMismatch` before it reads the
//! body length, the trailer or the body, so `restore_state` never sees such a snapshot.
//!
//! The foreign snapshots here are built from this build's own capture of
//! `fixtures/frame_delay.jsonld` after two frames (`support/snapshot_bytes.rs` has the recipe) by
//! rewriting the version field, the fingerprint that covers it and the trailer, with FNV-1a-128
//! written independently of the codec. Such bytes are consistent in every respect, so the version
//! check is the only thing that can refuse them. Snapshots earlier layouts really wrote are in
//! `state_old_layout.rs`.

#[path = "support/snapshot_bytes.rs"]
mod snapshot_bytes;

use allocation_counter::measure;
use oce_api::{EngineStateError, EngineStateSnapshot, EngineVersion, OcError};
use oce_graph::blob::ENGINE_VERSION;
use snapshot_bytes::{
    ABI_FIELD, BODY_LENGTH_FIELD, FORMAT_FIELD, HEADER_BYTES, SHORTEST_INSPECTED, TRAILER_BYTES,
    VERSION_FIELD, computed_trailer, delay_after_two_frames, delay_frame, delay_target,
    fingerprint, manifest_range, reseal, restamp, stamped_version, trailer, version_field,
};

/// This build's populated snapshot: the delay model after the two recipe frames.
fn captured() -> Vec<u8> {
    delay_after_two_frames()
        .state_snapshot()
        .unwrap()
        .into_bytes()
}

/// The versions of an `EngineVersionMismatch`, as `(found, expected)`; any other outcome fails.
fn version_refusal(bytes: &[u8]) -> (EngineVersion, EngineVersion) {
    match EngineStateSnapshot::from_bytes(bytes) {
        Err(EngineStateError::EngineVersionMismatch { found, expected }) => (found, expected),
        Err(other) => panic!("expected EngineVersionMismatch, got {other:?}"),
        Ok(_) => panic!("a snapshot of another engine version was accepted"),
    }
}

/// The offset and detail of a `MalformedSnapshot`; any other outcome fails.
fn malformed(bytes: &[u8]) -> (u64, String) {
    match EngineStateSnapshot::from_bytes(bytes) {
        Err(EngineStateError::MalformedSnapshot { offset, detail }) => (offset, detail),
        Err(other) => panic!("expected MalformedSnapshot, got {other:?}"),
        Ok(_) => panic!("malformed snapshot was accepted"),
    }
}

fn patch_differs() -> EngineVersion {
    EngineVersion {
        patch: ENGINE_VERSION.patch.wrapping_add(1),
        ..ENGINE_VERSION
    }
}

fn minor_differs() -> EngineVersion {
    EngineVersion {
        minor: ENGINE_VERSION.minor.wrapping_add(1),
        ..ENGINE_VERSION
    }
}

fn major_differs() -> EngineVersion {
    EngineVersion {
        major: ENGINE_VERSION.major.wrapping_add(1),
        ..ENGINE_VERSION
    }
}

#[test]
fn header_names_this_build_between_the_revisions_and_the_body_length() {
    let bytes = captured();
    assert_eq!(&bytes[..8], b"OCESTAT\0");
    assert_eq!(bytes[FORMAT_FIELD], 1u32.to_le_bytes());
    assert_eq!(bytes[ABI_FIELD], 1u32.to_le_bytes());
    assert_eq!(stamped_version(&bytes), ENGINE_VERSION);
    let body = u64::from_le_bytes(bytes[BODY_LENGTH_FIELD].try_into().unwrap());
    assert_eq!(
        usize::try_from(body).unwrap(),
        bytes.len() - HEADER_BYTES - TRAILER_BYTES
    );
    // The trailer covers the header, version field included, and so does the fingerprint.
    assert_eq!(trailer(&bytes), computed_trailer(&bytes));
    let manifest = manifest_range(&bytes, HEADER_BYTES);
    assert_eq!(
        u128::from_le_bytes(bytes[HEADER_BYTES..HEADER_BYTES + 16].try_into().unwrap()),
        fingerprint(1, ENGINE_VERSION, &bytes[manifest])
    );
}

#[test]
fn version_field_is_the_package_version_without_a_pre_release_suffix() {
    // Cargo's three numeric components: a `-rc.1` or `+build` suffix is in none of them, so a
    // pre-release build stamps, and accepts, the same six bytes as the release it precedes.
    let package = EngineVersion {
        major: env!("CARGO_PKG_VERSION_MAJOR").parse().unwrap(),
        minor: env!("CARGO_PKG_VERSION_MINOR").parse().unwrap(),
        patch: env!("CARGO_PKG_VERSION_PATCH").parse().unwrap(),
    };
    assert_eq!(ENGINE_VERSION, package);
    let release = env!("CARGO_PKG_VERSION").split(['-', '+']).next().unwrap();
    assert_eq!(
        release,
        format!("{}.{}.{}", package.major, package.minor, package.patch)
    );
    assert_eq!(captured()[VERSION_FIELD], version_field(package));
}

#[test]
fn snapshot_and_blob_record_the_same_engine_version() {
    let engine = delay_after_two_frames();
    let snapshot = engine.state_snapshot().unwrap();
    let blob = engine.compile_blob().unwrap().bytes;
    // A blob header is the magic, the format and the profile, then the version at bytes 8..14.
    assert_eq!(&blob[..4], b"OCEB");
    assert_eq!(snapshot.as_bytes()[VERSION_FIELD], blob[8..14]);
}

#[test]
fn capture_then_restore_on_the_same_version_is_byte_identical() {
    let mut source = delay_after_two_frames();
    let bytes = source.state_snapshot().unwrap().into_bytes();
    let parsed = EngineStateSnapshot::from_bytes(&bytes).unwrap();
    assert_eq!(parsed.as_bytes(), bytes);

    let mut target = delay_target();
    target.restore_state(&parsed).unwrap();
    assert_eq!(target.state_snapshot().unwrap().as_bytes(), bytes);

    // The restored engine continues as the uninterrupted one does: the delayed output is the sum
    // the second recipe frame stored, and the next snapshots are equal byte for byte.
    let continued = delay_frame(&mut source, 2.0, -0.25, 4.0);
    let restored = delay_frame(&mut target, 2.0, -0.25, 4.0);
    assert!(continued.bit_eq(&oce_api::Value::Real(2.5)));
    assert!(restored.bit_eq(&continued));
    assert_eq!(
        target.state_snapshot().unwrap().as_bytes(),
        source.state_snapshot().unwrap().as_bytes()
    );
}

#[test]
fn snapshot_differing_by_patch_only_is_refused() {
    let found = patch_differs();
    let bytes = restamp(&captured(), found);
    assert_eq!(version_refusal(&bytes), (found, ENGINE_VERSION));
}

#[test]
fn snapshot_differing_by_minor_only_is_refused() {
    let found = minor_differs();
    let bytes = restamp(&captured(), found);
    assert_eq!(version_refusal(&bytes), (found, ENGINE_VERSION));
}

#[test]
fn snapshot_differing_by_major_only_is_refused() {
    let found = major_differs();
    let bytes = restamp(&captured(), found);
    assert_eq!(version_refusal(&bytes), (found, ENGINE_VERSION));
}

#[test]
fn every_other_spelling_of_the_version_field_is_refused() {
    let captured = captured();
    let this = ENGINE_VERSION;
    let others = [
        EngineVersion {
            patch: this.patch.wrapping_sub(1),
            ..this
        },
        EngineVersion {
            minor: this.minor.wrapping_sub(1),
            ..this
        },
        // The components in another order, and the two ends of the field's range.
        EngineVersion {
            major: this.patch,
            minor: this.major,
            patch: this.minor,
        },
        EngineVersion {
            major: 0,
            minor: 0,
            patch: 0,
        },
        EngineVersion {
            major: u16::MAX,
            minor: u16::MAX,
            patch: u16::MAX,
        },
    ];
    for found in others {
        if found == this {
            continue;
        }
        let bytes = restamp(&captured, found);
        assert_eq!(version_refusal(&bytes), (found, this), "{found:?}");
    }
}

#[test]
fn version_refusal_names_both_versions_and_wraps_into_the_facade_error() {
    let found = patch_differs();
    let error = EngineStateSnapshot::from_bytes(&restamp(&captured(), found)).unwrap_err();
    let message = format!(
        "state snapshot written by engine {}.{}.{}; this build is {}.{}.{}",
        found.major,
        found.minor,
        found.patch,
        ENGINE_VERSION.major,
        ENGINE_VERSION.minor,
        ENGINE_VERSION.patch
    );
    assert_eq!(error.to_string(), message);
    let facade = OcError::from(error);
    assert!(matches!(
        facade,
        OcError::State(EngineStateError::EngineVersionMismatch { found: f, expected: e })
            if f == found && e == ENGINE_VERSION
    ));
}

#[test]
fn refusal_leaves_a_restore_target_untouched_and_still_restorable() {
    // A refused snapshot never becomes an `EngineStateSnapshot`, so there is nothing to hand to
    // `restore_state`: the target keeps its state and its durable restore window.
    let genuine = captured();
    let mut target = delay_target();
    let before = target.state_snapshot().unwrap();
    version_refusal(&restamp(&genuine, minor_differs()));
    assert_eq!(
        target.state_snapshot().unwrap().as_bytes(),
        before.as_bytes()
    );
    target
        .restore_state(&EngineStateSnapshot::from_bytes(&genuine).unwrap())
        .unwrap();
    assert_eq!(target.state_snapshot().unwrap().as_bytes(), genuine);
}

#[test]
fn version_field_patched_to_this_build_fails_the_trailer() {
    // A snapshot another version wrote, with only its six version bytes rewritten to claim this
    // build: the trailer covers the header, so the edit is caught as corruption.
    let foreign = restamp(&captured(), patch_differs());
    let mut patched = foreign.clone();
    patched[VERSION_FIELD].copy_from_slice(&version_field(ENGINE_VERSION));
    let changed: Vec<usize> = (0..patched.len())
        .filter(|&index| patched[index] != foreign[index])
        .collect();
    assert!(!changed.is_empty() && changed.iter().all(|index| VERSION_FIELD.contains(index)));
    match EngineStateSnapshot::from_bytes(&patched) {
        Err(EngineStateError::IntegrityMismatch { expected, found }) => assert_eq!(
            (expected, found),
            (computed_trailer(&patched), trailer(&patched))
        ),
        Err(other) => panic!("expected IntegrityMismatch, got {other:?}"),
        Ok(_) => panic!("a patched version field was accepted"),
    }
}

#[test]
fn version_field_patched_with_a_new_trailer_fails_the_fingerprint() {
    // The same edit with the trailer recomputed: the fingerprint in the body was taken over the
    // other version, so it no longer matches the header the bytes now carry.
    for foreign in [patch_differs(), minor_differs(), major_differs()] {
        let mut patched = restamp(&captured(), foreign);
        patched[VERSION_FIELD].copy_from_slice(&version_field(ENGINE_VERSION));
        reseal(&mut patched);
        assert_eq!(
            malformed(&patched),
            (
                HEADER_BYTES as u64,
                "execution fingerprint does not match its manifest".to_owned()
            ),
            "{foreign:?}"
        );
    }
}

#[test]
fn rewriting_every_check_reproduces_this_builds_bytes() {
    // The fingerprint and the trailer are FNV-1a-128: corruption checks, not authentication. A
    // host that rewrites the version field and both of them has produced this build's bytes
    // exactly; binding bytes to a build is the host envelope's job. This is also the control for
    // `restamp`: with this build's version it changes nothing.
    let genuine = captured();
    let round_trip = restamp(&restamp(&genuine, minor_differs()), ENGINE_VERSION);
    assert_eq!(round_trip, genuine);
    EngineStateSnapshot::from_bytes(&round_trip).unwrap();
}

#[test]
fn every_flipped_bit_of_the_version_field_is_a_version_refusal() {
    // No trailer is recomputed: the version is checked before integrity, so a corrupted version
    // field reports the version it now spells.
    let genuine = captured();
    for bit in 0..VERSION_FIELD.len() * 8 {
        let mut bytes = genuine.clone();
        bytes[VERSION_FIELD.start + bit / 8] ^= 1 << (bit % 8);
        let found = stamped_version(&bytes);
        assert_ne!(found, ENGINE_VERSION);
        assert_eq!(
            version_refusal(&bytes),
            (found, ENGINE_VERSION),
            "bit {bit}"
        );
    }
}

#[test]
fn magic_and_format_refusals_precede_the_version_check() {
    let foreign = restamp(&captured(), patch_differs());

    let mut bad_magic = foreign.clone();
    bad_magic[0] ^= 1;
    assert_eq!(
        malformed(&bad_magic),
        (0, "invalid state-snapshot magic".to_owned())
    );

    let mut other_format = foreign.clone();
    other_format[FORMAT_FIELD].copy_from_slice(&2u32.to_le_bytes());
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&other_format),
        Err(EngineStateError::UnsupportedFormat { revision: 2 })
    ));

    // The execution-state ABI revision is read on the way to the version field and refuses
    // nothing at parse: another ABI revision under another engine version is a version refusal.
    let mut other_abi = foreign;
    other_abi[ABI_FIELD].copy_from_slice(&7u32.to_le_bytes());
    assert_eq!(
        version_refusal(&other_abi),
        (patch_differs(), ENGINE_VERSION)
    );
}

#[test]
fn version_refusal_precedes_the_length_the_trailer_and_the_body() {
    let found = minor_differs();
    let foreign = restamp(&captured(), found);

    // A body length that cannot be true, a wrong trailer and a body of noise, one at a time and
    // together: none of them is read.
    let mut absurd_length = foreign.clone();
    absurd_length[BODY_LENGTH_FIELD].copy_from_slice(&u64::MAX.to_le_bytes());
    let mut wrong_trailer = foreign.clone();
    *wrong_trailer.last_mut().unwrap() ^= 0x80;
    let mut noise = foreign.clone();
    noise[VERSION_FIELD.end..].fill(0xa5);
    for bytes in [&absurd_length, &wrong_trailer, &noise] {
        assert_eq!(version_refusal(bytes), (found, ENGINE_VERSION));
    }

    // Every truncation and every extension: below a header and a trailer the bytes are too short
    // to be a snapshot of any version, and from there on the version refuses them.
    let mut extended = foreign.clone();
    extended.extend_from_slice(&[0; 64]);
    for length in 0..=extended.len() {
        let bytes = &extended[..length];
        if length < SHORTEST_INSPECTED {
            assert_eq!(
                malformed(bytes),
                (
                    0,
                    "snapshot is shorter than the fixed header and trailer".to_owned()
                ),
                "length {length}"
            );
        } else {
            assert_eq!(
                version_refusal(bytes),
                (found, ENGINE_VERSION),
                "length {length}"
            );
        }
    }
}

#[test]
fn this_builds_header_still_meets_the_length_check_at_every_truncation() {
    // The control for the test above: with this build's version in place the same truncations and
    // extensions reach the body-length field, which refuses them at its own offset.
    let mut extended = captured();
    let genuine_length = extended.len();
    extended.extend_from_slice(&[0; 64]);
    for length in SHORTEST_INSPECTED..=extended.len() {
        if length == genuine_length {
            continue;
        }
        let (offset, detail) = malformed(&extended[..length]);
        assert_eq!(offset, BODY_LENGTH_FIELD.start as u64, "length {length}");
        assert_eq!(
            detail,
            format!("body length names {genuine_length} total bytes, found {length}")
        );
    }
}

#[test]
fn refusing_another_version_allocates_nothing_that_grows_with_the_input() {
    // Eight mebibytes behind a header of another version: the refusal copies and decodes none of
    // it. The decoder's only allocation before the version check is its fixed budget counter.
    let found = major_differs();
    let mut bytes = restamp(&captured(), found);
    bytes.resize(8 * 1024 * 1024, 0x5a);
    let mut outcome = None;
    let info = measure(|| outcome = Some(version_refusal(&bytes)));
    assert_eq!(outcome, Some((found, ENGINE_VERSION)));
    assert!(
        info.bytes_total <= 256,
        "refusal allocated {} bytes",
        info.bytes_total
    );
}
