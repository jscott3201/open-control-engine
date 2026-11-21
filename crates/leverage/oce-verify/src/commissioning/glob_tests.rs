//! The glob matcher against a second implementation, exhaustively over short patterns and
//! texts, its cost in steps, and a key's prefix.

use super::*;

/// A second implementation: the textbook table over characters, `table[i][j]` being whether the
/// first `i` characters of the pattern match the first `j` of the text.
fn reference(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let mut table = vec![vec![false; text.len() + 1]; pattern.len() + 1];
    table[0][0] = true;
    for i in 1..=pattern.len() {
        for j in 0..=text.len() {
            table[i][j] = match pattern[i - 1] {
                '*' => table[i - 1][j] || (j > 0 && table[i][j - 1]),
                '?' => j > 0 && table[i - 1][j - 1],
                literal => j > 0 && table[i - 1][j - 1] && text[j - 1] == literal,
            };
        }
    }
    table[pattern.len()][text.len()]
}

/// Every string of up to `length` characters over `alphabet`.
fn strings(alphabet: &[char], length: usize) -> Vec<String> {
    let mut all = vec![String::new()];
    let mut last = all.clone();
    for _ in 0..length {
        let mut next = Vec::new();
        for text in &last {
            for character in alphabet {
                let mut longer = text.clone();
                longer.push(*character);
                next.push(longer);
            }
        }
        all.extend(next.iter().cloned());
        last = next;
    }
    all
}

fn bound(pattern: &str, text: &str) -> u64 {
    u64::try_from((pattern.len() + 1) * (text.len() + 1)).expect("fits")
}

#[test]
fn every_short_pattern_matches_what_the_table_says_within_its_step_bound() {
    // Patterns over two letters and both wildcards, texts over the two letters: 1,365 patterns
    // against 127 texts.
    let patterns = strings(&['a', 'b', '*', '?'], 5);
    let texts = strings(&['a', 'b'], 6);
    assert_eq!((patterns.len(), texts.len()), (1_365, 127));
    let (mut matched, mut most) = (0_u32, 0_u64);
    for pattern in &patterns {
        for text in &texts {
            let expected = reference(pattern, text);
            assert_eq!(
                glob_matches(pattern, text),
                expected,
                "{pattern:?} against {text:?}"
            );
            let steps = match_steps(pattern, text);
            assert!(
                steps <= bound(pattern, text),
                "{pattern:?} against {text:?}: {steps}"
            );
            most = most.max(steps);
            matched += u32::from(expected);
        }
    }
    assert!(matched > 20_000, "{matched}");
    assert!(most >= 20, "the worst case backtracks: {most}");
}

#[test]
fn a_wildcard_takes_whole_characters_whatever_their_width() {
    // One, two, three and four bytes; a `?` or a `*` never stops inside one.
    let letters = ['a', '\u{e9}', '\u{20ac}', '\u{1f321}'];
    let mut alphabet = letters.to_vec();
    alphabet.extend(['*', '?']);
    let patterns = strings(&alphabet, 3);
    let texts = strings(&letters, 4);
    for pattern in &patterns {
        for text in &texts {
            assert_eq!(
                glob_matches(pattern, text),
                reference(pattern, text),
                "{pattern:?} against {text:?}"
            );
            assert!(match_steps(pattern, text) <= bound(pattern, text));
        }
    }
    // A pattern of two bytes that are one character does not match half of a wider one.
    assert!(!glob_matches("\u{e9}", "\u{e9}\u{e9}"));
    assert!(!glob_matches("?", "\u{e9}\u{e9}"));
    assert!(glob_matches("??", "\u{e9}\u{1f321}"));
}

