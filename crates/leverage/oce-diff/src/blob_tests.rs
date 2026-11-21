//! The three axes of a blob comparison on facts a summary cannot be built to hold, below the
//! public entry: the facade's `BlobSummary` is `#[non_exhaustive]` and only `inspect_blob` makes
//! one, so a state length that differs under an equal CRC, a class index outside its table or an
//! error the facade documents it never returns are reached here. The public entry's tests are
//! `tests/blob_*.rs`.

use oce_api::{BlobError, ContentId, MalformedFault, OcError};

use super::{
    BlobSide, ClassChange, Equality, Instance, class_changes, content_id, initial_state, refused,
};
use crate::data::Side;
use crate::work::tests::work;

fn id(digit: char) -> ContentId {
    ContentId::parse(&format!("oce:canon1:{}", digit.to_string().repeat(64))).expect("an id")
}

fn instance(class: Option<&str>, state_words: u32, initial_state_crc: u32) -> Instance<'_> {
    Instance {
        class,
        state_words,
        initial_state_crc,
    }
}

#[test]
fn content_ids_are_unknown_unless_both_sides_record_one() {
    let (a, b) = (id('a'), id('b'));
    assert_eq!(content_id(Some(&a), Some(&a)), Equality::Same);
    assert_eq!(content_id(Some(&a), Some(&a.clone())), Equality::Same);
    assert_eq!(content_id(Some(&a), Some(&b)), Equality::Different);
    assert_eq!(content_id(Some(&b), Some(&a)), Equality::Different);
    assert_eq!(content_id(None, Some(&a)), Equality::Unknown);
    assert_eq!(content_id(Some(&a), None), Equality::Unknown);
    assert_eq!(content_id(None, None), Equality::Unknown);
}

#[test]
fn class_changes_are_one_entry_a_path_sorted_bytewise() {
    let base = [
        ("CDL.Logical.And", 1),
        ("CDL.Reals.Add", 2),
        ("CDL.Reals.Switch", 3),
        ("CDL.reals.lower", 4),
    ];
    let head = [
        ("CDL.Logical.And", 1),
        ("CDL.Logical.Or", 5),
        ("CDL.Reals.Switch", 6),
        ("CDL.Z", 7),
    ];
    let changes = class_changes(&base, &head);
    assert_eq!(
        changes,
        [
            ClassChange::Added {
                subject: "CDL.Logical.Or".to_owned(),
                hash: 5
            },
            ClassChange::Removed {
                subject: "CDL.Reals.Add".to_owned(),
                hash: 2
            },
            ClassChange::SignatureChanged {
                subject: "CDL.Reals.Switch".to_owned(),
                old: 3,
                new: 6
            },
            // `Z` (0x5a) sorts before `r` (0x72): bytes, not a case-folded order.
            ClassChange::Added {
                subject: "CDL.Z".to_owned(),
                hash: 7
            },
            ClassChange::Removed {
                subject: "CDL.reals.lower".to_owned(),
                hash: 4
            },
        ]
    );
    let subjects: Vec<&str> = changes.iter().map(ClassChange::subject).collect();
    assert_eq!(
        subjects,
        [
            "CDL.Logical.Or",
            "CDL.Reals.Add",
            "CDL.Reals.Switch",
            "CDL.Z",
            "CDL.reals.lower"
        ]
    );
}

#[test]
fn class_changes_mirror_by_swapping_added_and_removed_and_the_two_hashes() {
    let base = [("A", 1), ("B", 2), ("C", 3)];
    let head = [("B", 9), ("C", 3), ("D", 4)];
    let forward = class_changes(&base, &head);
    let mirrored: Vec<ClassChange> = class_changes(&head, &base);
    let swapped: Vec<ClassChange> = forward
        .into_iter()
        .map(|change| match change {
            ClassChange::Added { subject, hash } => ClassChange::Removed { subject, hash },
            ClassChange::Removed { subject, hash } => ClassChange::Added { subject, hash },
            ClassChange::SignatureChanged { subject, old, new } => ClassChange::SignatureChanged {
                subject,
                old: new,
                new: old,
            },
        })
        .collect();
    assert_eq!(mirrored, swapped);
}

#[test]
fn equal_and_empty_class_tables_have_no_change() {
    assert!(class_changes(&[], &[]).is_empty());
    let table = [("CDL.Reals.Add", u64::MAX), ("CDL.Reals.Sub", 0)];
    assert!(class_changes(&table, &table).is_empty());
    assert_eq!(
        class_changes(&[("X", 0)], &[("X", u64::MAX)]),
        [ClassChange::SignatureChanged {
            subject: "X".to_owned(),
            old: 0,
            new: u64::MAX
        }]
    );
}

