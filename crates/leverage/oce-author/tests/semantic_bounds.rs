//! `MAX_SEMANTIC_FIELD_BYTES` pinned at its boundary for atoms, free-text fields and haystack
//! tags: 256 bytes accepted, 257 refused as `invalid-semantic-point`.

use crate::support;

use oce_author::{MAX_SEMANTIC_FIELD_BYTES, Sequence, validate};

use support::{codes, econ_enable};

fn with(set: impl Fn(&mut Sequence, String)) -> Vec<(&'static str, String)> {
    let mut at = econ_enable();
    set(&mut at, "a".repeat(MAX_SEMANTIC_FIELD_BYTES));
    assert_eq!(codes(&validate(&at)), vec![], "accepted at the limit");
    let mut past = econ_enable();
    set(&mut past, "a".repeat(MAX_SEMANTIC_FIELD_BYTES + 1));
    codes(&validate(&past))
}

/// Sets one field of the first input to the given value.
type Setter = fn(&mut Sequence, String);

#[test]
fn every_semantic_field_is_bounded_at_256_bytes() {
    assert_eq!(MAX_SEMANTIC_FIELD_BYTES, 256);
    let refused = vec![("invalid-semantic-point", "TOut".to_owned())];
    let setters: [(&str, Setter); 9] = [
        ("unit", |s, v| s.inputs[0].semantic.unit = Some(v)),
        ("brick", |s, v| s.inputs[0].semantic.brick = Some(v)),
        ("s223", |s, v| s.inputs[0].semantic.s223 = Some(v)),
        ("qudt", |s, v| s.inputs[0].semantic.qudt = Some(v)),
        ("quantity", |s, v| s.inputs[0].quantity = Some(v)),
        ("display_unit", |s, v| s.inputs[0].display_unit = Some(v)),
        ("name", |s, v| s.inputs[0].semantic.name = Some(v)),
        ("label", |s, v| s.inputs[0].semantic.label = Some(v)),
        ("haystack", |s, v| s.inputs[0].semantic.haystack = vec![v]),
    ];
    for (field, set) in setters {
        assert_eq!(with(set), refused, "{field}");
    }
}

#[test]
fn free_text_counts_bytes_not_characters() {
    let mut s = econ_enable();
    s.inputs[0].semantic.label = Some("\u{e9}".repeat(128));
    assert_eq!(
        codes(&validate(&s)),
        vec![],
        "128 two-byte characters are 256 bytes"
    );
    s.inputs[0].semantic.label = Some(format!("{}a", "\u{e9}".repeat(128)));
    assert_eq!(
        codes(&validate(&s)),
        vec![("invalid-semantic-point", "TOut".to_owned())]
    );
}
