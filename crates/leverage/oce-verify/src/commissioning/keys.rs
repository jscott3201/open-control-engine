//! `outputs` and `indicators` keys: exact names, globs, and which outputs each key selects
//! (commissioning note, decisions 17 and 27).
//!
//! A key starts with its reference's `sequence` and a `.`, byte for byte, and that prefix is
//! literal. The rest is a **glob** when it holds `*` or `?`, where `*` stands for any run of
//! characters, the empty run included, `?` for exactly one character, and nothing else is
//! special: no escape, no character class, no path separator. A glob is matched against a whole
//! name, anchored at both ends, so `u` selects only `u` and never `uFan`. Any other rest is an
//! **exact name**.
//!
//! The names a key is matched against are the sequence's outputs as a glob sees them:
//! `<sequence>.<last segment>` of each output's canonical path. A load cannot know them, so the
//! matching is done when a run binds a reference to a sequence, with what this module supplies:
//! the matcher ([`glob_matches`]), a key split from its prefix ([`OverrideKey`]), the selection
//! of a whole map against a list of names ([`SpecReference::select_output_keys`],
//! [`SpecReference::select_indicator_keys`]) with its `unmatched-key` refusal, and the two
//! resolutions over a selection: tolerances, where "overrides apply in declaration order, later
//! fields winning" ([`crate::CommissioningSpec::tolerances_for`]), and indicators, where every
//! key that selects an output adds its indicators ([`SpecReference::indicators_for`]).
//!
//! **Cost.** One match of a pattern of `p` bytes against a name of `n` bytes takes at most
//! `(p + 1) * (n + 1)` steps, with no recursion and no allocation. A selection counts every
//! step, and one unit per key and name it compares, against
//! [`SpecLimits::max_key_match_work`]; past it the selection is refused as
//! `work-limit-exceeded`, so a spec of many long globs against a sequence of many long names
//! costs a stated amount of work and no more. The count depends on the keys and names alone.

use std::collections::BTreeSet;

use super::error::{MAX_SPEC_DETAIL_BYTES, SpecError, SpecErrorKind, shown};
use super::limits::SpecLimits;
use super::model::SpecReference;
use super::spec::CommissioningSpec;
use super::tolerances::PartialTolerances;

/// A counter of matcher steps against a limit.
struct Work {
    spent: u64,
    limit: u64,
}

/// The work limit was passed.
struct Exhausted;

impl Work {
    fn spend(&mut self, units: u64) -> Result<(), Exhausted> {
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
fn matches(pattern: &str, text: &str, work: &mut Work) -> Result<bool, Exhausted> {
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
///
/// ```
/// use oce_verify::glob_matches;
///
/// assert!(glob_matches("TSup*", "TSupHeaEco"));
/// assert!(glob_matches("y?oo*", "yCooVal"));
/// assert!(!glob_matches("u", "uFan"));
/// assert!(!glob_matches("*Fan", "uFanSta"));
/// ```
#[must_use]
pub fn glob_matches(pattern: &str, text: &str) -> bool {
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
pub struct OverrideKey<'a> {
    rest: &'a str,
}

impl<'a> OverrideKey<'a> {
    /// `key` without the prefix `<sequence>.`; `None` when `key` does not start with `sequence`
    /// and a `.`, byte for byte. The prefix is literal: a `*` in it is a `*`.
    #[must_use]
    pub fn parse(key: &'a str, sequence: &str) -> Option<Self> {
        let rest = key.strip_prefix(sequence)?.strip_prefix('.')?;
        Some(Self { rest })
    }

    /// The key after its prefix: an exact name, or a glob.
    #[must_use]
    pub fn rest(&self) -> &'a str {
        self.rest
    }

    /// Whether the rest holds `*` or `?`, which makes the key a glob; otherwise it is an exact
    /// name.
    #[must_use]
    pub fn is_glob(&self) -> bool {
        self.rest.contains(['*', '?'])
    }

    /// Whether the key selects the output whose name after the prefix is `name`: a glob by
    /// [`glob_matches`], whole, and an exact name by equality of the bytes.
    #[must_use]
    pub fn selects(&self, name: &str) -> bool {
        if self.is_glob() {
            glob_matches(self.rest, name)
        } else {
            self.rest == name
        }
    }
}

/// Which keys of one map select which names: the result of
/// [`SpecReference::select_output_keys`] or [`SpecReference::select_indicator_keys`].
///
/// Every key selects at least one name; a name may be selected by none, and then no override
/// applies to it. It holds one bit per key and name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeySelection {
    keys: usize,
    names: usize,
    /// One row of `words` words per name; bit `k` of a row is key `k`.
    words: usize,
    bits: Vec<u64>,
}

impl KeySelection {
    fn new(keys: usize, names: usize) -> Self {
        let words = keys.div_ceil(64);
        Self {
            keys,
            names,
            words,
            bits: vec![0; names.saturating_mul(words)],
        }
    }