#[test]
fn the_hostile_shapes_stay_within_the_step_bound_at_full_length() {
    // The longest key and the longest name a spec and a sequence can hold: 4,096 bytes each.
    let text = "a".repeat(4_096);
    for (pattern, expected) in [
        // Every `*` must give way in turn before the final `b` fails.
        (format!("{}b", "a*".repeat(2_047)), false),
        (format!("{}b", "*a".repeat(2_047)), false),
        ("*".repeat(4_096), true),
        (format!("{}b", "*".repeat(4_095)), false),
        ("?".repeat(4_096), true),
        ("?".repeat(4_095), false),
        (format!("*{}", "a".repeat(4_095)), true),
        (format!("*{}b", "a".repeat(4_094)), false),
        (format!("{}*", "?".repeat(4_095)), true),
    ] {
        assert_eq!(
            glob_matches(&pattern, &text),
            expected,
            "{}...",
            &pattern[..8]
        );
        let steps = match_steps(&pattern, &text);
        assert!(
            steps <= bound(&pattern, &text),
            "{}...: {steps}",
            &pattern[..8]
        );
        // And far below it for these shapes: the last `*` alone gives way, once per character.
        assert!(steps <= 16 * 4_096 + 8_192, "{}...: {steps}", &pattern[..8]);
    }
    // A `*` gives way one character at a time, never one byte: over two characters of four
    // bytes each it restarts twice, so the work a selection counts is in characters.
    assert_eq!(match_steps("*b", "\u{1f321}\u{1f321}"), 4);
    assert_eq!(match_steps("*b", "aa"), 4);
    assert_eq!(match_steps("*\u{e9}", "\u{20ac}\u{20ac}\u{20ac}"), 6);
    // The steps of a match are a function of the two strings, pinned here for small ones.
    assert_eq!(match_steps("T*p", "yFan"), 1);
    assert_eq!(match_steps("T*p", "TSup"), 5);
    assert_eq!(match_steps("T*p", "TSupHeaEco"), 13);
    assert_eq!(match_steps("", ""), 0);
    assert_eq!(match_steps("***", ""), 3);
    assert_eq!(match_steps("abc", "abc"), 3);
}

#[test]
fn a_work_counter_refuses_one_unit_past_its_limit() {
    let mut work = Work { spent: 0, limit: 3 };
    assert!(work.spend(3).is_ok(), "at the limit");
    assert!(work.spend(0).is_ok());
    assert!(work.spend(1).is_err(), "one past it");
    let mut empty = Work { spent: 0, limit: 0 };
    assert!(empty.spend(0).is_ok());
    assert!(empty.spend(1).is_err());
    // A count past `u64` saturates and is refused, never wrapped into room.
    let mut large = Work {
        spent: u64::MAX - 1,
        limit: u64::MAX - 1,
    };
    assert!(large.spend(u64::MAX).is_err());
    assert_eq!(large.spent, u64::MAX);
    // A match under a limit gives the same answer as one without, or runs out.
    let mut exact = Work { spent: 0, limit: 5 };
    assert!(matches!(matches("T*p", "TSup", &mut exact), Ok(true)));
    let mut short = Work { spent: 0, limit: 4 };
    assert!(matches("T*p", "TSup", &mut short).is_err());
}

