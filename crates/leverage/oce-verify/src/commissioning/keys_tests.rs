//! The glob matcher against a second implementation, exhaustively over short patterns and
//! texts, and its cost in steps.

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
fn a_selection_holds_one_bit_per_key_and_name() {
    let mut selection = KeySelection::new(130, 3);
    assert_eq!((selection.key_count(), selection.name_count()), (130, 3));
    for (key, name) in [(0, 0), (63, 0), (64, 0), (129, 0), (1, 2), (65, 2)] {
        selection.set(key, name);
    }
    assert_eq!(selection.keys_for(0).collect::<Vec<_>>(), [0, 63, 64, 129]);
    assert_eq!(selection.keys_for(1).count(), 0);
    assert_eq!(selection.keys_for(2).collect::<Vec<_>>(), [1, 65]);
    assert!(selection.selects(64, 0) && !selection.selects(64, 1) && !selection.selects(62, 0));
    // Out of range: no bit, no panic, and a write past the end is dropped.
    assert!(!selection.selects(130, 0) && !selection.selects(0, 3));
    selection.set(0, 3);
    selection.set(usize::MAX / 2, 2);
    assert_eq!(selection.keys_for(3).count(), 0);
    // No key: no word, and nothing selects.
    let none = KeySelection::new(0, 5);
    assert_eq!(none.keys_for(0).count(), 0);
    assert!(!none.selects(0, 0));
}