#[test]
fn initial_state_is_unknown_unless_the_lists_align_by_class() {
    let a = instance(Some("A"), 1, 10);
    let b = instance(Some("B"), 0, 0);
    // A count that differs.
    assert_eq!(initial_state(&[a], &[a, b]), Equality::Unknown);
    assert_eq!(initial_state(&[], &[b]), Equality::Unknown);
    // A class that differs at one position, the state equal everywhere.
    assert_eq!(
        initial_state(&[a, b], &[a, instance(Some("C"), 0, 0)]),
        Equality::Unknown
    );
    // The same classes in another order.
    assert_eq!(initial_state(&[a, b], &[b, a]), Equality::Unknown);
    // A difference of state before the misaligned position does not make it `Different`.
    assert_eq!(
        initial_state(
            &[a, b],
            &[instance(Some("A"), 1, 11), instance(Some("C"), 0, 0)]
        ),
        Equality::Unknown
    );
    // A class index outside the table names no class, so it aligns with nothing, itself
    // included.
    let unnamed = instance(None, 0, 0);
    assert_eq!(initial_state(&[unnamed], &[unnamed]), Equality::Unknown);
    assert_eq!(initial_state(&[unnamed], &[b]), Equality::Unknown);
    assert_eq!(initial_state(&[b], &[unnamed]), Equality::Unknown);
}

#[test]
fn aligned_lists_differ_by_state_length_or_crc_at_any_position() {
    let a = instance(Some("A"), 2, 0xdead_beef);
    let b = instance(Some("B"), 0, 0);
    assert_eq!(initial_state(&[], &[]), Equality::Same);
    assert_eq!(initial_state(&[a, b], &[a, b]), Equality::Same);
    // The CRC at the first, the last position.
    assert_eq!(
        initial_state(&[a, b], &[instance(Some("A"), 2, 0xdead_beee), b]),
        Equality::Different
    );
    assert_eq!(
        initial_state(&[b, a], &[b, instance(Some("A"), 2, 0)]),
        Equality::Different
    );
    // The same CRC over words of another length is another state.
    assert_eq!(
        initial_state(&[a], &[instance(Some("A"), 3, 0xdead_beef)]),
        Equality::Different
    );
    assert_eq!(
        initial_state(&[b], &[instance(Some("B"), u32::MAX, 0)]),
        Equality::Different
    );
}

#[test]
fn a_blob_refusal_is_unreadable_blob_and_any_other_error_is_kept_as_itself() {
    let refusal = BlobError::Malformed {
        offset: 0,
        fault: MalformedFault::Truncated,
    };
    let error = refused(Side::Head, OcError::Blob(refusal));
    assert_eq!(
        error,
        crate::DiffError::UnreadableBlob {
            side: Side::Head,
            refusal
        }
    );
    assert_eq!(error.code(), "unreadable-blob");
    assert_eq!(
        error.to_string(),
        "head: the blob has no summary: malformed blob at byte 0: Truncated"
    );

    let error = refused(Side::Base, OcError::InputDomain("p".to_owned()));
    assert_eq!(
        error,
        crate::DiffError::UnexpectedRefusal {
            side: Side::Base,
            message: "input value outside declared domain for 'p'".to_owned()
        }
    );
    assert_eq!(error.code(), "unexpected-refusal");
    assert!(
        error
            .to_string()
            .starts_with("base: the facade refused the blob with an error that is not a blob")
    );
}

#[test]
fn a_side_is_written_as_accepted_and_its_content_id() {
    let accepted = BlobSide {
        content_id: Some(id('0')),
        decode_refusal: None,
    };
    assert!(accepted.accepted());
    assert_eq!(
        serde_json::to_string(&accepted).expect("a side serializes"),
        format!(
            "{{\"accepted\":true,\"content_id\":\"oce:canon1:{}\"}}",
            "0".repeat(64)
        )
    );
    let refused = BlobSide {
        content_id: None,
        decode_refusal: Some(BlobError::UnusedClass { class: 3 }),
    };
    assert!(!refused.accepted());
    assert_eq!(
        serde_json::to_string(&refused).expect("a side serializes"),
        "{\"accepted\":false,\"content_id\":null}"
    );
}

#[test]
fn a_hash_is_written_as_sixteen_lowercase_hex_digits() {
    let changes = [
        ClassChange::Added {
            subject: "a".to_owned(),
            hash: 0,
        },
        ClassChange::SignatureChanged {
            subject: "b".to_owned(),
            old: 0xABCD,
            new: u64::MAX,
        },
    ];
    assert_eq!(
        serde_json::to_string(&changes).expect("changes serialize"),
        concat!(
            "[{\"kind\":\"added\",\"subject\":\"a\",\"hash\":\"0x0000000000000000\"},",
            "{\"kind\":\"signature-changed\",\"subject\":\"b\",",
            "\"old\":\"0x000000000000abcd\",\"new\":\"0xffffffffffffffff\"}]"
        )
    );
}

#[test]
fn the_comparison_costs_linear_work_in_the_instances() {
    let classes: Vec<String> = (0..1_000).map(|i| format!("C{i:04}")).collect();
    let instances: Vec<Instance<'_>> = (0..100_000_u32)
        .map(|i| instance(Some(&classes[(i % 1_000) as usize]), i % 3, i))
        .collect();
    let (units, equality) = work(|| initial_state(&instances, &instances));
    assert_eq!(equality, Equality::Same);
    assert!(units <= 100_000, "{units} units for 100,000 instances");
    let table: Vec<(&str, u64)> = classes.iter().map(|path| (path.as_str(), 0)).collect();
    let (units, changes) = work(|| class_changes(&table, &table));
    assert!(changes.is_empty());
    // Two ordered maps built and one probe a class: a few comparisons a class and bit.
    assert!(
        units <= 2_000 + 8 * 1_000 * 10,
        "{units} units for 1,000 classes"
    );
}
