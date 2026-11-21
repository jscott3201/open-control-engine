//! Which outputs of a sequence a reference's `outputs` and `indicators` keys select, and the
//! tolerances and indicators that follow (commissioning note, decisions 17 and 27; the owner's
//! ruling of 2026-11-21 on precedence).
//!
//! A load cannot know a sequence's outputs, so this is done when a run binds a reference to a
//! sequence. The caller passes the sequence's output **canonical paths**, sorted ascending by
//! their bytes with none repeated, as `FrameSlots::outputs()` lists them, and each key selects:
//!
//! - **a glob** (its rest holds `*` or `?`): every output whose last `.`-segment the rest
//!   matches, whole ([`super::glob`]);
//! - **an exact name** (any other rest): the one output the rest names by the replay name rule
//!   ([`oce_replay::resolve_path`]): the path itself, or the one path that ends with the rest at
//!   a `.` boundary.
//!
//! A key that does not start with the reference's `sequence` and a `.`, a glob that matches no
//! output and an exact name that names none are each `unmatched-key`; an exact name that names
//! more than one output is `ambiguous-output`.
//!
//! **Precedence does not depend on the order of anything in the file.** For one output:
//!
//! 1. the spec's top-level `tolerances` come first;
//! 2. every glob that selects the output is laid over them;
//! 3. every exact key that selects the output is laid over that: **an exact key beats a glob**,
//!    field by field.
//!
//! Two keys of one kind (two globs, or two exact names) that select one output may give
//! different fields, or one field the same value, and then they combine. If they give one field
//! **different values**, no order could decide between them, and the selection is refused as
//! `conflicting-override`. Indicators have no such conflict: every rule that selects an output
//! adds its indicators, each name once, and the output is compared only where all are true.
//!
//! **Cost.** Every key is compared with every output, one work unit each, and a glob's matcher
//! steps are counted on top, against [`SpecLimits::max_key_match_work`]. The product of the two
//! counts is checked before anything is allocated, so a selection past the limit costs its
//! refusal and nothing else. What a selection holds is one bit per key and output: at most an
//! eighth of the work limit in bytes, 32 MiB at the maximum.

// Bind (issue #662) is the caller of everything here. Until it lands only this crate's tests
// call it, so outside a test build the items are unused.
#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "called by the commissioning bind, issue #662; by this crate's tests until then"
    )
)]

use std::collections::BTreeSet;

use oce_replay::PathIndex;

use super::error::{MAX_SPEC_DETAIL_BYTES, SpecError, SpecErrorKind, shown};
use super::glob::{Exhausted, OverrideKey, Work, last_segment, matches};
use super::limits::SpecLimits;
use super::model::{IndicatorRule, OutputOverride, SpecReference};
use super::tolerances::{PartialTolerances, TOLERANCE_NAMES};
use crate::number::Number;

/// One bit per key and output, a row per key: `keys * ceil(outputs / 64)` words.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Selected {
    outputs: usize,
    /// The words of one key's row.
    words: usize,
    bits: Vec<u64>,
}

impl Selected {
    fn new(keys: usize, outputs: usize) -> Self {
        let words = outputs.div_ceil(64);
        Self {
            outputs,
            words,
            bits: vec![0; keys.saturating_mul(words)],
        }
    }

    fn set(&mut self, key: usize, output: usize) {
        let word = key.saturating_mul(self.words).saturating_add(output / 64);
        if output < self.outputs
            && let Some(word) = self.bits.get_mut(word)
        {
            *word |= 1 << (output % 64);
        }
    }

    /// Whether `key` selects `output`; `false` for an index past either end.
    fn has(&self, key: usize, output: usize) -> bool {
        output < self.outputs
            && self
                .bits
                .get(key.saturating_mul(self.words).saturating_add(output / 64))
                .is_some_and(|word| word & (1 << (output % 64)) != 0)
    }
}

/// The keys of one map matched against a sequence's outputs.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Matched {
    selected: Selected,
    /// Whether each key, in the map's order, is a glob; an exact name otherwise.
    globs: Vec<bool>,
}

