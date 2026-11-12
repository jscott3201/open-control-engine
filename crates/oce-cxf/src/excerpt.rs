//! Bounded quotations of document text in diagnostic messages (GitHub issue #602).
//!
//! A refusal that names a key or a token the document wrote copies it into its message. A hostile
//! document can write a key or a token of any length, so a message quotes at most
//! [`EXCERPT_BYTES`] bytes of one, cut on a character boundary and marked with an ellipsis. A
//! canonical `@id` is not cut: it is the identity the message is about.

use std::borrow::Cow;
use std::collections::BTreeMap;

/// The most bytes of one key or token a message quotes.
pub(crate) const EXCERPT_BYTES: usize = 64;

/// The most keys one message names; the rest are counted.
const NAMED_KEYS: usize = 4;

/// `text` whole when it is at most [`EXCERPT_BYTES`] bytes long, otherwise its longest prefix of
/// at most that many bytes that ends on a character boundary, followed by `…`.
pub(crate) fn excerpt(text: &str) -> Cow<'_, str> {
    if text.len() <= EXCERPT_BYTES {
        return Cow::Borrowed(text);
    }
    let mut end = EXCERPT_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    Cow::Owned(format!("{}…", &text[..end]))
}

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

    #[test]
    fn a_short_text_is_quoted_whole_and_a_long_one_is_cut_on_a_character_boundary() {
        let at_limit = "k".repeat(EXCERPT_BYTES);
        assert_eq!(excerpt(&at_limit), at_limit);
        let over = "k".repeat(EXCERPT_BYTES + 1);
        assert_eq!(excerpt(&over), format!("{}…", "k".repeat(EXCERPT_BYTES)));
        // A two-byte character straddling the limit is left out whole, never split.
        let straddling = format!("{}é", "k".repeat(EXCERPT_BYTES - 1));
        assert_eq!(
            excerpt(&straddling),
            format!("{}…", "k".repeat(EXCERPT_BYTES - 1))
        );
        // A four-byte character ending exactly at the limit is kept.
        let ending = format!("{}😀k", "k".repeat(EXCERPT_BYTES - 4));
        assert_eq!(
            excerpt(&ending),
            format!("{}😀…", "k".repeat(EXCERPT_BYTES - 4))
        );
        assert_eq!(excerpt(""), "");
    }

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
