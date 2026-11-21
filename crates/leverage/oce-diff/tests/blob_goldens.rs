//! The blob comparison as bytes (issue #667): three pinned pairs with golden JSON, the same bytes
//! on every run, and the mirror.
//!
//! - **Goldens.** `tests/fixtures/blobs/{same,class_change,initial_state}.comparison.json`,
//!   compared byte for byte and rewritten only by the crate's bless switch
//!   (`OCE_BLESS_DIFF=1 cargo nextest run -p oce-diff`). `same` is the facade's checked-in blob of
//!   `ahu_economizer` against a fresh compile of its CXF; `class_change` is the facade's worked
//!   one-block blob against a copy whose `CDL.Reals.AddParameter` hash has one bit flipped, as a
//!   build of the same engine version with another record for the class would write it;
//!   `initial_state` is the pair of authoring texts beside the goldens, compiled. Each golden
//!   records content ids and class hashes, so it moves when a blob of the pair does: a change to
//!   the content id's encoding, to a class signature or to what the texts compile to.
//! - **Determinism.** The same two inputs give the same JSON on every run.
//! - **The mirror.** `compare_blobs(b, a)` is `compare_blobs(a, b)` with `added` and `removed`
//!   swapped, the old and new hash of each `signature-changed` class swapped and the two sides
//!   swapped; `same_bytes`, `content_id` and `initial_state` do not move. Held over every pair
//!   of the facade's 46 blobs and the three pinned pairs.

mod support;

use oce_api::Engine;
use oce_diff::{BlobComparison, ClassChange};

use support::blobs::{compare, compiled, golden_blob, with_signature_flipped};
use support::{compare_or_bless, fixture, read};

#[path = "../../../oce-api/tests/support/blob_corpus.rs"]
mod blob_corpus;

use blob_corpus::FIXTURES;

/// The three pinned pairs, by golden name.
fn pinned() -> [(&'static str, Vec<u8>, Vec<u8>); 3] {
    let source = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../oce-cxf/tests/fixtures/g36/ahu_economizer.jsonld");
    let mut engine = Engine::new();
    engine
        .load_cxf(&std::fs::read(&source).expect("read the fixture"))
        .expect("the fixture loads");
    let fresh = engine.compile_blob().expect("the fixture compiles").bytes;
    let small = golden_blob("generic_air_economizer_high_limits_ashrae_differential");
    let changed = with_signature_flipped(&small, 0, 0);
    [
        ("same", golden_blob("ahu_economizer"), fresh),
        ("class_change", small, changed),
        (
            "initial_state",
            compiled(&read("blobs/initial_state.base.oce")),
            compiled(&read("blobs/initial_state.head.oce")),
        ),
    ]
}

#[test]
fn the_three_pinned_pairs_match_their_comparison_json_goldens() {
    for (name, base, head) in pinned() {
        compare_or_bless(
            &fixture(&format!("blobs/{name}.comparison.json")),
            &compare(&base, &head).to_json(),
        );
    }
}

#[test]
fn a_comparison_is_byte_identical_across_runs() {
    for (name, base, head) in pinned() {
        let first = compare(&base, &head).to_json();
        let again: Vec<String> = (0..3).map(|_| compare(&base, &head).to_json()).collect();
        assert!(again.iter().all(|json| *json == first), "{name}");
    }
    let blobs: Vec<Vec<u8>> = FIXTURES.iter().map(|name| golden_blob(name)).collect();
    for pair in blobs.windows(2) {
        assert_eq!(
            compare(&pair[0], &pair[1]).to_json(),
            compare(&pair[0], &pair[1]).to_json()
        );
    }
}

/// Assert that `backward` is the mirror of `forward`.
fn assert_mirror(forward: &BlobComparison, backward: &BlobComparison, what: &str) {
    assert_eq!(backward.same_bytes, forward.same_bytes, "{what}");
    assert_eq!(backward.content_id, forward.content_id, "{what}");
    assert_eq!(backward.initial_state, forward.initial_state, "{what}");
    assert_eq!(backward.base, forward.head, "{what}");
    assert_eq!(backward.head, forward.base, "{what}");
    let swapped: Vec<ClassChange> = forward
        .classes
        .iter()
        .map(|change| match change.clone() {
            ClassChange::Added { subject, hash } => ClassChange::Removed { subject, hash },
            ClassChange::Removed { subject, hash } => ClassChange::Added { subject, hash },
            ClassChange::SignatureChanged { subject, old, new } => ClassChange::SignatureChanged {
                subject,
                old: new,
                new: old,
            },
            other => panic!("{what}: a class change of an unknown kind: {other:?}"),
        })
        .collect();
    assert_eq!(backward.classes, swapped, "{what}");
}

#[test]
fn the_mirror_swaps_added_and_removed_and_the_sides_and_nothing_else() {
    for (name, base, head) in pinned() {
        assert_mirror(&compare(&base, &head), &compare(&head, &base), name);
    }
    let blobs: Vec<Vec<u8>> = FIXTURES.iter().map(|name| golden_blob(name)).collect();
    let mut with_both_kinds = 0;
    for i in 0..blobs.len() {
        for j in i + 1..blobs.len() {
            let what = format!("{} -> {}", FIXTURES[i], FIXTURES[j]);
            let forward = compare(&blobs[i], &blobs[j]);
            assert_mirror(&forward, &compare(&blobs[j], &blobs[i]), &what);
            let added = forward
                .classes
                .iter()
                .any(|change| matches!(change, ClassChange::Added { .. }));
            let removed = forward
                .classes
                .iter()
                .any(|change| matches!(change, ClassChange::Removed { .. }));
            with_both_kinds += usize::from(added && removed);
        }
    }
    // The mirror is exercised on both kinds at once, not only on one-sided pairs.
    assert!(with_both_kinds > 0);
}
