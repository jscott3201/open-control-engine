//! The three axes of a blob comparison over real blobs (issue #667; diff note, decisions 3, 8
//! and 15, and its adapter test: "a blob compared with itself is same, and a pinned
//! class-signature change shows in `classes`").
//!
//! - **Oracle.** Every ordered pair of the facade's 46 checked-in blobs is compared, and each
//!   comparison is held to one computed here from the facade's checked-in summaries of the same
//!   blobs (`crates/oce-api/tests/fixtures/blob_summary/`, written by the facade's own test
//!   code), with a second implementation of the three rules.
//! - **Class signatures.** One engine version gives each class one hash, so a
//!   `signature-changed` class comes only from a build of the same version whose registry
//!   differs. Such a blob is made here by flipping one bit of a class's recorded hash, for every
//!   class of a multi-class blob: exactly that class is `signature-changed`, and the side says
//!   this build refuses it.
//! - **Initial state.** A changed initial parameter, compiled from two authoring texts, is
//!   `initial_state: different` with an equal class table; lists that do not align by class are
//!   `unknown`.
//! - **Content ids.** A blob that records none is `unknown` on that axis and on no other.

mod support;

use oce_api::{BlobError, Engine};
use oce_diff::{BlobComparison, ClassChange, Equality};

use support::blobs::{
    compare, compiled, facade_fixture, golden_blob, golden_summary, summary,
    with_content_id_cleared, with_initial_state_crc_flipped, with_signature_flipped,
    with_unused_class,
};
use support::read;

#[path = "../../../oce-api/tests/support/blob_corpus.rs"]
mod blob_corpus;

use blob_corpus::FIXTURES;

/// A multi-class blob with stateful blocks: 11 classes, 12 blocks, three of them stateful.
const RICH: &str = "ahu_economizer";

/// Assert that `comparison` is same on every axis, both sides accepted.
fn assert_same_everywhere(comparison: &BlobComparison, what: &str) {
    assert!(comparison.same_bytes, "{what}");
    assert_eq!(comparison.content_id, Equality::Same, "{what}");
    assert!(comparison.classes.is_empty(), "{what}");
    assert_eq!(comparison.initial_state, Equality::Same, "{what}");
    assert!(
        comparison.base.accepted() && comparison.head.accepted(),
        "{what}"
    );
    assert_eq!(comparison.base, comparison.head, "{what}");
}

#[test]
fn every_golden_blob_against_itself_is_same_on_every_axis() {
    assert_eq!(FIXTURES.len(), 46);
    for name in FIXTURES {
        let blob = golden_blob(name);
        let comparison = compare(&blob, &blob);
        assert_same_everywhere(&comparison, name);
        assert_eq!(
            comparison.base.content_id,
            summary(&blob).content_id,
            "{name}"
        );
    }
}

#[test]
fn a_fresh_compile_of_a_fixture_is_the_same_blob_as_its_golden() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../oce-cxf/tests/fixtures/g36/{RICH}.jsonld"));
    let mut engine = Engine::new();
    engine
        .load_cxf(&std::fs::read(&path).expect("read the fixture"))
        .expect("the fixture loads");
    let fresh = engine.compile_blob().expect("the fixture compiles").bytes;
    assert_same_everywhere(&compare(&golden_blob(RICH), &fresh), RICH);
    assert_same_everywhere(&compare(&fresh, &golden_blob(RICH)), RICH);
}

/// What a facade summary golden records, read with `serde_json` alone.
struct Recorded {
    content_id: String,
    /// Class path and hash, in table order.
    classes: Vec<(String, u64)>,
    /// Class path, state word count and initial-state CRC, in block id order.
    instances: Vec<(String, u64, u32)>,
}

fn hex(value: &serde_json::Value) -> u64 {
    u64::from_str_radix(value.as_str().expect("a hex string"), 16).expect("hex digits")
}

fn recorded(name: &str) -> Recorded {
    let golden = golden_summary(name);
    let classes: Vec<(String, u64)> = golden["classes"]
        .as_array()
        .expect("classes")
        .iter()
        .map(|class| {
            (
                class["class_path"].as_str().expect("a path").to_owned(),
                hex(&class["signature_hash"]),
            )
        })
        .collect();
    let instances = golden["instances"]
        .as_array()
        .expect("instances")
        .iter()
        .map(|instance| {
            let index =
                usize::try_from(instance["class"].as_u64().expect("an index")).expect("an index");
            (
                classes[index].0.clone(),
                instance["state_words"].as_u64().expect("a count"),
                u32::try_from(hex(&instance["initial_state_crc"])).expect("a CRC-32C"),
            )
        })
        .collect();
    Recorded {
        content_id: golden["content_id"].as_str().expect("an id").to_owned(),
        classes,
        instances,
    }
}