impl Matched {
    /// The keys that select `output`, as indexes into the map, ascending.
    fn keys_for(&self, output: usize) -> impl Iterator<Item = usize> + '_ {
        (0..self.globs.len()).filter(move |key| self.selected.has(*key, output))
    }
}

/// An output as a glob sees it: `<sequence>.<last segment>` of its canonical path.
fn seen_by_a_glob(sequence: &str, path: &str) -> String {
    format!("{sequence}.{}", last_segment(path))
}

/// Append to `detail` as many of the `total` `names` as its bound holds, quoted, then the count
/// left out. The names are taken one at a time and no further than the last one that fits, so a
/// sequence of a million outputs costs the few that are listed.
fn list_names(detail: &mut String, total: usize, names: impl Iterator<Item = String>) {
    // Room is kept for the count of the names left out.
    let room = MAX_SPEC_DETAIL_BYTES.saturating_sub(40);
    let mut listed = 0;
    for name in names {
        let next = shown(&name);
        if detail.len() + next.len() + 2 > room {
            break;
        }
        if listed > 0 {
            detail.push_str(", ");
        }
        detail.push_str(&next);
        listed += 1;
    }
    if listed < total {
        let separator = if listed > 0 { ", " } else { "" };
        detail.push_str(&format!("{separator}and {} more", total - listed));
    }
}

/// The refusal of a key that selects no output of the sequence: `unmatched-key`, on `map`
/// (`outputs` or `indicators`), with a sentence that names the key, how it was read (an exact
/// name, a glob, or a key without the reference's prefix) and the sequence's outputs as a glob
/// sees them, as many as the sentence's bound holds, so a typo cannot pass silently.
pub(crate) fn unmatched_key(map: &str, key: &str, sequence: &str, paths: &[&str]) -> SpecError {
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
    if paths.is_empty() {
        detail.push_str("the sequence has no outputs");
    } else {
        detail.push_str("the outputs are ");
        list_names(
            &mut detail,
            paths.len(),
            paths.iter().map(|path| seen_by_a_glob(sequence, path)),
        );
    }
    SpecError::on(SpecErrorKind::UnmatchedKey, map, &detail)
}

/// The refusal of an exact key whose rest names more than one output by the replay name rule:
/// `ambiguous-output`, with the canonical paths it names.
fn ambiguous_output(map: &str, key: &str, candidates: &[String]) -> SpecError {
    let mut detail = format!(
        "the key {} names more than one output of the sequence; write more of its path; it names ",
        shown(key)
    );
    list_names(&mut detail, candidates.len(), candidates.iter().cloned());
    SpecError::on(SpecErrorKind::AmbiguousOutput, map, &detail)
}

fn work_limit_exceeded(map: &str, limit: u64) -> SpecError {
    SpecError::on(
        SpecErrorKind::WorkLimitExceeded,
        map,
        &format!("matching the keys against the outputs needs more than {limit} work units"),
    )
}

/// Match `count` keys, the `index`th of which `key_at` gives, against `paths`.
fn select<'k>(
    map: &str,
    sequence: &str,
    count: usize,
    key_at: impl Fn(usize) -> &'k str,
    paths: &[&str],
    limits: &SpecLimits,
) -> Result<Matched, SpecError> {
    let limit = limits.effective().max_key_match_work;
    // Every key is compared with every output, one unit each at the least. A selection whose
    // pairs alone pass the limit is refused here, before anything is allocated.
    let pairs = u64::try_from(count)
        .unwrap_or(u64::MAX)
        .saturating_mul(u64::try_from(paths.len()).unwrap_or(u64::MAX));
    if pairs > limit {
        return Err(work_limit_exceeded(map, limit));
    }
    let mut work = Work {
        spent: pairs,
        limit,
    };
    let exhausted = |_: Exhausted| work_limit_exceeded(map, limit);
    let mut matched = Matched {
        selected: Selected::new(count, paths.len()),
        globs: Vec::with_capacity(count),
    };
    // Built for the first exact key, and for none when every key is a glob.
    let mut by_name: Option<PathIndex<'_>> = None;
    for index in 0..count {
        let key = key_at(index);
        let Some(parsed) = OverrideKey::parse(key, sequence) else {
            return Err(unmatched_key(map, key, sequence, paths));
        };
        matched.globs.push(parsed.is_glob());
        if parsed.is_glob() {
            let mut any = false;
            for (output, path) in paths.iter().enumerate() {
                if matches(parsed.rest(), last_segment(path), &mut work).map_err(exhausted)? {
                    matched.selected.set(index, output);
                    any = true;
                }
            }
            if !any {
                return Err(unmatched_key(map, key, sequence, paths));
            }
        } else {
            let names = by_name.get_or_insert_with(|| PathIndex::new(paths));
            match names.resolve(parsed.rest()) {
                Ok(output) => matched.selected.set(index, output),
                Err(candidates) if candidates.is_empty() => {
                    return Err(unmatched_key(map, key, sequence, paths));
                }
                Err(candidates) => return Err(ambiguous_output(map, key, &candidates)),
            }
        }
    }
    Ok(matched)
}

