//! Bytes the facade reads no summary from (issue #667): each is the typed
//! `DiffError::UnreadableBlob`, carrying the side and the facade's own refusal, and none panics.
//!
//! The defects are the facade's: empty, truncated and bit-flipped copies of its checked-in blobs,
//! a class path this build does not register, and the release-0.4.0 blob it keeps to prove that
//! another engine version's blob is refused, all read in place under
//! `crates/oce-api/tests/fixtures/`. So blobs compare only when one engine version wrote both
//! and this build registers every class they name. Every case is also held to what
//! `oce_api::inspect_blob` says of the same bytes, so the comparison adds no refusal of its own
//! and drops none.

mod support;

use oce_api::{BlobError, ENGINE_VERSION, EngineVersion, MalformedFault, OcError, inspect_blob};
use oce_diff::{DiffError, Side, compare_blobs};

use support::blobs::{golden_blob, legacy_blob, only_occurrence, reseal};

/// The smallest of the facade's blobs: one `AddParameter` block, one input, one output.
const SMALL: &str = "generic_air_economizer_high_limits_ashrae_differential";

/// The refusal `compare_blobs` gives when `bytes` is on `side` and a good blob on the other.
fn refusal_on(side: Side, bytes: &[u8]) -> DiffError {
    let good = golden_blob(SMALL);
    let result = match side {
        Side::Base => compare_blobs(bytes, &good),
        _ => compare_blobs(&good, bytes),
    };
    result.expect_err("the bytes are refused")
}

/// The facade's own refusal of `bytes`.
fn facade_refusal(bytes: &[u8]) -> BlobError {
    match inspect_blob(bytes) {
        Err(OcError::Blob(refusal)) => refusal,
        other => panic!("the facade refuses the bytes as a blob: {other:?}"),
    }
}

#[test]
fn empty_truncated_and_bit_flipped_bytes_refuse_with_the_facades_typed_error() {
    let good = golden_blob(SMALL);
    let mut flipped = good.clone();
    flipped[good.len() / 2] ^= 0x10;
    let cases: [(&str, Vec<u8>); 4] = [
        ("empty", Vec::new()),
        ("one byte short", good[..good.len() - 1].to_vec()),
        ("the header alone", good[..52].to_vec()),
        ("one bit flipped", flipped),
    ];
    for (what, bytes) in &cases {
        for side in [Side::Base, Side::Head] {
            let error = refusal_on(side, bytes);
            assert_eq!(
                error,
                DiffError::UnreadableBlob {
                    side,
                    refusal: facade_refusal(bytes)
                },
                "{what}"
            );
            assert_eq!(error.code(), "unreadable-blob", "{what}");
        }
    }
    // The specific refusals, so that a change of what the facade says is seen here.
    let refusals: Vec<BlobError> = cases
        .iter()
        .map(|(_, bytes)| facade_refusal(bytes))
        .collect();
    assert!(matches!(
        refusals[0],
        BlobError::Malformed {
            offset: 0,
            fault: MalformedFault::Truncated
        }
    ));
    assert!(
        matches!(
            refusals[1],
            BlobError::Malformed {
                fault: MalformedFault::LengthMismatch,
                ..
            }
        ),
        "{:?}",
        refusals[1]
    );
    assert!(
        matches!(
            refusals[2],
            BlobError::Malformed {
                fault: MalformedFault::Truncated | MalformedFault::LengthMismatch,
                ..
            }
        ),
        "{:?}",
        refusals[2]
    );
    assert!(
        matches!(refusals[3], BlobError::IntegrityMismatch { .. }),
        "{:?}",
        refusals[3]
    );
}

#[test]
fn another_engine_versions_blob_refuses_with_engine_version_mismatch() {
    let legacy = legacy_blob();
    let expected = BlobError::EngineVersionMismatch {
        found: EngineVersion {
            major: 0,
            minor: 4,
            patch: 0,
        },
        expected: ENGINE_VERSION,
    };
    for side in [Side::Base, Side::Head] {
        let error = refusal_on(side, &legacy);
        assert_eq!(
            error,
            DiffError::UnreadableBlob {
                side,
                refusal: expected
            }
        );
        assert_eq!(
            error.to_string(),
            format!(
                "{}: the blob has no summary: blob encoded by engine 0.4.0; this build is {}.{}.{}",
                if side == Side::Base { "base" } else { "head" },
                ENGINE_VERSION.major,
                ENGINE_VERSION.minor,
                ENGINE_VERSION.patch
            )
        );
    }
    // Against itself, too: a blob is compared only by the engine version that wrote it.
    assert_eq!(
        compare_blobs(&legacy, &legacy),
        Err(DiffError::UnreadableBlob {
            side: Side::Base,
            refusal: expected
        })
    );
}

