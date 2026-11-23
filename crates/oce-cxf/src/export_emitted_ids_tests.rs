//! An `@id` held in parts compares as the `String` it spells, so the duplicate check finds the
//! same repeats it found when it held each `@id` whole.

use std::borrow::Cow;
use std::collections::BTreeSet;

use super::EmittedId;

/// The `String` an `@id` spells.
fn spelled(id: &EmittedId<'_>) -> String {
    match id {
        EmittedId::Whole(whole) => whole.to_string(),
        EmittedId::Member(block, name) => format!("{block}.{name}"),
    }
}

/// Every way to hold the spellings below: whole, and as a member split at each `.`.
fn held(spellings: &[&'static str]) -> Vec<EmittedId<'static>> {
    let mut ids = Vec::new();
    for &s in spellings {
        ids.push(EmittedId::Whole(Cow::Borrowed(s)));
        ids.push(EmittedId::Whole(Cow::Owned(s.to_owned())));
        for (at, _) in s.match_indices('.') {
            ids.push(EmittedId::Member(Cow::Borrowed(&s[..at]), &s[at + 1..]));
            ids.push(EmittedId::Member(
                Cow::Owned(s[..at].to_owned()),
                &s[at + 1..],
            ));
        }
    }
    ids
}

const SPELLINGS: &[&str] = &[
    "",
    ".",
    "..",
    "a",
    "a.",
    ".a",
    "a.b",
    "a.b.c",
    "a.bc",
    "ab.c",
    "a.b0",
    "a.b\u{0}",
    "a\u{0}.b",
    "a-b",
    "a/b",
    "urn:x",
    "urn:x.k",
    "urn:x.k_1",
    "urn:x.k_10",
    "urn:x.k_1_1",
    "urn:x.y.k",
    "urn:xé.k",
    "urn:x\u{ff}.k",
    "urn:x.k\u{10ffff}",
];

#[test]
fn an_id_in_parts_compares_as_the_string_it_spells() {
    let ids = held(SPELLINGS);
    for a in &ids {
        for b in &ids {
            assert_eq!(a.cmp(b), spelled(a).cmp(&spelled(b)), "{a:?} against {b:?}");
            assert_eq!(a == b, spelled(a) == spelled(b), "{a:?} against {b:?}");
        }
    }
}

#[test]
fn a_set_of_ids_in_parts_finds_the_repeats_a_set_of_strings_finds() {
    // Insert every holding of every spelling, in the order given and reversed: each insertion is
    // refused exactly where the same spelling as a `String` is.
    for reversed in [false, true] {
        let mut ids = held(SPELLINGS);
        if reversed {
            ids.reverse();
        }
        let (mut parts, mut strings) = (BTreeSet::new(), BTreeSet::new());
        let mut refused = 0;
        for id in ids {
            let text = spelled(&id);
            let fresh = parts.insert(id);
            assert_eq!(fresh, strings.insert(text.clone()), "{text:?}");
            refused += usize::from(!fresh);
        }
        assert_eq!(parts.len(), SPELLINGS.len());
        assert!(refused > SPELLINGS.len());
    }
}