/// The class changes of two recorded tables, by a merge of the two sorted lists.
fn expected_classes(base: &[(String, u64)], head: &[(String, u64)]) -> Vec<ClassChange> {
    let (mut b, mut h) = (0, 0);
    let mut changes = Vec::new();
    while b < base.len() || h < head.len() {
        let order = match (base.get(b), head.get(h)) {
            (Some(left), Some(right)) => left.0.as_bytes().cmp(right.0.as_bytes()),
            (Some(_), None) => std::cmp::Ordering::Less,
            _ => std::cmp::Ordering::Greater,
        };
        match order {
            std::cmp::Ordering::Less => {
                changes.push(ClassChange::Removed {
                    subject: base[b].0.clone(),
                    hash: base[b].1,
                });
                b += 1;
            }
            std::cmp::Ordering::Greater => {
                changes.push(ClassChange::Added {
                    subject: head[h].0.clone(),
                    hash: head[h].1,
                });
                h += 1;
            }
            std::cmp::Ordering::Equal => {
                if base[b].1 != head[h].1 {
                    changes.push(ClassChange::SignatureChanged {
                        subject: base[b].0.clone(),
                        old: base[b].1,
                        new: head[h].1,
                    });
                }
                b += 1;
                h += 1;
            }
        }
    }
    changes
}

fn expected_initial_state(base: &[(String, u64, u32)], head: &[(String, u64, u32)]) -> Equality {
    let aligned = base.len() == head.len() && base.iter().zip(head).all(|(b, h)| b.0 == h.0);
    if !aligned {
        Equality::Unknown
    } else if base == head {
        Equality::Same
    } else {
        Equality::Different
    }
}

#[test]
fn every_ordered_pair_of_golden_blobs_matches_the_facades_checked_in_summaries() {
    let blobs: Vec<Vec<u8>> = FIXTURES.iter().map(|name| golden_blob(name)).collect();
    let records: Vec<Recorded> = FIXTURES.iter().map(|name| recorded(name)).collect();
    let (mut aligned, mut different_state, mut class_changes) = (0, 0, 0);
    for (i, base) in records.iter().enumerate() {
        for (j, head) in records.iter().enumerate() {
            let what = format!("{} -> {}", FIXTURES[i], FIXTURES[j]);
            let comparison = compare(&blobs[i], &blobs[j]);
            assert_eq!(comparison.same_bytes, i == j, "{what}");
            let ids = if base.content_id == head.content_id {
                Equality::Same
            } else {
                Equality::Different
            };
            assert_eq!(comparison.content_id, ids, "{what}");
            assert_eq!(
                comparison.classes,
                expected_classes(&base.classes, &head.classes),
                "{what}"
            );
            let state = expected_initial_state(&base.instances, &head.instances);
            assert_eq!(comparison.initial_state, state, "{what}");
            let sides = [(&comparison.base, base), (&comparison.head, head)];
            for (side, record) in sides {
                assert!(side.accepted(), "{what}");
                let id = side.content_id.as_ref().map(ToString::to_string);
                assert_eq!(id.as_deref(), Some(record.content_id.as_str()), "{what}");
            }
            if i != j {
                aligned += usize::from(state != Equality::Unknown);
                different_state += usize::from(state == Equality::Different);
                class_changes += usize::from(!comparison.classes.is_empty());
            }
        }
    }
    // What the corpus exercises, pinned so that a corpus that stopped exercising an axis is seen:
    // the ordered pairs of distinct blobs whose lists align (the high-limit variants are one
    // `AddParameter` block each), of those the ones whose initial state differs (none: a
    // changed initial state is the authoring pair's test), and the pairs whose class tables
    // differ, of 2,070.
    assert_eq!((aligned, different_state, class_changes), (62, 0, 2_006));
}

#[test]
fn a_flipped_signature_hash_is_exactly_that_class_signature_changed() {
    let base = golden_blob(RICH);
    let recorded = summary(&base);
    assert_eq!(recorded.classes.len(), 11);
    for (class, entry) in recorded.classes.iter().enumerate() {
        for bit in [0, 31, 63] {
            let what = format!("class {class} ({}), bit {bit}", entry.class_path);
            let head = with_signature_flipped(&base, class, bit);
            let flipped = entry.signature_hash ^ (1u64 << bit);
            let comparison = compare(&base, &head);
            assert_eq!(
                comparison.classes,
                [ClassChange::SignatureChanged {
                    subject: entry.class_path.clone(),
                    old: entry.signature_hash,
                    new: flipped,
                }],
                "{what}"
            );
            assert!(!comparison.same_bytes, "{what}");
            assert_eq!(comparison.content_id, Equality::Same, "{what}");
            assert_eq!(comparison.initial_state, Equality::Same, "{what}");
            // Compared is not runnable: this build refuses the head, and says why.
            assert!(comparison.base.accepted(), "{what}");
            assert_eq!(
                comparison.head.decode_refusal,
                Some(BlobError::ClassSignatureMismatch {
                    class: u32::try_from(class).expect("an index"),
                    expected: entry.signature_hash,
                    found: flipped,
                }),
                "{what}"
            );
            assert_eq!(comparison.base.content_id, comparison.head.content_id);
        }
    }
}

