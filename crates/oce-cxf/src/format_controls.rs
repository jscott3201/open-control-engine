//! Unicode format and control characters: characters no reader of a document can see (GitHub #641;
//! owner ruling of 2026-11-12).
//!
//! The decision of which characters those are is [`oce_diag::is_format_or_control`]: `Cc` by
//! [`char::is_control`], and `Cf` by [`oce_diag::FORMAT_RANGES`], the 170 code points in 21 ranges
//! Unicode 17.0.0 assigns to the category. It lives in `oce-diag` because every diagnostic list's
//! closing error quotes a subject with it, whichever stage built the list (issue #726); this crate
//! re-exports it. A test here checks every Unicode scalar value against a verbatim excerpt of
//! `DerivedGeneralCategory.txt` 17.0.0 checked in beside the tests (`tests/fixtures/unicode/`,
//! whose README says how it was cut), so a range that drops, gains or shifts a code point fails it.
//!
//! Both CXF readers refuse such a character in the same places with this one decision (which the
//! facade re-exports for the authoring importer): in a typing token (GitHub #641) and in a `unit`,
//! `quantity` or `displayUnit` term (issue #629).
//!
//! A message never quotes such a character as it is: [`oce_diag::visible`] writes each one as
//! `\u{…}`.

use oce_diag::is_format_or_control;

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

#[cfg(test)]
mod tests {
    use super::*;
    use oce_diag::{FORMAT_RANGES, is_format};

    /// The `Cc` and `Cf` sections of Unicode 17.0.0's `DerivedGeneralCategory.txt`, verbatim, with
    /// its header (`tests/fixtures/unicode/README.md` says how they were cut and checked).
    const DERIVED_CC_CF: &str =
        include_str!("../tests/fixtures/unicode/DerivedGeneralCategory-17.0.0-Cc-Cf.txt");

    /// The excerpt's data lines as `(first, last, category)`, each section's stated total checked
    /// against the code points its lines list.
    fn derived_ranges() -> Vec<(u32, u32, &'static str)> {
        assert_eq!(
            DERIVED_CC_CF.lines().next(),
            Some("# DerivedGeneralCategory-17.0.0.txt"),
            "the excerpt is of Unicode 17.0.0"
        );
        let mut ranges = Vec::new();
        let mut section = 0;
        let mut totals = Vec::new();
        for line in DERIVED_CC_CF.lines() {
            if let Some(total) = line.strip_prefix("# Total code points: ") {
                totals.push((total.parse::<u32>().expect("a count"), section));
                section = 0;
                continue;
            }
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (field, _comment) = line.split_once('#').expect("a data line has a comment");
            let (points, category) = field.split_once(';').expect("a data line has a category");
            let parse = |hex: &str| u32::from_str_radix(hex.trim(), 16).expect("hex code point");
            let (first, last) = match points.split_once("..") {
                Some((first, last)) => (parse(first), parse(last)),
                None => (parse(points), parse(points)),
            };
            assert!(first <= last, "{line}");
            section += last - first + 1;
            ranges.push((first, last, category.trim()));
        }
        assert_eq!(
            totals,
            [(65, 65), (170, 170)],
            "each section's stated total"
        );
        ranges
    }

    #[test]
    fn the_format_table_holds_every_code_point_unicode_17_assigns_to_cf_and_no_other() {
        // The second review of GitHub #641 shifted `(0x0890, 0x0891)` to `(0x0891, 0x0892)` and
        // every shape test above still passed. This one reads the categories from Unicode's own
        // data and checks every scalar value, so a range that drops, gains or shifts a code point
        // fails (that shift fails it at `U+0890 as Cf`).
        let derived = derived_ranges();
        let category = |code: u32| {
            derived
                .iter()
                .find(|&&(first, last, _)| (first..=last).contains(&code))
                .map(|&(_, _, category)| category)
        };
        let mut counts = (0, 0);
        for code in 0..=u32::from(char::MAX) {
            let Some(c) = char::from_u32(code) else {
                continue; // A surrogate is no scalar value, and its category is `Cs`.
            };
            let expected = category(code);
            assert_eq!(is_format(c), expected == Some("Cf"), "U+{code:04X} as Cf");
            assert_eq!(c.is_control(), expected == Some("Cc"), "U+{code:04X} as Cc");
            assert_eq!(
                is_format_or_control(c),
                expected.is_some(),
                "U+{code:04X} as either"
            );
            match expected {
                Some("Cf") => counts.0 += 1,
                Some("Cc") => counts.1 += 1,
                Some(other) => panic!("U+{code:04X}: an unexpected category `{other}`"),
                None => {}
            }
        }
        assert_eq!(counts, (170, 65), "the code points checked as Cf and Cc");
        // The table's ranges are the excerpt's `Cf` lines, one for one.
        let listed: Vec<(u32, u32)> = derived
            .iter()
            .filter(|&&(_, _, category)| category == "Cf")
            .map(|&(first, last, _)| (first, last))
            .collect();
        assert_eq!(listed, FORMAT_RANGES, "the table is the excerpt's Cf lines");
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
    fn a_message_names_an_invisible_character_by_code_point_and_category() {
        assert_eq!(describe('\u{FEFF}'), "U+FEFF, a format character (Cf)");
        assert_eq!(describe('\u{1}'), "U+0001, a control character (Cc)");
    }
}