    fn set(&mut self, key: usize, name: usize) {
        if let Some(word) = self.bits.get_mut(name * self.words + key / 64) {
            *word |= 1 << (key % 64);
        }
    }

    /// The number of keys the map has.
    #[must_use]
    pub fn key_count(&self) -> usize {
        self.keys
    }

    /// The number of names the keys were matched against.
    #[must_use]
    pub fn name_count(&self) -> usize {
        self.names
    }

    /// Whether the key at `key`, in declaration order, selects the name at `name`, in the order
    /// the names were given. `false` for an index past either end.
    #[must_use]
    pub fn selects(&self, key: usize, name: usize) -> bool {
        key < self.keys
            && name < self.names
            && self
                .bits
                .get(name * self.words + key / 64)
                .is_some_and(|word| word & (1 << (key % 64)) != 0)
    }

    /// The keys that select the name at `name`, as indexes in declaration order, ascending:
    /// the order their overrides apply in. Empty for a name no key selects and for an index
    /// past the end.
    pub fn keys_for(&self, name: usize) -> impl Iterator<Item = usize> + '_ {
        (0..self.keys).filter(move |key| self.selects(*key, name))
    }
}

/// The refusal of a key that selects no output of the sequence: `unmatched-key`, on `map`
/// (`outputs` or `indicators`), with a sentence that names the key, how it was read (an exact
/// name, a glob, or a key without the reference's prefix) and the sequence's output names as a
/// glob sees them, as many as the sentence's bound holds, so a typo cannot pass silently.
///
/// A selection raises it; it is public so that a caller that resolves an exact key by another
/// rule can raise the same refusal in the same words.
#[must_use]
pub fn unmatched_key(map: &str, key: &str, sequence: &str, names: &[&str]) -> SpecError {
    let read_as = match OverrideKey::parse(key, sequence) {
        None => format!(
            "it does not start with the sequence's prefix {}",
            shown(&format!("{sequence}."))
        ),
        Some(parsed) if parsed.is_glob() => "it was read as a glob".to_owned(),
        Some(_) => "it was read as an exact name".to_owned(),
    };
    let mut detail = format!(
        "the key {} selects no output of the sequence; {read_as}; ",
        shown(key)
    );
    if names.is_empty() {
        detail.push_str("the sequence has no outputs");
    } else {
        detail.push_str("the outputs are ");
        // Room is kept for the count of the names left out.
        let room = MAX_SPEC_DETAIL_BYTES.saturating_sub(40);
        let mut listed = 0;
        for name in names {
            let next = shown(name);
            if detail.len() + next.len() + 2 > room {
                break;
            }
            if listed > 0 {
                detail.push_str(", ");
            }
            detail.push_str(&next);
            listed += 1;
        }
        if listed < names.len() {
            let separator = if listed > 0 { ", " } else { "" };
            detail.push_str(&format!("{separator}and {} more", names.len() - listed));
        }
    }
    SpecError::on(SpecErrorKind::UnmatchedKey, map, &detail)
}