#[test]
fn a_changed_initial_parameter_shows_in_initial_state_and_the_content_id_alone() {
    let base = compiled(&read("blobs/initial_state.base.oce"));
    let head = compiled(&read("blobs/initial_state.head.oce"));
    let comparison = compare(&base, &head);
    assert!(!comparison.same_bytes);
    assert!(comparison.classes.is_empty(), "{:?}", comparison.classes);
    assert_eq!(comparison.initial_state, Equality::Different);
    // `y_start` is a bound parameter, which the canonical content id covers.
    assert_eq!(comparison.content_id, Equality::Different);
    assert!(comparison.base.accepted() && comparison.head.accepted());
    // The two texts differ in one value, of a stateful block; the stateless block's record and
    // the class table are the same on both sides.
    let (base_summary, head_summary) = (summary(&base), summary(&head));
    assert_eq!(base_summary.classes, head_summary.classes);
    let differing: Vec<u32> = base_summary
        .instances
        .iter()
        .zip(&head_summary.instances)
        .filter(|(b, h)| b != h)
        .map(|(b, _)| b.block)
        .collect();
    assert_eq!(differing.len(), 1);
    let block = differing[0] as usize;
    assert!(base_summary.instances[block].stateful);
    assert_eq!(
        base_summary.instances[block].state_words,
        head_summary.instances[block].state_words
    );
}

#[test]
fn a_stateless_parameter_change_differs_in_the_content_id_alone() {
    let text = read("blobs/initial_state.base.oce");
    let base = compiled(&text);
    let head = compiled(&text.replace("k = 2.0", "k = 3.0"));
    let comparison = compare(&base, &head);
    assert!(!comparison.same_bytes);
    assert_eq!(comparison.content_id, Equality::Different);
    assert!(comparison.classes.is_empty());
    assert_eq!(comparison.initial_state, Equality::Same);
}

#[test]
fn a_flipped_initial_state_crc_is_initial_state_different_alone() {
    let base = golden_blob(RICH);
    let recorded = summary(&base);
    let mut flipped = 0;
    for (block, instance) in recorded.instances.iter().enumerate() {
        if !instance.stateful {
            continue;
        }
        let Some(head) = with_initial_state_crc_flipped(&base, block) else {
            continue;
        };
        let what = format!("block {block}");
        let comparison = compare(&base, &head);
        assert_eq!(comparison.initial_state, Equality::Different, "{what}");
        assert!(comparison.classes.is_empty(), "{what}");
        assert_eq!(comparison.content_id, Equality::Same, "{what}");
        assert!(comparison.base.accepted(), "{what}");
        assert!(
            matches!(
                comparison.head.decode_refusal,
                Some(BlobError::InitialStateMismatch { block: refused, .. })
                    if refused as usize == block
            ),
            "{what}: {:?}",
            comparison.head.decode_refusal
        );
        flipped += 1;
    }
    // Every stateful block's CRC is found by its value alone in this blob.
    assert_eq!(flipped, 3);
}

