//! Unicode format and control characters: characters no reader of a document can see (GitHub #641;
//! owner ruling of 2026-11-12).
//!
//! A character is one when its Unicode general category is `Cf` (format) or `Cc` (control):
//!
//! - **`Cc`** is decided by [`char::is_control`], which the standard library defines as exactly
//!   that category: U+0000 to U+001F and U+007F to U+009F, 65 code points.
//! - **`Cf`** has no standard-library predicate, so [`FORMAT_RANGES`] lists it: the 170 code points
//!   in 21 ranges that Unicode 17.0.0 assigns to the category (`DerivedGeneralCategory.txt`, as
//!   Python's `unicodedata` 17.0.0 reports it), among them U+00AD SOFT HYPHEN, U+180E MONGOLIAN
//!   VOWEL SEPARATOR, U+200B ZERO WIDTH SPACE, U+2060 WORD JOINER and U+FEFF ZERO WIDTH NO-BREAK
//!   SPACE (the byte-order mark). A later Unicode version that adds a `Cf` code point needs the
//!   table extended; a test pins its size and boundaries.
//!
//! A message never quotes such a character as it is: [`visible`] writes each one as `\u{…}`.

use std::borrow::Cow;

/// The Unicode 17.0.0 general category `Cf`, as inclusive code-point ranges in ascending order.
pub(crate) const FORMAT_RANGES: [(u32, u32); 21] = [
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

/// Whether `c` is in the general category `Cf` ([`FORMAT_RANGES`], by binary search).
pub(crate) fn is_format(c: char) -> bool {
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

/// Whether `c` is a format (`Cf`) or control (`Cc`) character.
pub(crate) fn is_format_or_control(c: char) -> bool {
    c.is_control() || is_format(c)
}

/// The first format or control character of `text`, if it has one.
pub(crate) fn first_format_or_control(text: &str) -> Option<char> {
    text.chars().find(|&c| is_format_or_control(c))
}

/// How a message names `c`: its code point and category, `U+200B, a format character (Cf)`.
pub(crate) fn describe(c: char) -> String {
    let kind = if c.is_control() {
        "a control character (Cc)"
    } else {
        "a format character (Cf)"
    };
    format!("U+{:04X}, {kind}", u32::from(c))
}

/// `text` with each format or control character written as `\u{…}`, so a message never carries
/// one as it is.
pub(crate) fn visible(text: &str) -> Cow<'_, str> {
    if first_format_or_control(text).is_none() {
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
    fn the_reviewed_invisible_characters_are_format_or_control_and_visible_letters_are_not() {
        for c in [
            '\u{200B}',
            '\u{FEFF}',
            '\u{2060}',
            '\u{180E}',
            '\u{AD}',
            '\u{200D}',
            '\u{202E}',
            '\u{E0001}',
            '\u{1D173}',
            '\u{061C}',
        ] {
            assert!(
                is_format(c) && is_format_or_control(c),
                "{:04X}",
                u32::from(c)
            );
        }
        for c in [
            '\u{0}', '\u{1}', '\t', '\n', '\r', '\u{7F}', '\u{85}', '\u{9F}',
        ] {
            assert!(
                c.is_control() && is_format_or_control(c),
                "{:04X}",
                u32::from(c)
            );
            assert!(!is_format(c));
        }
        for c in [
            'a', ' ', '\u{A0}', '\u{200A}', '\u{3000}', '\u{2028}', 'é', '😀', '\u{A0}',
        ] {
            assert!(!is_format_or_control(c), "{:04X}", u32::from(c));
        }
        assert_eq!(
            first_format_or_control("S\u{200B}231:Block"),
            Some('\u{200B}')
        );
        assert_eq!(first_format_or_control("S231:Block"), None);
    }

    #[test]
    fn a_message_writes_each_invisible_character_as_an_escape() {
        assert_eq!(visible("S231:Block"), "S231:Block");
        assert_eq!(
            visible("\u{200B}S231:\u{1}Block"),
            "\\u{200B}S231:\\u{0001}Block"
        );
        assert_eq!(visible("\u{E0001}"), "\\u{E0001}");
        assert_eq!(describe('\u{FEFF}'), "U+FEFF, a format character (Cf)");
        assert_eq!(describe('\u{1}'), "U+0001, a control character (Cc)");
    }
}