/// Match every key of one map against `names`, in declaration order.
fn select(
    map: &str,
    sequence: &str,
    keys: &[&str],
    names: &[&str],
    limits: &SpecLimits,
) -> Result<KeySelection, SpecError> {
    let limit = limits.effective().max_key_match_work;
    let mut work = Work { spent: 0, limit };
    let exhausted = |_: Exhausted| {
        SpecError::on(
            SpecErrorKind::WorkLimitExceeded,
            map,
            &format!(
                "matching the keys against the output names needed more than {limit} work units"
            ),
        )
    };
    // What a key's rest is matched against: each name after the prefix, where it has it.
    let rests: Vec<Option<&str>> = names
        .iter()
        .map(|name| OverrideKey::parse(name, sequence).map(|parsed| parsed.rest))
        .collect();
    let mut selection = KeySelection::new(keys.len(), names.len());
    for (index, key) in keys.iter().enumerate() {
        let mut any = false;
        if let Some(parsed) = OverrideKey::parse(key, sequence) {
            let glob = parsed.is_glob();
            for (name, rest) in rests.iter().enumerate() {
                work.spend(1).map_err(exhausted)?;
                let Some(rest) = rest else { continue };
                let selected = if glob {
                    matches(parsed.rest, rest, &mut work).map_err(exhausted)?
                } else {
                    parsed.rest == *rest
                };
                if selected {
                    selection.set(index, name);
                    any = true;
                }
            }
        }
        if !any {
            return Err(unmatched_key(map, key, sequence, names));
        }
    }
    Ok(selection)
}

impl SpecReference {
    /// Which `outputs` keys select which of `names`: the sequence's outputs as a glob sees
    /// them, `<sequence>.<last segment>` of each output's canonical path, in any order the
    /// caller will index them by. An exact key selects the names equal to it and a glob the
    /// names it matches whole; a name without the reference's prefix is selected by nothing.
    ///
    /// # Errors
    /// `unmatched-key` for the first key, in declaration order, that selects no name (a key
    /// without the reference's prefix is one); `work-limit-exceeded` past
    /// [`SpecLimits::max_key_match_work`].
    pub fn select_output_keys(
        &self,
        names: &[&str],
        limits: &SpecLimits,
    ) -> Result<KeySelection, SpecError> {
        let keys: Vec<&str> = self.outputs().iter().map(|each| each.key()).collect();
        select("outputs", &self.sequence, &keys, names, limits)
    }

    /// Which `indicators` keys select which of `names`, as [`Self::select_output_keys`] does
    /// for `outputs`.
    ///
    /// # Errors
    /// As [`Self::select_output_keys`].
    pub fn select_indicator_keys(
        &self,
        names: &[&str],
        limits: &SpecLimits,
    ) -> Result<KeySelection, SpecError> {
        let keys: Vec<&str> = self.indicators().iter().map(|each| each.key()).collect();
        select("indicators", &self.sequence, &keys, names, limits)
    }

    /// The indicators of the output at `name` under `selection`
    /// ([`Self::select_indicator_keys`]): the names of every rule whose key selects it, in
    /// declaration order, each name once. The output is compared only where all of them are
    /// true; an empty list gates nothing.
    #[must_use]
    pub fn indicators_for(&self, selection: &KeySelection, name: usize) -> Vec<&str> {
        let mut all: Vec<&str> = Vec::new();
        let mut seen = BTreeSet::new();
        for key in selection.keys_for(name) {
            let Some(rule) = self.indicators().get(key) else {
                continue;
            };
            for indicator in rule.names() {
                if seen.insert(indicator.as_str()) {
                    all.push(indicator);
                }
            }
        }
        all
    }
}

impl CommissioningSpec {
    /// The tolerances the output at `name` is compared under, for `reference` and `selection`
    /// ([`SpecReference::select_output_keys`]): the spec's top-level `tolerances`, then the
    /// tolerances of every `outputs` key that selects the output, laid over one another in
    /// declaration order, later fields winning. A name nothing gives stays absent;
    /// [`PartialTolerances::or_zero`] reads it as zero.
    #[must_use]
    pub fn tolerances_for(
        &self,
        reference: &SpecReference,
        selection: &KeySelection,
        name: usize,
    ) -> PartialTolerances {
        let mut merged = self.tolerances().copied().unwrap_or_default();
        for key in selection.keys_for(name) {
            if let Some(each) = reference.outputs().get(key) {
                merged = merged.overlaid(each.tolerances());
            }
        }
        merged
    }
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;
