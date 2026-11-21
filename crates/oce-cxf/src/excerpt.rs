//! Bounded quotations of document text in diagnostic messages (GitHub issue #602).
//!
//! A refusal that names a key or a token the document wrote copies it into its message. A hostile
//! document can write a key or a token of any length, so a message quotes at most
//! [`oce_diag::EXCERPT_BYTES`] bytes of one ([`oce_diag::excerpt`]: cut on a character boundary and
//! marked with an ellipsis; it lives in `oce-diag` beside the diagnostic list whose closing error
//! uses it, issue #726). A canonical `@id` is not cut: it is the identity the message is about.
//! This module names a node's keys in one message.

use std::collections::BTreeMap;

use oce_diag::excerpt;

/// The most keys one message names; the rest are counted.
const NAMED_KEYS: usize = 4;

/// The keys of `keys`, in key order and each an [`excerpt`]: `` the key `k` `` for one,
/// `` the keys `a`, `b` `` for several, at most four of them, then how many more.
pub(crate) fn named_keys<V>(keys: &BTreeMap<String, V>) -> String {
    let named: Vec<String> = keys
        .keys()
        .take(NAMED_KEYS)
        .map(|key| format!("`{}`", excerpt(key)))
        .collect();
    let noun = if keys.len() == 1 {
        "the key"
    } else {
        "the keys"
    };
    let list = named.join(", ");
    match keys.len() - named.len() {
        0 => format!("{noun} {list}"),
        more => format!("{noun} {list} and {more} more"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oce_diag::EXCERPT_BYTES;

    #[test]
    fn keys_are_named_up_to_four_each_as_an_excerpt() {
        let keys = |names: &[&str]| -> BTreeMap<String, ()> {
            names.iter().map(|name| ((*name).to_owned(), ())).collect()
        };
        assert_eq!(named_keys(&keys(&["a"])), "the key `a`");
        assert_eq!(named_keys(&keys(&["b", "a"])), "the keys `a`, `b`");
        assert_eq!(
            named_keys(&keys(&["a", "b", "c", "d", "e"])),
            "the keys `a`, `b`, `c`, `d` and 1 more"
        );
        let long = "x".repeat(1 << 20);
        assert_eq!(
            named_keys(&keys(&[&long])),
            format!("the key `{}…`", "x".repeat(EXCERPT_BYTES))
        );
    }
}
