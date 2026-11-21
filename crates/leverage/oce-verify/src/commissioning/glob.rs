//! `outputs` and `indicators` keys: the prefix, exact names and globs (commissioning note,
//! decisions 17 and 27).
//!
//! A key starts with its reference's `sequence` and a `.`, byte for byte, and that prefix is
//! literal. The rest is a **glob** when it holds `*` or `?`, where `*` stands for any run of
//! characters, the empty run included, `?` for exactly one character, and nothing else is
//! special: no escape, no character class, no path separator. A glob is matched whole, anchored
//! at both ends, against the last `.`-segment of an output's canonical path, so `u` selects only
//! `u` and never `uFan`. Any other rest is an **exact name**, resolved by the replay name rule.
//!
//! **Cost.** One match of a pattern of `p` bytes against a text of `n` bytes takes at most
//! `(p + 1) * (n + 1)` steps, with no recursion and no allocation, and a `*` gives way one
//! character at a time. Every step is counted against a [`Work`] limit, so what a selection
//! spends depends on the keys and the outputs alone, never on a clock.

// Bind (issue #662) is the caller of everything here. Until it lands only this crate's tests
// call it, so outside a test build the items are unused.
#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "called by the commissioning bind, issue #662; by this crate's tests until then"
    )
)]

/// A counter of work units against a limit.
pub(crate) struct Work {
    pub(crate) spent: u64,
    pub(crate) limit: u64,
}

/// The work limit was passed.
pub(crate) struct Exhausted;

impl Work {
    /// Spend `units`; refused once the total passes the limit. The total saturates.
    pub(crate) fn spend(&mut self, units: u64) -> Result<(), Exhausted> {
        self.spent = self.spent.saturating_add(units);
        if self.spent > self.limit {
            Err(Exhausted)
        } else {
            Ok(())
        }
    }
}

/// The bytes of the UTF-8 character that starts with `lead`.
fn width(lead: u8) -> usize {
    match lead {
        0x00..=0x7F => 1,
        0x80..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

/// Whether `text` matches `pattern`, whole, counting one unit of `work` per step. Iterative:
/// on a mismatch the last `*` takes one more character and the match resumes after it.
pub(crate) fn matches(pattern: &str, text: &str, work: &mut Work) -> Result<bool, Exhausted> {
    let (pattern, text) = (pattern.as_bytes(), text.as_bytes());
    let (mut p, mut t) = (0, 0);
    // The pattern index after the last `*` met, and the text index up to which that `*` reaches.
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        work.spend(1)?;
        match pattern.get(p) {
            Some(b'*') => {
                p += 1;
                star = Some((p, t));
            }
            Some(b'?') => {
                p += 1;
                t = (t + width(text[t])).min(text.len());
            }
            Some(byte) if *byte == text[t] => {
                p += 1;
                t += 1;
            }
            _ => {
                let Some((after, reach)) = star else {
                    return Ok(false);
                };
                // `reach` is at most `t`, so it is inside the text.
                let reach =
                    (reach + text.get(reach).map_or(1, |lead| width(*lead))).min(text.len());
                star = Some((after, reach));
                p = after;
                t = reach;
            }
        }
    }
    // The text is used up: what is left of the pattern must match the empty run.
    let rest = &pattern[p.min(pattern.len())..];
    work.spend(u64::try_from(rest.len()).unwrap_or(u64::MAX))?;
    Ok(rest.iter().all(|byte| *byte == b'*'))
}

/// Whether `text` matches the glob `pattern`, whole: `*` is any run of characters, the empty
/// run included, `?` is exactly one character, and every other character is itself. Anchored at
/// both ends; a pattern with neither wildcard matches only itself.
///
/// Never panics and allocates nothing; at most `(pattern.len() + 1) * (text.len() + 1)` steps.
pub(crate) fn glob_matches(pattern: &str, text: &str) -> bool {
    let mut work = Work {
        spent: 0,
        limit: u64::MAX,
    };
    matches(pattern, text, &mut work).unwrap_or(false)
}

/// The steps one match takes, for the cost tests.
#[cfg(test)]
pub(crate) fn match_steps(pattern: &str, text: &str) -> u64 {
    let mut work = Work {
        spent: 0,
        limit: u64::MAX,
    };
    let _ = matches(pattern, text, &mut work);
    work.spent
}

/// An `outputs` or `indicators` key, split from its reference's prefix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OverrideKey<'a> {
    rest: &'a str,
}

impl<'a> OverrideKey<'a> {
    /// `key` without the prefix `<sequence>.`; `None` when `key` does not start with `sequence`
    /// and a `.`, byte for byte. The prefix is literal: a `*` in it is a `*`.
    pub(crate) fn parse(key: &'a str, sequence: &str) -> Option<Self> {
        let rest = key.strip_prefix(sequence)?.strip_prefix('.')?;
        Some(Self { rest })
    }

    /// The key after its prefix: an exact name, or a glob.
    pub(crate) fn rest(&self) -> &'a str {
        self.rest
    }

    /// Whether the rest holds `*` or `?`, which makes the key a glob; otherwise it is an exact
    /// name.
    pub(crate) fn is_glob(&self) -> bool {
        self.rest.contains(['*', '?'])
    }
}

/// The last `.`-segment of a canonical path: what a glob is matched against (`yCooVal` for
/// `…CoolingCoilValve.yCooVal`). A path with no `.` is its own last segment.
pub(crate) fn last_segment(path: &str) -> &str {
    path.rsplit('.').next().unwrap_or(path)
}

#[cfg(test)]
#[path = "glob_tests.rs"]
mod tests;