#[test]
fn a_glob_is_anchored_at_both_ends_and_nothing_else_is_special() {
    for (pattern, text, expected) in [
        // `*` is any run, the empty run included.
        ("TSup*", "TSup", true),
        ("TSup*", "TSupHeaEco", true),
        ("TSup*", "xTSup", false),
        ("TSup*", "TSu", false),
        ("*Fan", "uFan", true),
        ("*Fan", "Fan", true),
        ("*Fan", "uFanSta", false),
        ("y*Val", "yCooVal", true),
        ("y*Val", "yVal", true),
        ("y*Val", "yCooValve", false),
        ("*", "", true),
        ("*", "anything.at all", true),
        ("**", "x", true),
        ("*a*", "a", true),
        ("*a*", "bab", true),
        ("*a*", "bbb", false),
        // `?` is exactly one character.
        ("y?", "y1", true),
        ("y?", "y", false),
        ("y?", "y12", false),
        ("?", "", false),
        ("?", "y", true),
        ("??", "y", false),
        ("?*", "", false),
        ("?*", "y", true),
        ("*?", "y", true),
        ("y?oo*", "yCooVal", true),
        ("y?oo*", "yoo", false),
        // One character is one character, whatever its width in bytes.
        ("t?", "t\u{e9}", true),
        ("t?", "t\u{1f321}", true),
        ("t??", "t\u{1f321}", false),
        ("?\u{e9}", "\u{1f321}\u{e9}", true),
        ("*\u{e9}", "caf\u{e9}", true),
        ("*\u{e9}", "cafe", false),
        ("caf?", "cafe\u{301}", false),
        // Nothing else is special: no class, no escape, no separator, no case folding.
        ("[ab]", "a", false),
        ("[ab]", "[ab]", true),
        ("a.b", "a.b", true),
        ("a.b", "axb", false),
        ("a\\*", "a\\x", true),
        ("a\\*", "a*", false),
        ("{a,b}", "a", false),
        ("tsup*", "TSup", false),
        ("a*", "a/b.c", true),
        ("~", "x", false),
        // A pattern with neither wildcard matches only itself.
        ("yCooVal", "yCooVal", true),
        ("yCooVal", "yCooVa", false),
        ("u", "uFan", false),
        ("", "", true),
        ("", "a", false),
        // Backtracking: the last `*` gives way until the tail fits, or the text runs out.
        ("a*b*c", "abcbc", true),
        ("a*b*c", "abcbd", false),
        ("*aab", "aaaaab", true),
        ("a*a*a*b", "aaaaaaaa", false),
    ] {
        assert_eq!(
            glob_matches(pattern, text),
            expected,
            "{pattern:?} against {text:?}"
        );
        assert_eq!(glob_matches(pattern, text), reference(pattern, text));
    }
}

#[test]
fn the_prefix_is_literal_and_the_rest_is_a_glob_only_with_a_wildcard() {
    // The sequence's name is not a pattern: its `*` is a `*`.
    let key = OverrideKey::parse("a*b.y", "a*b").expect("the prefix");
    assert_eq!(key.rest(), "y");
    assert!(
        !key.is_glob(),
        "the wildcard is in the prefix, which is literal"
    );
    assert_eq!(OverrideKey::parse("axb.y", "a*b"), None);
    for (key, sequence) in [
        ("y", "s"),
        ("s", "s"),
        ("sy", "s"),
        ("S.y", "s"),
        ("s2.y", "s"),
        (" s.y", "s"),
        ("t.s.y", "s"),
        ("fanSta.y", "setPoiVAV1"),
        ("setPoiVAV.TSup*", "setPoiVAV1"),
    ] {
        assert_eq!(
            OverrideKey::parse(key, sequence),
            None,
            "{key} under {sequence}"
        );
    }
    // A sequence whose name holds a dot is one prefix; an empty rest is an exact name.
    let dotted = OverrideKey::parse("ahu.1.y*", "ahu.1").expect("the prefix");
    assert_eq!((dotted.rest(), dotted.is_glob()), ("y*", true));
    let empty = OverrideKey::parse("s.", "s").expect("the prefix");
    assert_eq!((empty.rest(), empty.is_glob()), ("", false));
    // Either wildcard makes a glob; a dotted rest without one is an exact name.
    for (rest, glob) in [
        ("y*", true),
        ("y?", true),
        ("*", true),
        ("a.b?", true),
        ("a.b", false),
    ] {
        let key = format!("s.{rest}");
        assert_eq!(
            OverrideKey::parse(&key, "s").map(|key| key.is_glob()),
            Some(glob),
            "{rest}"
        );
    }
    // Listing 12.1's key, as published.
    let listing = OverrideKey::parse("setPoiVAV1.TSup*", "setPoiVAV1").expect("the prefix");
    assert_eq!((listing.rest(), listing.is_glob()), ("TSup*", true));
}

#[test]
fn the_last_segment_is_what_follows_the_last_dot() {
    for (path, expected) in [
        (
            "http://example.org#Buildings.Utilities.CoolingCoilValve.yCooVal",
            "yCooVal",
        ),
        ("urn:x#top.ahu.uDam", "uDam"),
        ("urn:x#top", "urn:x#top"),
        ("y", "y"),
        ("a.", ""),
        (".y", "y"),
        ("", ""),
    ] {
        assert_eq!(last_segment(path), expected, "{path:?}");
    }
}