/// The first disagreement between two `outputs` keys of one kind over one output: the outputs in
/// the order given, the keys in the map's order, the fields in the order of
/// [`TOLERANCE_NAMES`].
fn first_conflict(
    overrides: &[OutputOverride],
    matched: &Matched,
    sequence: &str,
    paths: &[&str],
) -> Option<SpecError> {
    for (output, path) in paths.iter().enumerate() {
        // The first key of each kind to give each field, and the value it gives: exact names in
        // the first row, globs in the second.
        let mut given: [[Option<(usize, Number)>; 6]; 2] = [[None; 6]; 2];
        for key in matched.keys_for(output) {
            let kind = usize::from(matched.globs[key]);
            let written = overrides[key].tolerances.written;
            for (slot, value) in written.iter().enumerate() {
                let Some(value) = value else { continue };
                match given[kind][slot] {
                    None => given[kind][slot] = Some((key, *value)),
                    Some((_, earlier)) if same_value(earlier, *value) => {}
                    Some((other, earlier)) => {
                        return Some(conflicting_override(
                            (&overrides[other].key, earlier),
                            (&overrides[key].key, *value),
                            &seen_by_a_glob(sequence, path),
                            TOLERANCE_NAMES[slot],
                            matched.globs[key],
                        ));
                    }
                }
            }
        }
    }
    None
}

/// Whether two numbers are one value: `10` and `10.0` are, and so are `0` and `-0.0`.
fn same_value(left: Number, right: Number) -> bool {
    left.seconds().to_bits() == right.seconds().to_bits()
}

/// The refusal of two keys of one kind that give one output's tolerance field different values.
fn conflicting_override(
    first: (&str, Number),
    second: (&str, Number),
    output: &str,
    field: &str,
    globs: bool,
) -> SpecError {
    let kind = if globs { "globs" } else { "exact names" };
    SpecError::on(
        SpecErrorKind::ConflictingOverride,
        "outputs",
        &format!(
            "the keys {} and {} both select the output {} and give `{field}` different values, {} and {}; both are {kind}, so neither takes precedence",
            shown(first.0),
            shown(second.0),
            shown(output),
            first.1.canonical(),
            second.1.canonical(),
        ),
    )
}

/// Which `outputs` keys of one reference select which outputs of a sequence
/// ([`SpecReference::select_output_keys`]). It borrows the reference it was made from, so the
/// tolerances it gives are that reference's and no other's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OutputSelection<'a> {
    overrides: &'a [OutputOverride],
    matched: Matched,
}

impl OutputSelection<'_> {
    /// Whether the key at `key`, in the order of [`SpecReference::outputs`], selects the output
    /// at `output`, in the order the paths were given. `false` for an index past either end.
    pub(crate) fn selects(&self, key: usize, output: usize) -> bool {
        self.matched.selected.has(key, output)
    }

    /// The keys that select the output at `output`, as indexes into
    /// [`SpecReference::outputs`], ascending. Empty for an output no key selects and for an
    /// index past the end.
    pub(crate) fn keys_for(&self, output: usize) -> impl Iterator<Item = usize> + '_ {
        self.matched.keys_for(output)
    }

    /// The tolerances the output at `output` is compared under: `global`, the spec's top-level
    /// `tolerances`; then every glob that selects the output; then every exact key that selects
    /// it, so an exact key beats a glob field by field. Keys of one kind never disagree, or the
    /// selection would have been refused, so the result does not depend on any order. A name
    /// nothing gives stays absent; [`PartialTolerances::resolved`] reads it as zero.
    pub(crate) fn tolerances_for(
        &self,
        output: usize,
        global: Option<&PartialTolerances>,
    ) -> PartialTolerances {
        let mut merged = global.copied().unwrap_or_default();
        for globs in [true, false] {
            for key in self.matched.keys_for(output) {
                if self.matched.globs[key] == globs {
                    merged = merged.overlaid(&self.overrides[key].tolerances);
                }
            }
        }
        merged
    }
}