#[test]
fn block_lists_that_do_not_align_by_class_give_unknown_initial_state() {
    let text = read("blobs/initial_state.base.oce");
    let base = compiled(&text);
    // One block's class changes, the count stays: misaligned at one position.
    let swapped = compiled(&text.replace(
        "CDL.Reals.MultiplyByParameter(k = 2.0)",
        "CDL.Reals.AddParameter(p = 2.0)",
    ));
    let comparison = compare(&base, &swapped);
    assert_eq!(comparison.initial_state, Equality::Unknown);
    let kinds: Vec<(&str, &str)> = comparison
        .classes
        .iter()
        .map(|change| match change {
            ClassChange::Added { subject, .. } => ("added", subject.as_str()),
            ClassChange::Removed { subject, .. } => ("removed", subject.as_str()),
            ClassChange::SignatureChanged { subject, .. } => {
                ("signature-changed", subject.as_str())
            }
            _ => ("unknown kind", ""),
        })
        .collect();
    assert_eq!(
        kinds,
        [
            ("added", "CDL.Reals.AddParameter"),
            ("removed", "CDL.Reals.MultiplyByParameter")
        ]
    );
    // A block more: the counts differ.
    let grown = compiled(
        &text
            .replace(
                "y_start = 293.15)\n",
                "y_start = 293.15)\nblock spare = CDL.Discrete.UnitDelay(samplePeriod = 60.0, y_start = 293.15)\n",
            )
            .replace("hold.y -> TSupSet\n", "hold.y -> TSupSet\nuReq -> spare.u\n"),
    );
    assert_eq!(summary(&grown).instances.len(), 3);
    let comparison = compare(&base, &grown);
    assert_eq!(comparison.initial_state, Equality::Unknown);
    assert!(comparison.classes.is_empty());
    // Two blobs of the corpus with different block counts.
    let comparison = compare(&golden_blob(RICH), &golden_blob("multizone_vav_supply_fan"));
    assert_eq!(comparison.initial_state, Equality::Unknown);
}

#[test]
fn a_blob_that_records_no_content_id_is_unknown_on_that_axis_alone() {
    let base = golden_blob(RICH);
    let cleared = with_content_id_cleared(&base);
    assert_eq!(summary(&cleared).content_id, None);
    let pairs = [
        (&base, &cleared, false),
        (&cleared, &base, false),
        (&cleared, &cleared, true),
    ];
    for (left, right, same_bytes) in pairs {
        let comparison = compare(left, right);
        assert_eq!(comparison.content_id, Equality::Unknown);
        assert!(comparison.classes.is_empty());
        assert_eq!(comparison.initial_state, Equality::Same);
        // The decoder never reads the content id, so this build still accepts the blob.
        assert!(comparison.base.accepted() && comparison.head.accepted());
        assert_eq!(comparison.same_bytes, same_bytes);
    }
    let comparison = compare(&cleared, &base);
    assert_eq!(comparison.base.content_id, None);
    assert_eq!(comparison.head.content_id, summary(&base).content_id);
}

#[test]
fn an_aligned_pair_whose_class_tables_differ_compares_each_block_by_its_class_path() {
    // Issue #725: every aligned pair above shares one class table, so reading a block's class
    // one place off within its table would still align them. Here head's table holds one class
    // more, after every class base uses and used by no block: each position keeps its class
    // path, while its index is read against two tables of different lengths.
    let base = golden_blob(RICH);
    let (path, hash) = signature_of("CDL.Routing.BooleanExtractSignal");
    let head = with_unused_class(&base, path, hash);
    let (base_summary, head_summary) = (summary(&base), summary(&head));
    let classes = base_summary.classes.len();
    assert_eq!(head_summary.classes.len(), classes + 1);
    assert_eq!(
        head_summary.decode_refusal,
        Some(BlobError::UnusedClass {
            class: u32::try_from(classes).expect("a class index")
        })
    );
    // The blocks record the same indices, and the last class is one a block uses.
    let indices = |summary: &oce_api::BlobSummary| -> Vec<u32> {
        summary
            .instances
            .iter()
            .map(|instance| instance.class)
            .collect()
    };
    assert_eq!(indices(&base_summary), indices(&head_summary));
    assert!(indices(&base_summary).contains(&u32::try_from(classes - 1).expect("an index")));

    let forward = compare(&base, &head);
    assert_eq!(forward.initial_state, Equality::Same);
    assert_eq!(forward.content_id, Equality::Same);
    assert!(!forward.same_bytes);
    assert_eq!(
        forward.classes,
        [ClassChange::Added {
            subject: path.to_owned(),
            hash
        }]
    );
    assert!(forward.base.accepted() && !forward.head.accepted());
    let backward = compare(&head, &base);
    assert_eq!(backward.initial_state, Equality::Same);
    assert_eq!(
        backward.classes,
        [ClassChange::Removed {
            subject: path.to_owned(),
            hash
        }]
    );
    assert!(!backward.base.accepted() && backward.head.accepted());
}

/// A class's signature hash as the facade's pinned table records it
/// (`crates/oce-api/tests/fixtures/blob/class_signatures.tsv`, read in place).
fn signature_of(class: &'static str) -> (&'static str, u64) {
    let path = facade_fixture("blob/class_signatures.tsv");
    let table = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let hash = table
        .lines()
        .find_map(|line| {
            let (hash, listed) = line.split_once('\t')?;
            (listed == class).then(|| u64::from_str_radix(hash, 16).expect("a hex hash"))
        })
        .unwrap_or_else(|| panic!("{class} is pinned"));
    (class, hash)
}
