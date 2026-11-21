//! How a message quotes document text: bounded (GitHub issue #602) and with every character no
//! reader can see written as an escape (GitHub #641; owner ruling of 2026-11-12).
//!
//! A refusal that names a key, a token or an `@id` the document wrote copies it into its message.
//! A hostile document can write one of any length, so a message quotes at most [`EXCERPT_BYTES`]
//! bytes of it ([`excerpt`]), and it can hide characters in one, so a message writes each Unicode
//! format or control character as `\u{…}` ([`visible`]).
//!
//! A character is a format or control character when its Unicode general category is `Cf`
//! (format) or `Cc` (control):
//!
//! - **`Cc`** is decided by [`char::is_control`], which the standard library defines as exactly
//!   that category: U+0000 to U+001F and U+007F to U+009F, 65 code points.
//! - **`Cf`** has no standard-library predicate, so [`FORMAT_RANGES`] lists it: the 170 code points
//!   in 21 ranges that Unicode 17.0.0 assigns to the category (`DerivedGeneralCategory.txt`, as
//!   Python's `unicodedata` 17.0.0 reports it), among them U+00AD SOFT HYPHEN, U+180E MONGOLIAN
//!   VOWEL SEPARATOR, U+200B ZERO WIDTH SPACE, U+2060 WORD JOINER and U+FEFF ZERO WIDTH NO-BREAK
//!   SPACE (the byte-order mark). A later Unicode version that adds a `Cf` code point needs the
//!   table extended. `oce-cxf` checks every Unicode scalar value against a verbatim excerpt of
//!   `DerivedGeneralCategory.txt` 17.0.0 checked in beside its tests
//!   (`crates/oce-cxf/tests/fixtures/unicode/`), so a range that drops, gains or shifts a code
//!   point fails there.
//!
//! These live here, beside [`crate::DiagnosticList`], because the closing error of every list
//! quotes a subject this way, whichever stage built the list (issue #726).

use std::borrow::Cow;

/// The most bytes of one key, token or subject a message quotes.
pub const EXCERPT_BYTES: usize = 64;

/// `text` whole when it is at most [`EXCERPT_BYTES`] bytes long, otherwise its longest prefix of
/// at most that many bytes that ends on a character boundary, followed by `…`. Never panics; it
/// allocates only when it cuts.
#[must_use]
pub fn excerpt(text: &str) -> Cow<'_, str> {
    if text.len() <= EXCERPT_BYTES {
        return Cow::Borrowed(text);
    }
    let mut end = EXCERPT_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    Cow::Owned(format!("{}…", &text[..end]))
}

/// The Unicode 17.0.0 general category `Cf`, as inclusive code-point ranges in ascending order.
pub const FORMAT_RANGES: [(u32, u32); 21] = [
    (0x00AD, 0x00AD),
    (0x0600, 0x0605),
    (0x061C, 0x061C),
    (0x06DD, 0x06DD),
    (0x070F, 0x070F),
    (0x0890, 0x0891),
    (0x08E2, 0x08E2),
    (0x180E, 0x180E),
    (0x200B, 0x200F),
    (0x202A, 0x202E),
    (0x2060, 0x2064),
    (0x2066, 0x206F),
    (0xFEFF, 0xFEFF),
    (0xFFF9, 0xFFFB),
    (0x110BD, 0x110BD),
    (0x110CD, 0x110CD),
    (0x13430, 0x1343F),
    (0x1BCA0, 0x1BCA3),
    (0x1D173, 0x1D17A),
    (0xE0001, 0xE0001),
    (0xE0020, 0xE007F),
];

/// Whether `c` is in the general category `Cf` ([`FORMAT_RANGES`], by binary search). Never
/// panics and allocates nothing.
#[must_use]
pub fn is_format(c: char) -> bool {
    let code = u32::from(c);
    FORMAT_RANGES
        .binary_search_by(|&(low, high)| {
            if high < code {
                std::cmp::Ordering::Less
            } else if low > code {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// Whether `c` is a Unicode format (`Cf`) or control (`Cc`) character: one no reader of a
/// document can see.
///
/// `Cc` is [`char::is_control`]: U+0000 to U+001F and U+007F to U+009F, the tab, the line feed,
/// the carriage return and U+0085 among them. `Cf` is the 170 code points Unicode 17.0.0 assigns
/// to the category, among them U+00AD SOFT HYPHEN, U+180E MONGOLIAN VOWEL SEPARATOR, U+200B ZERO
/// WIDTH SPACE, the bidirectional controls U+202A to U+202E and U+2066 to U+2069, U+2060 WORD
/// JOINER and U+FEFF, the byte-order mark. Nothing else is: not a space or a no-break space, not a
/// line or paragraph separator (U+2028, U+2029: category `Z`), not a variation selector or a
/// combining mark (`M`), not a private-use, surrogate or unassigned code point (`Co`, `Cs`, `Cn`).
///
/// It is the one decision of which characters the CXF readers refuse in a typing token (a `@type`
/// value or `isOfDataType` reference) and in a `unit`, `quantity` or `displayUnit` term, so the
/// engine's loader and the authoring importer cannot disagree on it (GitHub #641, issue #629);
/// `oce-cxf` and the facade re-export it. It never panics and allocates nothing.
#[must_use]
pub fn is_format_or_control(c: char) -> bool {
    c.is_control() || is_format(c)
}

/// `text` with each format or control character ([`is_format_or_control`]) written as `\u{…}`
/// (at least four upper-case hex digits), so a message never carries one as it is. Never panics;
/// it allocates only when it escapes.
#[must_use]
pub fn visible(text: &str) -> Cow<'_, str> {
    if !text.contains(is_format_or_control) {
        return Cow::Borrowed(text);
    }
    Cow::Owned(
        text.chars()
            .map(|c| {
                if is_format_or_control(c) {
                    format!("\\u{{{:04X}}}", u32::from(c))
                } else {
                    c.to_string()
                }
            })
            .collect(),
    )
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
    fn the_format_table_is_unicode_17_s_cf_in_ascending_disjoint_ranges() {
        let size: u32 = FORMAT_RANGES.iter().map(|(low, high)| high - low + 1).sum();
        assert_eq!(size, 170, "Unicode 17.0.0 assigns 170 code points to Cf");
        for pair in FORMAT_RANGES.windows(2) {
            assert!(
                pair[0].1 + 1 < pair[1].0,
                "ranges ascend and never touch: {pair:?}"
            );
        }
        for (low, high) in FORMAT_RANGES {
            assert!(low <= high);
            for edge in [low, high] {
                assert!(
                    is_format(char::from_u32(edge).expect("a scalar value")),
                    "{edge:04X}"
                );
            }
            // The code points just outside each range are not Cf (U+2065, between two ranges, is
            // unassigned).
            for outside in [low - 1, high + 1] {
                if let Some(c) = char::from_u32(outside) {
                    assert!(!is_format(c), "{outside:04X}");
                }
            }
        }
    }

    #[test]
    fn a_message_writes_each_invisible_character_as_an_escape() {
        assert_eq!(visible("S231:Block"), "S231:Block");
        assert!(matches!(visible("S231:Block"), Cow::Borrowed(_)));
        assert_eq!(
            visible("\u{200B}S231:\u{1}Block"),
            "\\u{200B}S231:\\u{0001}Block"
        );
        assert_eq!(visible("\u{E0001}"), "\\u{E0001}");
    }
}