/// Which `indicators` keys of one reference select which outputs of a sequence
/// ([`SpecReference::select_indicator_keys`]). A type of its own, so a selection of `outputs`
/// keys cannot be read as one of `indicators` rules.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IndicatorSelection<'a> {
    rules: &'a [IndicatorRule],
    matched: Matched,
}

impl<'a> IndicatorSelection<'a> {
    /// Whether the rule at `key`, in the order of [`SpecReference::indicators`], selects the
    /// output at `output`. `false` for an index past either end.
    pub(crate) fn selects(&self, key: usize, output: usize) -> bool {
        self.matched.selected.has(key, output)
    }

    /// The indicators of the output at `output`: the names of every rule whose key selects it,
    /// each name once, in the order of the rules and then of each rule's list. The output is
    /// compared only where all of them are true; an empty list gates nothing.
    pub(crate) fn indicators_for(&self, output: usize) -> Vec<&'a str> {
        let mut all: Vec<&'a str> = Vec::new();
        let mut seen = BTreeSet::new();
        for key in self.matched.keys_for(output) {
            for indicator in self.rules[key].names() {
                if seen.insert(indicator.as_str()) {
                    all.push(indicator);
                }
            }
        }
        all
    }
}

impl SpecReference {
    /// Which `outputs` keys select which of a sequence's outputs. `paths` are the outputs'
    /// canonical paths, sorted ascending by their bytes with none repeated, as
    /// `FrameSlots::outputs()` lists them; out of that order an exact name can be missed, as
    /// [`oce_replay::resolve_path`] misses it.
    ///
    /// # Errors
    /// In this order: `work-limit-exceeded`, before anything is allocated, when the keys times
    /// the outputs pass [`SpecLimits::max_key_match_work`], and later when the matcher's steps
    /// do; for the first key in the order of [`Self::outputs`] that fails, `unmatched-key` (no
    /// prefix, a glob that matches nothing, an exact name that names nothing) or
    /// `ambiguous-output` (an exact name that names several); then `conflicting-override` for
    /// the first output two keys of one kind give one field different values for.
    pub(crate) fn select_output_keys(
        &self,
        paths: &[&str],
        limits: &SpecLimits,
    ) -> Result<OutputSelection<'_>, SpecError> {
        let overrides = self.outputs();
        let matched = select(
            "outputs",
            &self.sequence,
            overrides.len(),
            |index| overrides[index].key(),
            paths,
            limits,
        )?;
        if let Some(conflict) = first_conflict(overrides, &matched, &self.sequence, paths) {
            return Err(conflict);
        }
        Ok(OutputSelection { overrides, matched })
    }

    /// Which `indicators` keys select which of a sequence's outputs, as
    /// [`Self::select_output_keys`] does for `outputs`. Rules never conflict: every rule that
    /// selects an output applies.
    ///
    /// # Errors
    /// `work-limit-exceeded`, `unmatched-key` or `ambiguous-output`, as
    /// [`Self::select_output_keys`] gives them.
    pub(crate) fn select_indicator_keys(
        &self,
        paths: &[&str],
        limits: &SpecLimits,
    ) -> Result<IndicatorSelection<'_>, SpecError> {
        let rules = self.indicators();
        let matched = select(
            "indicators",
            &self.sequence,
            rules.len(),
            |index| rules[index].key(),
            paths,
            limits,
        )?;
        Ok(IndicatorSelection { rules, matched })
    }
}

#[cfg(test)]
#[path = "selection_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "selection_cost_tests.rs"]
mod cost_tests;