#[test]
fn a_class_this_build_does_not_register_refuses_with_unknown_class() {
    let good = golden_blob(SMALL);
    let path = b"CDL.Reals.AddParameter";
    let at = only_occurrence(&good, path, "the class path");
    let mut unregistered = good.clone();
    // The same length and still the one path of the table: only the name is unknown.
    unregistered[at + path.len() - 1] = b'x';
    reseal(&mut unregistered);
    for side in [Side::Base, Side::Head] {
        assert_eq!(
            refusal_on(side, &unregistered),
            DiffError::UnreadableBlob {
                side,
                refusal: BlobError::UnknownClass { class: 0 }
            }
        );
    }
}

#[test]
fn base_is_read_before_head() {
    let legacy = legacy_blob();
    assert_eq!(
        compare_blobs(&[], &legacy),
        Err(DiffError::UnreadableBlob {
            side: Side::Base,
            refusal: facade_refusal(&[])
        })
    );
    assert_eq!(
        compare_blobs(&legacy, &[]),
        Err(DiffError::UnreadableBlob {
            side: Side::Base,
            refusal: facade_refusal(&legacy)
        })
    );
}

/// Hold `compare_blobs(good, bytes)` and its mirror to what the facade says of `bytes`.
fn agrees_with_the_facade(good: &[u8], bytes: &[u8], what: &str) -> bool {
    let forward = compare_blobs(good, bytes);
    let backward = compare_blobs(bytes, good);
    match inspect_blob(bytes) {
        Ok(summary) => {
            let forward = forward.unwrap_or_else(|error| panic!("{what}: {error}"));
            let backward = backward.unwrap_or_else(|error| panic!("{what}: {error}"));
            assert_eq!(
                forward.head.decode_refusal, summary.decode_refusal,
                "{what}"
            );
            assert_eq!(forward.head.content_id, summary.content_id, "{what}");
            assert_eq!(backward.base, forward.head, "{what}");
            assert_eq!(forward.same_bytes, bytes == good, "{what}");
            true
        }
        Err(OcError::Blob(refusal)) => {
            assert_eq!(
                forward,
                Err(DiffError::UnreadableBlob {
                    side: Side::Head,
                    refusal
                }),
                "{what}"
            );
            assert_eq!(
                backward,
                Err(DiffError::UnreadableBlob {
                    side: Side::Base,
                    refusal
                }),
                "{what}"
            );
            false
        }
        Err(other) => panic!("{what}: the facade refused with {other:?}"),
    }
}

#[test]
fn every_truncation_and_bit_flip_of_a_blob_agrees_with_the_facade_and_never_panics() {
    let good = golden_blob(SMALL);
    let mut read = [0usize; 3];
    for length in 0..good.len() {
        let what = format!("the first {length} bytes");
        read[0] += usize::from(agrees_with_the_facade(&good, &good[..length], &what));
    }
    for at in 0..good.len() {
        for bit in 0..8 {
            let mut flipped = good.clone();
            flipped[at] ^= 1 << bit;
            let what = format!("byte {at} bit {bit}");
            read[1] += usize::from(agrees_with_the_facade(&good, &flipped, &what));
            // Resealed, the flip reaches the field instead of the trailer.
            if at < good.len() - 4 {
                reseal(&mut flipped);
                let what = format!("byte {at} bit {bit}, resealed");
                read[2] += usize::from(agrees_with_the_facade(&good, &flipped, &what));
            }
        }
    }
    // No truncation and no unsealed flip has a summary. A resealed flip has one when it lands
    // where the structural pass does not look (the content id, a hash, a parameter value), and
    // the comparison then reports it: 1,871 of the blob's 3,312 resealed flips, pinned so that a
    // sweep that stopped reaching the comparison is seen. The count is a function of the blob's
    // layout; a workspace version bump moves only the version bytes, whose every flip is refused.
    assert_eq!(read[0], 0);
    assert_eq!(read[1], 0);
    assert_eq!(read[2], 1_871);
}
